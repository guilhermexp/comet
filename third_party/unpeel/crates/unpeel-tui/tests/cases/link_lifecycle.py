"""Headless Link's release seam: key -> entitlement -> live relay lifecycle.

The lower-level relay conformance case starts with a hand-written entitlement;
this case proves a fresh terminal Host can actually acquire and use one while
its LAN server is already running, and that deactivation/rejection revoke only
Link without taking the LAN server down.
"""

import base64
import fcntl
import hashlib
import json
import os
import socket
import sys
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, HTTPServer

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import mobile_request, run  # noqa: E402


PUBLIC_KEY = "zt52Q3kzJSkUNuU8jrYCKlTDHycltUBp+siGzZ6ovDw="
LICENSE_KEY = (
    "CLRTY-eyJ2IjoxLCJpZCI6ImxpbmstdGVzdC1saWNlbnNlIiwiZW1haWwiOiJsaW5r"
    "LXRlc3RAZXhhbXBsZS5jb20iLCJwbGFuIjoicHJvIiwic2VhdHMiOjEsImlhdCI6"
    "MTc1NTEyOTYwMH0.iVrzCnPH8MSEvjIq1qVUnoQ7BLSCCd3AqVvwZ2IHfvTt6FHn"
    "l6Beo7aMKDW2AqbLb55_76YY3hMzftqFbd0kCQ"
)


class LicenseAPI:
    def __init__(self):
        self.requests = []
        self.reject_entitlement = False
        self.empty_rejection = False
        self.transient_entitlement = False
        self.block_activation = False
        self.block_entitlement = False
        self.activation_started = threading.Event()
        self.release_activation = threading.Event()
        self.entitlement_started = threading.Event()
        self.release_entitlement = threading.Event()
        self._lock = threading.Lock()
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):  # noqa: N802 - BaseHTTPRequestHandler API
                length = int(self.headers.get("content-length", "0"))
                try:
                    body = json.loads(self.rfile.read(length) or b"{}")
                except ValueError:
                    body = {}
                with owner._lock:
                    owner.requests.append((self.path, body))
                    entitlement_number = sum(
                        path == "/api/remote/entitlement"
                        for path, _ in owner.requests
                    )
                    reject = owner.reject_entitlement
                    empty_rejection = owner.empty_rejection
                    transient = owner.transient_entitlement
                    block_activation = owner.block_activation
                    block = owner.block_entitlement
                if self.path == "/api/activate":
                    if block_activation:
                        owner.activation_started.set()
                        owner.release_activation.wait(timeout=10)
                    self.respond(200, {"ok": True})
                elif self.path == "/api/deactivate":
                    self.respond(200, {"ok": True})
                elif self.path == "/api/remote/entitlement" and reject:
                    if empty_rejection:
                        self.respond_empty(403)
                    else:
                        self.respond(
                            403,
                            {"error": "revoked", "reason": "license revoked"},
                        )
                elif self.path == "/api/remote/entitlement" and transient:
                    self.respond(503, {"error": "temporarily unavailable"})
                elif self.path == "/api/remote/entitlement":
                    if block:
                        owner.entitlement_started.set()
                        owner.release_entitlement.wait(timeout=10)
                    self.respond(
                        200,
                        {
                            "entitlement": f"UNPRE-issued-{entitlement_number}",
                            "expires_at": int(time.time()) + 30 * 24 * 60 * 60,
                        },
                    )
                else:
                    self.respond(404, {"error": "not found"})

            def respond(self, status, value):
                encoded = json.dumps(value).encode()
                self.send_response(status)
                self.send_header("content-type", "application/json")
                self.send_header("content-length", str(len(encoded)))
                self.send_header("connection", "close")
                self.end_headers()
                self.wfile.write(encoded)

            def respond_empty(self, status):
                self.send_response(status)
                self.send_header("content-length", "0")
                self.send_header("connection", "close")
                self.end_headers()

            def log_message(self, _format, *_args):
                pass

        self.server = HTTPServer(("127.0.0.1", 0), Handler)
        self.port = self.server.server_address[1]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def count(self, path):
        with self._lock:
            return sum(request_path == path for request_path, _ in self.requests)

    def set_rejected(self, rejected, empty=False):
        with self._lock:
            self.reject_entitlement = rejected
            self.empty_rejection = empty

    def set_transient(self, transient):
        with self._lock:
            self.transient_entitlement = transient

    def block_next_activation(self):
        with self._lock:
            self.block_activation = True
        self.activation_started.clear()
        self.release_activation.clear()

    def release_blocked_activation(self):
        with self._lock:
            self.block_activation = False
        self.release_activation.set()

    def block_next_entitlement(self):
        with self._lock:
            self.block_entitlement = True
        self.entitlement_started.clear()
        self.release_entitlement.clear()

    def release_blocked_entitlement(self):
        with self._lock:
            self.block_entitlement = False
        self.release_entitlement.set()

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)


class FakeRelay:
    def __init__(self):
        self.server = socket.socket()
        self.server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.server.bind(("127.0.0.1", 0))
        self.server.listen()
        self.server.settimeout(0.2)
        self.port = self.server.getsockname()[1]
        self.accepted = 0
        self.active = 0
        self.authorizations = []
        self.rejected_authorizations = set()
        self._lock = threading.Lock()
        self._stop = threading.Event()
        self.thread = threading.Thread(target=self._serve, daemon=True)
        self.thread.start()

    def _serve(self):
        while not self._stop.is_set():
            try:
                connection, _ = self.server.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            threading.Thread(target=self._connection, args=(connection,), daemon=True).start()

    def _connection(self, connection):
        active = False
        try:
            connection.settimeout(0.5)
            head = b""
            while b"\r\n\r\n" not in head and len(head) < 16 * 1024:
                head += connection.recv(4096)
            text = head.decode("utf-8", "replace")
            headers = {}
            for line in text.split("\r\n")[1:]:
                if ":" in line:
                    name, value = line.split(":", 1)
                    headers[name.lower()] = value.strip()
            key = headers.get("sec-websocket-key", "")
            authorization = headers.get("authorization", "")
            with self._lock:
                self.authorizations.append(authorization)
                rejected = authorization in self.rejected_authorizations
            if rejected:
                connection.sendall(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                return
            accept = base64.b64encode(
                hashlib.sha1(
                    (key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()
                ).digest()
            ).decode()
            connection.sendall(
                (
                    "HTTP/1.1 101 Switching Protocols\r\n"
                    "Upgrade: websocket\r\n"
                    "Connection: Upgrade\r\n"
                    f"Sec-WebSocket-Accept: {accept}\r\n\r\n"
                ).encode()
            )
            with self._lock:
                self.accepted += 1
                self.active += 1
                active = True
            while not self._stop.is_set():
                try:
                    if not connection.recv(65536):
                        return
                except socket.timeout:
                    continue
        except OSError:
            pass
        finally:
            if active:
                with self._lock:
                    self.active = max(0, self.active - 1)
            try:
                connection.close()
            except OSError:
                pass

    def snapshot(self):
        with self._lock:
            return self.accepted, self.active, list(self.authorizations)

    def reject_entitlement(self, entitlement):
        with self._lock:
            self.rejected_authorizations.add(f"Bearer {entitlement}")

    def close(self):
        self._stop.set()
        try:
            self.server.close()
        except OSError:
            pass
        self.thread.join(timeout=2)


class NativeMobileOwner:
    """Exact-port stand-in for MobileRemoteServer during ownership tests."""

    def __init__(self, port, timeout=12):
        owner = self

        class ReusableHTTPServer(HTTPServer):
            allow_reuse_address = True

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):  # noqa: N802 - BaseHTTPRequestHandler API
                encoded = json.dumps({"owner": "native", "port": owner.port}).encode()
                self.send_response(200)
                self.send_header("content-type", "application/json")
                self.send_header("content-length", str(len(encoded)))
                self.send_header("connection", "close")
                self.end_headers()
                self.wfile.write(encoded)

            def log_message(self, _format, *_args):
                pass

        deadline = time.monotonic() + timeout
        self.server = None
        while self.server is None and time.monotonic() < deadline:
            try:
                self.server = ReusableHTTPServer(("0.0.0.0", port), Handler)
            except OSError:
                time.sleep(0.1)
        if self.server is None:
            raise RuntimeError(f"native owner could not claim persisted port {port}")
        self.port = self.server.server_port
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def close(self):
        if self.server is None:
            return
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)
        self.server = None


class LegacyNativeMobileOwner:
    """Released MobileRemoteServer: A collision falls back to random B and
    overwrites server-port, even though paired Direct controllers retain A."""

    def __init__(self, home, preferred_port):
        self.home = home
        owner = self

        class ReusableHTTPServer(HTTPServer):
            allow_reuse_address = True

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):  # noqa: N802 - BaseHTTPRequestHandler API
                encoded = json.dumps(
                    {"owner": "legacy-native", "port": owner.port}
                ).encode()
                self.send_response(200)
                self.send_header("content-type", "application/json")
                self.send_header("content-length", str(len(encoded)))
                self.send_header("connection", "close")
                self.end_headers()
                self.wfile.write(encoded)

            def log_message(self, _format, *_args):
                pass

        try:
            self.server = ReusableHTTPServer(("0.0.0.0", preferred_port), Handler)
        except OSError:
            self.server = ReusableHTTPServer(("0.0.0.0", 0), Handler)
        self.port = self.server.server_port
        with open(home.path("mobile", "server-port"), "w") as handle:
            handle.write(f"{self.port}\n")
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def close(self):
        if self.server is None:
            return
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)
        self.server = None


class NativeRelayOwner:
    """One live Relay connection standing in for the released Mac app."""

    def __init__(self, relay_port):
        self.socket = socket.create_connection(("127.0.0.1", relay_port), timeout=2)
        key = base64.b64encode(os.urandom(16)).decode()
        self.socket.sendall(
            (
                "GET /host HTTP/1.1\r\n"
                "Host: 127.0.0.1\r\n"
                "Upgrade: websocket\r\n"
                "Connection: Upgrade\r\n"
                f"Sec-WebSocket-Key: {key}\r\n"
                "Sec-WebSocket-Version: 13\r\n"
                "Authorization: Bearer native-legacy\r\n\r\n"
            ).encode()
        )
        response = b""
        while b"\r\n\r\n" not in response:
            response += self.socket.recv(4096)
        if b" 101 " not in response.split(b"\r\n", 1)[0]:
            raise RuntimeError(f"native relay stand-in failed: {response!r}")

    def close(self):
        if self.socket is None:
            return
        try:
            self.socket.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.socket.close()
        self.socket = None


class LegacyTuiPeer:
    """A pre-fix TUI: live app-ports entry, sidebar 404, no identity header."""

    def __init__(self, home):
        self.home = home

        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):  # noqa: N802 - BaseHTTPRequestHandler API
                encoded = b'{"error":"not found"}'
                self.send_response(404)
                self.send_header("content-type", "application/json")
                self.send_header("content-length", str(len(encoded)))
                self.send_header("connection", "close")
                self.end_headers()
                self.wfile.write(encoded)

            def log_message(self, _format, *_args):
                pass

        self.server = HTTPServer(("127.0.0.1", 0), Handler)
        self.port = self.server.server_port
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        registry = home.path("app-ports")
        try:
            with open(registry) as handle:
                ports = [int(value) for value in handle.read().split()]
        except (OSError, ValueError):
            ports = []
        ports = [value for value in ports if value != self.port] + [self.port]
        with open(registry, "w") as handle:
            handle.write("".join(f"{value}\n" for value in ports[-16:]))

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)


def write_license(home):
    with open(home.path("link-license.json"), "w") as handle:
        json.dump({"key": LICENSE_KEY}, handle)
    os.chmod(home.path("link-license.json"), 0o600)


def write_entitlement(home, entitlement, expires_at, mac_id="headless-link-host"):
    with open(home.path("mobile", "relay-entitlement.json"), "w") as handle:
        json.dump(
            {
                "entitlement": entitlement,
                "expiresAt": expires_at,
                "macID": mac_id,
            },
            handle,
        )
    os.chmod(home.path("mobile", "relay-entitlement.json"), 0o600)


def _entitlement_cache_is_fresh(path):
    try:
        with open(path) as handle:
            cached = json.load(handle)
    except (FileNotFoundError, ValueError):
        return False
    return (
        cached.get("entitlement", "").startswith("UNPRE-issued-")
        and cached.get("expiresAt", 0) > int(time.time()) + 7 * 24 * 60 * 60
    )


def tombstone_path(home):
    return home.path("link-disabled.json")


def write_tombstone(home, reason="user_disabled", generation="test-disable"):
    lock_path = home.path("link-license.lock")
    with open(lock_path, "a+") as lock:
        os.chmod(lock_path, 0o600)
        fcntl.flock(lock, fcntl.LOCK_EX)
        temporary = home.path(f".link-disabled.{uuid.uuid4()}.tmp")
        descriptor = os.open(temporary, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        with os.fdopen(descriptor, "w") as handle:
            json.dump(
                {
                    "version": 1,
                    "generation": generation,
                    "reason": reason,
                    "disabled_at": int(time.time()),
                },
                handle,
            )
        os.replace(temporary, tombstone_path(home))
        fcntl.flock(lock, fcntl.LOCK_UN)


def clear_tombstone(home):
    try:
        os.unlink(tombstone_path(home))
    except FileNotFoundError:
        pass


def read_tombstone(home):
    try:
        with open(tombstone_path(home)) as handle:
            return json.load(handle)
    except (FileNotFoundError, ValueError):
        return None


def stop_tui(tui, settings_open=False):
    if settings_open:
        # Once deactivated, Remote's selected row is an editable key field;
        # printable q belongs to that input. Escape closes Settings reliably.
        tui.send("\x1b", settle=0.3)
    tui.send("q", settle=0.3)
    if not tui.exited(timeout=5):
        raise RuntimeError("TUI did not exit cleanly between Link lifecycle phases")
    tui.close()


def wait_for(predicate, timeout=5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.1)
    return False


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.session("s1", label="a session", project_id="p")
    token = home.pair_device()
    with open(home.path("mobile", "devices.json")) as handle:
        devices = json.load(handle)
    devices["devices"][0]["relayTokenHash"] = "ab" * 32
    with open(home.path("mobile", "devices.json"), "w") as handle:
        json.dump(devices, handle)
    with open(home.path("mobile", "mac-id"), "w") as handle:
        handle.write("headless-link-host\n")
    port = home.reserve_mobile_port()

    api = case.track(LicenseAPI())
    relay = case.track(FakeRelay())
    env = {
        "UNPEEL_LICENSE_PUBLIC_KEY": PUBLIC_KEY,
        "UNPEEL_LICENSE_API_BASE_URL": f"http://127.0.0.1:{api.port}",
        "UNPEEL_RELAY_URL": f"ws://127.0.0.1:{relay.port}",
    }

    # A previous user disable is durable but must not make reactivation
    # impossible. Only this explicit activation + fresh entitlement commit
    # may clear the exact marker generation.
    write_tombstone(home)
    api.block_next_activation()

    # Pair-first, activate-second is the normal `unpeel pair --serve` shape:
    # the LAN server is already live when Settings receives the key.
    tui = case.pty(rows=48, cols=160, env=env)
    tui.read_for(5)
    alive = not tui.exited(timeout=0.1)
    case.check(
        "the standalone TUI remains running",
        alive,
        tui.all_text()[-2000:],
    )
    if not alive:
        return
    status, _ = mobile_request(port, "/mobile/bootstrap", token)
    case.check("the paired phone starts on LAN before Link activation", status == 200, str(status))
    case.check("no relay runs before an entitlement exists", relay.snapshot()[0] == 0)

    tui.send(",", settle=0.5)
    tui.send("\t", settle=0.3)
    tui.send("\t", settle=0.8)
    remote = tui.expect("paste license key")
    case.check("Settings Remote exposes headless Link activation", "paste license key" in remote)
    tui.send("j", settle=0.2)  # one paired-device row, then the key row
    tui.type(LICENSE_KEY, per_char=0.001, settle=0.2)
    tui.send("\r", settle=0.5)

    activation_started = tui.wait_for(lambda: api.activation_started.is_set(), timeout=4)
    # First Escape clears the editable key field; the second closes Settings.
    # Both must be handled while the HTTP worker remains deliberately stuck.
    tui.send("\x1b", settle=0.1)
    tui.send("\x1b", settle=0.2)
    responsive_screen = tui.screen(settle=0.2)
    case.check("the activation request reaches the slow API", activation_started)
    case.check(
        "a slow activation never blocks Settings input or rendering",
        not tui.exited(timeout=0.1) and "paste license key" not in responsive_screen,
        responsive_screen[-1200:],
    )
    # Another frontend/user decision wins while `/api/activate` is still in
    # flight. The late success must be compensated server-side and may not
    # publish a key, cache, or relay from the newer disable generation.
    write_tombstone(home, generation="deactivation-won-during-activation")
    api.release_blocked_activation()
    cache_path = home.path("mobile", "relay-entitlement.json")
    stale_activation_rejected = tui.wait_for(
        lambda: api.count("/api/deactivate") >= 1
        and not os.path.exists(home.path("link-license.json")),
        timeout=8,
    )
    case.check(
        "deactivation during activation wins over the late service response",
        stale_activation_rejected
        and not os.path.exists(cache_path)
        and relay.snapshot()[0] == 0
        and (read_tombstone(home) or {}).get("generation")
        == "deactivation-won-during-activation",
        f"marker={read_tombstone(home)}, requests={api.requests}",
    )

    # A new explicit attempt observes the current marker generation and may
    # now transition it through activation_pending to a fresh entitlement.
    tui.send(",", settle=0.4)
    tui.send("\t", settle=0.2)
    tui.send("\t", settle=0.5)
    tui.send("j", settle=0.2)
    tui.type(LICENSE_KEY, per_char=0.001, settle=0.2)
    tui.send("\r", settle=0.3)

    connected = tui.wait_for(lambda: relay.snapshot()[1] == 1, timeout=8)
    case.check("activation starts the relay without restarting the TUI", connected)
    case.check("activation and entitlement endpoints both ran", api.count("/api/activate") == 2 and api.count("/api/remote/entitlement") >= 1, str(api.requests))
    case.check("the issued entitlement is cached privately", os.path.exists(cache_path) and (os.stat(cache_path).st_mode & 0o777) == 0o600)
    case.check(
        "the headless Link key is stored privately",
        (os.stat(home.path("link-license.json")).st_mode & 0o777) == 0o600,
    )
    case.check(
        "explicit activation clears the matching user-disable marker only after entitlement commit",
        not os.path.exists(tombstone_path(home)),
    )

    # The successful second attempt leaves the same Link row selected.
    tui.expect("deactivate", timeout=4)
    tui.send("\r", settle=0.3)
    stopped = tui.wait_for(lambda: relay.snapshot()[1] == 0, timeout=5)
    case.check("deactivation stops the live relay", stopped)
    case.check("deactivation removes the entitlement cache", not os.path.exists(cache_path))
    disabled_marker = read_tombstone(home)
    case.check(
        "deactivation durably records a private user-disable marker",
        disabled_marker is not None
        and disabled_marker.get("reason") == "user_disabled"
        and bool(disabled_marker.get("generation"))
        and (os.stat(tombstone_path(home)).st_mode & 0o777) == 0o600,
        str(disabled_marker),
    )
    status, _ = mobile_request(port, "/mobile/bootstrap", token)
    case.check("deactivation preserves direct LAN phone control", status == 200, str(status))
    stop_tui(tui, settings_open=True)

    # Activation has a durable intermediate state: if the TUI exits after
    # committing the key but before the entitlement response, the next
    # process may refresh that exact generation without trusting old cache.
    api.block_next_entitlement()
    pending_tui = case.pty(rows=48, cols=160, env=env)
    pending_tui.read_for(3)
    pending_tui.send(",", settle=0.4)
    pending_tui.send("\t", settle=0.2)
    pending_tui.send("\t", settle=0.5)
    pending_tui.send("j", settle=0.2)
    pending_tui.type(LICENSE_KEY, per_char=0.001, settle=0.2)
    pending_tui.send("\r", settle=0.2)
    activation_pending = pending_tui.wait_for(
        lambda: api.entitlement_started.is_set()
        and (read_tombstone(home) or {}).get("reason") == "activation_pending",
        timeout=8,
    )
    try:
        with open(home.path("link-license.json")) as handle:
            pending_license = json.load(handle)
    except (FileNotFoundError, ValueError):
        pending_license = {}
    pending_marker = read_tombstone(home) or {}
    case.check(
        "activation commit durably transitions the exact marker to activation_pending",
        activation_pending
        and pending_license.get("activation_generation")
        == pending_marker.get("generation"),
        f"license={pending_license}, marker={pending_marker}",
    )
    stop_tui(pending_tui, settings_open=True)
    api.release_blocked_entitlement()
    before_pending_recovery = api.count("/api/remote/entitlement")
    recovered_tui = case.pty(rows=48, cols=160, env=env)
    recovered_after_restart = recovered_tui.wait_for(
        lambda: api.count("/api/remote/entitlement") > before_pending_recovery
        and relay.snapshot()[1] == 1
        and not os.path.exists(tombstone_path(home)),
        timeout=10,
    )
    case.check(
        "a restart finishes activation_pending with a fresh entitlement",
        recovered_after_restart,
        f"marker={read_tombstone(home)}, relay={relay.snapshot()}, requests={api.requests[-5:]}",
    )
    stop_tui(recovered_tui)
    wait_for(lambda: relay.snapshot()[1] == 0)

    # A persisted key refreshes the same way the native app does when the
    # entitlement enters its final seven days.
    write_entitlement(home, "UNPRE-near-expiry", int(time.time()) + 60)
    before_refresh = api.count("/api/remote/entitlement")
    refresh_tui = case.pty(rows=48, cols=160, env=env)
    refreshed = refresh_tui.wait_for(
        lambda: api.count("/api/remote/entitlement") > before_refresh,
        timeout=8,
    )
    committed = refresh_tui.wait_for(
        lambda: _entitlement_cache_is_fresh(cache_path),
        timeout=8,
    )
    try:
        with open(cache_path) as handle:
            refreshed_cache = json.load(handle)
    except (FileNotFoundError, ValueError):
        refreshed_cache = {}
    case.check("a near-expiry entitlement refreshes from the stored key", refreshed)
    case.check(
        "refresh installs a new full-lifetime entitlement",
        committed
        and refreshed_cache.get("entitlement", "").startswith("UNPRE-issued-")
        and refreshed_cache.get("expiresAt", 0) > int(time.time()) + 7 * 24 * 60 * 60,
        str(refreshed_cache),
    )
    stop_tui(refresh_tui)

    # A definitive server rejection cannot leave the old signed cache/uplink
    # alive, but still must not remove the local phone server.
    api.set_rejected(True, empty=True)
    write_license(home)
    write_entitlement(home, "UNPRE-rejected-cache", int(time.time()) + 60)
    before_rejection = api.count("/api/remote/entitlement")
    rejected_tui = case.pty(rows=48, cols=160, env=env)
    rejected = rejected_tui.wait_for(
        lambda: api.count("/api/remote/entitlement") > before_rejection
        and not os.path.exists(cache_path),
        timeout=8,
    )
    closed = rejected_tui.wait_for(lambda: relay.snapshot()[1] == 0, timeout=5)
    case.check("a rejected refresh removes the previously valid cache", rejected)
    case.check("a rejected refresh fails closed by stopping Link", closed)
    status, _ = mobile_request(port, "/mobile/bootstrap", token)
    case.check("a Link rejection still preserves the LAN server", status == 200, str(status))
    rejected_marker = read_tombstone(home)
    case.check(
        "a definitive entitlement rejection is durably suppressed",
        rejected_marker is not None
        and rejected_marker.get("reason") == "authorization_rejected",
        str(rejected_marker),
    )
    stop_tui(rejected_tui)

    # A successful HTTP refresh that began before a newer WS rejection has
    # stale authority. The rejection advances the generation while that
    # response is held, so releasing it cannot clear suppression or restore
    # the rejected cache; a later request must observe the new generation.
    api.set_rejected(False)
    clear_tombstone(home)
    write_license(home)
    stale_success_bearer = "UNPRE-stale-success-before-ws-rejection"
    write_entitlement(home, stale_success_bearer, int(time.time()) + 60)
    api.block_next_entitlement()
    relay.reject_entitlement(stale_success_bearer)
    rejection_race_tui = case.pty(rows=48, cols=160, env=env)
    rejection_won = rejection_race_tui.wait_for(
        lambda: api.entitlement_started.is_set()
        and (read_tombstone(home) or {}).get("reason")
        == "authorization_rejected"
        and not os.path.exists(cache_path),
        timeout=10,
    )
    rejection_generation = (read_tombstone(home) or {}).get("generation")
    case.check("WS rejection advances authority while refresh is held", rejection_won)
    api.release_blocked_entitlement()
    rejection_race_tui.read_for(2)
    case.check(
        "a pre-rejection success cannot clear the newer rejection generation",
        rejection_generation is not None
        and (read_tombstone(home) or {}).get("generation")
        == rejection_generation
        and not os.path.exists(cache_path)
        and relay.snapshot()[1] == 0,
        f"marker={read_tombstone(home)}, relay={relay.snapshot()}",
    )
    stop_tui(rejection_race_tui)
    before_rejection_recovery = api.count("/api/remote/entitlement")
    rejection_recovery_tui = case.pty(rows=48, cols=160, env=env)
    rejection_recovered = rejection_recovery_tui.wait_for(
        lambda: api.count("/api/remote/entitlement") > before_rejection_recovery
        and not os.path.exists(tombstone_path(home))
        and relay.snapshot()[1] == 1,
        timeout=10,
    )
    case.check(
        "a later refresh observing the rejection generation can recover",
        rejection_recovered,
        f"marker={read_tombstone(home)}, relay={relay.snapshot()}",
    )
    stop_tui(rejection_recovery_tui)
    wait_for(lambda: relay.snapshot()[1] == 0)

    # Deterministic stale-response race: deactivation commits while a refresh
    # response is held at the server. Releasing that successful old response
    # must not recreate the deleted cache.
    clear_tombstone(home)
    write_license(home)
    write_entitlement(home, "UNPRE-racing-cache", int(time.time()) + 60)
    api.block_next_entitlement()
    race_tui = case.pty(rows=48, cols=160, env=env)
    started = race_tui.wait_for(lambda: api.entitlement_started.is_set(), timeout=8)
    case.check("the refresh race reaches its held response", started)
    race_tui.send(",", settle=0.4)
    race_tui.send("\t", settle=0.2)
    race_tui.send("\t", settle=0.5)
    race_tui.send("j", settle=0.2)
    race_tui.send("\r", settle=0.2)
    locally_revoked = race_tui.wait_for(
        lambda: not os.path.exists(cache_path)
        and not os.path.exists(home.path("link-license.json")),
        timeout=3,
    )
    case.check("deactivation wins locally before the refresh returns", locally_revoked)
    api.release_blocked_entitlement()
    race_tui.read_for(1.5)
    case.check(
        "a late successful refresh cannot resurrect the entitlement cache",
        not os.path.exists(cache_path),
    )
    closed = race_tui.wait_for(lambda: relay.snapshot()[1] == 0, timeout=5)
    case.check("the stale-response race leaves Link stopped", closed)
    status, _ = mobile_request(port, "/mobile/bootstrap", token)
    case.check("the stale-response race leaves LAN control running", status == 200, str(status))
    stop_tui(race_tui, settings_open=True)

    # A transient entitlement-service outage may use a still-valid cache,
    # but cannot extend an expired one. These two starts use the same server
    # failure and differ only in signed cache expiry.
    api.set_transient(True)
    clear_tombstone(home)
    write_license(home)
    write_entitlement(home, "UNPRE-transient-valid", int(time.time()) + 60)
    before_transient = api.count("/api/remote/entitlement")
    transient_tui = case.pty(rows=48, cols=160, env=env)
    transient_requested = transient_tui.wait_for(
        lambda: api.count("/api/remote/entitlement") > before_transient,
        timeout=8,
    )
    transient_connected = transient_tui.wait_for(
        lambda: relay.snapshot()[1] == 1,
        timeout=8,
    )
    with open(cache_path) as handle:
        transient_cache = json.load(handle)
    case.check("a transient refresh failure is retried", transient_requested)
    case.check(
        "a transient failure preserves a still-valid cache and Link",
        transient_connected
        and transient_cache.get("entitlement") == "UNPRE-transient-valid",
        str(transient_cache),
    )
    stop_tui(transient_tui)
    case.check(
        "exiting the TUI closes its relay uplink",
        wait_for(lambda: relay.snapshot()[1] == 0),
        str(relay.snapshot()),
    )

    write_entitlement(home, "UNPRE-transient-expired", int(time.time()) - 1)
    before_expired = api.count("/api/remote/entitlement")
    accepted_before_expired = relay.snapshot()[0]
    expired_tui = case.pty(rows=48, cols=160, env=env)
    expired_requested = expired_tui.wait_for(
        lambda: api.count("/api/remote/entitlement") > before_expired,
        timeout=8,
    )
    expired_tui.read_for(1.5)
    case.check("an expired cache still attempts an off-thread refresh", expired_requested)
    case.check(
        "a transient failure never starts Link from an expired cache",
        relay.snapshot()[1] == 0 and relay.snapshot()[0] == accepted_before_expired,
        str(relay.snapshot()),
    )
    stop_tui(expired_tui)
    api.set_transient(False)

    # A fresh-looking cache for another Host identity is not authority for
    # this Host. It stays off until the endpoint returns a correctly bound
    # replacement, then starts with only that new bearer.
    clear_tombstone(home)
    write_license(home)
    write_entitlement(
        home,
        "UNPRE-wrong-host",
        int(time.time()) + 30 * 24 * 60 * 60,
        mac_id="some-other-host",
    )
    before_mismatch = api.count("/api/remote/entitlement")
    mismatch_tui = case.pty(rows=48, cols=160, env=env)
    mismatch_recovered = mismatch_tui.wait_for(
        lambda: api.count("/api/remote/entitlement") > before_mismatch
        and relay.snapshot()[1] == 1,
        timeout=10,
    )
    with open(cache_path) as handle:
        mismatch_cache = json.load(handle)
    mismatch_authorizations = relay.snapshot()[2]
    case.check(
        "a cache bound to another Host is refreshed before Link starts",
        mismatch_recovered
        and mismatch_cache.get("macID") == "headless-link-host"
        and "Bearer UNPRE-wrong-host" not in mismatch_authorizations,
        f"cache={mismatch_cache}, auth={mismatch_authorizations}",
    )
    stop_tui(mismatch_tui)
    wait_for(lambda: relay.snapshot()[1] == 0)

    # Relay 401/403 is distinct from a network reconnect. Invalidate the
    # rejected bearer, refresh immediately, and reconnect with the replacement.
    rejected_bearer = "UNPRE-relay-403"
    clear_tombstone(home)
    write_license(home)
    write_entitlement(home, rejected_bearer, int(time.time()) + 30 * 24 * 60 * 60)
    relay.reject_entitlement(rejected_bearer)
    before_403_refresh = api.count("/api/remote/entitlement")
    relay_403_tui = case.pty(rows=48, cols=160, env=env)
    relay_403_recovered = relay_403_tui.wait_for(
        lambda: api.count("/api/remote/entitlement") > before_403_refresh
        and relay.snapshot()[1] == 1,
        timeout=12,
    )
    with open(cache_path) as handle:
        relay_403_cache = json.load(handle)
    case.check(
        "relay 403 invalidates, refreshes, and reconnects with a new bearer",
        relay_403_recovered
        and f"Bearer {rejected_bearer}" in relay.snapshot()[2]
        and relay_403_cache.get("entitlement") != rejected_bearer,
        f"cache={relay_403_cache}, relay={relay.snapshot()}",
    )
    case.check(
        "a fresh entitlement clears the durable relay-rejection marker",
        not os.path.exists(tombstone_path(home)),
        str(read_tombstone(home)),
    )
    stop_tui(relay_403_tui)
    wait_for(lambda: relay.snapshot()[1] == 0)

    # Local deletion errors are not success. Deny unlink in the cache's parent
    # while leaving a readable, fully valid bearer in place: the root-level
    # tombstone must win both now and after a fresh TUI process starts.
    clear_tombstone(home)
    write_license(home)
    write_entitlement(
        home,
        "UNPRE-retained-after-delete-failure",
        int(time.time()) + 30 * 24 * 60 * 60,
    )
    deactivations_before_failure = api.count("/api/deactivate")
    deletion_tui = case.pty(rows=48, cols=160, env=env)
    deletion_started = deletion_tui.wait_for(lambda: relay.snapshot()[1] == 1, timeout=8)
    deletion_tui.send(",", settle=0.4)
    deletion_tui.send("\t", settle=0.2)
    deletion_tui.send("\t", settle=0.5)
    deletion_tui.send("j", settle=0.2)
    mobile_dir = home.path("mobile")
    os.chmod(mobile_dir, 0o500)
    deletion_tui.send("\r", settle=0.2)
    deletion_error = deletion_tui.wait_for_text("could not finish", timeout=4)
    retained_cache = os.path.isfile(cache_path)
    failed_marker = read_tombstone(home)
    case.check(
        "a local cache deletion failure is surfaced instead of claiming deactivation",
        deletion_started
        and deletion_error
        and retained_cache
        and os.path.exists(home.path("link-license.json"))
        and api.count("/api/deactivate") == deactivations_before_failure,
        deletion_tui.all_text()[-1000:],
    )
    case.check(
        "the failed deletion still commits a private user-disable tombstone first",
        failed_marker is not None
        and failed_marker.get("reason") == "user_disabled"
        and (os.stat(tombstone_path(home)).st_mode & 0o777) == 0o600,
        str(failed_marker),
    )
    case.check(
        "a failed local deactivation cannot restart Link",
        relay.snapshot()[1] == 0,
        str(relay.snapshot()),
    )
    os.chmod(mobile_dir, 0o700)
    status, _ = mobile_request(port, "/mobile/bootstrap", token)
    case.check("failed Link deactivation still preserves LAN", status == 200, str(status))
    stop_tui(deletion_tui, settings_open=True)

    refreshes_before_disabled_restart = api.count("/api/remote/entitlement")
    relay_accepts_before_disabled_restart = relay.snapshot()[0]
    disabled_restart_tui = case.pty(rows=48, cols=160, env=env)
    disabled_restart_tui.read_for(4)
    case.check(
        "a restart cannot refresh or trust the readable retained cache while user-disabled",
        os.path.isfile(cache_path)
        and api.count("/api/remote/entitlement")
        == refreshes_before_disabled_restart
        and relay.snapshot()[0] == relay_accepts_before_disabled_restart
        and relay.snapshot()[1] == 0,
        f"marker={read_tombstone(home)}, relay={relay.snapshot()}, requests={api.requests[-5:]}",
    )
    status, _ = mobile_request(port, "/mobile/bootstrap", token)
    case.check("durable Link suppression still preserves LAN after restart", status == 200, str(status))
    stop_tui(disabled_restart_tui)

    # A malformed headless key cannot inherit a valid old cache, and its
    # rejection generation must be stable enough for a corrected explicit
    # activation to snapshot and commit on the next attempt.
    clear_tombstone(home)
    with open(home.path("link-license.json"), "w") as handle:
        json.dump({"key": "CLRTY-malformed"}, handle)
    os.chmod(home.path("link-license.json"), 0o600)
    malformed_bearer = "UNPRE-malformed-key-cache"
    write_entitlement(
        home,
        malformed_bearer,
        int(time.time()) + 30 * 24 * 60 * 60,
    )
    malformed_relay_before = len(relay.snapshot()[2])
    malformed_tui = case.pty(rows=48, cols=160, env=env)
    malformed_suppressed = malformed_tui.wait_for(
        lambda: (read_tombstone(home) or {}).get("reason")
        == "authorization_rejected"
        and not os.path.exists(home.path("link-license.json"))
        and not os.path.exists(cache_path),
        timeout=8,
    )
    malformed_marker = read_tombstone(home) or {}
    malformed_tui.read_for(2)
    case.check(
        "a malformed key is removed and its old cache is durably suppressed",
        malformed_suppressed
        and len(relay.snapshot()[2]) == malformed_relay_before
        and f"Bearer {malformed_bearer}" not in relay.snapshot()[2],
        f"marker={malformed_marker}, relay={relay.snapshot()}",
    )
    case.check(
        "malformed-key maintenance keeps one stable rejection generation",
        (read_tombstone(home) or {}).get("generation")
        == malformed_marker.get("generation"),
        f"before={malformed_marker}, after={read_tombstone(home)}",
    )
    malformed_tui.send(",", settle=0.4)
    malformed_tui.send("\t", settle=0.2)
    malformed_tui.send("\t", settle=0.5)
    malformed_tui.send("j", settle=0.2)
    malformed_tui.type(LICENSE_KEY, per_char=0.001, settle=0.2)
    malformed_tui.send("\r", settle=0.2)
    malformed_recovered = malformed_tui.wait_for(
        lambda: relay.snapshot()[1] == 1
        and not os.path.exists(tombstone_path(home)),
        timeout=10,
    )
    case.check(
        "a valid explicit activation recovers after the malformed key",
        malformed_recovered,
        f"marker={read_tombstone(home)}, relay={relay.snapshot()}",
    )
    malformed_tui.send("\r", settle=0.2)
    malformed_tui.wait_for(lambda: relay.snapshot()[1] == 0, timeout=5)
    stop_tui(malformed_tui, settings_open=True)

    # A reachable native bridge owns the shared cache. Even a stored headless
    # key plus refresh-due cache must remain byte-for-byte untouched while the
    # app answers the sidebar route, and the TUI must not open its own uplink.
    clear_tombstone(home)
    write_license(home)
    write_entitlement(home, "UNPRE-native-owned", int(time.time()) + 60)
    with open(cache_path, "rb") as handle:
        native_cache_before = handle.read()
    native_api_before = api.count("/api/remote/entitlement")
    native_relay_before = len(relay.snapshot()[2])
    native_owner = case.track(NativeMobileOwner(port))
    app = case.app(
        sidebar={"projects": [], "mobile_endpoint_handoff": 1}
    )
    peer_tui = case.pty(rows=48, cols=160, env=env)
    legacy_peer = case.track(LegacyTuiPeer(home))
    bridge_tui = case.pty(rows=48, cols=160, env=env)

    def wait_for_frontends(predicate, timeout=12, poll=0.15):
        """Keep both PTY renderers flowing while either may own serving.

        Linux PTY buffers are small enough for an unconsumed renderer to
        block its run loop. Ownership is intentionally nondeterministic, so a
        wait that drains only one TUI can freeze the actual endpoint owner and
        turn a handoff assertion into a platform-specific timeout.
        """
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            peer_tui.read_for(poll)
            bridge_tui.read_for(poll)
            value = predicate()
            if value:
                return value
        return None

    wait_for_frontends(lambda: app.count("/mcp/sidebar") > 1, timeout=10)
    peer_tui.read_for(1.5)
    bridge_tui.read_for(1.5)
    with open(cache_path, "rb") as handle:
        native_cache_after = handle.read()
    case.check(
        "native bridge ownership prevents TUI entitlement refresh or cache mutation",
        api.count("/api/remote/entitlement") == native_api_before
        and native_cache_after == native_cache_before,
        str(api.requests[-5:]),
    )
    case.check(
        "native bridge ownership prevents a duplicate TUI relay uplink",
        len(relay.snapshot()[2]) == native_relay_before,
        str(relay.snapshot()),
    )
    status, native_bootstrap = mobile_request(port, "/mobile/bootstrap", token)
    case.check(
        "a legacy TUI peer cannot hide the validated native owner",
        status == 200
        and native_bootstrap.get("owner") == "native"
        and app.count("/mcp/sidebar") > 1,
        str((status, native_bootstrap, app.count("/mcp/sidebar"))),
    )

    # Lose the native bridge first while its listener is still draining. No
    # TUI may escape to a random port or start Link during this gap; once the
    # exact endpoint is released, exactly one of the two TUIs wins LAN + Link.
    app.close()
    peer_tui.read_for(1.5)
    bridge_tui.read_for(1.5)
    status, still_native = mobile_request(port, "/mobile/bootstrap", token)
    case.check(
        "an occupied canonical endpoint is retryable ownership, not a random fallback",
        status == 200
        and still_native.get("owner") == "native"
        and relay.snapshot()[1] == 0,
        str((status, still_native, relay.snapshot())),
    )
    first_takeover_connections = relay.snapshot()[0]
    native_owner.close()

    def tui_took_over_lan_and_link():
        status, bootstrap = mobile_request(
            port, "/mobile/bootstrap", token, timeout=1
        )
        accepted, active, _ = relay.snapshot()
        return (
            status == 200
            and bootstrap.get("macID") == "headless-link-host"
            and accepted > first_takeover_connections
            and active == 1
        )

    first_takeover = wait_for_frontends(tui_took_over_lan_and_link, timeout=12)
    case.check(
        "native exit hands the same LAN endpoint and one Link uplink to one TUI",
        bool(first_takeover),
        str(relay.snapshot()),
    )
    with open(home.path("mobile", "server-port")) as handle:
        case.check(
            "app-to-TUI takeover never rewrites the paired endpoint",
            handle.read().strip() == str(port),
        )

    # A released app can crash after rewriting A→B but before its hook port is
    # ever classifiable. The canonical mismatch itself revokes the TUI uplink;
    # once repeated offline state and the dead B listener prove the writer is
    # gone, the live A owner repairs the file and resumes exactly one Link.
    stale_recovery_connections = relay.snapshot()[0]
    stale_legacy_mobile = case.track(LegacyNativeMobileOwner(home, port))
    stale_fallback_port = stale_legacy_mobile.port
    # First prove the durable endpoint mismatch itself revoked Link. Closing
    # the stand-in before this boundary lets a busy runner observe an
    # unrelated reconnect gap and then mistakes that transient for the
    # authority transition this case is meant to cover. The stand-in has no
    # hook/sidebar port, so it remains the same pre-classification crash
    # scenario once the mismatch has been observed.
    stale_rewrite_revoked_link = wait_for_frontends(
        lambda: relay.snapshot()[1] == 0, timeout=8
    )
    stale_legacy_mobile.close()

    def stale_rewrite_recovered():
        try:
            with open(home.path("mobile", "server-port")) as handle:
                saved_port = int(handle.read().strip())
        except (OSError, ValueError):
            return False
        accepted, active, _ = relay.snapshot()
        return (
            saved_port == port
            and accepted > stale_recovery_connections
            and active == 1
        )

    stale_recovered = wait_for_frontends(stale_rewrite_recovered, timeout=30)
    status, stale_direct = mobile_request(port, "/mobile/bootstrap", token)
    try:
        with open(home.path("mobile", "server-port")) as handle:
            stale_saved_port = handle.read().strip()
    except OSError as error:
        stale_saved_port = f"error:{error}"
    case.check(
        "a pre-classification legacy rewrite revokes Link immediately",
        stale_rewrite_revoked_link and stale_fallback_port != port,
        str((stale_fallback_port, relay.snapshot())),
    )
    case.check(
        "a dead unclassified fallback is repaired without orphaning Direct",
        bool(stale_recovered)
        and status == 200
        and stale_direct.get("macID") == "headless-link-host",
        str(
            (
                status,
                stale_direct,
                relay.snapshot(),
                {
                    "original": port,
                    "fallback": stale_fallback_port,
                    "saved": stale_saved_port,
                    "connections_before": stale_recovery_connections,
                },
            )
        ),
    )

    # The CLI ships before the fixed Mac app. Released native has no sidebar
    # route: it falls back from occupied A to random B and overwrites the
    # canonical file. Its older list-presets route positively distinguishes it
    # from the headerless 404 peer TUI. Native owns Link, while the TUI keeps
    # the already-paired Direct endpoint A alive and repairs the file.
    legacy_mobile = case.track(LegacyNativeMobileOwner(home, port))
    legacy_app = case.app(sidebar=None, fail_routes=("/mcp/sidebar",))
    legacy_native_relay = case.track(NativeRelayOwner(relay.port))

    def legacy_native_handoff_is_stable():
        try:
            with open(home.path("mobile", "server-port")) as handle:
                saved_port = int(handle.read().strip())
        except (OSError, ValueError):
            return False
        _accepted, active, authorizations = relay.snapshot()
        return (
            legacy_mobile.port != port
            and legacy_app.count("/mcp/list-presets") > 0
            and saved_port == port
            and active == 1
            and "Bearer native-legacy" in authorizations
        )

    legacy_stable = wait_for_frontends(
        legacy_native_handoff_is_stable, timeout=30
    )
    status, legacy_direct = mobile_request(port, "/mobile/bootstrap", token)
    fallback_status, fallback_owner = mobile_request(
        legacy_mobile.port, "/mobile/bootstrap", token
    )
    case.check(
        "released native is distinguished from an old TUI and owns the sole Link",
        legacy_stable,
        str(
            (
                legacy_app.calls[-8:],
                relay.snapshot(),
                port,
                legacy_mobile.port,
            )
        ),
    )
    case.check(
        "released native fallback cannot orphan the paired Direct endpoint",
        status == 200
        and legacy_direct.get("macID") == "headless-link-host"
        and fallback_status == 200
        and fallback_owner.get("owner") == "legacy-native",
        str((status, legacy_direct, fallback_status, fallback_owner)),
    )

    legacy_exit_connections = relay.snapshot()[0]
    legacy_native_relay.close()
    legacy_mobile.close()
    legacy_app.close()

    def tui_resumed_after_legacy_native_exit():
        status, bootstrap = mobile_request(
            port, "/mobile/bootstrap", token, timeout=1
        )
        accepted, active, _ = relay.snapshot()
        return (
            status == 200
            and bootstrap.get("macID") == "headless-link-host"
            and accepted > legacy_exit_connections
            and active == 1
        )

    case.check(
        "legacy native exit returns Link to the existing Direct owner",
        bool(
            wait_for_frontends(
                tui_resumed_after_legacy_native_exit, timeout=30
            )
        ),
        str(relay.snapshot()),
    )

    # Reverse direction: the app's validated sidebar asks the TUI winner to
    # yield; native retries until it claims this SAME port. Closing it again
    # proves either TUI can win the next lease without duplicate Link owners.
    returned_app = case.app(
        sidebar={"projects": [], "mobile_endpoint_handoff": 1}
    )
    native_handoff_ready = wait_for_frontends(
        lambda: relay.snapshot()[1] == 0
        and mobile_request(port, "/mobile/bootstrap", token, timeout=0.2)[0]
        != 200,
        timeout=15,
    )
    returned_native = case.track(NativeMobileOwner(port, timeout=15))
    relay_stopped = bool(native_handoff_ready) and relay.snapshot()[1] == 0
    status, returned_bootstrap = mobile_request(port, "/mobile/bootstrap", token)
    case.check(
        "validated native return gets an exact reverse handback",
        relay_stopped
        and status == 200
        and returned_bootstrap.get("owner") == "native",
        str((relay_stopped, status, returned_bootstrap, relay.snapshot())),
    )
    with open(home.path("mobile", "server-port")) as handle:
        case.check(
            "reverse handback preserves the Direct endpoint",
            handle.read().strip() == str(port),
        )

    second_takeover_connections = relay.snapshot()[0]
    returned_native.close()
    returned_app.close()

    def tui_reclaimed_lan_and_link():
        status, bootstrap = mobile_request(
            port, "/mobile/bootstrap", token, timeout=1
        )
        accepted, active, _ = relay.snapshot()
        return (
            status == 200
            and bootstrap.get("macID") == "headless-link-host"
            and accepted > second_takeover_connections
            and active == 1
        )

    second_takeover = wait_for_frontends(tui_reclaimed_lan_and_link, timeout=30)
    case.check(
        "a second native exit again elects one TUI for LAN + Link",
        bool(second_takeover),
        str(relay.snapshot()),
    )
    stop_tui(bridge_tui)
    stop_tui(peer_tui)


run("link_lifecycle", body)
