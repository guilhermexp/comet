# Tasks

- [x] Record baseline UI and engine gates.
- [x] Commit immutable native-layout RED regression against baseline product.
- [x] Add one local multi-pin watch and right-aligned exact-path totals.
- [x] Cover updates, clean/unavailable folders, Worker worktrees and collapsed lifecycle; cover engine multi-pin lifetime.
- [x] Update DOX and validate OpenSpec; keep change active.
- [x] Run final gates and capture original dev-demo before/after in isolated profiles.

## Evidence

- Original RED commit: `57bc7ad4`; fixture-only correction authorized by Main: `57f96f18` (no history rewrite).
- Final frozen fixture fails by assertion against baseline product: `.tmp/verify/WT-20261011-comet-projects-worked-mostra-das-alteracoes-de-cada-projeto/red-v2.log`.
- Implementation: `ec3317c4`; UI gate: 2263 passed, 1 intentionally filtered; engine: 577 passed, 4 ignored.
- Native Xvfb + llvmpipe original dev-demo: `before-fixture.png` (no totals), `after-fixture.png` (+5 / −2, clean row blank), `after-live-update.png` (+6 / −2 after one added untracked line), in the same verification directory.
- Keep this change active; do not archive (ticket instruction).
