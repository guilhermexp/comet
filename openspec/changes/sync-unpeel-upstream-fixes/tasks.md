# Tasks

## 1. Take the fixes

- [x] 1.1 Replace `menu_prompt.rs` with the upstream file (passive-prefix rule
      plus its two tests).
- [x] 1.2 Apply the `sanitize_transcript_user_text` hunk in `transcripts/mod.rs`
      (`inner_xml_tag`, the three injected prefixes) and keep `shell_words`
      `pub(crate)`.
- [x] 1.3 Add `git_head_branch` to `controller_host.rs`, emit `gitBranch` in the
      project projection, and take its test.
- [x] 1.4 Add `--noproxy '*'` to the loopback POST in the shared script and in
      every per-runtime `lifecycle.sh`, with one conformance test that fails if
      any hook script posts to loopback without it.

## 2. Verification

- [x] 2.1 `cargo test -p unpeel-core` in `third_party/unpeel/crates`:
      668 passed, 0 failed with `--test-threads=1`. In parallel,
      `hook_assets::tests::gemini_hook_script_preserves_provider_metadata_for_lifecycle_events`
      fails intermittently on shared hook state — pre-existing, unrelated to
      these files, and it passes on its own.
- [x] 2.2 `cargo test -p zeron-workers-unpeel` (all green) and
      `cargo test -p zeron-ui` (1155 passed, 1 failed: the pre-existing
      `weekly_tone_neutral_when_no_usage_or_no_weekly_window` from `804d83fc`).
- [x] 2.3 `cargo fmt` on both workspaces.

## 3. Closeout

- [x] 3.1 Refresh `third_party/unpeel-upstream.toml` (upstream reachable again,
      base revision no longer resolves).
- [x] 3.2 Correct the "upstream stopped existing" note in
      `crates/workers-unpeel/AGENTS.md`, and drop the `git_branch`-is-always-None
      gotcha from `crates/ui/AGENTS.md`.
- [ ] 3.3 Archive once verified.
