"""Pairing Controllers from the terminal, verified against the SHIPPED Swift
client and crypto rather than a reimplementation.

`pairclient/main.swift` is compiled against the same `UnpeelShared` sources
the iPhone app uses, so this proves byte-compatibility: if the Rust pairing
handshake ever drifts from the shipped client, this case fails instead of a
user's phone. A tiny generated entry point also exercises the shared
`RemotePairingClient` with a macOS Controller identity against the first-device
`unpeel pair` path. On a machine without swiftc (Linux) the crypto half is
skipped with a NOTE — the QR half still runs."""

import sys, os, re, shutil, subprocess, json

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run, REPO  # noqa: E402

TESTS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SHARED = os.path.join(REPO, "apps", "shared", "UnpeelShared", "Sources", "UnpeelShared")


def build_pairclient(destination):
    if not shutil.which("swiftc"):
        return None
    sources = [
        os.path.join(TESTS, "pairclient", "main.swift"),
        os.path.join(SHARED, "RemoteControlProtocol.swift"),
        os.path.join(SHARED, "RelayProtocol.swift"),
    ]
    if not all(os.path.exists(source) for source in sources):
        return None
    os.makedirs(destination, exist_ok=True)
    binary = os.path.join(destination, "pairclient")
    result = subprocess.run(
        ["swiftc", "-O", "-o", binary, *sources],
        capture_output=True, text=True, timeout=300,
    )
    return binary if result.returncode == 0 else None


def build_mac_pairclient(destination):
    """Compile the shipped Apple pairing client with a macOS identity.

    The entry point only supplies identity and prints the result; request
    sealing, HTTP exchange, response opening, and Host binding all remain in
    UnpeelShared's production RemotePairingClient.
    """
    if not shutil.which("swiftc"):
        return None, "swiftc unavailable"
    sources = [
        os.path.join(SHARED, "RemoteControlProtocol.swift"),
        os.path.join(SHARED, "RelayProtocol.swift"),
        os.path.join(SHARED, "RemotePairingClient.swift"),
    ]
    missing = [source for source in sources if not os.path.exists(source)]
    if missing:
        return None, f"missing shared source: {missing[0]}"
    os.makedirs(destination, exist_ok=True)
    entrypoint = os.path.join(destination, "macpairclient.swift")
    with open(entrypoint, "w") as handle:
        handle.write(
            """import Foundation

@main
struct MacPairClient {
    static func main() async {
        guard CommandLine.arguments.count == 2,
              let payload = RemotePairingCode.decode(CommandLine.arguments[1])
        else {
            print("DECODE FAILED")
            return
        }
        let device = RemoteDeviceIdentity(
            id: "test-macos-controller-1",
            name: "Test Mac Controller",
            platform: "macOS",
            appVersion: "9.9.9"
        )
        do {
            let paired = try await RemotePairingClient().pair(
                payload: payload,
                device: device
            )
            print("MAC PAIRED ok deviceID=\\(paired.deviceID) hostID=\\(paired.macID)")
        } catch {
            print("PAIR FAILED: \\(error)")
        }
    }
}
"""
        )
    binary = os.path.join(destination, "macpairclient")
    result = subprocess.run(
        ["swiftc", "-O", "-o", binary, entrypoint, *sources],
        capture_output=True, text=True, timeout=300,
    )
    error = (result.stderr or result.stdout).strip()
    return (binary, "") if result.returncode == 0 else (None, error)


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.session("s1", label="a session", project_id="p")

    # ── with no paired device and no server, pairing explains itself ──
    plain = case.pty()
    plain.read_for(4.0)
    plain.send("m", settle=1.2)
    case.check(
        "pairing without a phone server explains why",
        "phone serving is off" in plain.expect("phone serving is off"),
    )
    plain.send("q", settle=1.0)
    plain.exited()

    # ── first-device pairing: a macOS Controller uses `unpeel pair` ──
    mac_binary, mac_build_error = build_mac_pairclient(home.path("build-mac"))
    mac_registered = []
    if not mac_binary:
        if not shutil.which("swiftc"):
            case.note("swiftc unavailable — skipped the macOS Controller pairing check")
        else:
            case.check(
                "the shipped macOS pairing client compiles",
                False,
                mac_build_error[-300:],
            )
    else:
        probe_bin = home.path("dns-probe-bin")
        probe_log = home.path("dns-sd-args")
        os.makedirs(probe_bin, exist_ok=True)
        dns_sd = os.path.join(probe_bin, "dns-sd")
        with open(dns_sd, "w") as handle:
            handle.write(
                "#!/bin/sh\n"
                "printf '%s\\n' \"$*\" > \"$UNPEEL_TEST_DNS_SD_LOG\"\n"
                "exec /bin/sleep 300\n"
            )
        os.chmod(dns_sd, 0o755)
        pair_cli = case.pty(
            args=("pair",),
            rows=50,
            cols=160,
            env={
                "PATH": probe_bin + os.pathsep + os.environ.get("PATH", ""),
                "UNPEEL_TEST_DNS_SD_LOG": probe_log,
                "UNPEEL_TEST_LAN_ADDRESS": "127.0.0.1",
            },
        )
        pair_cli.read_for(4.0)
        advertised = pair_cli.wait_for(lambda: os.path.exists(probe_log), timeout=5.0)
        try:
            with open(home.path("mobile", "mac-id")) as handle:
                first_mac_id = handle.read().strip()
            with open(probe_log) as handle:
                dns_sd_args = handle.read().strip()
        except FileNotFoundError:
            first_mac_id = ""
            dns_sd_args = ""
        case.check(
            "first-device Bonjour advertises the persisted Host identity",
            bool(advertised)
            and bool(first_mac_id)
            and f"macid={first_mac_id}" in dns_sd_args,
            dns_sd_args,
        )
        cli_match = re.search(
            r"UNPEEL:1:[0-9.]+:\d+:[0-9A-F-]+:[A-Z2-7]{26}:\d+",
            pair_cli.all_text(),
        )
        case.check(
            "unpeel pair exposes a first-device pairing code",
            cli_match is not None,
            pair_cli.all_text()[-240:],
        )
        if cli_match:
            paired = subprocess.run(
                [mac_binary, cli_match.group(0)],
                capture_output=True, text=True, timeout=60,
            )
            output = paired.stdout.strip()
            case.check(
                "the shipped macOS Controller pairs over the sealed wire",
                paired.returncode == 0
                and output.startswith("MAC PAIRED ok")
                and "deviceID=test-macos-controller-1" in output,
                output[:240],
            )
            reported = pair_cli.wait_for_text("paired", timeout=10.0)
            exited = pair_cli.exited(timeout=10.0)
            case.check(
                "unpeel pair completes after the first Controller",
                bool(reported) and exited and pair_cli.returncode == 0,
                pair_cli.all_text()[-240:],
            )

        try:
            with open(home.path("mobile", "devices.json")) as handle:
                mac_devices = json.load(handle)["devices"]
        except (FileNotFoundError, KeyError, ValueError):
            mac_devices = []
        mac_registered = [
            device
            for device in mac_devices
            if device.get("id") == "test-macos-controller-1"
        ]
        case.check(
            "the first paired principal is recorded as a macOS Controller",
            len(mac_devices) == 1
            and len(mac_registered) == 1
            and mac_registered[0].get("name") == "Test Mac Controller"
            and mac_registered[0].get("platform") == "macOS"
            and mac_registered[0].get("appVersion") == "9.9.9",
            str(mac_devices),
        )

    # Preserve the existing phone proof on platforms that cannot compile the
    # Apple client (or after a recorded macOS-pairing failure).
    if not mac_registered:
        home.pair_device(token="seed-token", name="seed")

    # ── with a device already paired the server runs, so 'm' offers a QR ──
    port = home.reserve_mobile_port()
    tui = case.pty(
        rows=50,
        cols=160,
        env={"UNPEEL_TEST_LAN_ADDRESS": "127.0.0.1"},
    )
    tui.read_for(6.0)
    tui.send("m", settle=1.5)
    screen = tui.expect("scan with the Unpeel iPhone app")

    case.check("the QR overlay renders", "scan with the Unpeel iPhone app" in screen)
    match = re.search(r"UNPEEL:1:[0-9.]+:\d+:[0-9A-F-]+:[A-Z2-7]{26}:\d+", screen)
    case.check("the pairing code is shown in full", match is not None, screen[:240])
    case.check(
        "the QR itself is drawn",
        any(block in screen for block in ("█", "▀", "▄")),
    )

    if not match:
        return

    binary = build_pairclient(home.path("build"))
    if not binary:
        case.note("swiftc unavailable — skipped the shipped-crypto pairing check")
        return

    code = match.group(0)
    first = subprocess.run([binary, code], capture_output=True, text=True, timeout=60)
    output = first.stdout.strip()
    case.check("the shipped iPhone client pairs", output.startswith("PAIRED ok"), output[:200])
    case.check("it is issued a full-length token", "tokenLen=43" in output, output[:200])
    case.check("and a 32-byte end-to-end key", "e2eLen=32" in output, output[:200])

    with open(home.path("mobile", "devices.json")) as handle:
        devices = json.load(handle)["devices"]
    registered = [device for device in devices if device["id"] == "test-device-1"]
    case.check(
        "the device is registered the way the app records it",
        len(registered) == 1
        and len(registered[0]["tokenHash"]) == 64
        and registered[0]["name"] == "Test iPhone"
        and registered[0].get("appVersion") == "9.9.9",
        str(registered),
    )
    case.check("its e2e key is persisted", os.path.exists(home.path("mobile", "e2e-keys.json")))

    replay = subprocess.run([binary, code], capture_output=True, text=True, timeout=60)
    case.check(
        "a replayed code is rejected",
        "STATUS 401" in replay.stdout and "pairing is not active" in replay.stdout,
        replay.stdout[:200],
    )
    del port


run("pairing", body)
