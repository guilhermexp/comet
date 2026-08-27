# Contributing to Unpeel

Thanks for wanting to help. A few things about this codebase are unusual —
reading this first will save you a closed PR.

## The ground rules

- **Unpeel is never an IDE.** No diff viewers, file trees, editor panes,
  language tooling, or any code-editor chrome — in *any* client. Agents here
  are for every kind of work, and the review surface is the terminal,
  transcripts, and screenshots. PRs adding code-centric UI will be declined
  regardless of quality. (Full philosophy: `AGENTS.md`.)
- **Nothing leaves the user's machines.** Features must not add cloud
  dependencies, telemetry, or server-side state for session/room content. The
  only operated service is Unpeel Link (rendezvous/relay/push), its relay is
  an opaque E2E transport, and everything local/direct works without it.
- **Compatibility is load-bearing.** Unpeel has paying users. Shared on-disk
  contracts (`~/.unpeel/*`), the remote protocol, licensing behavior, and
  resume machinery all have documented invariants — `AGENTS.md` is the map,
  and the `compat_*` TUI test cases are the guard. A failure there means a
  real user's install would break.

## Where contributions land best

**New agent CLI integrations** are the sweet spot: the per-provider knowledge
sits in a handful of choke points and the Swift side is
exhaustive-switch-driven, so the compiler walks you through it. Follow
*Adding a New Agent CLI* in `docs/agents/providers.md`.

Also welcome: Linux host hardening, terminal rendering fixes, provider hook
reliability, docs. For anything architectural, open an issue first — much of
the direction is already decided in `docs/plans/` and it's cheaper to align
before writing code.

## Getting set up

```bash
bun install
bun run dev:native      # macOS app → dist/Unpeel.app (never touch /Applications/Unpeel.app)
bun run dev:website     # unpeel.com dev server
cargo build --manifest-path crates/Cargo.toml   # backend + TUI
```

Know the quirks: dev builds must be started with `bun run dev:native` (stable
code-signing identity — a bare `swift run` breaks Keychain trust), and the
website worker needs the private `unpeel-account` sibling checkout to build —
outside contributors can work on everything else while that stub story lands
(see `docs/plans/open-source.md`).

## Tests

- `cargo test --manifest-path crates/Cargo.toml` — backend (always run after
  touching session launch or hooks)
- `swift build` in `apps/native/UnpeelNative` — the Mac app
- `crates/unpeel-tui/tests/run.sh` — 24 real-PTY end-to-end cases (~7 min;
  `./run.sh <filter>` for a subset)
- `apps/native/verify-attach.sh` / `verify-browser.sh` — end-to-end smoke
- `cd apps/relay && npm test` — relay protocol/crypto (run after any protocol
  change; the KAT vectors are shared with the Swift tests)

## Style

Match the surrounding code — its naming, idiom, and comment density. Comments
state constraints the code can't show, not narration. Commit messages explain
*why*.
