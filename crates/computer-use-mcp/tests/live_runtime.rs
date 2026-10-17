//! Against the ChatGPT.app runtime installed on this Mac:
//! `cargo test -p zeron-computer-use-mcp --test live_runtime -- --ignored`.

use std::io::Write;
use std::sync::mpsc::{Sender, channel};
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

#[test]
#[ignore = "needs ChatGPT.app with the unified-computer-use plugin"]
fn installed_runtime_lists_tools_and_reads_state() {
    let dirs = tempfile::tempdir().unwrap();
    let config = Config {
        data_dir: Some(dirs.path().join("data")),
        state_root: dirs.path().join("state"),
        session_key: "live-test".into(),
        ..Config::from_env(false)
    };
    let (reader, mut input) = std::io::pipe().unwrap();
    let (tx, output) = channel();
    let server = Server::new(config, Lines(tx, Vec::new()));
    server.events.spawn_client(reader);
    std::thread::spawn(move || server.run());
    let mut request = |id: u64, method: &str, params: Value| {
        writeln!(
            input,
            "{}",
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
        )
        .unwrap();
        output.recv_timeout(Duration::from_secs(150)).unwrap()
    };
    let tools = request(1, "tools/list", json!({}));
    assert_eq!(
        tools["result"]["tools"].as_array().unwrap().len(),
        2,
        "{tools}"
    );
    let state = request(
        2,
        "tools/call",
        json!({ "name": "computer_use", "arguments": {
            "code": "nodeRepl.write(JSON.stringify(await cua.getState()))",
            "timeout_ms": 60000
        }}),
    );
    let result = &state["result"];
    assert_eq!(result["isError"], false, "{state}");
    let text = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    eprintln!("{text}");
    assert!(
        text.contains("Finder") || text.contains("com.apple.finder"),
        "{text}"
    );
}
