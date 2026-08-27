"""Session context menus copy the shared Markdown transcript to the
controller terminal's clipboard, with the same range flyout as desktop."""

import base64
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run  # noqa: E402


OSC52 = re.compile(rb"\x1b\]52;c;([A-Za-z0-9+/=]+)\x07")


def copied_text(raw):
    matches = OSC52.findall(raw)
    if not matches:
        return None
    return base64.b64decode(matches[-1]).decode("utf-8")


def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")

    # Provider paths are deliberately trust-scoped. Give the fixture its own
    # HOME so the real resolver accepts this as a Claude transcript without
    # reading anything from the developer's actual provider history.
    transcript_dir = home.path(".claude", "projects", "-tmp")
    os.makedirs(transcript_dir, exist_ok=True)
    transcript_path = os.path.join(transcript_dir, "conversation.jsonl")
    records = [
        {
            "type": "user",
            "message": {"role": "user", "content": "copy this question"},
        },
        {
            "type": "assistant",
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "copy this answer"}],
            },
        },
    ]
    with open(transcript_path, "w") as handle:
        for record in records:
            handle.write(json.dumps(record) + "\n")

    home.session(
        "s1",
        label="agent conversation",
        command="claude --dangerously-skip-permissions",
        project_id="p",
        extra_manifest={"provider_transcript_path": transcript_path},
    )

    tui = case.pty(env={"HOME": home.root})
    tui.read_for(3.0)
    grid = tui.grid()
    session_row = next(
        row for row in range(grid.rows) if "agent conversation" in grid.row(row)
    )
    tui.click(5, session_row, button=2)
    menu = tui.expect("Copy transcript")
    case.check(
        "agent session menu offers transcript copy",
        "Copy transcript" in menu,
        menu[:240],
    )

    # Stopped root session: Rename, Pin, Resume, Copy transcript.
    tui.send("jjj", settle=0.3)
    tui.send("\r", settle=0.5)
    ranges = tui.expect("Last 20 entries", "Whole conversation")
    case.check(
        "copy transcript offers the desktop ranges",
        "Last 20 entries" in ranges
        and "Last 50 entries" in ranges
        and "Whole conversation" in ranges,
        ranges[:240],
    )

    start = len(tui.buffer)
    tui.send("\r", settle=0.1)
    markdown = tui.wait_for(lambda: copied_text(tui.buffer[start:]), timeout=10)
    case.check(
        "copy publishes Markdown through the terminal clipboard",
        markdown is not None
        and "# agent conversation" in markdown
        and "## User\n\ncopy this question" in markdown
        and "## Assistant\n\ncopy this answer" in markdown,
        markdown or "no OSC 52 clipboard payload",
    )
    case.check(
        "copy confirms completion",
        bool(tui.wait_for_text("transcript copied", timeout=5)),
    )


run("transcript_copy", body)
