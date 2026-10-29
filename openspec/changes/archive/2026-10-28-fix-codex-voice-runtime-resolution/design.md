# Design

## Context

See proposal.md. `installed_helper` currently validates only the physical package adjacent to the chosen Chat executable. Both the local host alias in `realtime_host.rs` and desktop `media_helper` depend on that assumption. The observed npm launcher has no adjacent manifest. The active app-server-daemon 0.165.0 package has the official layout and OpenAI-signed Codex and voice helper binaries; the standalone `current` target is absent. This establishes a discovery defect, not why the previous standalone installation disappeared.

## Goals / Non-Goals

**Goals:** One deterministic media resolver shared by local host and desktop, with offline filesystem fixtures that cannot fall back to the developer's actual installation.

**Non-Goals:** Changing the CLI resolver, provider calls, auth, updater policy, installing packages, scanning cached releases or borrowing plugin copies. UI error taxonomy for unrelated initialization failures is unchanged.

## Decisions

- Keep explicit `ZERON_VOICE_MEDIA_DIR` first and fail closed through existing `verify_runtime`.
- Try the supplied Chat executable's native package before `CODEX_HOME/packages/{standalone,app-server-daemon}/current/bin/codex` (platform suffix on Windows). Without `CODEX_HOME`, use the OS home plus `.codex`; empty/relative homes do not trigger a workspace-relative search. Read only the two active entries, never enumerate releases or plugins. Stable ordered preference avoids choosing a newer cached runtime unexpectedly.
- Keep `installed_helper` as the direct package validator; extend its metadata checks to Codex identity, exact entrypoint/resources paths and current target, in addition to layout/version and canonical helper containment. No executable or manifest values are used as arbitrary search paths. Reject non-executable helpers on Unix.
- Inject home and override inputs into a private resolver seam. The public function reads environment once; unit fixtures use the seam and never mutate process environment. Host adapter delegates to the public resolver, retaining its current signature.
- Missing candidates allow trying the next active package. If all fail, preserve the preferred candidate's rejection (or an unsupported fallback rejection when the preferred package was absent). An explicit override never participates in that fallback.

## Risks / Trade-offs

- The Codex package manager can remove a selected release after discovery → the existing bounded helper startup returns an error; never install or retry an unvalidated location.
- Package metadata describes a locally trusted installation; it is not a signature trust root → retain process isolation and native runtime verification, and separately verify signatures and manifest hashes for the observed device before the smoke. No arbitrary plugin/vendor search is added.
- Native initialization success does not prove a provider call or microphone operation → report those as untested and reserve Cargo checks for the parent, as instructed.

## Migration Plan

No data/configuration migration. Revert the resolver and adapter edits to restore prior discovery. Parent runs the grouped Cargo checks and review gate before integration; archive only after acceptance.
