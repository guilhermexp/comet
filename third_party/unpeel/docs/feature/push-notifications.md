# Push notifications (needs-input + notify-when-done)

Notify the user when an agent session **needs input** (a permission/attention
prompt) or, for sessions they opted in, **finishes a turn** — on whichever
device they're at: a macOS Notification Center banner when at the Mac, and an
APNs push to a paired iPhone when away (even with the app closed).

Two notification reasons, two independent channels, one shared per-session opt-in.

## Triggers (Mac, semantic + host-observed)

Both notification reasons feed the existing activity engine in
`UnpeelStore` (`apps/native/UnpeelNative/Sources/UnpeelNative/UnpeelStore.swift`):

- **Needs input** — a `PermissionRequest` hook (non-`AskUserQuestion`) →
  `.attention`. Always dispatched (input is time-sensitive).
- **Needs input, hookless menu** — the Host's parsed viewport raises
  `menu_prompt_active`; native dispatches once on its generation-bound
  false → true edge and re-arms when it falls. A matching
  `PermissionRequest` and menu edge deduplicate whichever arrives second. The
  initial app scan seeds silently, but a session first discovered after startup
  alerts even if its first observed sample is already active.
- **Finished** — a `Stop`/`StopFailure` hook → the session settled. Dispatched
  only when the session is in the **notify-when-done** opt-in set.

Local unread state and the macOS banner are gated on observation (skip if the
session is already visible on the Mac or another Controller). Phone delivery
is independent: a Mac-visible session still fans out through Link, and each
APNs target is suppressed only when that exact paired device is currently
viewing the session (`ViewerPresenceStore.isDeviceViewing`). Thus Direct/SSH
can remain the interactive transport while Link supplies background push.

`dispatchSessionPush(sessionID:kind:)` fans out to both channels with identical
copy (title = session label; body = "Needs your input" / "Finished").

## Channel 1 — macOS banner

`DesktopNotifier.swift` (UNUserNotificationCenter). Authorization requested at
launch (`AppDelegate`). `willPresent` returns `.banner` so it shows even while
Unpeel is frontmost (you may be on a different session); tapping selects the
session (`onSelectSession` → `revealSessionInSidebar`). Works with no relay,
no APNs, no phone — purely local.

## Channel 2 — APNs push to the iPhone

The Mac must **not** hold the APNs auth key (it would ship in a distributed
app), so the relay owns it and signs the provider JWT.

```
iPhone (registers token) ──/mobile/push-token──▶ Mac (devices.json)
Mac (hook event) ──POST /v1/push/<macID> (Bearer entitlement)──▶ relay Worker
relay ──ES256 JWT + POST /3/device/<token>──▶ APNs ──▶ iPhone
```

- **Per-session opt-in** (`notify_when_done`) is a Mac-side native overlay
  (`NativeOverlay.notifyWhenDoneKey`, `UnpeelStore.notifyWhenDoneSessionIDs`),
  toggled from the desktop sidebar context menu ("Notify when done") or the
  phone's organize sheet. Surfaced on `RemoteSessionSummary.notifyWhenDone`;
  carried across restart; pruned with the session. Needs-input pushes ignore
  the flag (always sent).
- **Token registration**: the phone requests notification auth + calls
  `registerForRemoteNotifications` (`PushManager` + `PushAppDelegate`,
  `apps/ios/UnpeelIOS/Sources/UnpeelIOS/PushManager.swift`), then
  `POST /mobile/push-token` (`RemotePushTokenRegistration` — hex token +
  `sandbox`/`production` environment). Stored per device in
  `~/.unpeel/mobile/devices.json` (`MobilePairingStore.setPushToken`). Re-sent
  on every (re)pair (epoch change).
- **Send path**: `RelayUplinkManager.sendPush` POSTs to `/v1/push/<macID>`
  with the relay entitlement as the bearer (the same paid-service gate as the
  streaming uplink) — stateless, no Durable Object, works even when the
  streaming WS is idle. `apps/relay/src/apns.mjs` signs the ES256 provider JWT
  with the Worker's APNs secret and posts to `api.push.apple.com` /
  `api.sandbox.push.apple.com` (per the device's environment).
- **Dead-token pruning**: APNs `BadDeviceToken` / `Unregistered` in the
  response → `MobilePairingStore.clearPushToken`, so the Mac stops pushing to
  a reinstalled/unregistered device.
- **Tap → deep link**: the APNs payload carries `sessionId`; a tap routes
  through `PushManager.onOpenSession` → `RemotePreviewStore.selectSessionByID`
  (with a pending fallback so a cold-launch tap selects the session once the
  first bootstrap poll loads it).
- **Deterministic test**: macOS Settings ▸ Notifications has separate Mac and
  phone test actions. The phone action deliberately bypasses viewer
  suppression and exercises the real Link/APNs path; “Last phone push” records
  the resulting delivery or operator error.

The push notification is coalesced per session via `apns.thread-id` /
`threadIdentifier`, so repeated events for one session collapse into a single
banner.

## Wire additions

- Shared (`RemoteControlProtocol.swift`): `RemoteSessionSummary.notifyWhenDone`,
  `RemoteSessionOrganizationPatch.notifyWhenDone`, `RemotePushTokenRegistration`.
  Both summary + patch use custom `Decodable` so the new fields are optional on
  the wire (older Macs omit them without failing the snapshot decode).
- Mac routes: `POST /mobile/push-token` (`MobileRemoteServer`).
- Relay route: `POST /v1/push/<macID>` (`worker.mjs` + `apns.mjs`).

## Operator setup (required before APNs works)

1. **APNs auth key**: developer.apple.com ▸ Keys ▸ create a key with the Apple
   Push Notifications service (APNs) enabled. Note its **Key ID** and download
   the `.p8` once.
2. **App ID capability**: enable **Push Notifications** on the App ID
   `com.unpeel.ios.remote` for the signing team. Without this, the device build
   fails with *"provisioning profile doesn't include the aps-environment
   entitlement / Push Notifications capability"* (the app entitlement is
   `App/UnpeelIOS.entitlements`; `UNPEEL_APNS_ENVIRONMENT` expands to
   `development` for Debug and `production` for Release/TestFlight).
3. **Relay configuration** (`cd apps/relay`): `APNS_TEAM_ID` and
   `APNS_TOPIC` are public identifiers committed in `wrangler.jsonc`; install
   the private signing material as Worker secrets:
   ```sh
   wrangler secret put APNS_KEY       # paste the .p8 contents
   wrangler secret put APNS_KEY_ID    # the 10-char Key ID
   wrangler secret list               # both names must be present
   npx wrangler deploy
   ```
4. Rebuild + install the iPhone app (device build) and the Mac app.

## Constraints / notes

- The **simulator can't receive real APNs** — push testing is device-only.
  macOS banners and the toggles work everywhere.
- Push requires a relay entitlement (the paid Unpeel Remote gate). No license
  ⇒ no phone push; the macOS banner still fires.
- Needs-input pushes to background phones regardless of whether the Mac is
  frontmost. A phone actively rendering that session suppresses only its own
  APNs target, not other paired phones.
- Debug builds report the **sandbox** environment; the relay targets the
  sandbox APNs host accordingly.
- The phone's pairing sheet reports permission, registration, and APNs
  environment state without displaying its token. macOS Settings ▸
  Notifications reports how many phone tokens are registered, Link
  entitlement/uplink status, and the last Relay/APNs result. These diagnostics
  are local UI state; they do not add service telemetry.

## Tests

- `cd apps/relay && npm test` (Worker/integration; push route is entitlement-
  gated like the host uplink).
- `swift build` in `apps/native/UnpeelNative` and `swift build` in
  `apps/shared/UnpeelShared`.
- iOS: `xcodebuild ... -destination 'id=<SIM_UDID>'` (compiles; push is a
  device-only runtime path). Device build needs the Push capability (step 2).
