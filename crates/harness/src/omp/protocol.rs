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
