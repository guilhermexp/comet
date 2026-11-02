# Tasks

- [x] 1.1 Add shared-layer regressions (vocabulary, bounds, live target) and keep existing peer/controller authorization and transcript tests green.
- [x] 1.2 Add `unpeel_core::session_control`; move key encoding, ANSI cleanup, live-target and delivery there with `InputAuthority`.
- [x] 1.3 Migrate peer `sessions` send/read paths and the controller `workers` send/read paths and `controller_api` target validation; delete the duplicated helpers.
- [x] 1.4 Update `third_party/unpeel/AGENTS.md`, `crates/workers-unpeel/AGENTS.md`, provenance.
- [x] 2.1 (MR7) Add shared regressions for target/list/output/transcript/wait and adapter regressions for denial and the task-completion boundary on both tools.
- [x] 2.2 (MR7) Add `read_target`, `hosted_sessions`, `read_output`/`OutputCursor`, `read_transcript`/`TranscriptProfile`, `page_text`, `WaitVocabulary`/`parse_wait_target`/`judge_wait` to `session_control`.
- [x] 2.3 (MR7) Migrate peer list/inspect/read_output/read_transcript/wait_for_status, controller bootstrap enumeration and `/mobile/output`, `/mobile/transcript-markdown`, and controller `wait_for_status`/`read_transcript` paging onto them; keep authorities, profiles, vocabularies and answer shapes separate.
- [x] 3.1 (GR7) `previous_offset` rounds forward to a char boundary so walking back through a non-ASCII transcript covers every byte; forward paging unchanged.
- [x] 3.2 (GR14) One terminal-text cleaner with an explicit carriage-return profile: peers read CR as a line break; the controller keeps in-line CR (as its cleaner always did), so parent notifications keep only the last paint of a redrawn status line.
- [x] 1.5 Verify: full Mac Core lib 740 passed/2 ignored, full Workers package 543 passed, and strict OpenSpec validation. Real Mac smoke: three actual session Hosts/PTYs from the current-source `zeron` binary; controller `workers` send_text/send_keys(ctrl-c), peer `sessions` send_text/send_keys(ctrl+c) and `POST /mobile/write` delivered the exact bracketed-paste and `0x03` bytes to raw-mode receivers. Cross-group Deny returned the actual Never allow policy error, wrote nothing, and a later permitted write arrived. All 21 checks passed; consumer RPC success, denial reason and a full-width zero-orphan process scan were independently verified. Evidence: `.tmp/verify/desktop-corrections/mac/operational-final/{acceptance.json,f4/f4-result.json}`. Hosts were killed through their real sockets and reaped; the disposable driver was removed. This proves key-byte delivery, not SIGINT, and used no provider/engine/private service.

Final Mac package review: `.tmp/verify/desktop-corrections/final-review-receipt.json` records the actual review mode, reviewed tree and automatic/manual summary path.
