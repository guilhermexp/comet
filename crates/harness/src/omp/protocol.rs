use serde_json::Value;

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
    let collapsed = value
        .chars()
        .map(|character| {
            if matches!(character, '\r' | '\n' | '\t') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let lower = collapsed.to_ascii_lowercase();
    for key in [
        "authorization",
        "api_key",
        "api-key",
        "apikey",
        "token",
        "password",
        "passwd",
        "secret",
        "client_secret",
        "credential",
    ] {
        if let Some(position) = lower.find(key) {
            let after = &collapsed[position + key.len()..];
            if after.trim_start().starts_with(':') || after.trim_start().starts_with('=') {
                let mut redacted = collapsed[..position + key.len()].trim_end().to_owned();
                redacted.push_str("=[redacted]");
                redacted.truncate(512);
                return redacted;
            }
        }
    }
    let mut sanitized = collapsed;
    if let Some(position) = sanitized.to_ascii_lowercase().find("bearer ") {
        let start = position + "bearer ".len();
        let end = sanitized[start..]
            .find(char::is_whitespace)
            .map(|offset| start + offset)
            .unwrap_or(sanitized.len());
        sanitized.replace_range(start..end, "[redacted]");
    }
    if let Some(start) = sanitized.find("-----BEGIN ")
        && let Some(relative_end) = sanitized[start..].find(" PRIVATE KEY-----")
    {
        let end = start + relative_end + " PRIVATE KEY-----".len();
        sanitized.replace_range(start..end, "[redacted private key]");
    }
    sanitized = sanitized.trim().to_owned();
    sanitized.truncate(512);
    sanitized
}
