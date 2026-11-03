//! Disposable, real session hosts and the controller's real stdio entry point.
#[cfg(not(unix))]
fn main() {}

#[cfg(unix)]
fn main() {
    unix::run();
}

#[cfg(unix)]
mod unix {
    use std::{
        fs,
        io::{BufRead, BufReader, Write},
        path::Path,
        process::{Child, Command, Stdio},
        sync::{Arc, Barrier},
        time::{Duration, Instant},
    };

    use serde_json::{Value, json};
    use unpeel_core::session_host::{self, SessionHostLaunch};
    use zeron_workers_unpeel::{
        LocalWorkersClient, WorkerParentNotificationKind, controller_mcp_handle_request,
        register_worker_parent_at, worker_parent_links_at,
    };

    fn wait_for(stage: &str, mut f: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(10);
        while !f() {
            assert!(Instant::now() < end, "fixture readiness timed out: {stage}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn write_exited(home: &Path, id: &str, project: &str) {
        let dir = home.join("app-sessions").join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("manifest.json"), serde_json::to_vec(&json!({
            "session": {"id": id, "project_id": project, "label": id, "command": "", "created_at": 1},
            "cwd": home, "state": "exited", "pid": null, "exit_code": 0
        })).unwrap()).unwrap();
    }

    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            for manifest in session_host::list_manifests() {
                if manifest.state == session_host::HostedSessionState::Running {
                    let _ = unpeel_core::session_ops::stop_session(&manifest.session.id);
                }
            }
        }
    }

    struct ControllerChild(Child);
    impl Drop for ControllerChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn request(id: u64, args: Value) -> Value {
        json!({"jsonrpc":"2.0", "id":id, "method":"tools/call", "params":{"name":"workers", "arguments":args}})
    }
    fn call(id: u64, args: Value) -> Value {
        let response = controller_mcp_handle_request(request(id, args)).unwrap();
        assert_eq!(response["result"]["isError"], false, "{response}");
        response["result"]["structuredContent"].clone()
    }

    fn restart(home: &Path, old: &str, chat: &str) -> String {
        write_exited(home, old, "project");
        register_worker_parent_at(&home.join("app-state.json"), old, chat, 1).unwrap();
        let result = call(1, json!({"action":"restart_worker", "session_id":old}));
        let id = result["session_id"].as_str().unwrap();
        assert_ne!(id, old, "restart returned the removed identity: {result}");
        assert_ne!(
            id, "noise",
            "restart adopted an unrelated session: {result}"
        );
        let actual = session_host::load_manifest(id).expect("returned ID must own a real host");
        assert_eq!(actual.state, session_host::HostedSessionState::Running);
        assert_eq!(actual.session.label, old);
        assert!(
            worker_parent_links_at(&home.join("app-state.json"))
                .unwrap()
                .iter()
                .any(|link| link.worker_session_id == id && link.parent_chat_id == chat)
        );
        id.to_owned()
    }

    fn normal(home: &Path) {
        let id = restart(home, "old-a", "chat-a");
        println!("PASS normal restart old-a -> {id}, noisy registry and parent chat-a");
    }

    fn concurrent(home: &Path) {
        for (old, chat) in [("old-b", "chat-b"), ("old-c", "chat-c")] {
            write_exited(home, old, "project");
            register_worker_parent_at(&home.join("app-state.json"), old, chat, 1).unwrap();
        }
        let barrier = Arc::new(Barrier::new(3));
        let (b, c) = std::thread::scope(|scope| {
            let bb = barrier.clone();
            let b = scope.spawn(move || {
                bb.wait();
                call(3, json!({"action":"restart_worker", "session_id":"old-b"}))
            });
            let bc = barrier.clone();
            let c = scope.spawn(move || {
                bc.wait();
                call(4, json!({"action":"restart_worker", "session_id":"old-c"}))
            });
            barrier.wait();
            (b.join().unwrap(), c.join().unwrap())
        });
        let b = b["session_id"].as_str().unwrap();
        let c = c["session_id"].as_str().unwrap();
        assert_ne!(b, c);
        for (actual_id, old, chat) in [(b, "old-b", "chat-b"), (c, "old-c", "chat-c")] {
            assert_ne!(actual_id, old);
            assert_eq!(
                session_host::load_manifest(actual_id)
                    .unwrap()
                    .session
                    .label,
                old
            );
            assert!(
                worker_parent_links_at(&home.join("app-state.json"))
                    .unwrap()
                    .iter()
                    .any(|link| link.worker_session_id == actual_id && link.parent_chat_id == chat)
            );
        }
        println!(
            "PASS simultaneous controller restarts: old-b -> {b}, old-c -> {c}, both parent bindings"
        );
    }

    fn same_id(home: &Path, profile: &str) {
        use std::os::unix::fs::PermissionsExt;
        let id = restart(home, "seed", "chat-seed");
        let bin = home.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let pi = bin.join("pi");
        fs::write(&pi, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&pi, fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        fs::write(
            home.join(profile),
            format!("export PATH='{}'\nPS1='C1-READY> '\n", path),
        )
        .unwrap();
        unsafe {
            std::env::set_var("PATH", path);
        }
        let mut same = session_host::load_manifest(&id).unwrap().session;
        same.id = "same-id".into();
        same.command = "pi".into();
        unpeel_core::session_ops::spawn_session(same, home.to_str().unwrap(), None, 80, 24)
            .unwrap();
        let client = LocalWorkersClient::new();
        wait_for("in-place resume capability", || {
            client.bootstrap().is_ok_and(|snapshot| {
                snapshot
                    .sessions
                    .iter()
                    .any(|s| s.id == "same-id" && s.capabilities.resume_agent)
            })
        });
        let result = client
            .session_action("same-id", zeron_workers_unpeel::SessionAction::ResumeAgent)
            .unwrap();
        assert_eq!(result.as_deref(), Some("same-id"));
        println!("PASS in-place resume returned same-id to LocalWorkersClient");
    }

    fn send_and_notify(home: &Path) {
        let id = restart(home, "old-a", "chat-a");
        let dir = home.join("app-sessions").join(&id);
        wait_for("shell prompt", || {
            fs::read(dir.join("output.bin"))
                .is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains("C1-READY>"))
        });
        let mut controller = ControllerChild(
            Command::new(std::env::current_exe().unwrap())
                .arg("__workers_mcp__")
                .env("COMET_WORKERS_CONTROLLER", "1")
                .env("COMET_WORKERS_PARENT_CHAT_ID", "chat-a")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut input = controller.0.stdin.take().unwrap();
        let mut output = BufReader::new(controller.0.stdout.take().unwrap());
        writeln!(
            input,
            "{}",
            request(
                2,
                json!({
                    "action":"send_text", "session_id":id,
                    "text":format!("printf delivered > '{}'", home.join("c1-delivered").display())
                })
            )
        )
        .unwrap();
        input.flush().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut line = String::new();
            let result = output.read_line(&mut line).map(|_| line);
            let _ = tx.send(result);
        });
        let line = match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(result) => result.unwrap(),
            Err(error) => {
                let _ = controller.0.kill();
                let _ = reader.join();
                panic!("controller response timed out: {error}");
            }
        };
        reader.join().unwrap();
        let sent: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(sent["result"]["isError"], false, "{sent}");
        wait_for("delivered input", || home.join("c1-delivered").exists());
        assert_eq!(
            fs::read_to_string(home.join("c1-delivered")).unwrap(),
            "delivered"
        );
        drop(input);
        wait_for("controller exit", || {
            controller.0.try_wait().unwrap().is_some()
        });

        let state: Value =
            serde_json::from_slice(&fs::read(home.join("app-state.json")).unwrap()).unwrap();
        let binding = &state["comet_worker_parent_notifications"][&id];
        let episode = binding["active_task_episode"]
            .as_u64()
            .expect("tracked task episode");
        assert!(
            episode > 0 && binding["task_episode_active"] == true,
            "submission binding: {binding}"
        );
        assert_eq!(
            binding["parent_chat_id"], "chat-a",
            "submission binding: {binding}"
        );
        let client = LocalWorkersClient::new();
        let session = client
            .bootstrap()
            .unwrap()
            .sessions
            .into_iter()
            .find(|s| s.id == id)
            .unwrap();
        let generation = session.runtime_generation;
        // Publish exactly the filesystem snapshot a runtime hook produces. The
        // real host's journal ingress admits and persists it with its real mtime.
        let event = json!({"hook_event_name":"Stop", "unpeel_runtime_generation":generation, "comet_task_episode":episode});
        fs::write(dir.join("last-hook-event.json"), event.to_string()).unwrap();
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            let session = client
                .bootstrap()
                .unwrap()
                .sessions
                .into_iter()
                .find(|s| s.id == id)
                .unwrap();
            let notifications =
                zeron_workers_unpeel::pending_worker_parent_notifications_with_evidence_at(
                    &home.join("app-state.json"),
                    &[session.clone()],
                    &home.join("app-sessions"),
                    |_| zeron_workers_unpeel::WorkerCompletionEvidence::quiescent(),
                )
                .unwrap();
            if notifications.iter().any(|n| {
                n.worker_session_id == id
                    && n.parent_chat_id == "chat-a"
                    && n.task_episode == episode
                    && n.runtime_generation == generation
                    && n.kind == WorkerParentNotificationKind::Completed
            }) {
                println!(
                    "PASS tracked send and completion: worker={id}, chat=chat-a, episode={episode}, generation={generation}"
                );
                break;
            }
            if Instant::now() >= end {
                let journal =
                    fs::read_to_string(dir.join("comet-hook-events.jsonl")).unwrap_or_default();
                panic!(
                    "completion missing: worker={id} episode={episode} generation={generation} binding={binding} event={event} journal={journal:?} session_activity={} notifications={notifications:?}",
                    session.activity
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn run() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if matches!(
            args.first().map(String::as_str),
            Some("__workers_mcp__" | "__session_host__")
        ) {
            assert!(zeron_workers_unpeel::run_session_host_mode_if_requested().unwrap());
            return;
        }
        if let [launch_path] = args.as_slice()
            && std::env::var_os("C1_FIXTURE_LAUNCH_MODE").is_some()
        {
            let launch: SessionHostLaunch =
                serde_json::from_slice(&fs::read(launch_path).unwrap()).unwrap();
            session_host::spawn_host_process_from_launch_file(launch_path).unwrap();
            wait_for("host socket", || {
                session_host::socket_path(&launch.session.id).exists()
            });
            if std::env::var_os("C1_INJECT_NOISE").is_some() {
                write_exited(Path::new(&launch.cwd), "noise", &launch.session.project_id);
            }
            return;
        }
        if !matches!(args.as_slice(), [flag, _] if flag == "--case") {
            let mut filters = Vec::new();
            let mut skipped = Vec::new();
            let mut exact = false;
            let mut list = false;
            let mut ignored = false;
            let mut args = args.iter();
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--nocapture" | "--show-output" | "--quiet" | "-q" | "--include-ignored" => {}
                    "--exact" => exact = true,
                    "--list" => list = true,
                    "--ignored" => ignored = true,
                    "--skip" => {
                        skipped.push(args.next().expect("--skip requires a filter").as_str())
                    }
                    "--test-threads" | "--format" | "--color" => {
                        args.next()
                            .unwrap_or_else(|| panic!("{arg} requires a value"));
                    }
                    _ if arg.starts_with("--skip=") => skipped.push(&arg["--skip=".len()..]),
                    _ if arg.starts_with("--test-threads=")
                        || arg.starts_with("--format=")
                        || arg.starts_with("--color=") => {}
                    _ if arg.starts_with('-') => panic!("unsupported test argument: {arg}"),
                    _ => filters.push(arg.as_str()),
                }
            }
            let matches = |case: &str, filter: &str| {
                if exact {
                    case == filter
                } else {
                    case.contains(filter)
                }
            };
            let cases = ["normal", "concurrent", "same-id", "send-notify"]
                .into_iter()
                .filter(|case| {
                    !ignored
                        && (filters.is_empty()
                            || filters.iter().any(|filter| matches(case, filter)))
                        && !skipped.iter().any(|filter| matches(case, filter))
                })
                .collect::<Vec<_>>();
            if list {
                for case in cases {
                    println!("{case}: test");
                }
                return;
            }
            let mut failed = Vec::new();
            for case in cases {
                let status = Command::new(std::env::current_exe().unwrap())
                    .args(["--case", case])
                    .status()
                    .unwrap();
                if !status.success() {
                    failed.push(case);
                }
            }
            assert!(failed.is_empty(), "failed scenarios: {failed:?}");
            return;
        }
        let [flag, case] = args.as_slice() else {
            unreachable!();
        };
        assert_eq!(flag, "--case");
        let home = tempfile::Builder::new()
            .prefix("c1-")
            .tempdir_in("/tmp")
            .unwrap();
        let zsh = Path::new("/bin/zsh").exists();
        let shell = if zsh { "/bin/zsh" } else { "/bin/bash" };
        let profile = if zsh { ".zshrc" } else { ".bash_profile" };
        unsafe {
            std::env::set_var("UNPEEL_HOME", home.path());
            std::env::set_var("HOME", home.path());
            std::env::set_var("SHELL", shell);
            std::env::set_var("UNPEEL_HOST_CMD", std::env::current_exe().unwrap());
            std::env::set_var("C1_FIXTURE_LAUNCH_MODE", "1");
            std::env::set_var("C1_INJECT_NOISE", "1");
            std::env::set_var("XDG_CONFIG_HOME", home.path().join("config"));
            std::env::set_var("ZERON_DATA_DIR", home.path().join("zeron"));
            std::env::set_var("COMET_WORKERS_HOOKS_DIR", home.path().join("hooks"));
            std::env::remove_var("PI_CODING_AGENT_DIR");
        }
        let _cleanup = Cleanup;
        fs::write(home.path().join(profile), "PS1='C1-READY> '\n").unwrap();
        fs::write(
            home.path().join("app-state.json"),
            serde_json::to_vec(&json!({
                "projects": [{"id":"project", "name":"Project", "path":home.path()}],
                "presets": [], "active_tabs": {}, "pinned_sessions": {}
            }))
            .unwrap(),
        )
        .unwrap();
        match case.as_str() {
            "normal" => normal(home.path()),
            "concurrent" => concurrent(home.path()),
            "same-id" => same_id(home.path(), profile),
            "send-notify" => send_and_notify(home.path()),
            _ => panic!("unknown fixture case: {case}"),
        }
    }
}
