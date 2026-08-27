# Herdr upstream feature requests (drafts)

Two drafts for [github.com/herdrdev/herdr](https://github.com/herdrdev/herdr)
issues, written 2026-08-13 against Herdr 0.8.0. Post manually after review —
they are requests from Unpeel's integration, so they should go out under the
operator's account. Once posted, replace each draft below with a link to the
issue.

Context for both: the Unpeel TUI runs as a Herdr pane and reports one
aggregate agent via `pane.report_agent` (source `custom:unpeel`). It
internally supervises N terminal sessions of its own.

---

## Draft 1 — Virtual / external agents (agent rows without panes)

**Title:** Feature request: source-scoped virtual agents — report multiple
agent rows from one pane

**Body:**

We build Unpeel, a terminal workspace whose TUI runs happily inside a Herdr
pane and reports aggregate status over the Socket API (`pane.report_agent`,
source `custom:unpeel`). The integration works great.

The limitation: our one pane supervises N internal agent sessions, but the
Agents list is pane-backed — one lifecycle authority per pane — so all of
them collapse into a single row ("2 working, 3 idle"). Users would love to
see each supervised agent as its own row, with its own state, and focus our
pane when they select one.

Would you consider a way for an integration to report multiple agent entries
scoped to its source? Two shapes that would both work for us:

1. **Source-scoped sub-agents on a pane:** `pane.report_agent` (or a sibling
   call) accepts a stable per-agent key in addition to `source`, so one
   reporting source can own several rows on its pane. Selecting a sub-agent
   row focuses the pane (optionally with an event delivered to the
   integration so it can select the matching internal session).
2. **Virtual/external agents:** register agent entries not backed by a pane
   at all, with the reporting socket connection as their lifecycle owner
   (rows drop when the connection goes away, like a released authority).

We looked at projecting each internal session as a real pane running a
viewer client, but that multiplies panes and processes for what is purely a
status/navigation concern — and `agent start` (reasonably) no longer runs
arbitrary argv as of 0.8.0.

Happy to prototype against a preview API and give feedback.

---

## Draft 2 — Right-click passthrough without a modifier

**Title:** Feature request: `right_click_passthrough_modifier = "always"` (or
per-pane app-owned right-click)

**Body:**

`right_click_passthrough_modifier` (0.7/0.8) is great — it lets pane apps
receive right-click hold/drag with a modifier held. Our TUI (Unpeel) has
herdr-style right-click context menus of its own, so users running it inside
a Herdr pane currently need the modifier for what feels like a first-class
gesture in the app they're focused on.

Two shapes that would solve it:

1. An `"always"` value for `right_click_passthrough_modifier`, forwarding
   plain right-click to the pane app and moving Herdr's pane menu behind the
   modifier (inverting today's arrangement, as an opt-in).
2. Per-pane / per-app: forward plain right-click when the pane app has
   requested mouse tracking (like normal clicks already behave under
   `mouse_capture`), keeping Herdr's menu on panes that don't.

Either would let terminal apps with their own context menus feel native
inside Herdr while keeping today's default untouched.
