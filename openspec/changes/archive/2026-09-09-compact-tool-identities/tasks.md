## Tasks
- [x] Cover skill and hub header identities with lifecycle tests and update the presenter.
- [x] Remove diff footer spacing, retaining header expansion and rounded clipping.
- [x] Update owner contracts; run UI tests, build and native review.

## Evidence
- Red tests reproduced missing Skill identity in the UI presenter and Rust/edge sanitizers; green after the bounded identifier whitelist.
- UI: 1209 passed. Doc: 121 unit + 1 integration passed. Edge: 39 unit + 11 workerd passed; typecheck passed. Native build passed.
- Native isolated mock review: Skill brainstorming and wait 2 jobs are visible; short and long diff viewports end without the black footer. Existing header disclosure remains interactive.
- Historical Skill calls already stored without identifiers retain the Skill fallback; no journal migration was performed.
