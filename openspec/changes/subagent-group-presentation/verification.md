# Verification

- Projection regression covers streaming and completed entries, bound/unbound children, failed task wrappers and unrelated ID prefixes.
- `openspec validate subagent-group-presentation --strict`: passed.
- `rustfmt --edition 2024 crates/ui/src/transcript.rs` and `git diff --check`: passed.
- Native appearance remains pending: CUA access to the installed application previously failed with `cgWindowNotFound` in this session. No screenshot or visual validation is claimed; archive remains pending.
- Verification uses `/tmp/comet-file-chip-check` to exclude unrelated ongoing workspace mutation RPC/proto edits in the main checkout.
- `cargo test -p zeron-ui --lib transcript::`: 134 passed in the isolated checkout.

## 2026-09-15 — individual containers

- Moved batch background/padding/radius onto individual agent links; standalone links no longer request full width.
- `cargo build -p zeron -q`: passed (outside sandbox; sandbox sccache was denied).
- `openspec validate subagent-group-presentation --strict`, targeted rustfmt and `git diff --check`: passed.
- Native demo review is pending; the installed app screenshot does not establish that it is running the edited build.

### Follow-up: keep agents side by side

- Individual fills increased to 8% (12% hover). Batch links no longer wrap; long names truncate and links shrink, with an 8px gap.
- `cargo build -p zeron -q`, strict OpenSpec validation, rustfmt and `git diff --check`: passed after the follow-up.
- Native visual verification remains blocked: opening the isolated QA app via CUA returned `timeoutReached` (-10005). Archive remains pending; the installed app screenshot is not evidence of the new build.

### Follow-up: reuse Chat input glass token

- User screenshot confirms individual agents remain side by side, but the fixed text-opacity fill looked too solid.
- Both batch and standalone links now use `theme.composer_glass_bg()`, matching the composer pill; hover no longer replaces that fill.
- `cargo build -p zeron -j 2 -q`: passed. Strict OpenSpec validation, targeted rustfmt and `git diff --check`: passed.
- New token appearance still requires native visual confirmation; previous screenshot predates this correction.
