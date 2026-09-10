## Tasks
- [x] Test grouping boundaries and implement a native sibling row.
- [x] Validate status and individual navigation with a mock fan-out.
- [x] Keep each larger avatar next to its own name and reuse the avatar in preview tabs.
- [x] Match file card neutral backgrounds to command cards.
- [x] Update owner contracts, run UI tests/build, validate and archive.

## Evidence
- Projection regression: red before grouping, green after; UI suite 1210 passed, harness suite 153 passed.
- Final cargo build passed after preview/background changes; cargo fmt and diff check passed.
- Native mock fan-out: Working and Completed summaries, distinct child previews, 28px avatar/name pairs.
- Native screenshot 2026-09-09 16:59:51: grouped pairs and matching preview avatar.
- Native screenshot 2026-09-09 17:01:34: short and long write/edit cards use neutral fill, semantic diff washes and rounded bottom edges without black bands.
