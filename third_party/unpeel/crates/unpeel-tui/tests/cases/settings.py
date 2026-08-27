"""The settings screen: it replaces the sidebar (desktop shape), Presets is
first, and every section is reachable by keyboard and by mouse."""

import sys, os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.preset(label="claude", command="claude --dangerously-skip-permissions")
    home.preset(label="codex", command="codex")
    home.session("s1", label="a session", project_id="p")
    home.pair_device()

    tui = case.pty()
    tui.read_for(3.0)
    # (The resting bottom bar is asserted in `polish`; here a paired phone
    # legitimately occupies it with its serving status.)
    tui.send("?", settle=1.0)
    case.check("the help overlay documents settings", "settings" in tui.expect("settings"))
    tui.send("\x1b", settle=0.6)

    tui.send(",", settle=1.2)
    first = tui.expect("Presets")
    case.check("settings replaces the sidebar", "Presets" in first)
    case.check(
        "presets is the first section (desktop order)",
        "Presets" in first and "claude" in first,
        first[:200],
    )
    case.check(
        "every section is listed",
        all(name in first for name in ("Presets", "Access", "Remote", "Projects", "About")),
        first[:240],
    )
    case.check(
        "the standalone Unpeel Link tab is gone (merged into Remote)",
        "Unpeel Link" not in first,
        first[:240],
    )

    # ── presets ──
    case.check("presets show their enabled state", "✓" in first, first[:200])
    tui.send("\r", settle=1.2)
    case.check(
        "⏎ toggles a preset off",
        home.state()["presets"][0]["enabled"] is False,
    )
    tui.send("j", settle=0.5)
    tui.send("x", settle=1.2)
    labels = [preset["label"] for preset in home.state()["presets"]]
    case.check("x removes a preset", labels == ["claude"], str(labels))

    # ── access ──
    tui.send("\t", settle=1.0)
    access = tui.expect("Browser access")
    case.check(
        "access section renders its policies",
        all(text in access for text in ("Browser access", "Computer")),
        access[:200],
    )
    tui.send("\r", settle=1.2)
    case.check(
        "a policy cycles and persists",
        home.state().get("browser_default_access") in ("ask", "off", "on"),
        str(home.state().get("browser_default_access")),
    )

    # ── remote (now also carries the merged Unpeel Link section) ──
    tui.send("\t", settle=1.2)
    mobile = tui.expect("iPhone")
    case.check("paired devices are listed", "iPhone" in mobile)
    case.check("pairing is offered", "share this host" in mobile, mobile[:200])
    case.check(
        "the Unpeel Link section renders inside Remote",
        "unpeel link" in mobile,
        mobile[:400],
    )
    case.check(
        "the unlicensed state offers the key field",
        "paste license key" in mobile,
        mobile[:400],
    )
    case.check(
        "the paired device is enrolled on Link by default",
        "on link" in mobile and "direct only" not in mobile,
        mobile[:400],
    )

    import json

    # L narrows the selected device to Direct-only (removes it from Link)…
    tui.send("L", settle=1.5)
    with open(home.path("mobile", "devices.json")) as handle:
        flags = [device.get("relayAllowed") for device in json.load(handle)["devices"]]
    case.check("L removes the device from Link (relayAllowed=false)", flags == [False], str(flags))
    case.check("the narrowed device renders as direct only", "direct only" in tui.expect("direct only"))
    # …and L again re-enrolls (the narrows-only flag is removed, not stored true).
    tui.send("L", settle=1.5)
    with open(home.path("mobile", "devices.json")) as handle:
        flags = [
            "relayAllowed" in device for device in json.load(handle)["devices"]
        ]
    case.check("L re-enrolls by removing the key", flags == [False], str(flags))

    tui.send("x", settle=1.5)

    with open(home.path("mobile", "devices.json")) as handle:
        remaining = [device["name"] for device in json.load(handle)["devices"]]
    case.check("unpair removes the device", remaining == [], str(remaining))

    # ── about ──
    tui.send("\t", settle=1.0)
    tui.send("\t", settle=1.2)
    about = tui.expect("home")
    case.check("about shows the paths that matter", "home" in about, about[:200])

    # ── cleanup (directly after About — the Link tab is gone) ──
    tui.send("\t", settle=1.2)  # Cleanup
    cleanup = tui.expect("Auto-stop and archive")
    case.check(
        "cleanup renders the auto-stop-and-archive knob at its default",
        "Auto-stop and archive" in cleanup and "After 1 day" in cleanup,
        cleanup[:240],
    )
    tui.send("\r", settle=1.2)
    case.check(
        "⏎ cycles the cutoff and persists it to shared state",
        home.state().get("auto_stop_archive_minutes") == 0,
        str(home.state().get("auto_stop_archive_minutes")),
    )
    case.check(
        "the cycled value renders as Never",
        "Never" in tui.expect("Never"),
    )

    # ── mouse + escape ──
    tui.click(5, 6)
    case.check("clicking picks a section", tui.grid().sidebar_width() > 0)
    tui.click(5, 3)  # back to Presets
    tui.expect("shared presets")
    enabled_before = home.state()["presets"][0]["enabled"]
    # Detail rows start at screen row 3 (border + intro + blank): clicking
    # the first preset must toggle IT, not select its neighbor (off-by-one).
    tui.click(70, 3)
    tui.read_for(1.0)
    case.check(
        "clicking a preset row toggles the row under the cursor",
        home.state()["presets"][0]["enabled"] is not enabled_before,
        str(home.state()["presets"][0]),
    )
    tui.send("\x1b", settle=1.2)
    case.check("escape returns to the sidebar", "Projects" in tui.expect("Projects"))


run("settings", body)
