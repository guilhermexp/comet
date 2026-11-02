# Design

## Context

See proposal.md. Each fix reuses an existing mechanism, as follows.

## Decisions

- **GH1: resolve twice, same rules.**
  - `resolve_update_plan` holds the existing discovery, the "installation changed since it was checked" guard and `update_plan`.
  - It runs before waiting, so a refused install never waits, and again right after the exclusive lease and the cancellation check, before the owner gate and any mutation.
  - A refusal after waiting releases the update like a cancellation: previous phase, no error state, and the reason is returned. That mirrors the same refusal before waiting.
  - A replacement recorded by the waiting reconciliation (GF10) passes the guard and gets its own plan. A replacement nothing probed is refused and rechecked.
  - Rejected: re-planning inside the reconciliation, which would act without the exclusive lease.
- **GH5: one sync point on the Chat switch.** `on_state_changed` calls the existing `sync_browser_details` once the destination's panels are restored, inside the selected-chat-change branch only.
  - A restored Browser is then a new identity, so Details closes once.
  - A non-Browser destination clears the identity.
  - Later notifications do not pass through that branch, so a manual reopen survives.
  - Rejected: substituting `right_pane_shown`, because activation sync precedes the promotion of a hidden pane.
- **GH2/GH3: live geometry and live visibility.**
  - `Fixture::probes` derives the control centers from the production widget constants and the current placement, and the page point from the page's live width and band.
  - Each sample reads `fixture_native_visible()` itself.
  - The negative mode only presents the page hidden for one nonzero exit sample, through the fixture's own presentation input.
- **GH4: `unless_vanished`** maps only `ReplyError::X11Error` with `ErrorKind::Window` to "skip"; the root query and every other error still fail. `find_own_window` takes the candidates, so a disposable smoke can inject a destroyed id.
- **GH6:** `cfg(unix)` on the default-root stage only. Product home resolution is unchanged.

## Risks / Trade-offs

- **Linux Xvfb without a window manager ignores resizes.** → The fixture records the real viewport and its probes follow it; the resize proof is Mac acceptance.
