use base64::Engine as _;
use serde_json::Value;
use std::sync::OnceLock;

use crate::HarnessError;

pub const MAX_INBOUND_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_OUTBOUND_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_PENDING_REQUESTS: usize = 64;

pub fn parse_frame(line: &str) -> Result<Value, HarnessError> {
    if line.len() > MAX_INBOUND_BYTES {
        return Err(HarnessError::Protocol(format!(
            "OMP RPC frame exceeded {MAX_INBOUND_BYTES} bytes"
        )));
    }
    let value: Value = serde_json::from_str(line).map_err(|_| {
        let snippet = sanitize_diagnostic(&line.chars().take(200).collect::<String>());
        HarnessError::Protocol(if snippet.is_empty() {
            "OMP RPC emitted malformed JSONL".into()
        } else {
            format!("OMP RPC emitted malformed JSONL: {snippet}")
        })
    })?;
    if !value.is_object() || value.get("type").and_then(Value::as_str).is_none() {
        return Err(HarnessError::Protocol(
            "OMP RPC emitted an invalid frame".into(),
        ));
    }
    Ok(value)
}

pub fn serialize_frame(value: &Value) -> Result<String, HarnessError> {
    let serialized = serde_json::to_string(value)
        .map_err(|error| HarnessError::Protocol(format!("OMP RPC encode failed: {error}")))?;
    if serialized.len() > MAX_OUTBOUND_BYTES {
        return Err(HarnessError::Protocol(format!(
            "OMP RPC outbound frame exceeded {MAX_OUTBOUND_BYTES} bytes"
        )));
    }
    Ok(serialized)
}

/// Remonta os frames `rpc_chunk` da v2 do protocolo do OMP.
///
/// O filho corta o JSONL em [`MAX_INBOUND_BYTES`]-ish por linha (1 MiB do lado
/// dele). Acima disso, um cliente que negociou a v2 recebe a resposta partida
/// em `rpc_chunk` base64 para remontar; um cliente v1 recebe no lugar
/// `{"success":false,"error":"RPC response exceeded the transport limit"}`. Era
/// o que apagava a lista de modelos do picker: o catalogo do OMP mede ~1,2 MiB
/// em 550 linhas, entao `get_available_models` NUNCA cabia em um frame.
#[derive(Default)]
pub struct ChunkAssembler {
    partial: Option<Partial>,
}

struct Partial {
    id: String,
    count: u64,
    declared: usize,
    next_index: u64,
    body: Vec<u8>,
}

impl ChunkAssembler {
    /// `Ok(None)` = a sequencia ainda nao fechou; frames comuns passam intactos.
    pub fn push(&mut self, frame: Value) -> Result<Option<Value>, HarnessError> {
        if frame.get("type").and_then(Value::as_str) != Some("rpc_chunk") {
            // Um frame comum no meio de uma sequencia significa pedaco perdido:
            // falhar alto e melhor que remontar JSON picotado.
            if self.partial.take().is_some() {
                return Err(chunk_error("sequence interrupted"));
            }
            return Ok(Some(frame));
        }
        let pushed = self.push_chunk(&frame);
        if pushed.is_err() {
            self.partial = None;
        }
        pushed
    }

    fn push_chunk(&mut self, frame: &Value) -> Result<Option<Value>, HarnessError> {
        let id = frame
            .get("chunkId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 128)
            .ok_or_else(|| chunk_error("id is missing or invalid"))?;
        let index = frame
            .get("index")
            .and_then(Value::as_u64)
            .ok_or_else(|| chunk_error("index is missing or invalid"))?;
        let count = frame
            .get("count")
            .and_then(Value::as_u64)
            .filter(|count| *count >= 2)
            .ok_or_else(|| chunk_error("count is missing or invalid"))?;
        let declared = frame
            .get("byteLength")
            .and_then(Value::as_u64)
            .and_then(|bytes| usize::try_from(bytes).ok())
            .filter(|bytes| *bytes > 0 && *bytes <= MAX_INBOUND_BYTES)
            .ok_or_else(|| {
                chunk_error(&format!(
                    "declared length is missing or over {MAX_INBOUND_BYTES} bytes"
                ))
            })?;
        let payload = frame
            .get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| chunk_error("data is missing"))
            .and_then(|data| {
                base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|_| chunk_error("data is not base64"))
            })?;

        let partial = match self.partial.as_mut() {
            Some(partial) => {
                if partial.id != id
                    || partial.count != count
                    || partial.declared != declared
                    || partial.next_index != index
                {
                    return Err(chunk_error("sequence mismatch"));
                }
                partial
            }
            None => {
                if index != 0 {
                    return Err(chunk_error("sequence must start at index 0"));
                }
                // ponytail: o buffer cresce sozinho em vez de reservar
                // `declared` de uma vez — o numero vem do filho, e um valor
                // inflado nao deve virar alocacao antes de um byte chegar.
                self.partial.insert(Partial {
                    id: id.to_owned(),
                    count,
                    declared,
                    next_index: 0,
                    body: Vec::new(),
                })
            }
        };
        partial.body.extend_from_slice(&payload);
        partial.next_index += 1;
        if partial.body.len() > partial.declared {
            return Err(chunk_error("sequence exceeds its declared length"));
        }
        if partial.next_index < partial.count {
            return Ok(None);
        }
        let partial = self.partial.take().expect("a sequence that just closed");
        if partial.body.len() != partial.declared {
            return Err(chunk_error("sequence length mismatch"));
        }
        let line =
            String::from_utf8(partial.body).map_err(|_| chunk_error("frame is not UTF-8"))?;
        parse_frame(&line).map(Some)
    }
}

fn chunk_error(reason: &str) -> HarnessError {
    HarnessError::Protocol(format!("OMP RPC chunk {reason}"))
}

pub fn sanitize_diagnostic(value: &str) -> String {
    static PRIVATE_KEY: OnceLock<regex::Regex> = OnceLock::new();
    static PREFIXED_TOKEN: OnceLock<regex::Regex> = OnceLock::new();
    static SECRET_ASSIGNMENT: OnceLock<regex::Regex> = OnceLock::new();
    static BEARER: OnceLock<regex::Regex> = OnceLock::new();

    let private_key = PRIVATE_KEY.get_or_init(|| {
        regex::Regex::new(
            r"(?is)-----BEGIN [^-\r\n]+ PRIVATE KEY-----.*?-----END [^-\r\n]+ PRIVATE KEY-----",
        )
        .expect("valid private-key sanitizer")
    });
    let prefixed_token = PREFIXED_TOKEN.get_or_init(|| {
        regex::Regex::new(r"(?i)\b(?:sk|key|token)-[A-Za-z0-9._-]{6,}\b")
            .expect("valid token sanitizer")
    });
    let secret_assignment = SECRET_ASSIGNMENT.get_or_init(|| {
        regex::Regex::new(
            r#"(?i)\b(api[_-]?key|auth(?:orization)?|token|password|passwd|secret|client[_-]?secret|credential)["']?\s*[:=]\s*[^\r\n,;]+"#,
        )
        .expect("valid secret sanitizer")
    });
    let bearer = BEARER.get_or_init(|| {
        regex::Regex::new(r"(?i)Bearer\s+[A-Za-z0-9._~+/-]+=*").expect("valid bearer sanitizer")
    });

    let private_keys = private_key.replace_all(value, "[redacted private key]");
    let bearer_tokens = bearer.replace_all(&private_keys, "Bearer [redacted]");
    let prefixed_tokens = prefixed_token.replace_all(&bearer_tokens, "[redacted]");
    let assigned = secret_assignment.replace_all(&prefixed_tokens, "$1=[redacted]");
    let mut sanitized = assigned
        .chars()
        .map(|character| {
            if matches!(character, '\r' | '\n' | '\t') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .trim()
        .to_owned();
    sanitized.truncate(512);
    sanitized
}
