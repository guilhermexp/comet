#![cfg(unix)]

use serde_json::{json, Value};
use std::fs;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::symlink;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn temp_home(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // Keep the Unix socket below macOS's short sockaddr_un limit.
    PathBuf::from("/tmp").join(format!("u-ar-{label}-{}-{nonce:x}", std::process::id()))
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    condition()
}

fn write_launch(home: &Path, session_id: &str, command: &str) -> PathBuf {
    let session_dir = home.join("app-sessions").join(session_id);
    fs::create_dir_all(&session_dir).unwrap();
    let path = session_dir.join("launch.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "session": {
                "id": session_id,
                "project_id": "project",
                "label": if command.is_empty() { "Terminal" } else { command },
                "custom_title": false,
                "command": command,
                "created_at": 1
            },
            "cwd": "/tmp",
            "dark_mode": true,
            "hook_port": null,
            "mcp_enabled": true,
            "browser_mcp_enabled": true,
            "computer_mcp_enabled": true,
            "initial_cols": 80,
            "initial_rows": 24
        }))
        .unwrap(),
    )
    .unwrap();
    path
}

fn spawn_host(home: &Path, launch: &Path, path: Option<&str>) -> Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_unpeel-host"));
    command
        .arg("__session_host__")
        .arg(launch)
        .env("UNPEEL_HOME", home)
        .env("HOME", home)
        .env("SHELL", "/bin/bash")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command.spawn().unwrap()
}

fn socket_command(home: &Path, session_id: &str, value: Value) -> Value {
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    let mut stream = UnixStream::connect(socket).unwrap();
    stream
        .write_all(format!("{}\n", serde_json::to_string(&value).unwrap()).as_bytes())
        .unwrap();
    let mut response = String::new();
    BufReader::new(stream).read_line(&mut response).unwrap();
    serde_json::from_str(response.trim()).unwrap()
}

fn manifest(home: &Path, session_id: &str) -> Value {
    serde_json::from_slice(
        &fs::read(
            home.join("app-sessions")
                .join(session_id)
                .join("manifest.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

fn stop_and_reap(home: &Path, session_id: &str, child: &mut Child) {
    if child.try_wait().unwrap().is_none() {
        let _ = socket_command(home, session_id, json!({ "type": "kill" }));
        let _ = wait_until(Duration::from_secs(4), || {
            child.try_wait().ok().flatten().is_some()
        });
    }
    if child.try_wait().unwrap().is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn recorded_pids(path: &Path) -> Vec<u32> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

fn write_executable(path: &Path, body: impl AsRef<[u8]>) {
    fs::write(path, body).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn initial_runtime_preparation_persists_host_minted_identity_before_ready() {
    let home = temp_home("initial-prep");
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let fake_claude = bin.join("claude");
    fs::write(
        &fake_claude,
        "#!/bin/bash\nprintf 'fixture claude launch\\n'\nexec -a claude /bin/sleep 300\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_claude).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_claude, permissions).unwrap();
    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let test_path = format!("{}:{inherited_path}", bin.display());
    fs::write(
        home.join(".bash_profile"),
        format!("export PATH='{}'\n", test_path.replace('\'', "'\\''")),
    )
    .unwrap();

    let session_id = "prepared-session";
    let launch = write_launch(&home, session_id, "claude --model fixture");
    let mut host = spawn_host(&home, &launch, Some(&test_path));
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    assert!(wait_until(Duration::from_secs(10), || socket.exists()));

    let ready = manifest(&home, session_id);
    let command = ready["session"]["command"].as_str().unwrap();
    let provider_id = ready["provider_session_id"].as_str().unwrap();
    assert_eq!(
        command,
        format!("claude --model fixture --session-id '{provider_id}'")
    );
    assert!(ready.get("managed_storage_path").is_none());
    let marker: Value = serde_json::from_slice(
        &fs::read(
            home.join("app-sessions")
                .join(session_id)
                .join("provider-session.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(marker["provider_session_id"], provider_id);

    // Once the minted conversation exists, the same Host-owned plan switches
    // from creation to exact resume and publishes its verified failure text
    // as opaque markers. Native never carries these provider strings.
    let transcript_dir = home.join(".claude").join("projects").join("fixture");
    fs::create_dir_all(&transcript_dir).unwrap();
    fs::write(transcript_dir.join(format!("{provider_id}.jsonl")), b"\n").unwrap();
    assert_eq!(
        socket_command(&home, session_id, json!({ "type": "write", "data": "x" }))["ok"],
        true
    );
    let plan_output = Command::new(env!("CARGO_BIN_EXE_unpeel-host"))
        .arg("__resume__")
        .arg(session_id)
        .env("UNPEEL_HOME", &home)
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(
        plan_output.status.success(),
        "resume plan stderr: {}",
        String::from_utf8_lossy(&plan_output.stderr)
    );
    let plan: Value = serde_json::from_slice(&plan_output.stdout).unwrap();
    assert_eq!(
        plan["command"],
        format!("claude --model fixture --resume '{provider_id}'")
    );
    assert_eq!(
        plan["failure_markers"],
        json!(["No conversation found with session ID", provider_id])
    );

    stop_and_reap(&home, session_id, &mut host);
    let _ = fs::remove_dir_all(home);
}

fn process_alive(pid: u32) -> bool {
    Command::new("/bin/kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[test]
fn resume_agent_keeps_host_identity_and_relaunches_exactly_from_owned_shell() {
    let home = temp_home("live");
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let fake_pi = bin.join("pi");
    let generations = home.join("runtime-generations");
    // Keep a native foreground process with argv[0] `pi`, but deliberately
    // ignore every argument so the second launch tolerates Pi's --continue
    // and system-context flags and remains observable after restart.
    fs::write(
        &fake_pi,
        format!(
            "#!/bin/bash\nprintf '%s\\n' \"${{UNPEEL_RUNTIME_GENERATION:-unset}}\" >> '{}'\nprintf 'fixture runtime launch\\n'\nif [ \"${{UNPEEL_RUNTIME_GENERATION:-unset}}\" = 1 ]; then exec -a pi /bin/sleep 2; fi\nexec -a pi /bin/sleep 300\n",
            generations.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_pi).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_pi, permissions).unwrap();
    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let test_path = format!("{}:{inherited_path}", bin.display());
    fs::write(
        home.join(".bash_profile"),
        format!("export PATH='{}'\n", test_path.replace('\'', "'\\''")),
    )
    .unwrap();

    let session_id = "same-session";
    let launch = write_launch(&home, session_id, "pi 300");
    let mut host = spawn_host(&home, &launch, Some(&test_path));
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    if !wait_until(Duration::from_secs(5), || socket.exists()) {
        let _ = host.kill();
        let status = host.wait().ok();
        let mut stderr = String::new();
        if let Some(pipe) = host.stderr.as_mut() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        panic!("Host did not bind socket (status {status:?}): {stderr}");
    }
    assert!(wait_until(Duration::from_secs(5), || {
        manifest(&home, session_id)["runtime"]["currentObservation"]["id"] == "pi"
    }));
    assert!(wait_until(Duration::from_secs(2), || {
        fs::read_to_string(&generations).is_ok_and(|value| value.lines().eq(["1"]))
    }));

    // Make restart choose the resume path, and seed the exact stale activity
    // record that must not survive into the new process generation.
    assert_eq!(
        socket_command(&home, session_id, json!({ "type": "write", "data": "x" }))["ok"],
        true
    );
    let session_dir = home.join("app-sessions").join(session_id);
    fs::write(
        session_dir.join("last-hook-event.json"),
        br#"{"event":"Stop"}"#,
    )
    .unwrap();
    fs::write(
        session_dir.join("appended-context.json"),
        br#"{"context":"remember this","updated_at":1}"#,
    )
    .unwrap();
    let before = manifest(&home, session_id);
    let managed_storage = home.join("pi-sessions").join(session_id);
    assert_eq!(
        before["session"]["command"],
        format!(
            "pi 300 --session-dir '{}'",
            managed_storage.to_string_lossy()
        )
    );
    assert_eq!(
        before["managed_storage_path"],
        managed_storage.to_string_lossy().as_ref()
    );
    assert!(managed_storage.is_dir());
    assert_eq!(before["mcp_enabled"], true);
    assert_eq!(before["browser_mcp_enabled"], true);
    assert_eq!(before["computer_mcp_enabled"], true);
    assert_eq!(before["mcp_client_registered"], false);
    assert_eq!(before["browser_client_registered"], false);
    assert_eq!(before["computer_client_registered"], false);
    let before_pid = before["pid"].as_u64().unwrap();
    let before_runtime_pid = before["runtime"]["currentObservation"]["pid"]
        .as_u64()
        .unwrap();

    assert!(wait_until(Duration::from_secs(5), || {
        manifest(&home, session_id)["runtime"].is_null()
    }));

    let output = Command::new(env!("CARGO_BIN_EXE_unpeel-host"))
        .arg("__resume_agent__")
        .arg(session_id)
        .env("UNPEEL_HOME", &home)
        .env("PATH", &test_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "resume stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(wait_until(Duration::from_secs(5), || {
        let current = manifest(&home, session_id);
        current["runtime_launch_generation"] == 2
            && current["runtime"]["currentObservation"]["id"] == "pi"
            && current["runtime"]["currentObservation"]["pid"].as_u64() != Some(before_runtime_pid)
    }));
    assert!(wait_until(Duration::from_secs(2), || {
        fs::read_to_string(&generations).is_ok_and(|value| value.lines().eq(["1", "2"]))
    }));

    let after = manifest(&home, session_id);
    assert_eq!(after["session"]["id"], session_id);
    assert_eq!(after["pid"].as_u64(), Some(before_pid));
    assert_eq!(after["state"], "running");
    assert_eq!(
        after["session"]["command"],
        format!(
            "pi 300 --session-dir '{}' --continue --append-system-prompt 'remember this'",
            managed_storage.to_string_lossy()
        )
    );
    assert_eq!(after["runtime_launch_generation"], 2);
    assert_eq!(after["mcp_client_registered"], false);
    assert_eq!(after["browser_client_registered"], false);
    assert_eq!(after["computer_client_registered"], false);
    assert!(after["runtime_launched_at"].as_u64().is_some());
    let launch_offset = after["runtime_launch_output_offset"].as_u64().unwrap();
    assert!(
        launch_offset > 0,
        "in-place generation keeps an output boundary"
    );
    assert!(wait_until(Duration::from_secs(2), || {
        fs::read(session_dir.join("output.bin")).is_ok_and(|output| {
            launch_offset <= output.len() as u64
                && String::from_utf8_lossy(&output[launch_offset as usize..])
                    .contains("fixture runtime launch")
        })
    }));
    assert!(!session_dir.join("last-hook-event.json").exists());
    assert!(!session_dir.join("appended-context.json").exists());
    assert!(socket.exists(), "same control socket remains live");

    // A request prepared concurrently from the preceding generation cannot
    // queue a second resume command after the first request releases its Host
    // mutex. The generation compare-and-swap rejects it before any signal.
    let current_runtime_pid = after["runtime"]["currentObservation"]["pid"]
        .as_u64()
        .unwrap();
    let stale = socket_command(
        &home,
        session_id,
        json!({ "type": "resume_agent", "expected_generation": 1 }),
    );
    assert_eq!(stale["ok"], false);
    assert!(stale["error"]
        .as_str()
        .is_some_and(|error| error.contains("generation changed")));
    std::thread::sleep(Duration::from_millis(150));
    let unchanged = manifest(&home, session_id);
    assert_eq!(unchanged["runtime_launch_generation"], 2);
    assert_eq!(
        unchanged["runtime"]["currentObservation"]["pid"].as_u64(),
        Some(current_runtime_pid)
    );

    stop_and_reap(&home, session_id, &mut host);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn blank_terminal_never_claims_mcp_registration_or_agent_restart() {
    let home = temp_home("blank");
    let session_id = "blank-session";
    let launch = write_launch(&home, session_id, "");
    let mut host = spawn_host(&home, &launch, None);
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    assert!(wait_until(Duration::from_secs(5), || socket.exists()));

    let running = manifest(&home, session_id);
    assert_eq!(running["mcp_client_registered"], false);
    assert_eq!(running["browser_client_registered"], false);
    assert_eq!(running["computer_client_registered"], false);
    assert_eq!(running["mcp_enabled"], true);
    assert_eq!(running["browser_mcp_enabled"], true);
    assert_eq!(running["computer_mcp_enabled"], true);
    assert_eq!(running["runtime_launch_generation"], 0);

    let inherited_generation = home.join("blank-inherited-generation");
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({
                "type": "write",
                "data": format!(
                    "printf '%s' \"${{UNPEEL_RUNTIME_GENERATION-unset}}\" > '{}'\r",
                    inherited_generation.display()
                )
            }),
        )["ok"],
        true
    );
    assert!(wait_until(Duration::from_secs(2), || {
        fs::read_to_string(&inherited_generation).is_ok_and(|value| value == "unset")
    }));

    // Automatic injection evidence stays false, but a provider the user
    // configured manually can start the same stdio server and receive only
    // the domains this blank Session was granted at launch.
    let mut mcp = Command::new(env!("CARGO_BIN_EXE_unpeel-host"))
        .arg("__mcp__")
        .env("UNPEEL_HOME", &home)
        .env("UNPEEL_SESSION_ID", session_id)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    mcp.stdin
        .take()
        .unwrap()
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test\",\"version\":\"1\"}}}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}\n",
        )
        .unwrap();
    let mcp_output = mcp.wait_with_output().unwrap();
    assert!(
        mcp_output.status.success(),
        "manual MCP stderr: {}",
        String::from_utf8_lossy(&mcp_output.stderr)
    );
    let tools_response = String::from_utf8_lossy(&mcp_output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|response| response["id"] == 2)
        .expect("tools/list response");
    let tool_names: Vec<&str> = tools_response["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert_eq!(tool_names, vec!["sessions", "browser", "computer"]);

    // Persisted Kiro configs from before the generic gate rename invoke the
    // legacy argv and carry only Kiro's Sessions/Browser aliases. Even though
    // this Session manifest also grants Computer, that unregistered domain
    // must remain absent and uncallable.
    let mut legacy_kiro_mcp = Command::new(env!("CARGO_BIN_EXE_unpeel-host"))
        .arg("__kiro_mcp__")
        .env("UNPEEL_HOME", &home)
        .env("UNPEEL_SESSION_ID", session_id)
        .env("UNPEEL_KIRO_SESSIONS_MCP_ENABLED", "yes")
        .env("UNPEEL_KIRO_BROWSER_MCP_ENABLED", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    legacy_kiro_mcp
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}\n{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"computer\",\"arguments\":{\"action\":\"help\"}}}\n",
        )
        .unwrap();
    let legacy_output = legacy_kiro_mcp.wait_with_output().unwrap();
    assert!(
        legacy_output.status.success(),
        "legacy Kiro MCP stderr: {}",
        String::from_utf8_lossy(&legacy_output.stderr)
    );
    let legacy_responses = String::from_utf8_lossy(&legacy_output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    let legacy_tools = legacy_responses
        .iter()
        .find(|response| response["id"] == 2)
        .expect("legacy Kiro tools/list response");
    let legacy_tool_names = legacy_tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(legacy_tool_names, vec!["sessions", "browser"]);
    let denied_computer = legacy_responses
        .iter()
        .find(|response| response["id"] == 3)
        .expect("legacy Kiro computer denial");
    assert_eq!(denied_computer["result"]["isError"], true);

    let rejected = socket_command(
        &home,
        session_id,
        json!({ "type": "restart_agent", "expected_generation": 0 }),
    );
    assert_eq!(rejected["ok"], false);

    stop_and_reap(&home, session_id, &mut host);
    assert!(wait_until(Duration::from_secs(3), || {
        manifest(&home, session_id)["state"] == "exited"
    }));
    let exited = manifest(&home, session_id);
    assert_eq!(exited["mcp_client_registered"], false);
    assert_eq!(exited["browser_client_registered"], false);
    assert_eq!(exited["computer_client_registered"], false);
    assert_eq!(exited["mcp_enabled"], true);
    assert_eq!(exited["browser_mcp_enabled"], true);
    assert_eq!(exited["computer_mcp_enabled"], true);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn kiro_registration_evidence_excludes_the_unimplemented_computer_domain() {
    let home = temp_home("kiro-mcp");
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let fake_kiro = bin.join("kiro-cli");
    fs::write(
        &fake_kiro,
        b"#!/bin/bash\nexec -a kiro-cli /bin/sleep 300\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_kiro).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_kiro, permissions).unwrap();
    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let test_path = format!("{}:{inherited_path}", bin.display());
    fs::write(
        home.join(".bash_profile"),
        format!("export PATH='{}'\n", test_path.replace('\'', "'\\''")),
    )
    .unwrap();

    let session_id = "kiro-registration";
    let launch = write_launch(&home, session_id, "kiro-cli");
    let mut host = spawn_host(&home, &launch, Some(&test_path));
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    assert!(wait_until(Duration::from_secs(5), || socket.exists()));
    let running = manifest(&home, session_id);
    assert_eq!(running["mcp_enabled"], true);
    assert_eq!(running["browser_mcp_enabled"], true);
    assert_eq!(running["computer_mcp_enabled"], true);
    assert_eq!(running["mcp_client_registered"], true);
    assert_eq!(running["browser_client_registered"], true);
    assert_eq!(running["computer_client_registered"], false);

    stop_and_reap(&home, session_id, &mut host);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn restart_agent_rejects_a_different_live_foreground_runtime() {
    let home = temp_home("mismatch");
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    symlink("/bin/sleep", bin.join("pi")).unwrap();
    let manual_generation = home.join("manual-runtime-generation");
    let mystery_pid = home.join("mystery-pid");
    let fake_claude = bin.join("claude");
    fs::write(
        &fake_claude,
        format!(
            "#!/bin/bash\nprintf '%s' \"${{UNPEEL_RUNTIME_GENERATION:-unset}}\" > '{}'\nexec -a claude /bin/sleep \"$@\"\n",
            manual_generation.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_claude).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_claude, permissions).unwrap();
    let mystery = bin.join("mystery");
    fs::write(
        &mystery,
        format!(
            "#!/bin/bash\nprintf '%s' \"$$\" > '{}'\nexec -a mystery /bin/sleep 300\n",
            mystery_pid.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&mystery).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&mystery, permissions).unwrap();
    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let test_path = format!("{}:{inherited_path}", bin.display());
    fs::write(
        home.join(".bash_profile"),
        format!("export PATH='{}'\n", test_path.replace('\'', "'\\''")),
    )
    .unwrap();

    let session_id = "mismatch-session";
    let launch = write_launch(&home, session_id, "pi 1");
    let mut host = spawn_host(&home, &launch, Some(&test_path));
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    assert!(wait_until(Duration::from_secs(5), || socket.exists()));
    // Once the stable pi command returns, start a different recognized agent
    // manually in the fallback shell. Observation may change presentation,
    // but must never authorize the stable pi restart recipe against Claude.
    std::thread::sleep(Duration::from_millis(1_300));
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({ "type": "write", "data": "claude 300\r" })
        )["ok"],
        true
    );
    assert!(wait_until(Duration::from_secs(5), || {
        manifest(&home, session_id)["runtime"]["currentObservation"]["id"] == "claude"
    }));
    assert_eq!(
        fs::read_to_string(&manual_generation).unwrap(),
        "unset",
        "generation must not leak into the fallback shell or a manually typed runtime"
    );

    let rejected = socket_command(
        &home,
        session_id,
        json!({ "type": "restart_agent", "expected_generation": 1 }),
    );
    assert_eq!(rejected["ok"], false);
    assert!(rejected["error"]
        .as_str()
        .is_some_and(|error| error.contains("terminal foreground is claude")));
    assert_eq!(manifest(&home, session_id)["runtime_launch_generation"], 1);

    // A different recognized runtime remains a blocker after Ctrl-Z returns
    // Bash to the foreground. The full-session proof must not look only for
    // the stable command's expected `pi` identity.
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({ "type": "write", "data": "\u{1a}" })
        )["ok"],
        true
    );
    let shell_ready = home.join("mismatch-shell-ready");
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({
                "type": "write",
                "data": format!("printf ready > '{}'\r", shell_ready.display())
            })
        )["ok"],
        true
    );
    assert!(wait_until(Duration::from_secs(3), || shell_ready.exists()));
    let background_rejected = socket_command(
        &home,
        session_id,
        json!({ "type": "resume_agent", "expected_generation": 1 }),
    );
    assert_eq!(
        background_rejected["ok"], false,
        "background response: {background_rejected}"
    );
    assert!(background_rejected["error"]
        .as_str()
        .is_some_and(|error| error.contains("claude")));
    assert_eq!(manifest(&home, session_id)["runtime_launch_generation"], 1);

    // Terminate Claude through ordinary job control, then start a foreground
    // job that is not a known runtime. Missing observation is not permission:
    // an isolated unknown process group must also survive Resume Agent.
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({ "type": "write", "data": "fg\r" })
        )["ok"],
        true
    );
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({ "type": "write", "data": "\u{3}" })
        )["ok"],
        true
    );
    assert!(wait_until(Duration::from_secs(5), || {
        manifest(&home, session_id)["runtime"].is_null()
    }));
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({ "type": "write", "data": "mystery\r" })
        )["ok"],
        true
    );
    assert!(wait_until(Duration::from_secs(5), || mystery_pid.exists()));
    let mystery_pid: u32 = fs::read_to_string(&mystery_pid)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let rejected = socket_command(
        &home,
        session_id,
        json!({ "type": "resume_agent", "expected_generation": 1 }),
    );
    assert_eq!(rejected["ok"], false);
    assert!(rejected["error"]
        .as_str()
        .is_some_and(|error| error.contains("unrecognized foreground job")));
    assert!(process_alive(mystery_pid));
    assert_eq!(manifest(&home, session_id)["runtime_launch_generation"], 1);

    stop_and_reap(&home, session_id, &mut host);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn resume_and_legacy_restart_reject_an_active_expected_runtime_without_signaling_it() {
    let home = temp_home("kill-race");
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let launches = home.join("runtime-launches");
    let term_seen = home.join("restart-term-seen");
    let fake_pi = bin.join("pi");
    // argv[0] remains `pi` for observation. The TERM trap is positive proof
    // that neither the new action nor the legacy spelling signaled the live
    // expected runtime before rejecting it.
    fs::write(
        &fake_pi,
        format!(
            "#!/bin/bash\nprintf '%s\\n' \"$$\" >> '{}'\nexec -a pi /bin/bash -c 'trap \"/usr/bin/touch {}\" TERM; while :; do /bin/sleep 1; done'\n",
            launches.display(),
            term_seen.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_pi).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_pi, permissions).unwrap();
    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let test_path = format!("{}:{inherited_path}", bin.display());
    fs::write(
        home.join(".bash_profile"),
        format!("export PATH='{}'\n", test_path.replace('\'', "'\\''")),
    )
    .unwrap();

    let session_id = "kill-race-session";
    let launch = write_launch(&home, session_id, "pi");
    let mut host = spawn_host(&home, &launch, Some(&test_path));
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    assert!(wait_until(Duration::from_secs(5), || socket.exists()));
    assert!(wait_until(Duration::from_secs(5), || {
        manifest(&home, session_id)["runtime"]["currentObservation"]["id"] == "pi"
    }));
    assert_eq!(
        socket_command(&home, session_id, json!({ "type": "write", "data": "x" }))["ok"],
        true
    );

    let runtime_pid = manifest(&home, session_id)["runtime"]["currentObservation"]["pid"]
        .as_u64()
        .unwrap() as u32;
    let resumed = socket_command(
        &home,
        session_id,
        json!({ "type": "resume_agent", "expected_generation": 1 }),
    );
    assert_eq!(resumed["ok"], false, "resume response: {resumed}");
    assert!(
        resumed["error"]
            .as_str()
            .is_some_and(|error| error.contains("agent is still running")),
        "resume response: {resumed}"
    );

    let legacy = socket_command(
        &home,
        session_id,
        json!({ "type": "restart_agent", "expected_generation": 1 }),
    );
    assert_eq!(legacy["ok"], false, "legacy response: {legacy}");
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !term_seen.exists(),
        "Resume Agent signaled the live runtime"
    );
    assert!(process_alive(runtime_pid));
    assert_eq!(recorded_pids(&launches), vec![runtime_pid]);
    assert_eq!(manifest(&home, session_id)["runtime_launch_generation"], 1);

    let killed = socket_command(&home, session_id, json!({ "type": "kill" }));
    assert_eq!(killed["ok"], true, "kill response: {killed}");

    assert!(wait_until(Duration::from_secs(5), || {
        host.try_wait().ok().flatten().is_some()
    }));
    assert!(wait_until(Duration::from_secs(3), || {
        manifest(&home, session_id)["state"] == "exited"
    }));
    let exited = manifest(&home, session_id);
    assert_eq!(exited["runtime_launch_generation"], 1);

    // Explicit Host stop is allowed to terminate the runtime, but neither
    // rejected in-place request may have launched a replacement.
    std::thread::sleep(Duration::from_millis(300));
    let launched_pids = recorded_pids(&launches);
    assert_eq!(launched_pids, vec![runtime_pid]);
    assert!(wait_until(Duration::from_secs(5), || {
        launched_pids.iter().all(|pid| !process_alive(*pid))
    }));
    let launches_after_exit = fs::read(&launches).unwrap_or_default();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        fs::read(&launches).unwrap_or_default(),
        launches_after_exit,
        "an agent launched after the Host had exited"
    );
    assert!(!socket.exists());

    stop_and_reap(&home, session_id, &mut host);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn raw_kill_waits_for_the_shell_resume_transaction() {
    let home = temp_home("kill-lock");
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let launches = home.join("runtime-launches");
    let fake_pi = bin.join("pi");
    fs::write(
        &fake_pi,
        format!(
            "#!/bin/bash\nprintf '%s\\n' \"$$\" >> '{}'\nif [ \"${{UNPEEL_RUNTIME_GENERATION:-unset}}\" = 1 ]; then exec -a pi /bin/sleep 1; fi\nexec -a pi /bin/sleep 300\n",
            launches.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_pi).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_pi, permissions).unwrap();
    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let test_path = format!("{}:{inherited_path}", bin.display());
    fs::write(
        home.join(".bash_profile"),
        format!("export PATH='{}'\n", test_path.replace('\'', "'\\''")),
    )
    .unwrap();

    let session_id = "kill-race-session";
    let launch = write_launch(&home, session_id, "pi");
    let mut host = spawn_host(&home, &launch, Some(&test_path));
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    assert!(wait_until(Duration::from_secs(5), || socket.exists()));
    assert!(wait_until(Duration::from_secs(5), || {
        manifest(&home, session_id)["runtime"]["currentObservation"]["id"] == "pi"
    }));
    assert_eq!(
        socket_command(&home, session_id, json!({ "type": "write", "data": "x" }))["ok"],
        true
    );
    assert!(wait_until(Duration::from_secs(5), || {
        manifest(&home, session_id)["runtime"].is_null()
    }));

    let session_dir = home.join("app-sessions").join(session_id);
    fs::write(
        session_dir.join("appended-context.json"),
        br#"{"context":"serialize the resume","updated_at":1}"#,
    )
    .unwrap();
    let context_lock_dir = home.join("session-appended-context-locks");
    fs::create_dir_all(&context_lock_dir).unwrap();
    // SHA-256("kill-race-session"), matching session_ops' path-safe lock key.
    let context_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(
            context_lock_dir
                .join("998c55837db7a940880f0b74fe231ac3f4799e9580d09cdf1abe21aa602640b8.lock"),
        )
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(context_lock.as_raw_fd(), libc::LOCK_EX) },
        0
    );

    // Two requests prove which phase the winner reached without timing: one
    // holds the Host's agent transaction lock while blocked on our context
    // flock, and the other is rejected as already in progress.
    let (resume_tx, resume_rx) = mpsc::channel();
    let mut resume_threads = Vec::new();
    for _ in 0..2 {
        let request_home = home.clone();
        let request_tx = resume_tx.clone();
        resume_threads.push(std::thread::spawn(move || {
            let response = socket_command(
                &request_home,
                session_id,
                json!({ "type": "resume_agent", "expected_generation": 1 }),
            );
            request_tx.send(response).unwrap();
        }));
    }
    drop(resume_tx);
    let concurrent = resume_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("one concurrent Resume request is rejected promptly");
    assert_eq!(concurrent["ok"], false, "concurrent response: {concurrent}");
    assert!(concurrent["error"]
        .as_str()
        .is_some_and(|error| error.contains("already in progress")));

    let (kill_tx, kill_rx) = mpsc::channel();
    let kill_home = home.clone();
    let kill_thread = std::thread::spawn(move || {
        kill_tx
            .send(socket_command(
                &kill_home,
                session_id,
                json!({ "type": "kill" }),
            ))
            .unwrap();
    });
    assert!(
        kill_rx.recv_timeout(Duration::from_millis(250)).is_err(),
        "raw Kill bypassed the in-progress Resume Agent transaction"
    );

    assert_eq!(
        unsafe { libc::flock(context_lock.as_raw_fd(), libc::LOCK_UN) },
        0
    );
    drop(context_lock);
    let resumed = resume_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("blocked Resume completes after context unlock");
    assert_eq!(resumed["ok"], true, "resume response: {resumed}");
    let killed = kill_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("Kill follows the committed Resume transaction");
    assert_eq!(killed["ok"], true, "kill response: {killed}");
    for thread in resume_threads {
        thread.join().unwrap();
    }
    kill_thread.join().unwrap();

    assert!(wait_until(Duration::from_secs(5), || {
        host.try_wait().ok().flatten().is_some()
    }));
    assert!(wait_until(Duration::from_secs(3), || {
        manifest(&home, session_id)["state"] == "exited"
    }));
    assert_eq!(manifest(&home, session_id)["runtime_launch_generation"], 2);
    let launched_pids = recorded_pids(&launches);
    assert!((1..=2).contains(&launched_pids.len()));
    assert!(wait_until(Duration::from_secs(5), || {
        launched_pids.iter().all(|pid| !process_alive(*pid))
    }));

    stop_and_reap(&home, session_id, &mut host);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn stopped_background_runtime_keeps_resume_unadvertised_and_is_never_injected_into() {
    let home = temp_home("ctrl-z");
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let runtime_pid_path = home.join("runtime-pids");
    write_executable(
        &bin.join("pi"),
        format!(
            "#!/bin/bash\nprintf '%s\\n' \"$$\" >> '{}'\nif [ \"${{UNPEEL_RUNTIME_GENERATION:-unset}}\" = 1 ]; then exec -a pi /bin/sleep 1; fi\nexec -a pi /bin/sleep 300\n",
            runtime_pid_path.display()
        ),
    );
    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let test_path = format!("{}:{inherited_path}", bin.display());
    fs::write(
        home.join(".bash_profile"),
        format!("export PATH='{}'\n", test_path.replace('\'', "'\\''")),
    )
    .unwrap();

    let session_id = "ctrl-z-session";
    let launch = write_launch(&home, session_id, "pi");
    let mut host = spawn_host(&home, &launch, Some(&test_path));
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    assert!(wait_until(Duration::from_secs(5), || socket.exists()));
    assert!(wait_until(Duration::from_secs(5), || {
        let current = manifest(&home, session_id);
        current["runtime"]["currentObservation"]["id"] == "pi"
            && current["runtime_launch_pending"] == false
    }));
    assert!(wait_until(Duration::from_secs(5), || {
        let current = manifest(&home, session_id);
        current["runtime"].is_null() && current["runtime_launch_pending"] == false
    }));
    // Start the same expected runtime as an ordinary interactive-shell job.
    // Unlike the initial `-c` startup script, this gives Bash normal job
    // control so Ctrl-Z leaves a real stopped background process behind.
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({ "type": "write", "data": "pi\r" }),
        )["ok"],
        true
    );
    assert!(wait_until(Duration::from_secs(5), || {
        manifest(&home, session_id)["runtime"]["currentObservation"]["id"] == "pi"
            && fs::read_to_string(&runtime_pid_path).is_ok_and(|pids| pids.lines().count() == 2)
    }));
    let runtime_pid = recorded_pids(&runtime_pid_path)[1];

    // Ctrl-Z stops the foreground runtime job and returns terminal control to
    // Bash. A foreground-only check would now misclassify this as OwnedShell.
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({ "type": "write", "data": "\u{1a}" }),
        )["ok"],
        true
    );
    let shell_ready = home.join("shell-ready");
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({
                "type": "write",
                "data": format!("printf ready > '{}'\r", shell_ready.display())
            }),
        )["ok"],
        true
    );
    assert!(wait_until(Duration::from_secs(3), || shell_ready.exists()));

    // Stay beyond the old six-miss hysteresis window. The all-session process
    // proof must retain the expected runtime observation, keeping summaries'
    // Resume Agent capability false before the request is attempted.
    std::thread::sleep(Duration::from_millis(2_200));
    let stopped = manifest(&home, session_id);
    assert!(process_alive(runtime_pid), "stopped runtime disappeared");
    assert_eq!(
        stopped["runtime"]["currentObservation"]["id"], "pi",
        "stopped manifest: {stopped}"
    );
    assert_eq!(
        stopped["runtime"]["currentObservation"]["pid"].as_u64(),
        Some(u64::from(runtime_pid))
    );
    assert_eq!(stopped["runtime_launch_pending"], false);

    let rejected = socket_command(
        &home,
        session_id,
        json!({ "type": "resume_agent", "expected_generation": 1 }),
    );
    assert_eq!(rejected["ok"], false, "resume response: {rejected}");
    assert!(rejected["error"]
        .as_str()
        .is_some_and(|error| error.contains("agent is still running")));
    assert!(process_alive(runtime_pid));
    assert_eq!(manifest(&home, session_id)["runtime_launch_generation"], 1);

    // Bring the stopped job back to the foreground so the ordinary Host Kill
    // path can terminate the complete job without leaving a stopped orphan.
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({ "type": "write", "data": "fg\r" }),
        )["ok"],
        true
    );
    std::thread::sleep(Duration::from_millis(200));
    stop_and_reap(&home, session_id, &mut host);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn background_runtime_exec_rename_retains_exact_job_blocker() {
    let home = temp_home("rename");
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let runtime_pid_path = home.join("runtime-pids");
    let renamed_marker = home.join("runtime-renamed");
    write_executable(
        &bin.join("pi"),
        format!(
            "#!/bin/bash\nprintf '%s\\n' \"$$\" >> '{}'\nif [ \"${{UNPEEL_RUNTIME_GENERATION:-unset}}\" = 1 ]; then exec -a pi /bin/sleep 1; fi\nexec -a pi /bin/bash -c \"trap '/bin/mkdir \\\"{}\\\"; exec -a mystery /bin/sleep 300' CONT; while :; do /bin/sleep 1; done\"\n",
            runtime_pid_path.display(),
            renamed_marker.display()
        ),
    );
    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let test_path = format!("{}:{inherited_path}", bin.display());
    fs::write(
        home.join(".bash_profile"),
        format!("export PATH='{}'\n", test_path.replace('\'', "'\\''")),
    )
    .unwrap();

    let session_id = "rename-session";
    let launch = write_launch(&home, session_id, "pi");
    let mut host = spawn_host(&home, &launch, Some(&test_path));
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    if !wait_until(Duration::from_secs(5), || socket.exists()) {
        let _ = host.kill();
        let status = host.wait().ok();
        let mut stderr = String::new();
        if let Some(pipe) = host.stderr.as_mut() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        panic!("Host did not bind socket (status {status:?}): {stderr}");
    }
    assert!(wait_until(Duration::from_secs(5), || {
        let current = manifest(&home, session_id);
        current["runtime"].is_null() && current["runtime_launch_pending"] == false
    }));

    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({ "type": "write", "data": "pi\r" }),
        )["ok"],
        true
    );
    assert!(wait_until(Duration::from_secs(5), || {
        manifest(&home, session_id)["runtime"]["currentObservation"]["id"] == "pi"
            && fs::read_to_string(&runtime_pid_path).is_ok_and(|pids| pids.lines().count() == 2)
    }));
    let runtime_pid = recorded_pids(&runtime_pid_path)[1];
    let observed = manifest(&home, session_id);
    assert_eq!(
        observed["runtime"]["currentObservation"]["processGroupID"].as_u64(),
        Some(u64::from(runtime_pid)),
        "interactive runtime owns an isolated job group: {observed}"
    );

    // Stop the recognized runtime, return Bash to the foreground, then resume
    // it as a background job. Its CONT trap execs the exact same PID into an
    // unrecognized argv/name. Fresh catalog matching now returns nothing; the
    // retained PID/start/PGID evidence must remain authoritative.
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({ "type": "write", "data": "\u{1a}" }),
        )["ok"],
        true
    );
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({ "type": "write", "data": "bg\r" }),
        )["ok"],
        true
    );
    assert!(wait_until(Duration::from_secs(3), || renamed_marker.is_dir()));
    std::thread::sleep(Duration::from_millis(2_200));

    let retained = manifest(&home, session_id);
    assert!(process_alive(runtime_pid), "renamed runtime disappeared");
    assert_eq!(
        retained["runtime"]["currentObservation"]["id"], "pi",
        "renamed exact identity must stay retained: {retained}"
    );
    assert_eq!(
        retained["runtime"]["currentObservation"]["pid"].as_u64(),
        Some(u64::from(runtime_pid))
    );

    let rejected = socket_command(
        &home,
        session_id,
        json!({ "type": "resume_agent", "expected_generation": 1 }),
    );
    assert_eq!(rejected["ok"], false, "resume response: {rejected}");
    assert!(rejected["error"]
        .as_str()
        .is_some_and(|error| error.contains("agent is still running")));
    assert!(process_alive(runtime_pid));
    assert_eq!(recorded_pids(&runtime_pid_path).len(), 2);
    assert_eq!(manifest(&home, session_id)["runtime_launch_generation"], 1);

    // Put the renamed job back in front so Host Kill owns and reaps it.
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({ "type": "write", "data": "fg\r" }),
        )["ok"],
        true
    );
    std::thread::sleep(Duration::from_millis(200));
    stop_and_reap(&home, session_id, &mut host);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn same_pid_pgid_shell_exec_command_is_not_the_owned_interactive_shell() {
    let home = temp_home("same-shell-exec");
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let generations = home.join("runtime-generations");
    write_executable(
        &bin.join("pi"),
        format!(
            "#!/bin/bash\nprintf '%s\\n' \"${{UNPEEL_RUNTIME_GENERATION:-unset}}\" >> '{}'\nexec -a pi /bin/sleep 1\n",
            generations.display()
        ),
    );
    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let test_path = format!("{}:{inherited_path}", bin.display());
    fs::write(
        home.join(".bash_profile"),
        format!("export PATH='{}'\n", test_path.replace('\'', "'\\''")),
    )
    .unwrap();

    let session_id = "same-shell-exec-session";
    let launch = write_launch(&home, session_id, "pi");
    let mut host = spawn_host(&home, &launch, Some(&test_path));
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    assert!(wait_until(Duration::from_secs(5), || socket.exists()));
    assert!(wait_until(Duration::from_secs(5), || {
        let current = manifest(&home, session_id);
        current["runtime"].is_null() && current["runtime_launch_pending"] == false
    }));

    let exec_pid_path = home.join("exec-pid");
    let exec_command = format!(
        "exec /bin/bash -c 'printf %s \"$$\" > \"{}\"; trap : TERM; while :; do /bin/sleep 1; done'\r",
        exec_pid_path.display()
    );
    assert_eq!(
        socket_command(
            &home,
            session_id,
            json!({ "type": "write", "data": exec_command }),
        )["ok"],
        true
    );
    assert!(wait_until(Duration::from_secs(3), || exec_pid_path.exists()));
    let exec_pid: u32 = fs::read_to_string(&exec_pid_path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(
        manifest(&home, session_id)["pid"].as_u64(),
        Some(u64::from(exec_pid)),
        "exec retains the Host-owned session leader PID"
    );

    let rejected = socket_command(
        &home,
        session_id,
        json!({ "type": "resume_agent", "expected_generation": 1 }),
    );
    assert_eq!(rejected["ok"], false, "resume response: {rejected}");
    assert!(rejected["error"]
        .as_str()
        .is_some_and(|error| error.contains("owned interactive login shell")));
    assert!(process_alive(exec_pid));
    assert_eq!(
        fs::read_to_string(&generations)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        vec!["1"]
    );
    assert_eq!(manifest(&home, session_id)["runtime_launch_generation"], 1);

    stop_and_reap(&home, session_id, &mut host);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn launch_pending_rejects_duplicate_initial_and_post_resume_submissions() {
    let home = temp_home("launch-pending");
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let generations = home.join("runtime-generations");
    let release_initial = home.join("release-initial");
    let release_resume = home.join("release-resume");
    write_executable(
        &bin.join("pi"),
        format!(
            "#!/bin/bash\ngeneration=${{UNPEEL_RUNTIME_GENERATION:-unset}}\nprintf '%s\\n' \"$generation\" >> '{}'\nif [ \"$generation\" = 1 ]; then release='{}'; duration=1; else release='{}'; duration=300; fi\nwhile [ ! -e \"$release\" ]; do /bin/sleep 0.05; done\nexec -a pi /bin/sleep \"$duration\"\n",
            generations.display(),
            release_initial.display(),
            release_resume.display()
        ),
    );
    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let test_path = format!("{}:{inherited_path}", bin.display());
    fs::write(
        home.join(".bash_profile"),
        format!("export PATH='{}'\n", test_path.replace('\'', "'\\''")),
    )
    .unwrap();

    let session_id = "launch-pending-session";
    let launch = write_launch(&home, session_id, "pi");
    let mut host = spawn_host(&home, &launch, Some(&test_path));
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    assert!(wait_until(Duration::from_secs(5), || socket.exists()));
    assert!(wait_until(Duration::from_secs(3), || {
        fs::read_to_string(&generations).is_ok_and(|value| value.lines().eq(["1"]))
    }));
    let initial = manifest(&home, session_id);
    assert_eq!(initial["runtime_launch_generation"], 1);
    assert_eq!(initial["runtime_launch_pending"], true);
    assert!(initial["runtime"].is_null());
    let duplicate_initial = socket_command(
        &home,
        session_id,
        json!({ "type": "resume_agent", "expected_generation": 1 }),
    );
    assert_eq!(duplicate_initial["ok"], false);
    assert!(duplicate_initial["error"]
        .as_str()
        .is_some_and(|error| error.contains("resume launch is pending")));
    assert_eq!(fs::read_to_string(&generations).unwrap().lines().count(), 1);

    fs::write(&release_initial, b"go").unwrap();
    assert!(wait_until(Duration::from_secs(5), || {
        let current = manifest(&home, session_id);
        current["runtime"]["currentObservation"]["id"] == "pi"
            && current["runtime_launch_pending"] == false
    }));
    assert!(wait_until(Duration::from_secs(5), || {
        let current = manifest(&home, session_id);
        current["runtime"].is_null() && current["runtime_launch_pending"] == false
    }));

    let resumed = socket_command(
        &home,
        session_id,
        json!({ "type": "resume_agent", "expected_generation": 1 }),
    );
    assert_eq!(resumed["ok"], true, "resume response: {resumed}");
    assert!(wait_until(Duration::from_secs(3), || {
        fs::read_to_string(&generations).is_ok_and(|value| value.lines().eq(["1", "2"]))
    }));
    let post_submit = manifest(&home, session_id);
    assert_eq!(post_submit["runtime_launch_generation"], 2);
    assert_eq!(post_submit["runtime_launch_pending"], true);
    assert!(post_submit["runtime"].is_null());

    let duplicate_resume = socket_command(
        &home,
        session_id,
        json!({ "type": "resume_agent", "expected_generation": 2 }),
    );
    assert_eq!(duplicate_resume["ok"], false);
    assert!(duplicate_resume["error"]
        .as_str()
        .is_some_and(|error| error.contains("resume launch is pending")));
    assert_eq!(fs::read_to_string(&generations).unwrap().lines().count(), 2);

    fs::write(&release_resume, b"go").unwrap();
    assert!(wait_until(Duration::from_secs(5), || {
        let current = manifest(&home, session_id);
        current["runtime"]["currentObservation"]["id"] == "pi"
            && current["runtime_launch_pending"] == false
    }));
    assert_eq!(fs::read_to_string(&generations).unwrap().lines().count(), 2);

    stop_and_reap(&home, session_id, &mut host);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn missing_initial_runtime_clears_pending_on_definitive_wrapper_completion() {
    let home = temp_home("missing-runtime");
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let test_path = format!("{}:/usr/bin:/bin", bin.display());
    fs::write(
        home.join(".bash_profile"),
        format!("export PATH='{}'\n", test_path.replace('\'', "'\\''")),
    )
    .unwrap();

    let session_id = "missing-runtime-session";
    let launch = write_launch(&home, session_id, "pi");
    let mut host = spawn_host(&home, &launch, Some(&test_path));
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    assert!(wait_until(Duration::from_secs(5), || socket.exists()));
    assert!(wait_until(Duration::from_secs(5), || {
        let current = manifest(&home, session_id);
        current["state"] == "running"
            && current["runtime_launch_generation"] == 1
            && current["runtime_launch_pending"] == false
            && current["runtime"].is_null()
    }));
    assert!(
        !home
            .join("app-sessions")
            .join(session_id)
            .join(".runtime-launch-complete")
            .exists(),
        "observer consumes the definitive completion marker"
    );

    stop_and_reap(&home, session_id, &mut host);
    let _ = fs::remove_dir_all(home);
}

/// The Host refuses to mint while the session is inside its quiet window, so
/// launch output and late scheduler noise must settle first.
fn mint_hibernation_token(home: &Path, session_id: &str) -> String {
    let mut last = Value::Null;
    assert!(
        wait_until(Duration::from_secs(10), || {
            last = socket_command(home, session_id, json!({ "type": "hibernation_token" }));
            last["ok"] == true
        }),
        "token response: {last}"
    );
    last["activity_token"].as_str().unwrap().to_owned()
}

#[test]
fn hibernation_rejects_runtime_output_after_confirmation() {
    let home = temp_home("hibernate-output");
    fs::create_dir_all(&home).unwrap();
    let emitter = home.join("emit-when-released");
    write_executable(
        &emitter,
        b"#!/bin/bash\nwhile [ ! -f \"$UNPEEL_HOME/release-output\" ]; do sleep 0.01; done\nprintf 'late output\\n'\nexec /bin/sleep 300\n",
    );

    let session_id = "hibernate-output-session";
    let launch = write_launch(&home, session_id, &emitter.to_string_lossy());
    let mut host = spawn_host(&home, &launch, None);
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    assert!(wait_until(Duration::from_secs(5), || socket.exists()));

    let token = mint_hibernation_token(&home, session_id);

    fs::write(home.join("release-output"), b"go").unwrap();
    assert!(wait_until(Duration::from_secs(5), || {
        let current = socket_command(&home, session_id, json!({ "type": "hibernation_token" }));
        current["activity_token"].as_str() != Some(token.as_str())
    }));

    let stale = socket_command(
        &home,
        session_id,
        json!({
            "type": "hibernate",
            "expected_activity_token": token,
        }),
    );
    assert_eq!(stale["ok"], false, "stale response: {stale}");
    assert!(host.try_wait().unwrap().is_none(), "Host must remain live");

    assert!(!home
        .join("app-sessions")
        .join(session_id)
        .join("archived.json")
        .exists());
    stop_and_reap(&home, session_id, &mut host);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn hibernation_rejects_host_input_after_confirmation() {
    let home = temp_home("hibernate-input");
    fs::create_dir_all(&home).unwrap();
    let sleeper = home.join("silent-sleeper");
    write_executable(&sleeper, b"#!/bin/bash\nstty -echo\nexec /bin/sleep 300\n");

    let session_id = "hibernate-input-session";
    let launch = write_launch(&home, session_id, &sleeper.to_string_lossy());
    let mut host = spawn_host(&home, &launch, None);
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    assert!(wait_until(Duration::from_secs(5), || socket.exists()));

    let token = mint_hibernation_token(&home, session_id);

    let written = socket_command(
        &home,
        session_id,
        json!({
            "type": "write",
            "data": "x",
            "write_id": null,
        }),
    );
    assert_eq!(written["ok"], true, "write response: {written}");

    let too_recent = socket_command(&home, session_id, json!({ "type": "hibernation_token" }));
    assert_eq!(
        too_recent["ok"], false,
        "input inside the quiet window must not be absorbed into a fresh token: {too_recent}"
    );

    let stale = socket_command(
        &home,
        session_id,
        json!({
            "type": "hibernate",
            "expected_activity_token": token,
        }),
    );
    assert_eq!(stale["ok"], false, "stale response: {stale}");
    assert!(host.try_wait().unwrap().is_none(), "Host must remain live");
    assert!(!home
        .join("app-sessions")
        .join(session_id)
        .join("archived.json")
        .exists());

    let settled = mint_hibernation_token(&home, session_id);
    assert_ne!(settled, token, "the input must be part of the next token");

    stop_and_reap(&home, session_id, &mut host);
    let _ = fs::remove_dir_all(home);
}

#[test]
fn hibernation_stops_the_runtime_when_nothing_changed_since_confirmation() {
    let home = temp_home("hibernate-accept");
    fs::create_dir_all(&home).unwrap();
    let sleeper = home.join("silent-sleeper");
    write_executable(&sleeper, b"#!/bin/bash\nstty -echo\nexec /bin/sleep 300\n");

    let session_id = "hibernate-accept-session";
    let launch = write_launch(&home, session_id, &sleeper.to_string_lossy());
    let mut host = spawn_host(&home, &launch, None);
    let socket = home
        .join("app-sessions")
        .join(session_id)
        .join("session.sock");
    assert!(wait_until(Duration::from_secs(5), || socket.exists()));

    let token = mint_hibernation_token(&home, session_id);
    let again = mint_hibernation_token(&home, session_id);
    assert_eq!(token, again, "a quiet session mints a stable token");

    let accepted = socket_command(
        &home,
        session_id,
        json!({
            "type": "hibernate",
            "expected_activity_token": token,
        }),
    );
    assert_eq!(accepted["ok"], true, "accepted response: {accepted}");
    assert!(
        wait_until(Duration::from_secs(8), || {
            manifest(&home, session_id)["state"] == "exited"
        }),
        "the Host must publish an exited manifest after a conditional stop"
    );
    assert!(wait_until(Duration::from_secs(5), || {
        host.try_wait().ok().flatten().is_some()
    }));

    stop_and_reap(&home, session_id, &mut host);
    let _ = fs::remove_dir_all(home);
}
