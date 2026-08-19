# Workers Guarded Hibernation Plan

**Status:** Separate destructive phase — not active in the current build.

**Goal:** Allow an explicitly opted-in Comet orchestrator to stop only proven-idle,
restorable background workers and resume them without losing transcript or terminal
history.

**Safety contract:** macOS pressure alone never authorizes hibernation. The policy
defaults off, never selects the visible session, and fails closed on any lifecycle,
output, transcript, PID, process-group, generation, or provider-identity uncertainty.

## Task 1: Persisted hibernation marker and failure states

- Add a Comet-owned marker containing session ID, provider identity, runtime
  generation, verified PID/start time, output offset/fingerprint, transcript
  fingerprint, and hibernated timestamp.
- Add explicit `hibernating`, `hibernated`, `hibernation_failed`, `resuming`, and
  `resume_failed` states; never map failures to idle/running.
- TDD migration, corruption, and stale-marker behavior.

## Task 2: Pure eligibility planner

- Require policy enabled, non-selected session, `activity == idle`, native resume
  capability, stable provider session identity, no pending input, configured idle
  age, and live-idle count above the configured cap.
- Reject working, blocked, attention, archived, terminal-only, incomplete resource
  attribution, and already transitioning sessions.
- Return typed rejection reasons for diagnostics and tests.

## Task 3: Confirmation window

- Capture PID/start time/process group/runtime generation, output offset and tail
  fingerprint, transcript fingerprint, activity timestamp, and input queue state.
- Re-sample after a bounded confirmation interval.
- Cancel when any field changes; never retry automatically in the same cycle.

## Task 4: Final fail-closed revalidation and stop

- Repeat every ownership/transcript/identity check immediately before action.
- Use only the existing owned `SessionAction::Stop`; never send an unverified signal.
- Persist the hibernation marker before exposing `hibernated`; preserve output and
  transcript journals.
- Bound each policy pass to one session initially.

## Task 5: Resume on selection

- Selecting a marked session invokes the existing provider-native `ResumeAgent`
  path with the stable provider identity.
- Clear the marker only after a new verified runtime generation is live.
- On failure, preserve marker/history and show `resume_failed` with a manual retry.

## Task 6: On-demand settings and diagnostics

- Activate the existing policy fields only after Tasks 1-5 ship.
- Keep status/details inside `Settings -> Resources`; do not add CPU/RAM or
  hibernation badges to terminal/sidebar/titlebar/menu bar.
- Add CLI inspection for eligibility and markers; no CLI action without an explicit
  command.

## Gates

- Unit tests cover every exclusion, confirmation invalidation, PID reuse, transcript
  drift, output drift, input pending, failure state, and resume-marker transition.
- macOS integration fixture proves disabled policy performs zero lifecycle actions.
- Real provider tests prove terminal output/transcript remain complete after
  hibernate/resume.
- Native visual QA confirms no new normal-operation monitor or badge.
- A separate explicit approval is required before enabling automatic lifecycle
  mutation in implementation.
