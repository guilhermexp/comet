# Verification

- `cargo test -p zeron-harness`: 162 passed, 0 failed.
- `the_preview_gate_is_the_commit_window` pins the gate to
  `zeron_doc::STREAM_COMMIT_MS` and asserts it is non-zero, so neither the drift
  nor an accidental removal can pass.
- `small_chunks_refresh_after_time_budget_and_flush_the_final_tail` had a
  hardcoded 110ms sleep that silently depended on the old 100ms gate. It now
  derives the sleep from `PREVIEW_GATE`.
- The existing linearity suites
  (`claude_progressive_megabyte_is_linear_and_coalesced`,
  `omp_progressive_megabyte_is_linear_and_coalesced`) still pass; they are what
  proves the gate cannot simply be deleted.
- `cargo fmt --all -- --check`: clean.
- `openspec validate align-preview-cadence-to-commit-window --strict`: valid.
- The live cadence has NOT been reviewed in the running app. This changes timing,
  which no unit here can judge; the aliasing analysis came from reading the two
  constants, not from an instrumented capture.

## Correction to the earlier recommendation
The analysis first proposed REMOVING the 100ms gate as redundant. That was
wrong: the gate bounds the emitted event stream, not only the doc commit, and
the linearity suites cap a streamed megabyte at 70 refreshes. Removing it would
emit one event per delta. Aligning the two periods is the fix; deleting one of
them is not.

## Flaky tests observed (pre-existing, not from this change)
Under a full `cargo test --workspace` run, three tests failed and then passed
3/3 in isolation:
- `zeron-harness`: `claude_progressive_megabyte_is_linear_and_coalesced`,
  `omp_progressive_megabyte_is_linear_and_coalesced` — they push 512KiB/1MiB
  through a WALL-CLOCK gate in a tight loop, so machine load decides how many
  emissions happen and the byte-ratio assertion swings with it. A time-gated
  pipeline measured by a volume assertion.
- `zeron-engine`: `holder_probe_reports_pid_without_disturbing_the_lock`.
Not addressed here; recorded so the next run does not re-diagnose them.
