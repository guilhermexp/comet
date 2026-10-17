//! Per-session state that outlives this process, and result rendering.
//!
//! Agent hosts recycle their MCP servers mid-conversation (OMP replaces its
//! `rpc-ui` process), so "preamble already sent" and "how many runtimes this
//! session had" live on disk under the session key, not in memory.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The runtime opens its long instructions block with this heading.
const PREAMBLE_PREFIX: &str = "## Computer Use";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    #[serde(default)]
    pub preamble_sent: bool,
    /// Runtimes this session has started. More than one means lost state.
    #[serde(default)]
    pub boots: u32,
}

/// `COMET_COMPUTER_USE_SESSION` (chat) → `UNPEEL_SESSION_ID` (Worker) → pid.
pub fn session_key() -> String {
    let raw = ["COMET_COMPUTER_USE_SESSION", "UNPEEL_SESSION_ID"]
        .iter()
        .find_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| format!("pid-{}", std::process::id()));
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub struct SessionStore {
    dir: PathBuf,
    memory: SessionState,
    shot_seq: u64,
}

impl SessionStore {
    /// `{root}/{key}/state.json`, screenshots in `{root}/{key}/shots/`.
    pub fn new(root: &Path, key: &str) -> Self {
        Self {
            dir: root.join(key),
            memory: SessionState::default(),
            shot_seq: 0,
        }
    }

    pub fn read(&self) -> SessionState {
        std::fs::read(self.dir.join("state.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or(self.memory)
    }

    pub fn write(&mut self, state: SessionState) {
        self.memory = state;
        if std::fs::create_dir_all(&self.dir).is_ok()
            && let Ok(bytes) = serde_json::to_vec(&state)
        {
            let _ = std::fs::write(self.dir.join("state.json"), bytes);
        }
    }

    /// Count a fresh runtime. Returns true when this session already had
    /// one, which means its REPL variables and handles are gone.
    pub fn note_boot(&mut self) -> bool {
        let mut state = self.read();
        state.boots += 1;
        self.write(state);
        state.boots > 1
    }

    /// Turn a runtime `tools/call` result into this server's result: the
    /// pending notice first, the preamble only once per session, images
    /// labeled by their bytes and saved to disk with the path in the text.
    pub fn render(&mut self, result: &Value, notice: Option<String>) -> Value {
        let mut content = Vec::new();
        let mut shots = Vec::new();
        if let Some(notice) = notice {
            content.push(json!({ "type": "text", "text": notice }));
        }
        for part in result
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            match part.get("type").and_then(Value::as_str) {
                Some("text") => {
                    let Some(text) = part.get("text").and_then(Value::as_str) else {
                        continue;
                    };
                    if text.starts_with(PREAMBLE_PREFIX) {
                        let mut state = self.read();
                        if state.preamble_sent {
                            continue;
                        }
                        state.preamble_sent = true;
                        self.write(state);
                    }
                    content.push(json!({ "type": "text", "text": text }));
                }
                Some("image") => {
                    let Some(data) = part.get("data").and_then(Value::as_str) else {
                        continue;
                    };
                    let declared = part
                        .get("mimeType")
                        .and_then(Value::as_str)
                        .unwrap_or("image/png");
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .unwrap_or_default();
                    let (mime, ext) = sniff_image(&bytes).unwrap_or_else(|| {
                        (declared, declared.rsplit('/').next().unwrap_or("bin"))
                    });
                    if let Some(path) = self.save_shot(&bytes, ext) {
                        shots.push(path.display().to_string());
                    }
                    content.push(json!({ "type": "image", "data": data, "mimeType": mime }));
                }
                _ => {}
            }
        }
        if !shots.is_empty() {
            content.push(json!({
                "type": "text",
                "text": format!("screenshots saved: {}", shots.join(", ")),
            }));
        }
        if content.is_empty() {
            content.push(json!({ "type": "text", "text": "(the runtime returned no content)" }));
        }
        json!({
            "content": content,
            "isError": result.get("isError").and_then(Value::as_bool) == Some(true),
        })
    }

    fn save_shot(&mut self, bytes: &[u8], ext: &str) -> Option<PathBuf> {
        if bytes.is_empty() {
            return None;
        }
        let dir = self.dir.join("shots");
        std::fs::create_dir_all(&dir).ok()?;
        self.shot_seq += 1;
        let path = dir.join(format!(
            "shot-{}-{}.{ext}",
            std::process::id(),
            self.shot_seq
        ));
        std::fs::write(&path, bytes).ok()?;
        Some(path)
    }
}

/// The runtime labels JPEG bytes as PNG; hosts reject a mislabeled image.
pub fn sniff_image(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(("image/jpeg", "jpg"))
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some(("image/png", "png"))
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(("image/webp", "webp"))
    } else if bytes.starts_with(b"GIF") {
        Some(("image/gif", "gif"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JPEG: &[u8] = &[0xff, 0xd8, 0xff, 0xe0, 0, 1, 2, 3];

    #[test]
    fn preamble_only_in_the_first_result_even_across_processes() {
        let root = tempfile::tempdir().unwrap();
        let result = json!({ "content": [
            { "type": "text", "text": "## Computer Use\nlong instructions" },
            { "type": "text", "text": "ok" },
        ]});
        let mut first = SessionStore::new(root.path(), "chat-1");
        let rendered = first.render(&result, None);
        assert_eq!(rendered["content"].as_array().unwrap().len(), 2);
        // A new process for the same session must not repeat it.
        let mut second = SessionStore::new(root.path(), "chat-1");
        let rendered = second.render(&result, None);
        assert_eq!(
            rendered["content"],
            json!([{ "type": "text", "text": "ok" }])
        );
    }

    #[test]
    fn image_is_labeled_by_its_bytes_and_saved() {
        let root = tempfile::tempdir().unwrap();
        let mut store = SessionStore::new(root.path(), "chat-1");
        let data = base64::engine::general_purpose::STANDARD.encode(JPEG);
        let result =
            json!({ "content": [{ "type": "image", "data": data, "mimeType": "image/png" }] });
        let rendered = store.render(&result, Some("notice".into()));
        let content = rendered["content"].as_array().unwrap();
        assert_eq!(content[0]["text"], "notice");
        assert_eq!(content[1]["mimeType"], "image/jpeg");
        let saved = content[2]["text"].as_str().unwrap();
        let path = saved.strip_prefix("screenshots saved: ").unwrap();
        assert!(path.ends_with(".jpg"), "{path}");
        assert_eq!(std::fs::read(path).unwrap(), JPEG);
    }

    #[test]
    fn boots_count_across_stores() {
        let root = tempfile::tempdir().unwrap();
        assert!(!SessionStore::new(root.path(), "w").note_boot());
        assert!(SessionStore::new(root.path(), "w").note_boot());
    }

    #[test]
    fn empty_result_says_so_and_keeps_is_error() {
        let root = tempfile::tempdir().unwrap();
        let mut store = SessionStore::new(root.path(), "k");
        let rendered = store.render(&json!({ "content": [], "isError": true }), None);
        assert_eq!(rendered["isError"], true);
        assert_eq!(rendered["content"].as_array().unwrap().len(), 1);
    }
}
