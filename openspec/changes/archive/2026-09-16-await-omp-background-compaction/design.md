## Context

The installed OMP binary's compact command uses runCommandInBackground. RPC returns agentInvoked:false immediately and later emits command_output with `Compaction complete. Tokens: before -> after (saved delta).` or `Compaction failed: reason`. get_state exposes the current contextUsage. Previous tests incorrectly represented compaction as synchronous.

## Decisions

Recognize a compaction prompt at the vendor boundary. Its ACK does not end the run; terminal command_output does. Ignore intermediate agent_end during this operation. Preserve the existing prompt deadline after the ACK and cancellation path. At completion reuse get_state publication before Done, suppressing the raw CLI completion prose because the UI already owns the clean marker. The marker uses the pre-command snapshot and freshly published post-command usage and accepts increases/equality. Capture the post-command snapshot synchronously when the UI observes completion instead of reading mutable state 800ms later.

## Risks / Trade-offs

OMP does not expose a structured local-command lifecycle here, so recognition of its terminal command text is vendor-specific and fixture-covered. Other nonempty command output is reported as failure (including argument validation); absent terminal output is bounded by the deadline. Automatic compaction outside an explicit compact prompt remains outside this change. The before count remains the context snapshot shown when the command was sent; no new sync fields are introduced.

## Verification

Asynchronous fake RPC must answer get_state with the old count until the background operation actually completes; tests cover terminal output before/after ACK, increased tokens, error, cancellation and timeout. Verify native UI marker/gauge through existing state and marker tests and build the app.

Verified: full omp_rpc suite 55 passed / 2 ignored; final targeted local_compaction suite 5 passed; UI marker test 1 passed; cargo build and git diff --check passed. An isolated synthetic session against the installed OMP confirmed ACK agentInvoked:false precedes terminal output and get_state changes from 103830 to 102555 afterward. Native end-to-end UI interaction was not repeated in this iteration.
