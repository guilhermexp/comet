# Terminal UI tests

End-to-end coverage for `unpeel`: each case drives the **real binary** in a
**real PTY** against a **private `UNPEEL_HOME`**, and asserts on what a user
would actually see and on what lands on disk.

```sh
./run.sh                  # everything (~15–20 min on a development Mac)
./run.sh archive unread   # cases whose name contains a filter
./run.sh -v drag          # stream a case's own output
UNPEEL_TUI_SKIP_BUILD=1 ./run.sh   # skip the cargo build
```

`cargo test -p unpeel-tui` runs the unit tests only. The suites here are
opt-in from cargo:

```sh
UNPEEL_TUI_PTY_TESTS=1 cargo test -p unpeel-tui --test pty_suites
```

## Why it is built this way

Three things caused every phantom pass and mystery failure while this was
being written. The harness fixes each one; please don't work around them.

**Fixtures are per-case and built from scratch.** Cases used to share one
`UNPEEL_HOME` and mutate each other's state, which produced both false
passes and false failures depending on run order. `Case` now creates and
destroys its own home.

**Assertions go through `expect()`, not a single frame.** Panes populate
asynchronously — disk polls, bridge round-trips, host spawns. `expect()`
polls fresh frames until the text appears, so a slow machine is slow rather
than red. Reach for `screen()` only for something already on screen.

**Frames are parsed into a real grid.** ratatui paints by jumping the cursor,
so the byte stream is not row-structured: "line 3 of the output" is not "row
3 of the screen". `Screen` reconstructs the grid, which is what makes
column-accurate assertions (`sidebar()` vs `preview()`) trustworthy — and
what makes "this is *not* in the sidebar" mean something.

Two environment facts bite hard:

- **The home path must be short.** A hosted session binds
  `<home>/app-sessions/<uuid>/session.sock`; `sockaddr_un` caps the path near
  104 bytes. macOS `TMPDIR` alone is ~49, which silently breaks every host
  spawn. The runner uses `/tmp/ut-<case>` and the harness refuses a home that
  would overflow.
- **Repaints must be forced.** ratatui emits only changed cells, so the
  harness toggles the window width to provoke a full redraw before reading.

## Writing a case

```python
import sys, os
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from harness import run

def body(case):
    home = case.home
    home.project("p", "unpeel", "/tmp")
    home.session("s1", label="a session", project_id="p", running=True)

    tui = case.pty()
    tui.read_for(3.0)
    case.check("the session is listed", "a session" in tui.expect("a session"))

run("my-case", body)
```

Useful pieces:

| | |
|---|---|
| `case.pty()` | the TUI in a PTY (`send`, `type`, `click`, `drag`, `scroll`, `expect`, `grid`, `sidebar`) |
| `case.app()` | a mock desktop app: owns a port, answers `/mcp/*`, records `calls`; `fail_routes=` simulates an **older** app |
| `case.host(id)` | a fake session host on the control socket — records writes, resizes, and agent-restart generations |
| `case.herdr()` | a fake newline-delimited Herdr Unix socket — records lifecycle reports and replies to the reporter's allowlisted methods |
| `home.session(...)` | a hosted-session dir; `running=True` parks a real pid and binds a socket |
| `home.marker(...)` | shared markers (`archived.json`, `title.json`, `read.json`) |
| `home.reserve_mobile_port()` | publish a phone endpoint (the TUI never writes that file itself) |
| `run_cli(home, [...])` | the headless CLI against the same home |

Name checks as the behaviour a user would recognise ("archive falls back to
the shared marker"), not the mechanism. Pass a `detail` argument — it is
printed only when the check fails, and a screen excerpt there is usually the
difference between a five-second and a fifty-minute diagnosis.

## What is covered

| case | what it protects |
|---|---|
| `sidebar` | projects group sessions; collapse; disappearance |
| `activity` | hook bus, 404 for foreign sessions, spinner, attention, port registry |
| `activity_menu` | sidebar activity spinner/dropdown, unread reveal, shared All recent history |
| `herdr` | aggregate idle/working/blocked reports, privacy boundary, clean lifecycle release |
| `verbs` | live agent-only restart plus archive/remove over the bridge, with confirms |
| `standalone` | the same verbs with **no app at all** — the Linux story |
| `remote_host` | strict SSH Controller scope through the real stdio gateway; remote sidebar/output/input/fit and zero Controller-home state |
| `unread` | derived unread, receipts, no flapping, re-marking |
| `archive` | no footer row; project-menu/keyboard entry, search, restore |
| `transcript_copy` | shared Markdown transcript ranges and terminal clipboard copy |
| `worktrees` | worktrees as inline collapsible folders under their parent |
| `drag` | reorder, blocks moving whole, never crossing projects |
| `mouse` | pane-relative forwarding, selection mode |
| `resize` | fit to pane, window resize, phone takes and returns the grid |
| `dialogs` | rename/preset/project dialogs, including over settings |
| `settings` | sections, preset editing, access policies, unpair |
| `palette` / `recents` | ctrl+K everywhere, archived fallback |
| `addproject` | home-rooted completion |
| `firstrun` | seeding from installed CLIs and real session history |
| `mobile` | bootstrap, output, organization, polite-guest port rules |
| `pairing` | QR + pairing against the **shipped Swift crypto** |
| `approvals` | MCP approvals from the terminal and from a phone |
| `cli` | headless verbs, including live agent-only restart vs stopped terminal Resume |
| `polish` | help, preset order, stars, layout persistence |
| `perf` | idle poll rate and CPU stay low |
| `compat_state` | **upgrade safety**: unmodelled keys, legacy/future manifests, corrupt files |
| `compat_bridge` | **version skew**: a newer TUI against an older app |

The two `compat_*` cases exist because users update the app and the CLI
independently. Treat a failure there as "this would break someone's install",
not as a test needing adjustment.
