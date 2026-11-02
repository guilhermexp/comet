# Design

## Context

See proposal.md for the confirmed findings. Every fix stays inside the code path the finding names; the archived changes are not reopened, only their stale wording is repaired.

## Goals / Non-Goals

**Goals:** fix each confirmed defect at its source with a consumer-visible regression that fails on the previous code.

**Non-Goals:**
- code for refuted GF8/GF11;
- a mixed-version or negotiation scheme for `inherit` (GF18);
- a generic ordering or occupancy framework;
- changes to settled layouts.

## Decisions

- **GF1/GF10: identity before and after the probe.**
  - The waiting branch takes the `BinaryFingerprint` of the executable the registry resolves, runs the cancellable probe, and resolves again.
  - Only an identical fingerprint is trusted:
    - current → `finish_already_current(…, Some(probed))`;
    - still older → recorded in `checked_binaries` while the update keeps waiting.
  - A mismatch records nothing, so the watcher keeps seeing a change against the last checked binary and reconciles again through the existing path.
  - The post-lease probe passes its pre-probe fingerprint only when it is unchanged.
  - `finish_already_current` never rediscovers. Rejected alternative: throttling probes by time, which is a new subsystem and still blesses the wrong binary.
- **GF5: honor a present empty value.** It matches `apps/zeron/src/paths.rs`, which accepts any present value; empty Windows fallbacks still count as unset.
- **GF3: one occupancy rule.** `voice_surfaces::native_right_inset(native, visible_width, right_of_it)` is zero only when the animated visible width is zero.
  - The Shell feeds it `right_visible_width` (which already includes close and fit-exit tweens).
  - The component fixture feeds it its own animated page width.
  - Settled states are unchanged because the settled visible width equals the old shown width.
- **GF4: rely on GPUI's activation.** GPUI already synthesizes a click for a complete Enter or Space press on a focused element, so the custom key-down toggle is removed and the accessibility click action stays.
- **GF7: a background load.** `navigate` keeps its user semantics. A private `load(…, user: false)` used by render-time pending-server resolution:
  - skips the focus move;
  - skips overwriting a focused address field, as native page updates already do.
  - It does not use `blur`, which would lose the original focus.
- **GF13/GF20: the order lives with the popover.**
  - `MenuBarRowOrder` keeps a per-session ordinal assigned the first time the popover sees a session. Sessions first seen together are ordered by creation time, then id; ordinals of vanished sessions are dropped.
  - The controller and the native fixture own one instance each.
  - `project_activity_menu` returns to its previous behavior for All recent.
  - Rejected: sorting by `(created_at, id)`, because UUID ids make an equal-time newcomer jump ahead.
- **GF2: own window or explicit failure.**
  - On Linux the fixture finds the viewable X11 window whose `_NET_WM_PID` is its process, through the existing `x11rb` dependency, and imports only that window.
  - Without X11 it fails. The uniform-frame rejection is kept.

## Risks / Trade-offs

- **The menu-bar order is in memory only.** After an app restart the popover starts again in creation order, as before. → Acceptable: positions only need to be stable while the user looks at the popover.
- **Linux CI without GPU presentation yields uniform own-window frames.** → They are rejected, never faked; the Mac review captures stay the visual evidence.
