## The free software

The Unpeel Mac app is **free** for local use — every local feature, no trial,
no account, and no credit card. Download it, launch it, and get to work. All
updates are included.

The product boundary is simple: software and work on machines you own belong
on the free side. LAN, VPN/direct, and SSH connections do not consume an
Unpeel-operated service.

## Unpeel Link

**Unpeel Link is $59 per seat per year.** Link is the operated service that
lets your devices find and reach a user-owned Unpeel Host when a direct path
is not available:

- account/device identity and Host rendezvous as the sign-in migration rolls
  out;
- the end-to-end encrypted Unpeel Relay;
- push delivery when an agent needs you.

Link is not a cloud session or App store. Your terminal output, transcripts,
files, screenshots, Room data, and Unpeel App state remain on the Host you
own. The relay forwards ciphertext and does not persist that content.

## License-key compatibility

Unpeel 0.2 labels the subscription **Unpeel Link**. Earlier builds called it
**Unpeel Pro** and called its relay feature **Unpeel Remote**. Buying today
emails one signed license key; one compatibility activation currently enrolls
one Host machine and unlocks the shipped remote/iPhone/workspace surfaces. This
remains supported while Link's per-person device model replaces Host activation
as the primary flow.

Existing purchases, keys, and activations will remain valid through the
migration. The future Link seat is assigned to one human account whose devices
are separately revocable; that account model is not yet the behavior of the
released client.

## Activating a current build

Paste the emailed key in **Settings ▸ Remote** in the Mac app or terminal UI.
Unpeel verifies its signature, then binds that Host machine to one
current-format activation. To move it, deactivate the old Host from its
settings or from [your account](/account).

## Managing your subscription

[Your account](/account) is a passwordless portal. Today it lets you copy a
legacy key and free activated Hosts. It will become the Link device/seat portal
as account-based clients ship. No app password is stored.

## Updates

Updates use the built-in updater and are signed, notarized, and verified before
installation. Update artifacts are public and updates are free for everyone,
with or without Link.

## Privacy posture

There is no behavioral analytics or usage profiling in the app. Update checks
carry a client-minted random install UUID and version for day-granularity,
aggregate active-install counting; the analytics record is not linked to a
license, account, email, IP, or hardware identity. The terminal UI skips
update checks entirely when `UNPEEL_NO_UPDATE` is set. Canonical content stays
on your Host. Link stores only the minimum account, device, seat, routing,
revocation, and abuse metadata needed to operate the service; it never stores
session, Room, or Unpeel App content. Notification metadata follows the
separately disclosed Apple Push path.
