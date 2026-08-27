//! Plain-file persistence (`~/.unpeel/todos.json` by default): a flat JSON
//! list, atomic whole-file overwrite. Deliberately boring — the plugin plan
//! calls for a plain file under `~/.unpeel`, nothing session-scoped.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Todo {
    pub id: u64,
    pub text: String,
    pub done: bool,
    pub created_at: u64,
}

pub struct Store {
    path: PathBuf,
    pub todos: Vec<Todo>,
}

pub fn default_path() -> PathBuf {
    match std::env::var("UNPEEL_HOME").ok().filter(|h| !h.is_empty()) {
        Some(home) => PathBuf::from(home).join("todos.json"),
        None => match std::env::var("HOME").ok().filter(|h| !h.is_empty()) {
            Some(home) => PathBuf::from(home).join(".unpeel").join("todos.json"),
            None => PathBuf::from("unpeel-todos.json"),
        },
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl Store {
    /// Load from `path`; a missing file is an empty list, an unreadable one
    /// is an error (never silently overwrite a file we couldn't parse).
    pub fn load(path: &Path) -> Result<Store, String> {
        let todos = match std::fs::read_to_string(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(format!("read {}: {e}", path.display())),
            Ok(raw) => {
                let json: serde_json::Value = serde_json::from_str(&raw)
                    .map_err(|e| format!("parse {}: {e}", path.display()))?;
                json.get("todos")
                    .and_then(|t| t.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| {
                                Some(Todo {
                                    id: item.get("id")?.as_u64()?,
                                    text: item.get("text")?.as_str()?.to_string(),
                                    done: item
                                        .get("done")
                                        .and_then(|d| d.as_bool())
                                        .unwrap_or(false),
                                    created_at: item
                                        .get("created_at")
                                        .and_then(|c| c.as_u64())
                                        .unwrap_or(0),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            }
        };
        Ok(Store {
            path: path.to_path_buf(),
            todos,
        })
    }

    pub fn save(&self) -> Result<(), String> {
        let items: Vec<serde_json::Value> = self
            .todos
            .iter()
            .map(|todo| {
                serde_json::json!({
                    "id": todo.id,
                    "text": todo.text,
                    "done": todo.done,
                    "created_at": todo.created_at,
                })
            })
            .collect();
        let body = serde_json::json!({ "todos": items });
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(&body).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    pub fn add(&mut self, text: &str) -> u64 {
        let id = self.todos.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        self.todos.push(Todo {
            id,
            text: text.to_string(),
            done: false,
            created_at: now_ms(),
        });
        id
    }

    pub fn open_count(&self) -> usize {
        self.todos.iter().filter(|t| !t.done).count()
    }

    pub fn done_count(&self) -> usize {
        self.todos.iter().filter(|t| t.done).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "unpeel-todos-{tag}-{}-{}.json",
            std::process::id(),
            now_ms()
        ))
    }

    #[test]
    fn missing_file_is_empty_and_roundtrips() {
        let path = temp_file("roundtrip");
        let mut store = Store::load(&path).unwrap();
        assert!(store.todos.is_empty());
        store.add("write the plan");
        let id = store.add("ship it");
        store.todos.iter_mut().find(|t| t.id == id).unwrap().done = true;
        store.save().unwrap();

        let reloaded = Store::load(&path).unwrap();
        assert_eq!(reloaded.todos, store.todos);
        assert_eq!(reloaded.open_count(), 1);
        assert_eq!(reloaded.done_count(), 1);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn corrupt_file_errors_instead_of_clobbering() {
        let path = temp_file("corrupt");
        std::fs::write(&path, "not json").unwrap();
        assert!(Store::load(&path).is_err());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn unknown_keys_survive_being_ignored() {
        let path = temp_file("compat");
        std::fs::write(
            &path,
            r#"{"todos":[{"id":1,"text":"a","done":false,"future_key":true}],"version":9}"#,
        )
        .unwrap();
        let store = Store::load(&path).unwrap();
        assert_eq!(store.todos.len(), 1);
        std::fs::remove_file(&path).ok();
    }
}
