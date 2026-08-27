## The short version

Unpeel is local-first. Canonical terminal sessions, agent conversations,
prompts, files, screenshots, Room data, and Unpeel App state live on a Host
you own. Unpeel-operated services do not store that content.

If you use Unpeel Link away from home, end-to-end encrypted traffic passes
through the relay, but the relay cannot decrypt it and does not retain it.
Push notifications are a separately disclosed metadata path through Unpeel
and Apple.

## What the app does not collect

- **No behavioral telemetry.** The app contains no behavioral analytics, ad
  tracking, or usage-profile collection. The narrow anonymous update-count
  record described below is the only product-activity measurement.
- **No account for local use.** You can run Unpeel locally without signing in,
  activating a license, or using Link.
- **No cloud content copy.** The Host keeps the authoritative content. Link is
  not a session, Room, file, App-state, or offline-sync database.

Agent CLIs you run inside Unpeel (Claude, Codex, Gemini, and others) contact
their own providers under their own accounts and privacy policies. Unpeel does
not become that provider or store a copy of that traffic.

## What the website and Link control plane collect

- **Purchases.** Stripe handles checkout; we do not see your card details. We
  store the purchasing email, subscription, seat, and compatible license data
  needed to deliver and manage the purchase.
- **Account portal.** Website sign-in uses an emailed magic link, so we do not
  store an account password.
- **Link identity and routing.** As Link account/device sign-in rolls out, the
  service may store account ids and normalized email, device ids/names/
  platforms/public keys, seat assignment, opaque Host or Room identifiers and
  membership edges, and the routing/assertion/revocation metadata required to
  authorize and locate a Host.
- **Operations and abuse prevention.** Standard short-lived request logs may
  include IP address, user agent, connection timing, ciphertext size, and
  bounded rate-limit or audit metadata.
- **Anonymous update counting.** When the Mac app or an installed terminal UI
  checks for updates, it sends a random install UUID and its version. The UUID
  is minted by that client, is not hardware-derived, and is distinct from the
  license or Link device id. We store only that UUID, the first- and last-seen
  UTC dates (day granularity), release channel, client type, and version for
  aggregate active-install counts. The analytics record is not linked to an
  email, account, license, IP, or hardware identity. These anonymous rows are
  retained for historical install counts. The terminal UI skips the check and
  this record when `UNPEEL_NO_UPDATE` is set.
- **Contact.** If you contact support or join a mailing list, we keep your
  email and correspondence.
- **No ad trackers.** The website uses no advertising trackers or third-party
  analytics scripts.

Link never stores terminal output, transcripts, prompts, commands, RoomFS or
RoomStore content, messages, todos, documents, screenshots, artifact bytes,
attachments, Unpeel App state, content encryption keys, or offline writes. A
Host being offline means its Sessions and Rooms are offline.

## Relay and notifications

- **Interactive relay.** Host and Controller traffic is encrypted end to end.
  The relay observes routing metadata, timing, and ciphertext size, but cannot
  read the content and is not a delivery queue.
- **Push.** APNs device token plus a bounded notification payload (title/body,
  resource identifier, and notification kind) passes through Unpeel's push
  endpoint and Apple. Unpeel Apps and Rooms do not silently put arbitrary
  content into notifications.

## Current license activation and updates

- **Activation.** Current builds send the signed license key, a device id, and
  the Host's device name (the Mac display name or terminal Host label) to bind
  and manage a compatibility seat. Link device enrollment will replace this as
  the primary flow; existing keys remain supported.
- **Updates.** The app checks `unpeel.com` for public signed update artifacts.
  Current clients may include the license and device headers for compatibility;
  standard short-lived server logs may record IP address and user agent. The
  separate random install UUID is used only for the aggregate counting
  described above, not to build a behavioral profile.

## Providers and storage

Cloudflare hosts the website, purchase/account database, Link control plane,
and encrypted relay; Stripe processes payments; our email provider sends
transactional mail; Apple delivers push notifications. We do not sell personal
data or share it for advertising.

## Deleting your data

[Send us a message](/contact) from your account email to request deletion of
account and Link control-plane data. Deleting current license records revokes
the associated key; bookkeeping, fraud-prevention, and security records may be
retained where legally or operationally required. Deleting Link metadata
cannot delete content from a Host because Unpeel never held that content.

## Changes

We will update this policy before collecting a new category of Link metadata
and note material changes here. Questions: [use the contact form](/contact).
