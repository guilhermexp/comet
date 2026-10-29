//! Consumer coverage: only a private synthetic journal/socket, never a Worker.
use std::fs::{self, OpenOptions};
use std::io::Write;

use unpeel_core::session_host;
use zeron_workers_unpeel::LocalWorkersClient;

#[test]
fn terminal_history_pages_every_retained_byte_without_host_ipc() {
    // This binary has one test; set the environment before constructing clients
    // or starting threads. TempDir outlives all client/socket activity.
    let home = tempfile::TempDir::new().unwrap();
    unsafe {
        std::env::set_var("UNPEEL_HOME", home.path());
        std::env::set_var("COMET_WORKERS_HOOKS_DIR", home.path().join("hooks"));
    };
    let id = "synthetic-history";
    let path = session_host::output_path(id);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let floor = 4096_u64;
    let mut bytes = vec![0; floor as usize];
    bytes.extend_from_slice(b"EARLIEST RETAINED OUTPUT\r\n");
    bytes.resize(floor as usize + 8 * 1024 * 1024, b'x');
    fs::write(&path, &bytes).unwrap();
    let retention = session_host::output_retention_path(id);
    fs::write(
        &retention,
        format!(r#"{{"version":1,"retained_from":{floor}}}"#),
    )
    .unwrap();
    let manifest = session_host::manifest_path(id);
    let stale_manifest = serde_json::to_vec(&serde_json::json!({
        "session": {"id": id, "project_id": "fixture", "label": "Fixture", "command": "synthetic"},
        "cwd": home.path(), "state": "running", "pid": std::process::id(),
        "exit_code": null, "updated_at": 1, "heartbeat_at": 1
    }))
    .unwrap();
    fs::write(&manifest, &stale_manifest).unwrap();

    // A socket that never accepts/replies models a wedged Host. The journal
    // path must not even connect to it, including for manifest-health probes.
    #[cfg(unix)]
    let listener = {
        let listener =
            std::os::unix::net::UnixListener::bind(session_host::socket_path(id)).unwrap();
        listener.set_nonblocking(true).unwrap();
        listener
    };
    // Check the passive seam before constructing a client: client startup
    // separately migrates legacy telemetry and may health-check manifests.
    let passive = session_host::read_retained_output_chunk(id, 0, 256 * 1024).unwrap();
    assert_eq!(passive.next_offset, floor + 256 * 1024);
    assert_eq!(fs::read(&manifest).unwrap(), stale_manifest);
    #[cfg(unix)]
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    fs::remove_file(&manifest).unwrap();
    let client = LocalWorkersClient::new();
    let first = client.read_terminal_output(id, 0, 0).unwrap();
    assert_eq!(first.output.offset, floor);
    assert!(first.output.truncated);
    assert!(first.output.data.starts_with(b"EARLIEST RETAINED OUTPUT"));
    let boundary = first.available_end;
    assert_eq!(boundary, bytes.len() as u64);
    let mut recovered = first.output.data;
    let mut cursor = first.output.next_offset;
    let mut pages = 1;
    // Growth must not move the consumer's opening boundary.
    let live = b"\r\nLIVE AFTER OPEN\r\n";
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(live)
        .unwrap();
    while cursor < boundary {
        let page = client.read_terminal_output(id, cursor, 0).unwrap();
        assert_eq!(page.output.offset, cursor);
        assert!(!page.output.truncated);
        assert!(!page.output.data.is_empty());
        assert!(page.output.data.len() <= 256 * 1024);
        recovered.extend(page.output.data);
        cursor = page.output.next_offset;
        pages += 1;
    }
    assert_eq!(pages, 32);
    assert_eq!(recovered, bytes[floor as usize..]);
    let page = client.read_terminal_output(id, cursor, 0).unwrap();
    assert_eq!(page.output.data, live);
    let eof = client
        .read_terminal_output(id, page.output.next_offset, 0)
        .unwrap();
    assert!(eof.output.data.is_empty());
    assert!(!eof.output.truncated);
    #[cfg(unix)]
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );

    // Retention advances between pages: report the gap, restart at the new
    // floor, and keep the page bound instead of jumping to the final tail.
    let new_floor = floor + 1024 * 1024;
    fs::write(
        &retention,
        format!(r#"{{"version":1,"retained_from":{new_floor}}}"#),
    )
    .unwrap();
    let page = client.read_terminal_output(id, floor, 0).unwrap();
    assert!(page.output.truncated);
    assert_eq!(page.output.offset, new_floor);
    assert_eq!(page.output.data.len(), 256 * 1024);
    for invalid in ["", "../escape", "a/b", "a\\b"] {
        assert!(client.read_terminal_output(invalid, 0, 0).is_err());
        assert!(client.read_current_viewport(invalid).is_err());
    }
    let pending = client.read_terminal_output("missing", 0, 0).unwrap();
    assert!(pending.output.data.is_empty());
    assert_eq!(pending.output.next_offset, 0);
    assert!(!pending.output.truncated);
    let pending_path = session_host::output_path("missing");
    fs::create_dir_all(pending_path.parent().unwrap()).unwrap();
    fs::write(&pending_path, b"FIRST WORKER OUTPUT\r\n").unwrap();
    let started = client
        .read_terminal_output("missing", pending.output.next_offset, 0)
        .unwrap();
    assert_eq!(started.output.data, b"FIRST WORKER OUTPUT\r\n");
    assert!(!started.output.truncated);
    // A cursor can outlive a removed journal. Honor the requested wait rather
    // than creating a hot empty-read loop in a visible terminal.
    let began = std::time::Instant::now();
    let gone = client.read_terminal_output("gone", 10, 40).unwrap();
    assert!(began.elapsed() >= std::time::Duration::from_millis(40));
    assert_eq!(gone.output.next_offset, 10);
    assert!(gone.output.data.is_empty());
    // The bounded reader also works when no control socket exists at all.
    #[cfg(unix)]
    {
        drop(listener);
        fs::remove_file(session_host::socket_path(id)).unwrap();
    }
    assert_eq!(
        client.read_terminal_output(id, 0, 0).unwrap().output.offset,
        new_floor
    );
    #[cfg(unix)]
    {
        use std::io::{BufRead, BufReader};
        use std::sync::mpsc;
        use std::time::Duration;
        let listener =
            std::os::unix::net::UnixListener::bind(session_host::socket_path(id)).unwrap();
        let (seen_tx, seen_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let host = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut command = String::new();
            BufReader::new(socket.try_clone().unwrap())
                .read_line(&mut command)
                .unwrap();
            let command: serde_json::Value = serde_json::from_str(&command).unwrap();
            assert_eq!(command["type"], "viewport_snapshot");
            assert_eq!(command["cols"], 0);
            assert_eq!(command["rows"], 0);
            seen_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let reply = serde_json::json!({"ok": true, "viewport": {
                "cols": 80, "rows": 24, "outputOffset": 0, "truncated": false,
                "cursorRow": 0, "cursorCol": 0, "scrollbackRows": 0,
                "viewportStartRow": 0, "scrollOffsetRows": 0, "viewportRows": [],
                "inputModesKnown": true, "mouseReporting": true, "alternateScreen": true
            }});
            writeln!(socket, "{reply}").unwrap();
        });
        let modes_client = client.clone();
        let modes = std::thread::spawn(move || modes_client.read_current_viewport(id));
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // The snapshot cannot return until after this independent read succeeds.
        assert_eq!(
            client.read_terminal_output(id, 0, 0).unwrap().output.offset,
            new_floor
        );
        release_tx.send(()).unwrap();
        let viewport = modes.join().unwrap().unwrap();
        assert!(viewport.input_modes.known && viewport.input_modes.mouse_reporting);
        assert!(viewport.input_modes.alternate_screen);
        host.join().unwrap();
    }
}
