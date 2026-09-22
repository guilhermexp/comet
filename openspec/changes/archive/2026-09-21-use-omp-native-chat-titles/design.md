## Context
The installed OMP 18.2.7 supports /rename without arguments, backed by generateSessionTitle and configured tiny/commit/smol roles. RPC exposes get_state.sessionName and switch_session. The source checkout found in tmp is older than the installed CLI; do not replace the installed binary.

## Decisions
The engine opts in through host-local RunControls only while the Chat is untitled and no title-harness override is configured. Reuse the existing OMP RPC process after a successful native agent_end, before shutdown. Read sessionName first; when absent and the session has messages, invoke /rename with no arguments. Drain command output privately and publish only a host-local NativeTitle event before Done. The engine consumes it before journaling or broadcast, then reuses its existing guarded Chat/branch rename code. No title sidecar, model selection, coding prompt or extra provider request path is introduced.

Automatic OMP Chats no longer launch the Comet title generator at dispatch. If native metadata is absent at completion, only the existing textual fallback is used. Explicit alternative title harness preferences retain their existing behavior. Recap is a distinct operation and is not converted into a title.

## Risks and verification
The native rename runs after the main model completes and must have a bounded wait; cancellation and native failures must preserve the run outcome. Read-only fixtures validate command sequencing, private command output, existing title reuse and failed generation. Installed OMP 18.2.7 metadata and an isolated-session smoke confirm /rename without arguments; the older source checkout is not the runtime contract. Title generation remains owned by OMP's roles and fallback policy.
