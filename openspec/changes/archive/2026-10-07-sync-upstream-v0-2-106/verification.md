# Integration verification

Base: fork b01350d6. Upstream parent: 916cb1ccb1342c2061963a5f6ecf5c977ba46f2c.
All checks below execute the adapted merge worktree, not an upstream checkout.
Final merge identity is recorded in docs/upstream-sync.md; evidence below names the adapted worktree and tested source changes.

## Offline checks

- Python 3.12: voice packaging 6 tests, split-host transport 17 tests PASS.
- System Python 3.9 cannot run the packaging fixtures (`hashlib.file_digest`); the development tool requires Python 3.12+, now documented. No runtime code was changed to hide this requirement.
- Edge: TypeScript typecheck PASS; unit 61 + workerd 25 tests PASS.
- Packaging shell syntax and four microphone/entitlement plists PASS.
- Mobile/iOS: targeted rustfmt and Swift frontend parse PASS; these do not prove Xcode linking or native voice behavior.
- Workflow YAML parsed successfully; remote jobs have not been executed for this merge revision.
- OpenSpec change validation and all 62 canonical specs PASS with strict validation. Two pre-existing Purpose placeholders (global-file-preview and macos-dev-identity) were replaced with descriptions of their existing contracts; requirements/scenarios are unchanged.

Initial workspace/all-targets check failed on the newly required `RunControls.realtime` field in engine recap startup. This is an integration compile defect; it is repaired before rerunning the check.

Core re-review: the bounded account warmup (500 ms) and explicit engine-owned voice root grant resolve the reported findings. The reviewer clarified that `SessionsGrant` already mounts both `comet-sessions` and `zeron`; the initial explanation of a mini-sessions-only toolset was incomplete. The final adaptation disables Worker grants for the dedicated voice coordinator, keeps the MCP map empty and preserves ordinary idle fail-closed authorization. The integrated workspace execution has passed both voice root-grant and shutdown/owner fixtures; complete suite results are recorded below at closeout.

## Integrated evidence already completed

- Vendor zui: 4 window-blur policy tests and both closing-punctuation paths PASS. Native AppKit bundle reports `PASS: native window glass backing lifecycle and configuration`; executable built from the adapted vendored tree, launched through LaunchServices on the graphical macOS session.
- Attachment one-to-one regression: a controlled negative run disabled the paired-upload exclusion and the sanitized-collision fixture failed; restoring the exact original source made the fixture pass. The source was restored before the final workspace snapshot.
- Independent core/UI review approved after root grant, bounded account warmup, shared attachment derivation, both pending-send keys and durable Steer transport fixes. Limited followup review approved iOS artifact copying and Bash 3.2 environment propagation.
- Initial iOS compilation attempts failed before linking because this machine's global sccache rejects the mobile profile's incremental setting. The serial simulator build is retried with explicit `CARGO_INCREMENTAL=0`; the script itself does not force a default or skip the core. A failed Xcode build from the missing core is not counted as simulator test evidence.

## Required integrated checks

- `cargo check --locked --workspace --all-targets` PASS (3m09s) on the initial integrated source snapshot; the subsequent all-targets rerun also passed after the documented corrections.
- Full workspace test compilation/linking completed (74m50s); initial execution: 5,051 PASS, 21 FAIL and 44 pre-existing ignored tests across nine targets. No new ignore or skip was introduced. Isolated reproductions and corrected-group evidence are distinguished below. This first workspace execution is not a green final suite.
- Native desktop app build PASS (19m54s), isolated fork0.2.18 signed bundle created. The combined stage16 then failed on an optional sidebar example whose required feature was not enabled; that failure is not a failure of the app build, and the optional example is not acceptance evidence. Simulator core compilation PASS (mobile profile15m04s), host binding-generator build PASS (21m01s). Swift bindings were regenerated and whitespace normalized by the build script. Xcode simulator linked-unit group PASS: 12 tests, zero failures (51s execution).

The Codex warmup fixture incorrectly collected until stream EOF while the steering mailbox and fake app-server remained open. Codex intentionally parks after a turn. It now requires the first authoritative Completed Done within the original five seconds, then drops the stream; independent review approved cleanup and unchanged protocol assertions. The old binary is retained for a controlled failure, followed by recompilation and the entire Codex test group.

UI execution initially passed 2,222 tests and failed ten. Corrections preserve meaningful assertions: restore Image 1/2/3 identities independently, current chip geometry, lazy path-backed file source bytes, and initialize voice fixtures through the normal shell helper. The oversized external file exposed a genuine missing 24 MiB staging guard; it is repaired along with aggregation of synchronous folder and asynchronous size refusals, keeping the exact existing refusal assertions. Independent review approved these changes. Recompiled all-target check PASS (1m28s). The rebuilt complete UI group PASS: 2,232 tests, zero failures (58.07s); all ten initial UI failures are resolved. Rebuilt queued-attachments group PASS: 3 tests. Rebuilt Codex group PASS: 32 tests, four pre-existing live-provider ignores; the warmup fixture now finishes at the authoritative turn boundary.

## Acceptance limits

No real provider voice call, microphone grant or physical-device Live Activity was exercised by offline tests. Installed Zeron.app remains running its earlier binary; this merge does not automatically deploy or replace it.

## Isolated reproductions of the original binaries

The exact workspace-built executables were reused before recompilation, preserving the feature graph. There were 59 executions: 16 PASS and 43 FAIL. Expected old-fixture failures are controls, not passing acceptance evidence. Archive installation and stalled OpenCode POST each passed three times; all three WebRTC peer fixtures passed three times each. Shutdown request accounting failed twice out of three, worktree wall-time and OMP coalescing comparison failed three times each. These fixture defects are repaired separately and require rebuilt evidence.

The existing `remote_preview_churn_does_not_accumulate_tasks_or_memory` failed all three isolated runs at backend connection drainage. Independent review found its counter follows real connection lifetimes; changing the zero-drain assertion or forcing Connection: close would hide the contract. The cause is not established. `crates/preview` source/manifests and transport dependency versions are unchanged versus b01350d6, so this is recorded as unresolved baseline acceptance evidence rather than silently counted as green. macOS RSS helper reports zero; these runs do not prove resident memory limits on macOS.

The first rebuilt shutdown fixture passed three repeats using synchronous accepted-connection accounting. The first rebuilt RPC gate fixture failed before its start marker and left an owned terminal process alive; that test helper and its terminal were stopped, making serial stage15 exit143. This is not passing evidence. A followup removes the interactive-shell bang expression, captures bounded failure diagnostics, closes the terminal and shuts down the core; this change did not establish bang parsing as the cause. Stage19 rebuilt shutdown accounting passed three repeats; its RPC fixture failed after47.61s with a returned terminal but no start marker or replay output, and cleanup completed. The reviewed final fixture observes the RPC first under the original four-second deadline (including Git preparation), then observes the start marker with the setup release gate still closed. Its fresh executable and the deterministic OMP coalescing fixture require stage22 runtime proof.

## Native desktop fixture

The adapted bundle `sh.zeron.sync.validation` was launched with isolated UI/engine/Worker profiles and IPC49789. The Git fixture and attachment files were disposable. A first startup stalled in `read_regular_file`/filesystem `open` on a Documents fixture; the same bundle responded after using `/private/tmp` attachments. Cause was not established and no permission was granted to bypass it.

The native screenshot rendered distinct Image1 and notes.txt chips. AX typing retained both references at the caret; native Return cleared the AX composer and produced `Preview Image1.png`. An independent read of the owned engine's public WatchSessions RPC confirmed a completed turn and idle session on the fixture device, with zero provider tokens. Mock1 was visible in the native model selector. This is actual native intake/send evidence, not an internal component setter. The fixture Space was seeded through public Mutate RPC after attempting the UI chooser. Full visual chooser/echo/Files acceptance is not claimed: some screenshot frames did not match the latest AX state. The unavailable Codex voice runtime refusal was visible for this host's npm installation; microphone, actual voice and physical Live Activity are not proven. Only the fixture app/daemon were stopped afterwards.

Stage17 exposed two String `.display()` calls in new failure diagnostics before running the corrected fixtures. Those compile errors are repaired and formatted; stage19 rebuilds the actual executable paths before the three-repeat proof.

### Native repeat with all profiles under private tmp

A repeat used one shared engine/UI profile under `/private/tmp`, with the same adapted bundle and disposable Git/attachment fixtures. Native keyboard deletion removed only Image1; CmdZ restored the exact index1 reference once, retaining notes.txt index2. Native Return completed a Mock1 turn. The screenshot visibly showed one image chip, one file chip and the typed text in the user echo, with an empty composer. Public WatchSessions independently confirmed completed/idle on the fixture device. Files opened through its accessible button and visibly showed alpha.rs/README.md in a single Explorer/Changes surface. This repeat supplies the intake/remove/undo/send/echo/Files visual evidence missing from the first profile. Project chooser, real voice and physical Live Activity remain outside this proof. No production code changed for the repeat.

## Linked iOS interface execution

Stage21 selected four UI tests on simulator D03053E2-677E-4766-9821-BD97433E5578 with the regenerated core. Unknown-Chat route recovery and opening the voice stage passed. Reopening the stage from the live strip failed its line23 assertion, and voice settings failed its line61 assertion for the chosen Maple row. These are actual failures, not a green simulator suite; followup diagnosis and proof are recorded below. Xcode collected simulator diagnostics after the test suite.

## Final core repeat

Stage22 OMP progressive-megabyte coalescing PASS on all three rebuilt-executable repeats. The final workspace/all-targets check PASS (11.87s). The RPC executable rebuilt (14.65s), but its first repeat FAIL (46.71s): the unchanged four-second CreateWorktree deadline expired; after releasing setup it returned a terminal with no start marker/replay output. Cleanup completed and the helper stopped at the failure; two remaining repeats were not run. Neither marker reordering nor the fixture gate establishes the underlying cause. This remains unresolved acceptance evidence alongside preview churn, with no relaxed deadline or assertion. A final green workspace suite is not claimed.

The iOS failures were diagnosed from the finalized xcresult hierarchy: DEBUG preview used an unseeded Chat and real routing refused it; its fixture now references the existing seeded chat-veil. The Voice style row had no subtitle, so selected Maple was not visible; its subtitle now follows selectedStyle/default. Independent review approved both, real routing remains unchanged, and the same assertions plus the linked unit group rerun in stage23. The initial post-test simulator diagnostic collector was stopped after tests finished; xcresult finalized with its actual2PASS/2FAIL, exit65.

## Optional sidebar example

Stage23 enabled the feature required by the optional sidebar example and exposed a real fixture compile error: the merge resolution called Shell::new without the fork WorkersModel argument. It did not reach Xcode. The example now uses the existing shell::test_shell constructor, preserving normal example WorkersModel/resource-monitor initialization; production desktop sources are unchanged. The isolated-package check also forced a narrower dependency-feature rebuild. Stage25 checks the feature with the complete workspace/all-targets feature union, then runs the linked iOS unit/interface proof. Stage24 formatting/source snapshot passed before this one-file correction; stage26 repeats it on the final source.

## Final linked proof and source closeout

Stage25 workspace/all-targets check with zeron-ui/project-palette-fixture PASS (2m18s), including the corrected sidebar example. Xcode simulator test SUCCEEDED: 12 linked unit tests plus4 interface tests, zero failures, zero skips. Unknown Chat recovery and all three VoiceFlow assertions passed; both stage21 failures are resolved without changing the tests. The same Rust simulator library and regenerated Swift bindings were used, with only the reviewed two-file Swift correction since stage20.

Stage26 cargo fmt --all --check PASS; source snapshot checked1,600 Rust/Swift/manifest digests against the tested snapshot plus individually recorded integration amendments, PASS. Strict change and all62 canonical specs PASS after adding the explicit iOS preference/navigation e2e scenario. Final source includes no new ignores or skipped assertions. The optional example correction is fixture-only; the native desktop production source remains the source proved in stage16/native2.

The protected Cargo queue is stopped after completed proof, so its disposable target is cleaned. Only the owned simulator and private fixture apps/daemons are stopped; the installed app and its Chat data remain untouched. The final archive/main commits are recorded in upstream-sync history.

## Merge identity

Actual integration merge: cd38b6dc084ac32b76538926285ccf22c85c3b10, parents b01350d6781332d715f3527de377fbd586278066 and916cb1ccb1342c2061963a5f6ecf5c977ba46f2c. The clean primary main was advanced by fast-forward to that merge. Graft was rebuilt on main after the final source correction. The protected verification wrapper exited0 and removed its target; the owned simulator was already Shutdown. Canonical specs were synchronized before archive, so archive uses --skip-specs to avoid applying the same additions twice. No source change is part of the archival closeout commit.
