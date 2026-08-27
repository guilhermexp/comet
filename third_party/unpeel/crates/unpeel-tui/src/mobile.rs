//! App-less `/mobile/*` server: the same wire contract as the native
//! `MobileRemoteServer.swift`, so an already-paired phone keeps working when
//! only the TUI runs. Plain HTTP/1.1, Bearer auth against the shared
//! `~/.unpeel/mobile/devices.json` token hashes, JSON keys in the Swift
//! dialect (camelCase with capital-ID suffixes, optionals omitted).
//!
//! Single-owner rule: the exact persisted listener is the phone + Link lease.
//! A TUI yields that lease only to a validated native sidebar, then retries it
//! when the native frontend disappears; concurrent TUIs cannot bind it twice.
//! The native app remains the platform owner for APNs when it runs.
//! Pairing and approvals work app-lessly. Artifacts, resumable uploads, and
//! session creation use the shared Host contract; bootstrap advertises the
//! exact supported subset through the versioned capability descriptor. Push
//! tokens and relay credential recovery remain native/platform-owned for now.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::FromRawFd;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use unpeel_core::app_paths;
use unpeel_core::controller_api::{
    ControllerEffects, ControllerPrincipal, ControllerRequest, HostBootstrapContext,
    HostCreateContext, HostCreateProject, HostRouteContext,
};

const IO_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_BODY: usize = 4 * 1024 * 1024;
/// The TUI main loop publishes the phone-facing snapshot here every rescan:
/// pre-built bootstrap arrays plus the project-scoped archive catalog, all in
/// the Swift wire dialect and desktop sidebar order.
pub type SharedSnapshot = Arc<Mutex<crate::sessions::MobileSnapshot>>;
/// session id → when a phone last resized it via this server (drives the
/// "Resized for mobile" tag in the TUI preview).
pub type MobileResizes = Arc<Mutex<HashMap<String, Instant>>>;
type ProjectColorWriter<'a> = &'a dyn Fn(&str, Option<&str>) -> Result<(), String>;

pub struct MobileServer {
    pub port: u16,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    bonjour: Arc<Mutex<Option<std::process::Child>>>,
    remote: Arc<Mutex<Option<std::process::Child>>>,
    accept_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    active_connections: Arc<Mutex<HashMap<u64, TcpStream>>>,
    worker_threads: Arc<Mutex<Vec<std::thread::JoinHandle<()>>>>,
}

impl MobileServer {
    /// The listener is the serving lease, while the persisted endpoint is
    /// the handoff rendezvous. If another frontend publishes a different
    /// endpoint, this server must retire before it can own Link or mutate
    /// shared authorization state.
    pub fn owns_configured_endpoint(&self) -> bool {
        configured_server_port_at(&mobile_dir()) == Some(self.port)
    }

    /// Released native builds can fall back to a random port when this TUI
    /// owns the saved endpoint, then overwrite `server-port`. The paired phone
    /// will not adopt that unauthenticated replacement. While legacy native
    /// owns Link, keep this listener serving Direct and repair its rendezvous
    /// under the lock shared with capability-aware native/TUI claimers.
    pub fn restore_legacy_configured_endpoint(&self) -> bool {
        restore_server_port_at(&mobile_dir(), self.port)
    }

    /// Stand down: stop accepting, kill the Bonjour advertisement. Called
    /// the moment the app becomes reachable again (it owns the phone
    /// endpoint) and on TUI exit.
    pub fn stop(&self) {
        if self
            .shutdown
            .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            return;
        }
        // Retire the shared claim before releasing the socket. Contenders
        // still fail the exact bind until the accept loop exits, but once a
        // successor publishes the same port this older owner must never
        // compare-delete the successor's lease.
        clear_tui_owner_port_at(&mobile_dir(), self.port);
        for handle in [&self.bonjour, &self.remote] {
            if let Ok(mut guard) = handle.lock() {
                if let Some(mut child) = guard.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        // `pair --serve` hands this exact endpoint to the interactive TUI.
        // Wait for the accept loop to drop its listener before the next
        // server tries to bind the same port; otherwise the Controller's
        // freshly paired endpoint can be stale before its first bootstrap.
        if let Ok(mut guard) = self.accept_thread.lock() {
            if let Some(thread) = guard.take() {
                let _ = thread.join();
            }
        }
        // The accept loop is now gone, so this set cannot grow. Interrupt
        // keep-alive reads/writes before joining every worker; otherwise a
        // polite app↔TUI takeover can retain the old endpoint for 30 seconds.
        if let Ok(connections) = self.active_connections.lock() {
            for stream in connections.values() {
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        }
        if let Ok(mut workers) = self.worker_threads.lock() {
            for worker in workers.drain(..) {
                let _ = worker.join();
            }
        }
    }
}

impl Drop for MobileServer {
    fn drop(&mut self) {
        // Pairing/setup can fail after the listener and helper processes are
        // live. Keep every exceptional return on the same cleanup path as an
        // ordinary TUI hand-back or exit.
        self.stop();
    }
}

/// One-process endpoint handoff used by `unpeel pair --serve`.
///
/// This is deliberately not Bonjour rediscovery: the paired Controller must
/// never send its long-lived bearer token to a plaintext candidate based only
/// on an unauthenticated TXT record. The pairing listener records its exact
/// port here, releases it, and the interactive TUI binds that same port before
/// serving the newly paired Controller.
static NEXT_START_PORT: std::sync::OnceLock<Mutex<Option<u16>>> = std::sync::OnceLock::new();

fn next_start_port() -> &'static Mutex<Option<u16>> {
    NEXT_START_PORT.get_or_init(|| Mutex::new(None))
}

pub fn remember_paired_port(port: u16, hand_off_to_tui: bool) {
    if port == 0 {
        return;
    }
    // Keep a headless Host reachable at the endpoint sealed into the pairing
    // response across later TUI restarts. `server-port` is the canonical
    // app/TUI handoff endpoint: establish it only when absent, never overwrite
    // the native app's existing choice. The headless fallback records the
    // paired port separately for installations that already have another
    // native endpoint.
    persist_paired_port_at(&mobile_dir(), port);
    if hand_off_to_tui {
        if let Ok(mut guard) = next_start_port().lock() {
            *guard = Some(port);
        }
    }
}

fn persist_paired_port_at(dir: &std::path::Path, port: u16) {
    if port == 0 || std::fs::create_dir_all(dir).is_err() {
        return;
    }
    {
        if let Ok(mut canonical) = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join("server-port"))
        {
            let _ = canonical.write_all(format!("{port}\n").as_bytes());
        }
        let _ = std::fs::write(dir.join("headless-server-port"), format!("{port}\n"));
    }
}

fn read_port(path: &std::path::Path) -> Option<u16> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .filter(|port| *port > 0)
}

fn configured_server_port_at(dir: &std::path::Path) -> Option<u16> {
    read_port(&dir.join("server-port")).or_else(|| read_port(&dir.join("headless-server-port")))
}

fn atomic_write_server_port_at(dir: &std::path::Path, port: u16) -> bool {
    let temporary = dir.join(format!(
        ".server-port.{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(format!("{port}\n").as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary, dir.join("server-port"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.is_ok()
}

fn atomic_write_tui_owner_port_at(dir: &std::path::Path, port: u16) -> bool {
    let temporary = dir.join(format!(
        ".tui-server-port.{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(format!("{port}\n").as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary, dir.join("tui-server-port"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.is_ok()
}

fn publish_tui_owner_port_at(dir: &std::path::Path, port: u16) -> bool {
    if port == 0 || std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let lock_path = dir.join("server-port.lock");
    let Ok(lock) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
    else {
        return false;
    };
    #[cfg(unix)]
    if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lock), libc::LOCK_EX) } != 0 {
        return false;
    }
    let published = atomic_write_tui_owner_port_at(dir, port);
    #[cfg(unix)]
    unsafe {
        libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lock), libc::LOCK_UN);
    }
    published
}

fn clear_tui_owner_port_at(dir: &std::path::Path, port: u16) {
    let lock_path = dir.join("server-port.lock");
    let Ok(lock) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
    else {
        return;
    };
    #[cfg(unix)]
    if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lock), libc::LOCK_EX) } != 0 {
        return;
    }
    if read_port(&dir.join("tui-server-port")) == Some(port) {
        let _ = std::fs::remove_file(dir.join("tui-server-port"));
    }
    #[cfg(unix)]
    unsafe {
        libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lock), libc::LOCK_UN);
    }
}

/// Publish a first endpoint without letting two concurrently-starting TUIs
/// both believe their OS-assigned listener won. The socket is already bound
/// when this runs; the file lock chooses one durable winner, and the rename
/// makes readers see either the old file or the complete new value.
fn claim_initial_server_port_at(dir: &std::path::Path, port: u16) -> bool {
    if port == 0 || std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let lock_path = dir.join("server-port.lock");
    let Ok(lock) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
    else {
        return false;
    };
    #[cfg(unix)]
    if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lock), libc::LOCK_EX) } != 0 {
        return false;
    }

    let claimed = match configured_server_port_at(dir) {
        Some(existing) => existing == port,
        None => atomic_write_server_port_at(dir, port),
    };
    #[cfg(unix)]
    unsafe {
        libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lock), libc::LOCK_UN);
    }
    claimed
}

fn restore_server_port_at(dir: &std::path::Path, port: u16) -> bool {
    if port == 0 || std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let lock_path = dir.join("server-port.lock");
    let Ok(lock) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
    else {
        return false;
    };
    #[cfg(unix)]
    if unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lock), libc::LOCK_EX) } != 0 {
        return false;
    }
    let restored =
        read_port(&dir.join("server-port")) == Some(port) || atomic_write_server_port_at(dir, port);
    #[cfg(unix)]
    unsafe {
        libc::flock(std::os::fd::AsRawFd::as_raw_fd(&lock), libc::LOCK_UN);
    }
    restored
}

pub(crate) fn canonical_server_port() -> Option<u16> {
    read_port(&mobile_dir().join("server-port"))
}

pub(crate) fn configured_server_port() -> Option<u16> {
    configured_server_port_at(&mobile_dir())
}

pub(crate) fn local_endpoint_is_listening(port: u16) -> bool {
    port > 0
        && TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(100),
        )
        .is_ok()
}

/// Bind the shared IPv4 endpoint with the same pre-bind reuse policy as the
/// native Swift server. This is required for an immediate pair→TUI or
/// app→TUI handoff after an accepted socket enters TIME_WAIT.
fn bind_reusable_ipv4_listener(port: u16) -> Option<TcpListener> {
    unsafe {
        let fd = libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0);
        if fd < 0 {
            return None;
        }
        let close_on_error = || {
            libc::close(fd);
            None
        };
        let enabled: libc::c_int = 1;
        if libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            &enabled as *const _ as *const libc::c_void,
            std::mem::size_of_val(&enabled) as libc::socklen_t,
        ) != 0
        {
            return close_on_error();
        }
        let descriptor_flags = libc::fcntl(fd, libc::F_GETFD);
        if descriptor_flags < 0
            || libc::fcntl(fd, libc::F_SETFD, descriptor_flags | libc::FD_CLOEXEC) != 0
        {
            return close_on_error();
        }
        let mut address: libc::sockaddr_in = std::mem::zeroed();
        #[cfg(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        ))]
        {
            address.sin_len = std::mem::size_of::<libc::sockaddr_in>() as u8;
        }
        address.sin_family = libc::AF_INET as libc::sa_family_t;
        address.sin_port = port.to_be();
        address.sin_addr = libc::in_addr {
            s_addr: libc::INADDR_ANY,
        };
        if libc::bind(
            fd,
            &address as *const _ as *const libc::sockaddr,
            std::mem::size_of_val(&address) as libc::socklen_t,
        ) != 0
            || libc::listen(fd, 128) != 0
        {
            return close_on_error();
        }
        Some(TcpListener::from_raw_fd(fd))
    }
}

fn bind_mobile_listener(
    handoff: Option<u16>,
    persisted: Option<u16>,
    headless_persisted: Option<u16>,
) -> Option<TcpListener> {
    if let Some(port) = handoff.or(persisted).or(headless_persisted) {
        // Exact or nothing: paired Controllers persist this endpoint and do
        // not trust Bonjour to adopt a replacement. EADDRINUSE means another
        // frontend still owns serving; the TUI retries after it releases the
        // socket instead of becoming an unreachable second owner.
        return bind_reusable_ipv4_listener(port);
    }
    bind_reusable_ipv4_listener(0)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn mobile_dir() -> std::path::PathBuf {
    app_paths::unpeel_home().join("mobile")
}

fn sha256_hex(token: &str) -> String {
    // Minimal SHA-256 (FIPS 180-4). Local, dependency-free; auth compares
    // against lowercase-hex tokenHash values in devices.json.
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bytes = token.as_bytes();
    let bit_len = (bytes.len() as u64) * 8;
    let mut message = bytes.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for block in message.chunks(64) {
        let mut w = [0u32; 64];
        for (i, chunk) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}

pub fn paired_device_count() -> usize {
    std::fs::read(mobile_dir().join("devices.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get("devices").and_then(|d| d.as_array()).map(|a| a.len()))
        .unwrap_or(0)
}

fn bearer_token(header: &str) -> Option<&str> {
    let (scheme, token) = header.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

pub(crate) fn principal_for_bearer(
    headers: &HashMap<String, String>,
) -> Option<ControllerPrincipal> {
    let token = bearer_token(headers.get("authorization")?)?;
    let hash = sha256_hex(token);
    std::fs::read(mobile_dir().join("devices.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get("devices").cloned())
        .and_then(|d| d.as_array().cloned())
        .and_then(|devices| {
            devices.iter().find_map(|device| {
                if device.get("tokenHash").and_then(|value| value.as_str()) != Some(hash.as_str()) {
                    return None;
                }
                Some(ControllerPrincipal::PairedDevice {
                    device_id: device.get("id")?.as_str()?.to_owned(),
                    name: device
                        .get("name")
                        .and_then(|value| value.as_str())
                        .unwrap_or("device")
                        .to_owned(),
                })
            })
        })
}

/// ANSI/UTF-8 boundary scan (port of `align_tail_start_in_window`): returns
/// the last safe boundary at or before `data.len()`, scanning from 0.
fn last_safe_boundary(data: &[u8]) -> usize {
    #[derive(PartialEq)]
    enum S {
        Ground,
        Esc,
        Csi,
        Osc,
        OscEsc,
    }
    let mut state = S::Ground;
    let mut boundary = 0usize;
    let mut i = 0usize;
    while i < data.len() {
        let b = data[i];
        match state {
            S::Ground => match b {
                0x1b => state = S::Esc,
                0x80..=0xbf => {} // utf-8 continuation: not a boundary start
                _ => {
                    // boundary after complete utf-8 scalar
                    let len = if b < 0x80 {
                        1
                    } else if b >= 0xf0 {
                        4
                    } else if b >= 0xe0 {
                        3
                    } else {
                        2
                    };
                    if i + len <= data.len() {
                        i += len;
                        boundary = i;
                        continue;
                    } else {
                        break;
                    }
                }
            },
            S::Esc => match b {
                b'[' => state = S::Csi,
                b']' | b'P' | b'X' | b'^' | b'_' => state = S::Osc,
                _ => {
                    state = S::Ground;
                    boundary = i + 1;
                }
            },
            S::Csi => {
                if (0x40..=0x7e).contains(&b) {
                    state = S::Ground;
                    boundary = i + 1;
                }
            }
            S::Osc => match b {
                0x07 => {
                    state = S::Ground;
                    boundary = i + 1;
                }
                0x1b => state = S::OscEsc,
                _ => {}
            },
            S::OscEsc => {
                state = if b == b'\\' {
                    boundary = i + 1;
                    S::Ground
                } else {
                    S::Osc
                };
            }
        }
        i += 1;
    }
    boundary
}

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub(crate) struct Request {
    pub(crate) request_id: Option<String>,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) query: HashMap<String, String>,
    pub(crate) headers: HashMap<String, String>,
    pub(crate) body: Vec<u8>,
    pub(crate) keep_alive: bool,
}

fn read_request(stream: &mut TcpStream, pending: &mut Vec<u8>) -> Option<Request> {
    let header_end = loop {
        if let Some(pos) = pending.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        if pending.len() > 8 * 1024 * 1024 {
            return None;
        }
        let mut chunk = [0u8; 16 * 1024];
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(n) => pending.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&pending[..header_end]).into_owned();
    let mut lines = head.lines();
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let version = parts.next().unwrap_or("HTTP/1.1").to_string();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_lowercase(), value.trim().to_string());
        }
    }
    let content_length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if content_length > MAX_BODY {
        return None;
    }
    while pending.len() < header_end + content_length {
        let mut chunk = [0u8; 16 * 1024];
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(n) => pending.extend_from_slice(&chunk[..n]),
        }
    }
    let body = pending[header_end..header_end + content_length].to_vec();
    pending.drain(..header_end + content_length);

    let (path, query_string) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };
    let mut query = HashMap::new();
    for pair in query_string.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        query.insert(k.to_string(), urldecode(v));
    }
    let connection = headers.get("connection").map(|s| s.to_lowercase());
    let keep_alive = match connection.as_deref() {
        Some("close") => false,
        Some(v) if v.contains("keep-alive") => true,
        _ => version == "HTTP/1.1",
    };
    let request_id = headers
        .get("x-unpeel-request-id")
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .cloned();
    Some(Request {
        request_id,
        method,
        path,
        query,
        headers,
        body,
        keep_alive,
    })
}

fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn respond(stream: &mut TcpStream, status: u16, body: &str, keep_alive: bool) -> bool {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        504 => "Gateway Timeout",
        _ => "Error",
    };
    let connection = if keep_alive { "keep-alive" } else { "close" };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: {connection}\r\n\r\n{body}",
        body.len()
    )
    .and_then(|_| stream.flush())
    .is_ok()
}

fn error_body(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

fn safe_session_id(id: &str) -> bool {
    !id.is_empty() && !id.contains('/') && !id.contains('\\') && !id.contains("..")
}

fn session_dir(id: &str) -> std::path::PathBuf {
    app_paths::app_sessions_root().join(id)
}

/// Live `__remote__` advertisement (port + TLS fingerprint) when that server
/// runs — the app or a future TUI supervisor spawns it; we just relay state.
pub(crate) fn remote_server_advertisement() -> (Option<u64>, Option<String>) {
    let Ok(raw) = std::fs::read(app_paths::unpeel_home().join("remote.json")) else {
        return (None, None);
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&raw) else {
        return (None, None);
    };
    let pid = value.get("pid").and_then(|v| v.as_u64());
    let alive = pid.is_some_and(|p| unsafe { libc::kill(p as libc::pid_t, 0) == 0 });
    if !alive {
        return (None, None);
    }
    (
        value.get("port").and_then(|v| v.as_u64()),
        value
            .get("fingerprint")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
    )
}

fn handle_output(request: &Request) -> (u16, String) {
    let Some(session_id) = request
        .query
        .get("session_id")
        .or_else(|| request.query.get("sessionID"))
        .filter(|s| safe_session_id(s))
    else {
        return (400, error_body("invalid session id"));
    };
    let path = session_dir(session_id).join("output.bin");
    let limit = request
        .query
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(512 * 1024)
        .clamp(1, 8 * 1024 * 1024);
    let offset = request
        .query
        .get("offset")
        .and_then(|v| v.parse::<u64>().ok());
    let wait_ms = request
        .query
        .get("wait_ms")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
        .min(25_000);

    let mut size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if wait_ms > 0 {
        if let Some(offset) = offset {
            if offset == size {
                let deadline = Instant::now() + Duration::from_millis(wait_ms);
                while Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(20));
                    size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    if size > offset {
                        break;
                    }
                }
            }
        }
    }

    let chunk = match unpeel_core::session_host::read_output_chunk(
        session_id,
        offset,
        Some(limit as usize),
        Some(limit as usize),
    ) {
        Ok(chunk) => chunk,
        Err(error) => return (500, error_body(&error)),
    };
    let start = chunk.next_offset.saturating_sub(chunk.data.len() as u64);
    let truncated = offset.map_or(start > 0, |requested| requested != start);
    let mut data = chunk.data;
    if !truncated {
        let boundary = last_safe_boundary(&data);
        data.truncate(boundary);
    }
    let body = serde_json::json!({
        "sessionID": session_id,
        "offset": start,
        "nextOffset": start + data.len() as u64,
        "dataBase64": base64(&data),
        "truncated": truncated,
        "capturedAtUnixMs": now_ms(),
    });
    (200, body.to_string())
}

fn body_json(request: &Request) -> serde_json::Value {
    serde_json::from_slice(&request.body).unwrap_or(serde_json::Value::Null)
}

fn controller_body(request: &Request) -> (serde_json::Value, Option<String>) {
    if request.body.is_empty() {
        return (serde_json::Value::Null, None);
    }
    match serde_json::from_slice(&request.body) {
        Ok(value) => (value, None),
        Err(_) => (serde_json::Value::Null, Some(base64(&request.body))),
    }
}

fn body_session_id(body: &serde_json::Value) -> Option<String> {
    let session_id = body.get("sessionID")?.as_str()?.trim();
    safe_session_id(session_id).then(|| session_id.to_owned())
}

/// Resolve the headless create catalog from the published Host-owned snapshot.
/// Preset scope travels beside the public bootstrap DTO because that DTO does
/// not expose it. Controller-supplied paths never enter this catalog.
fn headless_create_context(
    snapshot: &SharedSnapshot,
    hook_port: Option<u16>,
) -> Option<HostCreateContext> {
    let (bootstrap, presets) = {
        let snapshot = snapshot.lock().ok()?;
        (snapshot.bootstrap.clone(), snapshot.create_presets.clone())
    };

    let projects = bootstrap
        .get("projects")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|project| {
            let id = project.get("id")?.as_str()?.to_owned();
            let path = project
                .get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let worktree_branch = project
                .get("worktreeBranch")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            Some(HostCreateProject {
                id,
                path: path.clone(),
                is_folder: project
                    .get("isGroup")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
                    || project
                        .get("isFolder")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                // A worktree project publishes its canonical checkout as its
                // own path. Treat the branch as the Host-owned discriminator;
                // a top-level project never accepts an arbitrary request path.
                worktree_path: worktree_branch.as_ref().map(|_| path),
                worktree_branch,
            })
        })
        .collect();
    let executor = Arc::new(move |resolved| {
        unpeel_core::controller_api::execute_headless_session_create(resolved, hook_port)
    });
    Some(HostCreateContext::new(projects, presets, executor))
}

fn headless_controller_effects(hook_port: Option<u16>) -> ControllerEffects {
    ControllerEffects::new(Arc::new(move |request| {
        unpeel_core::controller_api::execute_headless_session_action(request, hook_port)
    }))
}

// Keep the transport-owned inputs explicit at this adapter boundary: tests
// override only the effect executor, while production supplies the same
// snapshot/auth/resize components used by the LAN and Relay entry points.
#[allow(clippy::too_many_arguments)]
fn handle_with_effects(
    request: &Request,
    principal: &ControllerPrincipal,
    snapshot: &SharedSnapshot,
    mark_read: &Sender<String>,
    hook_port: Option<u16>,
    resizes: &MobileResizes,
    approvals: &Arc<crate::approvals::ApprovalHub>,
    controller_effects_override: Option<&ControllerEffects>,
) -> (u16, String) {
    let route_context = if request.method == "GET" && request.path == "/mobile/bootstrap" {
        let core = snapshot
            .lock()
            .ok()
            .map(|guard| guard.bootstrap.clone())
            .unwrap_or_default();
        let (port, fingerprint) = remote_server_advertisement();
        let mut context = HostBootstrapContext::headless(core);
        context.host_id = std::fs::read_to_string(mobile_dir().join("mac-id"))
            .map(|value| value.trim().to_owned())
            .ok();
        context.remote_server_port = port.and_then(|value| u16::try_from(value).ok());
        context.remote_server_certificate_fingerprint = fingerprint;
        context.pending_approvals = approvals.list_json();
        Some(HostRouteContext {
            bootstrap: Some(context),
            archived_sessions_by_project: HashMap::new(),
        })
    } else if request.method == "GET" && request.path == "/mobile/archive" {
        snapshot.lock().ok().map(|guard| HostRouteContext {
            bootstrap: None,
            archived_sessions_by_project: guard.archived_sessions_by_project.clone(),
        })
    } else {
        None
    };
    let (body, body_base64) = controller_body(request);
    let controller_request = ControllerRequest {
        id: request.request_id.clone(),
        method: request.method.clone(),
        path: request.path.clone(),
        query: request.query.clone(),
        body,
        content_type: request.headers.get("content-type").cloned(),
        body_base64,
        principal: principal.clone(),
    };
    let create_context = (request.method == "POST" && request.path == "/mobile/sessions")
        .then(|| headless_create_context(snapshot, hook_port))
        .flatten();
    let owned_controller_effects = matches!(
        (request.method.as_str(), request.path.as_str()),
        ("POST", "/mobile/restart-session") | ("POST", "/mobile/session-action")
    )
    .then(|| headless_controller_effects(hook_port));
    let controller_effects = controller_effects_override.or(owned_controller_effects.as_ref());
    if let Some(response) = unpeel_core::controller_api::route_with_effects(
        &controller_request,
        route_context.as_ref(),
        create_context.as_ref(),
        controller_effects,
    ) {
        // The core owns the resize semantics; the TUI adapter retains only
        // its presentation-side ownership timer so it does not immediately
        // resize the shared PTY back to the local preview grid.
        if response.status == 200 && request.path == "/mobile/resize" {
            if let Some(session_id) = body_session_id(&controller_request.body) {
                if let Ok(mut guard) = resizes.lock() {
                    guard.insert(session_id, Instant::now());
                }
            }
        }
        if response.status == 200 && request.path == "/mobile/mark-read" {
            if let Some(session_id) = body_session_id(&controller_request.body) {
                let _ = mark_read.send(session_id);
            }
        }
        return (response.status, response.body_json());
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/mobile/output") => handle_output(request),
        ("POST", "/mobile/session-organization") => {
            let body = body_json(request);
            let Some(session_id) = body_session_id(&body) else {
                return (400, error_body("invalid session id"));
            };

            let title = match body.get("title") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(value)) => {
                    let value = value.trim();
                    (!value.is_empty()).then(|| value.to_owned())
                }
                Some(_) => return (400, error_body("title must be a string")),
            };
            let pinned = match body.get("pinned") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::Bool(value)) => Some(*value),
                Some(_) => return (400, error_body("pinned must be a boolean")),
            };
            let archived = match body.get("archived") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::Bool(value)) => Some(*value),
                Some(_) => return (400, error_body("archived must be a boolean")),
            };
            let notify_when_done = match body.get("notifyWhenDone") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::Bool(value)) => Some(*value),
                Some(_) => {
                    return (400, error_body("notifyWhenDone must be a boolean"));
                }
            };
            let published = snapshot
                .lock()
                .ok()
                .and_then(|snapshot| snapshot.bootstrap.get("sessions").cloned())
                .and_then(|sessions| sessions.as_array().cloned())
                .is_some_and(|sessions| {
                    sessions.iter().any(|session| {
                        session.get("id").and_then(serde_json::Value::as_str)
                            == Some(session_id.as_str())
                    })
                });
            if !published && unpeel_core::session_host::load_manifest(&session_id).is_none() {
                return (404, error_body("unknown session"));
            }
            // Headless Hosts have no push registration/delivery pipeline yet.
            // Match native resource ordering: type-check, resolve the Session,
            // then reject the unsupported platform field before applying
            // anything. A compound patch can never half-apply behind a 400,
            // 404, or 501 response.
            if notify_when_done.is_some() {
                return (
                    501,
                    error_body("notifyWhenDone is not supported by this Host"),
                );
            }
            if title.is_none() && pinned.is_none() && archived.is_none() {
                // Match the shipped native DTO: an empty patch, explicit
                // nulls, and a title that trims to empty are successful no-ops.
                return (200, r#"{"ok":true}"#.into());
            }

            // This v1 route spans app-state.json, title.json, and the archive
            // marker/control socket, so it cannot be a cross-file transaction.
            // Put the only fallible shared-state precondition first: if pin
            // persistence fails, title/archive are untouched. Once a pin or
            // title lands, any later failure is effect-unknown; Controllers
            // must refresh Host state before deciding whether to retry and
            // must not manufacture a fresh request id blindly.
            if let Some(pinned) = pinned {
                if let Err(e) = unpeel_core::session_ops::set_pinned(&session_id, pinned) {
                    return (
                        500,
                        error_body(&format!("organization pin preflight failed: {e}")),
                    );
                }
            }
            if let Some(title) = title {
                if let Err(e) = unpeel_core::session_ops::set_title(&session_id, &title) {
                    return (
                        500,
                        error_body(&format!(
                            "organization update effect unknown; refresh Host state: {e}"
                        )),
                    );
                }
            }
            match archived {
                Some(true) => {
                    if let Err(e) = unpeel_core::session_ops::archive_session(&session_id) {
                        return (
                            500,
                            error_body(&format!(
                                "organization update effect unknown; refresh Host state: {e}"
                            )),
                        );
                    }
                }
                Some(false) => {
                    if let Err(e) = unpeel_core::session_ops::restore_session(&session_id) {
                        return (
                            500,
                            error_body(&format!(
                                "organization update effect unknown; refresh Host state: {e}"
                            )),
                        );
                    }
                }
                None => {}
            }
            (200, r#"{"ok":true}"#.into())
        }
        ("POST", "/mobile/project-organization") => {
            handle_project_organization(&body_json(request), snapshot)
        }
        ("POST", "/mobile/resize-desktop") => {
            // This is the phone's FIT verb: on the desktop it letterboxes
            // the surface AND resizes the PTY to the phone grid. App-less
            // the letterbox half doesn't exist (the TUI letterboxes its
            // preview on its own), but the PTY resize is the part that makes
            // the terminal fit the phone — do it for real.
            let body = body_json(request);
            let Some(session_id) = body_session_id(&body) else {
                return (400, error_body("invalid session id"));
            };
            if body.get("clear").and_then(|v| v.as_bool()) == Some(true) {
                if let Ok(mut guard) = resizes.lock() {
                    guard.remove(&session_id);
                }
                return (200, r#"{"ok":true}"#.into());
            }
            let cols = body
                .get("columns")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                .clamp(2, 300);
            let rows = body
                .get("rows")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                .clamp(2, 120);
            match crate::control::send_resize(&session_dir(&session_id), cols as u16, rows as u16) {
                Ok(()) => {
                    if let Ok(mut guard) = resizes.lock() {
                        guard.insert(session_id, Instant::now());
                    }
                    (200, r#"{"ok":true}"#.into())
                }
                Err(_) => (404, error_body("session host unavailable")),
            }
        }
        ("POST", "/mobile/approvals/answer") => {
            let body = body_json(request);
            let (Some(id), Some(approved)) = (
                body.get("id").and_then(|v| v.as_str()),
                body.get("approved").and_then(|v| v.as_bool()),
            ) else {
                return (400, error_body("request failed"));
            };
            if approvals.answer(id, approved) {
                (200, r#"{"ok":true}"#.into())
            } else {
                (409, error_body("approval no longer pending"))
            }
        }
        _ => (404, error_body("not found")),
    }
}

/// POST /mobile/project-organization (capability `project.organization.set`):
/// shared disk-backed semantics live in
/// `unpeel_core::controller_host::project_organization_response` (the SSH
/// gateway serves the same function), resolved against the published
/// bootstrap — display-ordered, so sibling indices mean exactly what the
/// Controller saw. The TUI adapter adds the one platform capability a bare
/// gateway lacks: folder colors, written into the desktop app's UserDefaults
/// on macOS with a state-bus ping so every frontend re-reads.
fn handle_project_organization(
    body: &serde_json::Value,
    snapshot: &SharedSnapshot,
) -> (u16, String) {
    let projects: Vec<serde_json::Value> = snapshot
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .bootstrap
                .get("projects")
                .and_then(serde_json::Value::as_array)
                .cloned()
        })
        .unwrap_or_default();
    let write_color = |project_id: &str, color: Option<&str>| -> Result<(), String> {
        crate::overlay::write_project_folder_color(project_id, color)?;
        // Colors live in UserDefaults, outside the app-state choke point's
        // own announce — ping peers explicitly, like the local color menu.
        unpeel_core::state_bus::announce(
            unpeel_core::state_bus::Change::AppState,
            unpeel_core::session_ops::own_listener_port_public(),
        );
        Ok(())
    };
    let color_writer: Option<ProjectColorWriter<'_>> =
        if crate::overlay::project_folder_color_supported() {
            Some(&write_color)
        } else {
            None
        };
    let (status, body) =
        unpeel_core::controller_host::project_organization_response(body, &projects, color_writer);
    (status, body.to_string())
}

/// The authenticated adapter boundary shared by LAN and Relay transports.
/// Keep method/path semantics here so every transport — and the conformance
/// harness — observes the same response rather than bypassing route guards.
#[allow(clippy::too_many_arguments)]
fn handle_authenticated_with_effects(
    request: &Request,
    principal: &ControllerPrincipal,
    snapshot: &SharedSnapshot,
    mark_read: &Sender<String>,
    hook_port: Option<u16>,
    resizes: &MobileResizes,
    approvals: &Arc<crate::approvals::ApprovalHub>,
    controller_effects_override: Option<&ControllerEffects>,
) -> (u16, String) {
    if !request.path.starts_with("/mobile/") || request.path == "/mobile/pair" {
        return (404, error_body("not found"));
    }
    if request.method != "GET" && request.method != "POST" {
        return (405, error_body("method not allowed"));
    }
    handle_with_effects(
        request,
        principal,
        snapshot,
        mark_read,
        hook_port,
        resizes,
        approvals,
        controller_effects_override,
    )
}

pub(crate) fn handle_authenticated(
    request: &Request,
    principal: &ControllerPrincipal,
    snapshot: &SharedSnapshot,
    mark_read: &Sender<String>,
    hook_port: Option<u16>,
    resizes: &MobileResizes,
    approvals: &Arc<crate::approvals::ApprovalHub>,
) -> (u16, String) {
    handle_authenticated_with_effects(
        request, principal, snapshot, mark_read, hook_port, resizes, approvals, None,
    )
}

#[allow(clippy::too_many_arguments)]
fn handle_connection(
    mut stream: TcpStream,
    snapshot: SharedSnapshot,
    mark_read: Sender<String>,
    hook_port: Option<u16>,
    resizes: MobileResizes,
    approvals: Arc<crate::approvals::ApprovalHub>,
    pairing: Arc<crate::pairing::PairingWindow>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    let mut pending = Vec::new();
    for _ in 0..1000 {
        if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let Some(request) = read_request(&mut stream, &mut pending) else {
            return;
        };
        let keep = request.keep_alive;
        if !request.path.starts_with("/mobile/") {
            respond(&mut stream, 404, &error_body("not found"), keep);
        } else if request.path == "/mobile/pair" {
            if request.method != "POST" {
                respond(&mut stream, 405, &error_body("method not allowed"), false);
            } else {
                let mac_id = std::fs::read_to_string(mobile_dir().join("mac-id"))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                let (status, body) = pairing.handle_pair(&request.body, &mac_id, &hostname());
                // Pairing is a one-shot exchange. Force-close even when the
                // URLSession client requested HTTP/1.1 keep-alive so a
                // `pair --serve` handoff never retains this listener's port.
                let sent = respond(&mut stream, status, &body, false);
                if status == 200 {
                    pairing.finish_response(sent);
                }
            }
            return;
        } else if request.method != "GET" && request.method != "POST" {
            respond(&mut stream, 405, &error_body("method not allowed"), keep);
        } else {
            match principal_for_bearer(&request.headers) {
                None => {
                    respond(&mut stream, 401, &error_body("unauthorized"), keep);
                }
                Some(principal) => {
                    let (status, body) = handle_authenticated(
                        &request, &principal, &snapshot, &mark_read, hook_port, &resizes,
                        &approvals,
                    );
                    respond(&mut stream, status, &body, keep);
                }
            }
        }
        if !keep {
            return;
        }
    }
}

/// The LAN address a phone can reach us on: first AF_INET interface that
/// isn't loopback/tunnel/awdl/bridge (mirrors `preferredLANAddress`).
pub fn preferred_lan_address() -> String {
    if let Ok(address) = std::env::var("UNPEEL_TEST_LAN_ADDRESS") {
        if address.parse::<std::net::Ipv4Addr>().is_ok() {
            return address;
        }
    }
    // Portable getifaddrs walk (no `ipconfig` shellout, so this works on
    // Linux hosts too): first running AF_INET interface that isn't loopback
    // or a tunnel/virtual device — same selection rule as the app.
    const SKIP: [&str; 7] = ["lo", "utun", "awdl", "llw", "bridge", "docker", "veth"];
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        return "127.0.0.1".into();
    }
    let mut best = None;
    let mut current = list;
    while !current.is_null() {
        let entry = unsafe { &*current };
        current = entry.ifa_next;
        if entry.ifa_addr.is_null() || entry.ifa_name.is_null() {
            continue;
        }
        let addr = unsafe { &*entry.ifa_addr };
        if addr.sa_family as i32 != libc::AF_INET {
            continue;
        }
        let flags = entry.ifa_flags as i32;
        if flags & libc::IFF_UP == 0 || flags & libc::IFF_RUNNING == 0 {
            continue;
        }
        let name = unsafe { std::ffi::CStr::from_ptr(entry.ifa_name) }
            .to_string_lossy()
            .into_owned();
        if SKIP.iter().any(|prefix| name.starts_with(prefix)) {
            continue;
        }
        let sockaddr: &libc::sockaddr_in =
            unsafe { &*(entry.ifa_addr as *const libc::sockaddr_in) };
        let octets = u32::from_be(sockaddr.sin_addr.s_addr).to_be_bytes();
        if octets[0] == 127 {
            continue;
        }
        best = Some(format!(
            "{}.{}.{}.{}",
            octets[0], octets[1], octets[2], octets[3]
        ));
        break;
    }
    unsafe { libc::freeifaddrs(list) };
    best.unwrap_or_else(|| "127.0.0.1".into())
}

/// Claim the configured endpoint and start its Bonjour advertisement. A known
/// occupied endpoint returns None so the caller can retry the same address;
/// an OS-assigned port is used only for a first run with no configured port,
/// then published atomically before the listener becomes authoritative.
pub fn start(
    snapshot: SharedSnapshot,
    mark_read: Sender<String>,
    hook_port: Option<u16>,
    resizes: MobileResizes,
    approvals: Arc<crate::approvals::ApprovalHub>,
    pairing: Arc<crate::pairing::PairingWindow>,
) -> Option<MobileServer> {
    let dir = mobile_dir();
    let persisted = read_port(&dir.join("server-port"));
    let headless_persisted = read_port(&dir.join("headless-server-port"));
    let previous_tui_owner = read_port(&dir.join("tui-server-port"));
    let handoff = next_start_port().lock().ok().and_then(|guard| *guard);
    // The persisted endpoint is the cross-process ownership claim. A paired
    // Controller will not trust Bonjour to adopt a different plaintext URL,
    // so an occupied known port is retryable ownership loss, never permission
    // to start a second server elsewhere. If no usable endpoint exists, the
    // first TUI binds an OS port and atomically publishes it; concurrent TUIs
    // close their losing listeners and retry the winner's exact endpoint.
    // A released native app can rewrite canonical A→fallback B while a TUI
    // still owns the phone's saved Direct endpoint A. Remembering the active
    // TUI lease prevents a second TUI from claiming B and making that stale
    // rewrite self-sustaining. If the original TUI crashed, its listener is
    // gone and the next TUI reclaims A, then repairs the canonical file.
    let listener = bind_mobile_listener(
        handoff,
        previous_tui_owner.or(persisted),
        headless_persisted,
    )?;
    let port = listener.local_addr().ok()?.port();
    if handoff.is_none()
        && persisted.is_none()
        && headless_persisted.is_none()
        && !claim_initial_server_port_at(&dir, port)
    {
        return None;
    }
    if handoff.is_none()
        && previous_tui_owner == Some(port)
        && persisted != Some(port)
        && !restore_server_port_at(&dir, port)
    {
        return None;
    }
    if !publish_tui_owner_port_at(&dir, port) {
        return None;
    }
    if handoff == Some(port) {
        if let Ok(mut guard) = next_start_port().lock() {
            if *guard == Some(port) {
                *guard = None;
            }
        }
    }
    // `stop()` joins the accept loop. Never start it in blocking mode or a
    // rare fcntl failure could make shutdown wait forever with no incoming
    // connection to wake `accept`.
    if listener.set_nonblocking(true).is_err() {
        return None;
    }

    // Bonjour: same service/TXT contract as the app. macOS ships `dns-sd`;
    // Linux hosts use avahi's `avahi-publish-service` when present. Neither
    // is required — the phone's saved endpoint still works, and rediscovery
    // only needs this after an address change.
    let mac_id = std::fs::read_to_string(mobile_dir().join("mac-id"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let name = hostname();
    let bonjour_child = std::process::Command::new("dns-sd")
        .args([
            "-R",
            &name,
            "_unpeel-remote._tcp",
            ".",
            &port.to_string(),
            &format!("macid={mac_id}"),
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()
        .or_else(|| {
            std::process::Command::new("avahi-publish-service")
                .args([
                    &name,
                    "_unpeel-remote._tcp",
                    &port.to_string(),
                    &format!("macid={mac_id}"),
                ])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .ok()
        });
    let bonjour = Arc::new(Mutex::new(bonjour_child));
    // The WSS terminal server: standalone, verifies paired-device tokens
    // itself, writes its port + TLS fingerprint into ~/.unpeel/remote.json —
    // which /mobile/bootstrap relays so the phone gets its full terminal
    // (control bar, resize, live stream) instead of the long-poll fallback.
    let remote_child = unpeel_core::session_ops::resolve_host_binary()
        .ok()
        .and_then(|bin| {
            std::process::Command::new(bin)
                .arg("__remote__")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .ok()
        });
    let remote = Arc::new(Mutex::new(remote_child));
    let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let active_connections = Arc::new(Mutex::new(HashMap::<u64, TcpStream>::new()));
    let worker_threads = Arc::new(Mutex::new(Vec::<std::thread::JoinHandle<()>>::new()));

    let accept_shutdown = Arc::clone(&shutdown);
    let accept_connections = Arc::clone(&active_connections);
    let accept_workers = Arc::clone(&worker_threads);
    let accept_thread = std::thread::spawn(move || loop {
        if accept_shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            return; // listener drops here, releasing the port
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let snapshot = Arc::clone(&snapshot);
                let mark_read = mark_read.clone();
                let resizes = Arc::clone(&resizes);
                let approvals = Arc::clone(&approvals);
                let pairing = Arc::clone(&pairing);
                let connection_shutdown = Arc::clone(&accept_shutdown);
                let connections = Arc::clone(&accept_connections);
                let connection_id = {
                    static NEXT_CONNECTION_ID: std::sync::atomic::AtomicU64 =
                        std::sync::atomic::AtomicU64::new(1);
                    NEXT_CONNECTION_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                };
                if let Ok(control) = stream.try_clone() {
                    if let Ok(mut guard) = connections.lock() {
                        guard.insert(connection_id, control);
                    }
                }
                let worker = std::thread::spawn(move || {
                    handle_connection(
                        stream,
                        snapshot,
                        mark_read,
                        hook_port,
                        resizes,
                        approvals,
                        pairing,
                        connection_shutdown,
                    );
                    if let Ok(mut guard) = connections.lock() {
                        guard.remove(&connection_id);
                    }
                });
                if let Ok(mut workers) = accept_workers.lock() {
                    let mut index = 0;
                    while index < workers.len() {
                        if workers[index].is_finished() {
                            let finished = workers.swap_remove(index);
                            let _ = finished.join();
                        } else {
                            index += 1;
                        }
                    }
                    workers.push(worker);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return,
        }
    });
    Some(MobileServer {
        port,
        shutdown,
        bonjour,
        remote,
        accept_thread: Mutex::new(Some(accept_thread)),
        active_connections,
        worker_threads,
    })
}

pub(crate) fn hostname() -> String {
    let mut buffer = [0u8; 256];
    let rc = unsafe { libc::gethostname(buffer.as_mut_ptr() as *mut libc::c_char, buffer.len()) };
    if rc == 0 {
        let name = buffer.split(|&b| b == 0).next().unwrap_or(&[]);
        let name = String::from_utf8_lossy(name).into_owned();
        name.trim_end_matches(".local").to_string()
    } else {
        "Mac".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("unpeel-mobile-{label}-{}", uuid::Uuid::new_v4()))
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ConformanceFixture {
        schema_version: u16,
        cases: Vec<ConformanceCase>,
    }

    #[derive(serde::Deserialize)]
    struct ConformanceCase {
        id: String,
        method: String,
        path: String,
        #[serde(default)]
        query: HashMap<String, String>,
        #[serde(default)]
        body: serde_json::Value,
        expected: ConformanceExpected,
    }

    #[derive(serde::Deserialize)]
    struct ConformanceExpected {
        tui: u16,
    }

    #[test]
    fn sha256_known_answer() {
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn bearer_parser_rejects_malformed_unicode_without_slicing() {
        assert_eq!(bearer_token("Bearer token"), Some("token"));
        assert_eq!(bearer_token("bearer   token"), Some("token"));
        assert_eq!(bearer_token("Bærer token"), None);
        assert_eq!(bearer_token("Bearer "), None);
    }

    #[test]
    fn paired_port_preserves_existing_native_canonical_endpoint() {
        let dir = scratch_dir("canonical-port");
        std::fs::create_dir_all(&dir).unwrap();
        let original = b"41234\n";
        std::fs::write(dir.join("server-port"), original).unwrap();

        persist_paired_port_at(&dir, 42345);

        assert_eq!(std::fs::read(dir.join("server-port")).unwrap(), original);
        assert_eq!(
            std::fs::read_to_string(dir.join("headless-server-port")).unwrap(),
            "42345\n"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn persisted_headless_port_rebinds_after_process_style_restart() {
        let dir = scratch_dir("headless-restart");
        let probe = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        persist_paired_port_at(&dir, port);
        // Simulate an older/separate headless installation with no native
        // canonical file: the headless record alone must restore the endpoint.
        std::fs::remove_file(dir.join("server-port")).unwrap();

        let restored = read_port(&dir.join("headless-server-port"));
        let rebound = bind_mobile_listener(None, None, restored).unwrap();

        assert_eq!(configured_server_port_at(&dir), Some(port));
        assert_eq!(rebound.local_addr().unwrap().port(), port);
        drop(rebound);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn occupied_exact_handoff_fails_closed() {
        let occupied = TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let port = occupied.local_addr().unwrap().port();

        assert!(bind_mobile_listener(Some(port), None, None).is_none());
    }

    #[test]
    fn occupied_persisted_endpoint_waits_for_the_same_port() {
        let occupied = TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let port = occupied.local_addr().unwrap().port();

        assert!(bind_mobile_listener(None, Some(port), None).is_none());
        drop(occupied);

        let claimed = bind_mobile_listener(None, Some(port), None)
            .expect("the released persisted endpoint becomes claimable");
        assert_eq!(claimed.local_addr().unwrap().port(), port);
    }

    #[test]
    fn active_tui_owner_blocks_a_stale_legacy_fallback_claim() {
        let dir = scratch_dir("tui-owner-port");
        std::fs::create_dir_all(&dir).unwrap();
        let direct = TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let direct_port = direct.local_addr().unwrap().port();
        let fallback_probe = TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let fallback_port = fallback_probe.local_addr().unwrap().port();
        drop(fallback_probe);
        std::fs::write(dir.join("server-port"), format!("{fallback_port}\n")).unwrap();
        assert!(publish_tui_owner_port_at(&dir, direct_port));

        let canonical = read_port(&dir.join("server-port"));
        let owner = read_port(&dir.join("tui-server-port"));
        assert!(bind_mobile_listener(None, owner.or(canonical), None).is_none());
        let fallback_stays_free = TcpListener::bind(("0.0.0.0", fallback_port)).unwrap();
        drop(fallback_stays_free);

        drop(direct);
        let reclaimed = bind_mobile_listener(None, owner.or(canonical), None).unwrap();
        assert_eq!(reclaimed.local_addr().unwrap().port(), direct_port);
        assert!(restore_server_port_at(&dir, direct_port));
        assert_eq!(read_port(&dir.join("server-port")), Some(direct_port));
        clear_tui_owner_port_at(&dir, direct_port);
        assert!(!dir.join("tui-server-port").exists());
        drop(reclaimed);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stopping_an_old_tui_cannot_clear_a_newer_owner() {
        let dir = scratch_dir("tui-owner-compare-delete");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(publish_tui_owner_port_at(&dir, 41_001));
        assert!(publish_tui_owner_port_at(&dir, 41_002));

        clear_tui_owner_port_at(&dir, 41_001);

        assert_eq!(read_port(&dir.join("tui-server-port")), Some(41_002));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn first_dynamic_endpoint_has_one_durable_winner() {
        let dir = scratch_dir("initial-port-race");
        std::fs::create_dir_all(&dir).unwrap();
        let first = bind_mobile_listener(None, None, None).unwrap();
        let second = bind_mobile_listener(None, None, None).unwrap();
        let first_port = first.local_addr().unwrap().port();
        let second_port = second.local_addr().unwrap().port();
        assert_ne!(first_port, second_port);

        assert!(claim_initial_server_port_at(&dir, first_port));
        assert!(!claim_initial_server_port_at(&dir, second_port));
        assert_eq!(configured_server_port_at(&dir), Some(first_port));
        assert_eq!(
            std::fs::read_to_string(dir.join("server-port")).unwrap(),
            format!("{first_port}\n")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_initial_endpoint_is_atomically_replaced() {
        let dir = scratch_dir("corrupt-initial-port");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("server-port"), b"not-a-port\n").unwrap();
        let listener = bind_mobile_listener(None, None, None).unwrap();
        let port = listener.local_addr().unwrap().port();

        assert!(claim_initial_server_port_at(&dir, port));
        assert_eq!(configured_server_port_at(&dir), Some(port));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn legacy_native_random_fallback_cannot_replace_owned_direct_endpoint() {
        let dir = scratch_dir("legacy-native-port-repair");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("server-port"), b"41234\n").unwrap();

        assert!(restore_server_port_at(&dir, 42345));
        assert_eq!(read_port(&dir.join("server-port")), Some(42345));
        assert_eq!(
            std::fs::read_to_string(dir.join("server-port")).unwrap(),
            "42345\n"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pairing_route_closes_keep_alive_socket_before_exact_rebind() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handler = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let snapshot = Arc::new(Mutex::new(crate::sessions::MobileSnapshot::default()));
            let (mark_read, _receiver) = std::sync::mpsc::channel();
            handle_connection(
                stream,
                snapshot,
                mark_read,
                None,
                Arc::new(Mutex::new(HashMap::new())),
                Arc::new(crate::approvals::ApprovalHub::default()),
                Arc::new(crate::pairing::PairingWindow::default()),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        });
        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        client
            .write_all(
                b"GET /mobile/pair HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n",
            )
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        handler.join().unwrap();

        let response = String::from_utf8(response).unwrap();
        assert!(response.contains("HTTP/1.1 405"));
        assert!(response.contains("Connection: close"));
        let rebound = bind_mobile_listener(Some(port), None, None)
            .expect("the one-shot pairing worker released the exact endpoint");
        assert_eq!(rebound.local_addr().unwrap().port(), port);
    }

    #[test]
    fn stop_joins_accept_loop_before_exact_rebind() {
        let listener = TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let accept_shutdown = Arc::clone(&shutdown);
        let accept_thread = std::thread::spawn(move || loop {
            if accept_shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            match listener.accept() {
                Ok(_) => {}
                Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => return,
            }
        });
        let server = MobileServer {
            port,
            shutdown,
            bonjour: Arc::new(Mutex::new(None)),
            remote: Arc::new(Mutex::new(None)),
            accept_thread: Mutex::new(Some(accept_thread)),
            active_connections: Arc::new(Mutex::new(HashMap::new())),
            worker_threads: Arc::new(Mutex::new(Vec::new())),
        };

        server.stop();
        let rebound = bind_mobile_listener(Some(port), None, None)
            .expect("stop returned only after the listener released its exact port");
        assert_eq!(rebound.local_addr().unwrap().port(), port);
    }

    #[test]
    fn base64_known_answer() {
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b""), "");
    }

    #[test]
    fn boundary_withholds_partial_escape() {
        assert_eq!(last_safe_boundary(b"hello \x1b[31m red"), 15);
        assert_eq!(last_safe_boundary(b"hello \x1b[3"), 6);
        assert_eq!(last_safe_boundary(b"plain"), 5);
        // partial utf-8 tail withheld
        assert_eq!(last_safe_boundary(&[b'a', 0xe2, 0x82]), 1);
    }

    #[test]
    fn headless_adapter_runs_the_shared_conformance_fixture() {
        let fixture: ConformanceFixture =
            serde_json::from_str(include_str!("../../../protocol/host-conformance-v1.json"))
                .expect("valid host conformance fixture");
        assert_eq!(fixture.schema_version, 1);

        let snapshot = Arc::new(Mutex::new(crate::sessions::MobileSnapshot {
            bootstrap: serde_json::json!({
                "folders": [],
                "projects": [{ "id": "conformance-project" }],
                "presets": [],
                "sessions": [
                    { "id": "conformance-session" },
                    { "id": "conformance-restart" },
                    { "id": "conformance-stop-live" },
                    { "id": "conformance-stop-exited" },
                    { "id": "conformance-action-restart" },
                    { "id": "conformance-action-restart-agent" },
                    { "id": "conformance-restart-agent-exited" },
                    { "id": "conformance-action-resume-agent" },
                    { "id": "conformance-resume-agent-exited" },
                    { "id": "conformance-remove" },
                ],
            }),
            archived_sessions_by_project: HashMap::from([(
                "conformance-project".into(),
                Vec::new(),
            )]),
            create_presets: Vec::new(),
        }));
        let (mark_read, _receiver) = std::sync::mpsc::channel();
        let resizes = Arc::new(Mutex::new(HashMap::new()));
        let approvals = Arc::new(crate::approvals::ApprovalHub::default());
        let principal = ControllerPrincipal::OwnerTransport {
            transport: "conformance".into(),
            subject: None,
        };
        let effects = ControllerEffects::new(Arc::new(|request| {
            use unpeel_core::controller_api::{ControllerEffectError, ControllerSessionAction};
            match (request.session_id.as_str(), request.action) {
                ("conformance-restart", ControllerSessionAction::Restart)
                | ("conformance-stop-live", ControllerSessionAction::Stop)
                | ("conformance-action-restart", ControllerSessionAction::Restart)
                | ("conformance-action-restart-agent", ControllerSessionAction::RestartAgent)
                | ("conformance-action-resume-agent", ControllerSessionAction::ResumeAgent)
                | ("conformance-remove", ControllerSessionAction::Remove) => Ok(()),
                ("conformance-stop-exited", ControllerSessionAction::Stop)
                | ("conformance-restart-agent-exited", ControllerSessionAction::RestartAgent)
                | ("conformance-resume-agent-exited", ControllerSessionAction::ResumeAgent) => {
                    Err(ControllerEffectError::SessionNotRunning)
                }
                ("conformance-broken", _) => Err(ControllerEffectError::Failed(
                    "conformance lifecycle failure".into(),
                )),
                ("conformance-unknown", _) => Err(ControllerEffectError::UnknownSession),
                _ => Err(ControllerEffectError::Failed(
                    "unexpected conformance lifecycle request".into(),
                )),
            }
        }));

        for case in fixture.cases {
            let request = Request {
                request_id: Some(format!("conformance-{}", case.id)),
                method: case.method,
                path: case.path,
                query: case.query,
                headers: HashMap::new(),
                body: if case.body.is_null() {
                    Vec::new()
                } else {
                    serde_json::to_vec(&case.body).expect("fixture body")
                },
                keep_alive: false,
            };
            let (status, body) = handle_authenticated_with_effects(
                &request,
                &principal,
                &snapshot,
                &mark_read,
                None,
                &resizes,
                &approvals,
                Some(&effects),
            );
            assert_eq!(status, case.expected.tui, "conformance case {}", case.id);
            if case.id == "bootstrap.valid" {
                let response: serde_json::Value =
                    serde_json::from_str(&body).expect("bootstrap response json");
                assert_eq!(
                    response.get("hostProtocol"),
                    Some(
                        &serde_json::to_value(
                            unpeel_core::controller_protocol::HostProtocolDescriptor::headless_v1()
                        )
                        .expect("descriptor json")
                    )
                );
            }
            if case.id == "archive.known-empty" {
                let response: serde_json::Value =
                    serde_json::from_str(&body).expect("archive response json");
                assert_eq!(
                    response,
                    serde_json::json!({
                        "projectID": "conformance-project",
                        "sessions": [],
                    })
                );
            }
        }
    }
}
