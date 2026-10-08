# zeron-mobile

Pai: [`../AGENTS.md`](../AGENTS.md)

## Purpose

Rust core and UniFFI boundary consumed by the native iOS app. It owns client-side transcript rows, text layout, and mobile projections of shared protocol data; Swift owns navigation and UIKit rendering.

## Ownership

This crate owns `Cargo.toml` and `src/`. The Swift app and generated binding lifecycle live in [`../../apps/ios/AGENTS.md`](../../apps/ios/AGENTS.md) and [`../../scripts/AGENTS.md`](../../scripts/AGENTS.md).

## Local Contracts

- `layout` emits platform-neutral row text, links, and widgets. The iOS transcript renderer consumes these values; keep labels, colors, preview references, and row geometry consistent with that contract.
- Canonical attachment links are parsed and paired through `zeron-proto`. Use `pair_attachment_mentions` so each attachment path is consumed at most once; image chips retain their full upload path for preview activation, and unmatched uploads remain in the image or file strip.
- Copy text uses the canonical attachment prompt projection, while display text may replace the links with chips.

## Work Guidance

- Keep attachment matching in `zeron-proto`; do not duplicate filename sanitization or infer upload identity from display labels in this crate.
- Add transcript rendering regressions beside the relevant layout behavior in `src/layout/tests.rs`.
- Regenerate and verify Swift bindings through the iOS build scripts when changing exported UniFFI APIs.

## Verification

Run focused Rust checks through the repository Cargo wrapper on macOS/Linux, for example `scripts/cargo-verify.py -- cargo test -p zeron-mobile layout::tests::sanitized_attachment_name_collision_keeps_only_the_unpaired_upload_pill`.

| Layer / path | Tier | How to run |
|---|---|---|
| `src/layout/**` | unit | `scripts/cargo-verify.py -- cargo test -p zeron-mobile layout::tests` |
| `src/client_ffi/voice.rs` | unit | `scripts/cargo-verify.py -- cargo test -p zeron-mobile client_ffi::voice` |

## Child DOX Index

No child docs; `layout` and `client_ffi` are modules within the mobile crate rather than independently owned packages.
