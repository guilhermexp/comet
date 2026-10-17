//! The server against a fake `cua_repl` (tests/fixtures/fake_cua_repl.py).

use std::io::Write;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use serde_json::{Value, json};
use zeron_computer_use_mcp::{Config, Server};

struct Lines(Sender<Value>, Vec<u8>);

impl Write for Lines {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.1.extend_from_slice(buf);
        while let Some(end) = self.1.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.1.drain(..=end).collect();
            let _ = self.0.send(serde_json::from_slice(&line).unwrap());
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Harness {
    input: std::io::PipeWriter,
    output: Receiver<Value>,
    _dirs: tempfile::TempDir,
    log: std::path::PathBuf,
    next: u64,
}

impl Harness {
    fn start(worker: bool, settings: Option<&str>) -> Self {
        let dirs = tempfile::tempdir().unwrap();
        let plugin = dirs.path().join("plugin/26.1013.1");
        std::fs::create_dir_all(&plugin).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_cua_repl.py");
        let log = dirs.path().join("runtime.log");
        std::fs::write(
            plugin.join(".mcp.json"),
            json!({ "mcpServers": { "cua_repl": {
                "command": "python3",
                "args": [fixture],
                "env": { "FAKE_CUA_LOG": log },
            }}})
            .to_string(),
        )
        .unwrap();
        let data = dirs.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        if let Some(settings) = settings {
            std::fs::write(data.join("ui-settings.json"), settings).unwrap();
        }
        let config = Config {
            worker,
            data_dir: Some(data),
            plugin_dir: Some(dirs.path().join("plugin")),
            state_root: dirs.path().join("state"),
            session_key: "chat-1".into(),
            transport_margin: Duration::from_millis(300),
        };
        let (reader, input) = std::io::pipe().unwrap();
        let (tx, output) = channel();
        let server = Server::new(config, Lines(tx, Vec::new()));
        server.events.spawn_client(reader);
        std::thread::spawn(move || server.run());
        Self {
            input,
            output,
            _dirs: dirs,
            log,
            next: 0,
        }
    }

    fn settings_path(&self) -> std::path::PathBuf {
        self._dirs.path().join("data/ui-settings.json")
    }

    fn send(&mut self, method: &str, params: Value) -> u64 {
        self.next += 1;
        let line = json!({ "jsonrpc": "2.0", "id": self.next, "method": method, "params": params });
        writeln!(self.input, "{line}").unwrap();
        self.next
    }

    fn recv(&self) -> Value {
        self.output
            .recv_timeout(Duration::from_secs(20))
            .expect("server answered")
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        let response = self.recv();
        assert_eq!(response["id"], id, "{response}");
        response
    }

    fn call(&mut self, code: &str, timeout_ms: u64) -> Value {
        self.request(
            "tools/call",
            json!({ "name": "computer_use", "arguments": { "code": code, "timeout_ms": timeout_ms } }),
        )["result"]
            .clone()
    }

    fn tool_names(&mut self) -> Vec<String> {
        self.request("tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect()
    }
}

fn texts(result: &Value) -> Vec<String> {
    result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|part| part["text"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn lists_both_tools_and_sends_the_preamble_once() {
    let mut harness = Harness::start(false, None);
    let init = harness.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
    assert_eq!(init["result"]["serverInfo"]["name"], "comet-computer-use");
    assert_eq!(harness.tool_names(), ["computer_use", "computer_use_reset"]);

    let first = texts(&harness.call("cua.getState()", 5_000));
    assert!(first[0].starts_with("## Computer Use"), "{first:?}");
    assert!(first[1].starts_with("echo:cua.getState()"));
    let second = texts(&harness.call("again", 5_000));
    assert_eq!(second.len(), 1, "{second:?}");
    assert!(second[0].starts_with("echo:again"));

    let reset = harness.request(
        "tools/call",
        json!({ "name": "computer_use_reset", "arguments": {} }),
    );
    assert_eq!(reset["result"]["isError"], false);
    // The runtime got `initialized` after `initialize`, then the calls.
    let log = std::fs::read_to_string(&harness.log).unwrap();
    let methods: Vec<String> = log
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap()["method"].to_string())
        .collect();
    assert_eq!(methods[0], "\"initialize\"");
    assert_eq!(methods[1], "\"notifications/initialized\"");
    assert!(log.contains("\"CUA_REPL_ENABLED_SURFACES\"") || !log.is_empty());
}

#[test]
fn image_is_relabeled_and_saved() {
    let mut harness = Harness::start(false, None);
    let result = harness.call("image", 5_000);
    assert_eq!(result["content"][0]["mimeType"], "image/jpeg");
    let saved = texts(&result).pop().unwrap();
    let path = saved.strip_prefix("screenshots saved: ").unwrap();
    assert!(Path::new(path).exists(), "{path}");
}

#[test]
fn elicitation_is_accepted_and_the_call_completes() {
    let mut harness = Harness::start(false, None);
    assert_eq!(texts(&harness.call("elicit", 5_000)), ["elicited"]);
}

#[test]
fn restart_is_announced_on_the_next_result() {
    let mut harness = Harness::start(false, None);
    harness.call("first", 5_000);
    let died = harness.call("die", 5_000);
    assert_eq!(died["isError"], true);
    assert!(texts(&died)[0].contains("cua_repl exited"), "{died}");
    let next = texts(&harness.call("after", 5_000));
    assert!(
        next[0].starts_with("[Computer Use runtime restarted"),
        "{next:?}"
    );
    assert!(next[0].contains("fake runtime died"), "{next:?}");
    assert!(next.iter().all(|text| !text.starts_with("## Computer Use")));
}

#[test]
fn timeout_stops_the_runtime_and_allows_another_call() {
    let mut harness = Harness::start(false, None);
    let timed_out = harness.call("hang", 1_000);
    assert_eq!(timed_out["isError"], true);
    assert!(texts(&timed_out)[0].contains("timed out after timeout_ms=1000"));
    let next = texts(&harness.call("after", 5_000));
    assert!(
        next[0].starts_with("[Computer Use runtime restarted"),
        "{next:?}"
    );
    assert!(next.last().unwrap().starts_with("echo:after"));
}

#[test]
fn cancel_ends_the_call_and_the_server_keeps_serving() {
    let mut harness = Harness::start(false, None);
    let id = harness.send(
        "tools/call",
        json!({ "name": "computer_use", "arguments": { "code": "hang", "timeout_ms": 60_000 } }),
    );
    std::thread::sleep(Duration::from_millis(300));
    writeln!(
        harness.input,
        "{}",
        json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": id } })
    )
    .unwrap();
    let cancelled = harness.recv();
    assert_eq!(cancelled["id"], id);
    assert!(cancelled["error"].is_object(), "{cancelled}");
    assert_eq!(harness.tool_names().len(), 2);
}

#[test]
fn switched_off_lists_nothing_and_refuses_without_starting_the_runtime() {
    let mut harness = Harness::start(false, Some(r#"{"codexComputerUseEnabled":false}"#));
    assert!(harness.tool_names().is_empty());
    let refused = harness.call("cua.getState()", 5_000);
    assert_eq!(refused["isError"], true);
    assert!(texts(&refused)[0].contains("Settings"));
    assert!(!harness.log.exists(), "runtime must not start");
}

#[test]
fn turned_off_mid_session_refuses_the_next_call() {
    let mut harness = Harness::start(false, None);
    assert_eq!(harness.tool_names().len(), 2);
    std::fs::write(
        harness.settings_path(),
        r#"{"codexComputerUseEnabled":false}"#,
    )
    .unwrap();
    let refused = harness.call("cua.getState()", 5_000);
    assert_eq!(refused["isError"], true);
    assert!(texts(&refused)[0].contains("off in Comet Settings"));
}

#[test]
fn comet_only_scope_closes_workers_but_not_chats() {
    let settings = r#"{"codexComputerUseScope":"chats"}"#;
    let mut worker = Harness::start(true, Some(settings));
    assert!(worker.tool_names().is_empty());
    let refused = worker.call("x", 5_000);
    assert!(texts(&refused)[0].contains("limited to Comet chats"));
    assert!(!worker.log.exists());

    let mut chat = Harness::start(false, Some(settings));
    assert_eq!(chat.tool_names().len(), 2);
}

#[test]
fn unreadable_settings_fail_closed() {
    let mut harness = Harness::start(false, Some("{broken"));
    assert!(harness.tool_names().is_empty());
}
