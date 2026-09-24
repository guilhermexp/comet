//! `comet-sessions` stdio MCP: `help`, `list_spaces`, and `create`.
//!
//! The engine stamps the endpoint and identity. This process checks that
//! identity before every action and drops its authority env before serving.

use std::io::{BufRead, Write};
use std::time::Duration;

use serde_json::{Value, json};
use zeron_proto::{EngineInfo, Space, SpawnChatResult};

pub const SESSIONS_MCP_ARG: &str = "__sessions_mcp__";
const MARKER_ENV: &str = "COMET_SESSIONS_CONTROLLER";
const PARENT_ENV: &str = "COMET_SESSIONS_PARENT_CHAT_ID";
const ENDPOINT_ENV: &str = "COMET_SESSIONS_ENDPOINT";
const ENGINE_ENV: &str = "COMET_SESSIONS_ENGINE_ID";
const TOOL_NAME: &str = "sessions";

const OVERRIDE_FIELDS: &[&str] = &[
    "harness",
    "model",
    "reasoning",
    "sandbox",
    "model_options",
    "modelOptions",
];

/// Values the engine put in the environment, already removed from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionsAuthority {
    pub parent_chat_id: String,
    pub endpoint: String,
    pub engine_id: String,
}

pub trait EngineApi {
    fn info(&self) -> Result<EngineInfo, String>;
    fn spaces(&self) -> Result<Vec<Space>, String>;
    fn parent_space_id(&self, parent_chat_id: &str) -> Result<Option<String>, String>;
    fn spawn(
        &self,
        parent_chat_id: &str,
        prompt: &str,
        space_id: Option<&str>,
    ) -> Result<SpawnChatResult, String>;
}

pub fn run_stdio() -> Result<(), String> {
    let authority = consume_authority()?;
    let engine = RpcEngine::connect(&authority.endpoint)?;
    let stdin = std::io::stdin();
    serve(stdin.lock(), std::io::stdout(), |request| {
        handle_request(request, &engine, &authority)
    })
}

/// Require the Comet marker, then remove the marker, parent, endpoint, and
/// engine id so descendants cannot inherit them.
pub fn consume_authority() -> Result<SessionsAuthority, String> {
    if std::env::var(MARKER_ENV).ok().as_deref() != Some("1") {
        return Err(format!(
            "{SESSIONS_MCP_ARG} is reserved for Comet orchestrator sessions"
        ));
    }
    let parent_chat_id = required_env(PARENT_ENV)?;
    let endpoint = required_env(ENDPOINT_ENV)?;
    let engine_id = required_env(ENGINE_ENV)?;
    // SAFETY: startup, before any child process or extra thread.
    unsafe {
        std::env::remove_var(MARKER_ENV);
        std::env::remove_var(PARENT_ENV);
        std::env::remove_var(ENDPOINT_ENV);
        std::env::remove_var(ENGINE_ENV);
    }
    Ok(SessionsAuthority {
        parent_chat_id,
        endpoint,
        engine_id,
    })
}

fn required_env(name: &str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

pub fn serve<R, W, H>(reader: R, mut writer: W, mut handler: H) -> Result<(), String>
where
    R: BufRead,
    W: Write,
    H: FnMut(Value) -> Option<Value>,
{
    for line in reader.lines() {
        let line = line.map_err(|error| error.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let request = match serde_json::from_str::<Value>(&line) {
            Ok(request) => request,
            Err(_) => {
                write_message(
                    &mut writer,
                    &error_response(Value::Null, -32700, "Parse error"),
                )?;
                continue;
            }
        };
        if let Some(response) = handler(request) {
            write_message(&mut writer, &response)?;
        }
    }
    Ok(())
}

pub fn handle_request(
    request: Value,
    engine: &dyn EngineApi,
    authority: &SessionsAuthority,
) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str);
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || method.is_none() {
        return id.map(|id| error_response(id, -32600, "Invalid Request"));
    }
    let method = method.expect("checked");
    let id = id?;
    Some(match method {
        "initialize" => {
            let protocol_version = request
                .pointer("/params/protocolVersion")
                .cloned()
                .unwrap_or_else(|| json!("2024-11-05"));
            result_response(
                id,
                json!({
                    "protocolVersion": protocol_version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "comet-sessions", "version": env!("CARGO_PKG_VERSION") }
                }),
            )
        }
        "ping" => result_response(id, json!({})),
        "tools/list" => result_response(id, json!({ "tools": [tool_definition()] })),
        "tools/call" => match call_tool(&request, engine, authority) {
            Ok(value) => tool_success(id, value),
            Err(error) => tool_error(id, &error),
        },
        _ => error_response(id, -32601, "Method not found"),
    })
}

fn call_tool(
    request: &Value,
    engine: &dyn EngineApi,
    authority: &SessionsAuthority,
) -> Result<Value, String> {
    let name = request.pointer("/params/name").and_then(Value::as_str);
    if name != Some(TOOL_NAME) {
        return Err("Unknown tool. Use 'sessions'.".into());
    }
    let info = engine.info()?;
    if info.device_id != authority.engine_id {
        return Err(format!(
            "engine identity mismatch: endpoint is {}, expected {}",
            info.device_id, authority.engine_id
        ));
    }
    let arguments = request
        .pointer("/params/arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    dispatch_action(engine, authority, &arguments)
}

pub fn dispatch_action(
    engine: &dyn EngineApi,
    authority: &SessionsAuthority,
    arguments: &Value,
) -> Result<Value, String> {
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    match action {
        "help" => Ok(json!({
            "tool": TOOL_NAME,
            "actions": ["help", "list_spaces", "create"],
            "create": "create(prompt, space_id?) opens a native chat in the parent's space (or the given space) and immediately runs prompt with the parent's current harness, model, reasoning, and sandbox. There is no model override. The child does not receive this tool.",
        })),
        "list_spaces" => {
            let current = engine.parent_space_id(&authority.parent_chat_id)?;
            let spaces = engine.spaces()?;
            Ok(json!({
                "spaces": spaces.into_iter().map(|space| {
                    json!({
                        "id": space.id,
                        "name": space.name.clone().unwrap_or_else(|| display_name(&space.path)),
                        "path": space.path,
                        "current": current.as_deref() == Some(space.id.as_str()),
                    })
                }).collect::<Vec<_>>()
            }))
        }
        "create" => {
            if arguments.as_object().is_some_and(|object| {
                OVERRIDE_FIELDS
                    .iter()
                    .any(|field| object.contains_key(*field))
            }) {
                return Err(
                    "create does not accept harness, model, reasoning, or sandbox overrides".into(),
                );
            }
            let prompt = arguments
                .get("prompt")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|prompt| !prompt.is_empty())
                .ok_or_else(|| "prompt is required".to_owned())?;
            let space_id = arguments
                .get("space_id")
                .or_else(|| arguments.get("spaceId"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            let created = engine.spawn(&authority.parent_chat_id, prompt, space_id.as_deref())?;
            Ok(json!({
                "chatId": created.chat_id,
                "spaceId": created.space_id,
                "deviceId": created.device_id,
            }))
        }
        _ => Err("Unknown action. Use help, list_spaces, or create.".into()),
    }
}

fn display_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_owned()
}

fn tool_definition() -> Value {
    json!({
        "name": TOOL_NAME,
        "description": "Open a new native Comet chat that runs a prompt with this chat's current model configuration. create starts that run immediately. The new chat cannot open further chats.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["help", "list_spaces", "create"] },
                "prompt": { "type": "string" },
                "space_id": { "type": "string" }
            },
            "required": ["action"],
            "additionalProperties": false
        }
    })
}

fn write_message(writer: &mut impl Write, message: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, message).map_err(|error| error.to_string())?;
    writer.write_all(b"\n").map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())
}

fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tool_error(id: Value, message: &str) -> Value {
    result_response(
        id,
        json!({
            "content": [{ "type": "text", "text": message }],
            "isError": true
        }),
    )
}

fn tool_success(id: Value, value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    result_response(
        id,
        json!({
            "content": [{ "type": "text", "text": text }],
            "structuredContent": value,
            "isError": false
        }),
    )
}

struct RpcEngine {
    runtime: tokio::runtime::Runtime,
    client: zeron_rpc::RpcClient,
}

impl RpcEngine {
    fn connect(endpoint: &str) -> Result<Self, String> {
        let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
        let client = runtime
            .block_on(zeron_rpc::connect_ws(endpoint))
            .map_err(|error| error.to_string())?;
        Ok(Self { runtime, client })
    }

    fn first_item(&self, method: &str) -> Result<Value, String> {
        self.runtime.block_on(async {
            let mut subscription = self
                .client
                .subscribe(method, json!({}))
                .await
                .map_err(|error| error.to_string())?;
            tokio::time::timeout(Duration::from_secs(5), subscription.recv())
                .await
                .map_err(|_| format!("{method} timed out"))?
                .ok_or_else(|| format!("{method} closed"))
        })
    }
}

impl EngineApi for RpcEngine {
    fn info(&self) -> Result<EngineInfo, String> {
        self.runtime.block_on(async {
            self.client
                .call_as(zeron_rpc::methods::ENGINE_INFO, json!({}))
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn spaces(&self) -> Result<Vec<Space>, String> {
        let value = self.first_item(zeron_rpc::methods::WATCH_SPACES)?;
        serde_json::from_value(value).map_err(|error| error.to_string())
    }

    fn parent_space_id(&self, parent_chat_id: &str) -> Result<Option<String>, String> {
        let value = self.first_item(zeron_rpc::methods::WATCH_CHATS)?;
        let chats: Vec<zeron_proto::Chat> =
            serde_json::from_value(value).map_err(|error| error.to_string())?;
        chats
            .into_iter()
            .find(|chat| chat.id == parent_chat_id)
            .map(|chat| Ok(chat.space_id))
            .ok_or_else(|| format!("parent chat {parent_chat_id} is not on this engine"))?
    }

    fn spawn(
        &self,
        parent_chat_id: &str,
        prompt: &str,
        space_id: Option<&str>,
    ) -> Result<SpawnChatResult, String> {
        let mut params = json!({
            "parentChatId": parent_chat_id,
            "prompt": prompt,
        });
        if let Some(space_id) = space_id {
            params["spaceId"] = json!(space_id);
        }
        self.runtime.block_on(async {
            self.client
                .call_as(zeron_rpc::methods::SPAWN_CHAT, params)
                .await
                .map_err(|error| error.to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use zeron_proto::WorkspaceScope;

    struct Fake {
        engine_id: String,
        spaces: Vec<Space>,
        parent_space: Option<String>,
        spawns: Mutex<Vec<(String, Option<String>)>>,
    }

    impl EngineApi for Fake {
        fn info(&self) -> Result<EngineInfo, String> {
            Ok(EngineInfo {
                device_id: self.engine_id.clone(),
                workspace_scope: WorkspaceScope::Local,
                cursor_sdk_version: None,
                capabilities: Vec::new(),
            })
        }
        fn spaces(&self) -> Result<Vec<Space>, String> {
            Ok(self.spaces.clone())
        }
        fn parent_space_id(&self, _: &str) -> Result<Option<String>, String> {
            Ok(self.parent_space.clone())
        }
        fn spawn(
            &self,
            _: &str,
            prompt: &str,
            space_id: Option<&str>,
        ) -> Result<SpawnChatResult, String> {
            self.spawns
                .lock()
                .unwrap()
                .push((prompt.to_owned(), space_id.map(str::to_owned)));
            Ok(SpawnChatResult {
                chat_id: "child".into(),
                space_id: space_id.map(str::to_owned),
                device_id: self.engine_id.clone(),
            })
        }
    }

    fn space(id: &str, name: Option<&str>, path: &str) -> Space {
        let mut value = json!({
            "id": id,
            "deviceId": "engine-1",
            "path": path,
            "gitDetected": false,
            "createdAt": "2026-09-24T00:00:00Z"
        });
        if let Some(name) = name {
            value["name"] = json!(name);
        }
        serde_json::from_value(value).unwrap()
    }

    fn authority() -> SessionsAuthority {
        SessionsAuthority {
            parent_chat_id: "parent".into(),
            endpoint: "ws://127.0.0.1:9".into(),
            engine_id: "engine-1".into(),
        }
    }

    fn fake() -> Fake {
        Fake {
            engine_id: "engine-1".into(),
            spaces: vec![
                space("space-parent", Some("Parent"), "/tmp/parent"),
                space("space-other", None, "/tmp/other"),
            ],
            parent_space: Some("space-parent".into()),
            spawns: Mutex::new(Vec::new()),
        }
    }

    fn call(engine: &Fake, arguments: Value) -> Value {
        handle_request(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": "sessions", "arguments": arguments }
            }),
            engine,
            &authority(),
        )
        .unwrap()
    }

    #[test]
    fn wrong_engine_identity_creates_nothing() {
        let mut engine = fake();
        engine.engine_id = "other-engine".into();
        let response = call(&engine, json!({"action": "create", "prompt": "hi"}));
        assert_eq!(response["result"]["isError"], true);
        assert!(engine.spawns.lock().unwrap().is_empty());
        let listed = call(&engine, json!({"action": "list_spaces"}));
        assert_eq!(listed["result"]["isError"], true);
    }

    #[test]
    fn blank_prompt_and_overrides_are_rejected() {
        let engine = fake();
        for arguments in [
            json!({"action": "create"}),
            json!({"action": "create", "prompt": "   "}),
            json!({"action": "create", "prompt": "hi", "model": "other"}),
            json!({"action": "create", "prompt": "hi", "harness": "claude"}),
            json!({"action": "create", "prompt": "hi", "reasoning": "high"}),
        ] {
            let response = call(&engine, arguments);
            assert_eq!(response["result"]["isError"], true, "{response}");
        }
        assert!(engine.spawns.lock().unwrap().is_empty());
    }

    #[test]
    fn list_spaces_marks_the_parent_space() {
        let engine = fake();
        let response = call(&engine, json!({"action": "list_spaces"}));
        assert_eq!(response["result"]["isError"], false);
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        let listed: Value = serde_json::from_str(text).unwrap();
        let spaces = listed["spaces"].as_array().unwrap();
        assert_eq!(spaces.len(), 2);
        assert_eq!(spaces[0]["current"], true);
        assert_eq!(spaces[0]["name"], "Parent");
        assert_eq!(spaces[1]["name"], "other");
        assert_eq!(spaces[1]["path"], "/tmp/other");
        assert_eq!(spaces[1]["current"], false);
    }

    #[test]
    fn startup_scrubs_authority_env() {
        static LOCK: Mutex<()> = Mutex::new(());
        let _guard = LOCK.lock().unwrap();
        // SAFETY: this test serializes env access with `LOCK`.
        unsafe { std::env::remove_var(MARKER_ENV) };
        let refused = consume_authority();
        assert!(refused.is_err());
        unsafe {
            std::env::set_var(MARKER_ENV, "1");
            std::env::set_var(PARENT_ENV, "parent");
            std::env::set_var(ENDPOINT_ENV, "ws://127.0.0.1:9");
            std::env::set_var(ENGINE_ENV, "engine-1");
        }
        let authority = consume_authority().unwrap();
        assert_eq!(authority.parent_chat_id, "parent");
        assert!(std::env::var(MARKER_ENV).is_err());
        assert!(std::env::var(PARENT_ENV).is_err());
        assert!(std::env::var(ENDPOINT_ENV).is_err());
        assert!(std::env::var(ENGINE_ENV).is_err());
    }
}
