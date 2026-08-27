# App Store listing — Unpeel Remote (iOS)

> Draft 2026-07-24 for the 1.0 App Store submission (ASC app 6789123784,
> bundle `com.unpeel.ios.remote`). Everything here is copy-paste-ready for
> App Store Connect but NOT yet entered — review, edit, then either paste it
> in ASC or ask an agent to apply it via the ASC API. Checklist state lives in
> RELEASE.md ("iOS: remaining App Store launch checklist") in the private
> operational repo (`~/Dev/unpeel-account`).
>
> **Naming update (2026-08-10):** the customer-facing paid service is Unpeel
> Link. Current license-key builds may still render “Unpeel Pro”; preserve
> that compatibility, but do not use Pro as the product name in new listing
> copy.

## Name & subtitle

- **Name** (30 chars max): `Unpeel Remote`
- **Subtitle** (30 chars max): `Steer AI agents on your Mac`

## Category / age

- Primary: **Developer Tools**, secondary: **Productivity**
- Age rating: 4+ (no objectionable content; questionnaire all "No")

## Promotional text (170 chars, editable without review)

> Run Claude Code, Codex, Gemini CLI and more on your Mac — watch, steer, and
> launch sessions from your iPhone or iPad. Self-hosted, end-to-end encrypted.

## Description

> Unpeel Remote is the companion app for Unpeel, the free Mac app that runs
> and supervises your CLI AI agents (Claude Code, Codex, Gemini CLI, and
> more) in persistent terminal sessions.
>
> Pair your iPhone or iPad with your Mac by scanning a QR code, and your
> whole agent fleet is in your pocket:
>
> • See every session at a glance — which agents are working, which are
>   waiting for you
> • Get notified when an agent needs input or finishes a task
> • Open any session as a live terminal: type, answer menus, scroll —
>   with a keyboard built for terminal control
> • Start new sessions in any project, from your own presets
> • Review screenshots and images from agent browser sessions, mark them
>   up, and send them back
> • Dictate prompts with on-device speech recognition
>
> On your network, the connection is direct — Mac to phone, TLS-pinned.
> With Unpeel Link, Unpeel Remote reaches your Mac from anywhere through an
> end-to-end encrypted relay: forward-secret encryption between your devices
> means we never see your terminals, prompts, or code. Nothing about your
> sessions is stored on our servers.
>
> Unpeel Remote requires the free Unpeel app on your Mac
> (download at unpeel.com).

Note: no purchase links and no "$" anywhere — Link is mentioned only as an
operated connectivity service; buying happens outside the app (Apple 3.1.3(b)
multiplatform services; the app itself must never link to the buy page).

## Keywords (100 chars max, comma-separated)

`claude,codex,gemini,terminal,agent,ai,cli,remote,ssh,developer,tmux,pair`

(69 chars — room to iterate. Do NOT include "Unpeel" — the name already
matches; avoid competitor app names, Apple rejects those.)

## URLs

- Support URL: `https://unpeel.com/contact`
- Marketing URL: `https://unpeel.com`
- Privacy Policy URL: `https://unpeel.com/legal`

## App Privacy questionnaire

Do not reuse the old “Data collection: none” answer. For the current 0.2
companion build, enter and verify these answers in App Store Connect before a
production submission:

- **Data Used to Track You: No.** There are no ad or tracking SDKs and data is
  not combined with third-party advertising profiles.
- **Data Linked to You: No** for this compatibility-key build. The iOS app has
  no account sign-in or customer identity. Revisit this when Link account
  sign-in ships.
- **Data Not Linked to You → Identifiers → Device ID → App Functionality.**
  APNs tokens and opaque device/pairing identifiers are retained as needed to
  route notifications and relay connections. They are not advertising ids.
- Interactive session content remains end-to-end encrypted; the relay forwards
  ciphertext and does not retain terminal, transcript, prompt, file, or
  screenshot content.
- Bounded notification metadata (session title / event type) transits the
  Unpeel push endpoint and Apple only to deliver the requested notification
  and is not retained as a content store. Confirm App Store Connect's current
  wording for transient push payloads when completing the questionnaire; do
  not infer “none” from the lack of analytics.
- Keep these answers aligned with the public privacy policy before every
  submission.
- Encryption export compliance is pre-answered in the Info.plist
  (`ITSAppUsesNonExemptEncryption=false` — standard algorithms exemption:
  CryptoKit AES-GCM/X25519/HKDF only).

## Review notes (Beta App Review notes worked; reuse + extend)

> Unpeel Remote is a remote control for the reviewer's own Mac running the
> free Unpeel app — like a hardware companion app, it cannot be meaningfully
> used without the user's own second device. Pairing requires scanning a QR
> code shown by the Mac app on the same network.
>
> Demo video showing install → pairing → controlling live agent sessions:
> <RECORD AND LINK — required, this is what prevents a "couldn't test" 2.1>
>
> The app sells nothing: it is free, contains no in-app purchases, and does
> not link to any external purchase flow. Some capabilities (off-network
> relay access) activate automatically when the paired Mac has an Unpeel Link
> subscription, purchased and managed entirely outside this app
> (multiplatform services, guideline 3.1.3(b)).

## Screenshots (required sizes: 6.9" mandatory; iPad 13" if iPad ships in 1.0)

Shot list (capture on device or sim with a real paired Mac, dark mode):

1. Sidebar fleet view — several sessions, busy spinners + attention dot
2. Live terminal with agent output + menu control bar answering a prompt
3. Notification banner ("needs your input") over lock screen
4. New-session sheet with presets
5. Session gallery with a browser screenshot + markup
6. Pairing screen with QR (staged)

Decision needed: **iPad in 1.0?** `TARGETED_DEVICE_FAMILY` is already "1,2"
(the app builds and runs as a native iPad app), so shipping iPad requires
only iPad screenshots + testing the layout. If iPad layout isn't ready,
change to "1" before archiving — the listing follows the binary.

## Submission mechanics

1. Bump `CFBundleShortVersionString` to `1.0` (build number continues the
   TestFlight ledger in RELEASE.md).
2. Archive + upload with the CLI recipe in RELEASE.md (manual signing —
   the App Store profile now includes Push Notifications; first
   `-allowProvisioningUpdates` build regenerates it).
3. Verify a production APNs push on that TestFlight build before submitting.
4. ASC ▸ App Store tab ▸ create version 1.0 ▸ paste this doc's content ▸
   attach build ▸ answer export compliance (already pre-answered) ▸ Submit.
5. After approval: set phased release OFF (small user base), release, then
   point `unpeel.com/ios` at the App Store URL (IOS_TESTFLIGHT_URL var in
   apps/website/wrangler.jsonc — rename pending; deploy apps/website).
