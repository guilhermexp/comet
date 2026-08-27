# Open Source — everything except the operated Unpeel Link backend

> **Status (amended 2026-08-10): Decided direction, not scheduled.** Publish
> every client, Host, local runtime, SDK, protocol, and product document,
> including the **iPhone/iPad app**. Keep only the backend implementation of
> the operated **Unpeel Link** service closed: account/device identity,
> entitlement/seat assignment, Host/room rendezvous, Relay, and push delivery.
> This file records the boundary, the pre-publication checklist, and how it
> interlocks with pricing. Nothing here is a commitment to a date.

**Public-copy status (2026-08-10):** the website now states this committed
direction and clearly says the source has not been published yet. Existing
terms remain the authority until a repository license and the service/code
split below actually ship; marketing must not call the current repository open
source early.

## Why this and pricing are the same decision

Once the client is public, **every client-side gate is a speed bump**. Anyone
can delete `if license.isPro` and rebuild. That makes the canonical Link
boundary in `docs/plans/unpeel-link.md` not merely the cleaner story but the
only one that survives:

- **Enforceable**: official Link endpoints verify account seat assignments;
  `apps/relay/src/worker.mjs` verifies short-lived Host and Controller
  entitlements and push. Server-side, in code we run, on a service we pay for.
- **Honest**: you pay for infrastructure we operate, not for a checkbox in a
  binary you compiled yourself.
- **Coherent with self-hosting**: local sessions, LAN pairing and
  Mac-to-Mac on your own network cost nothing, because they cost *us* nothing
  and we could not police them anyway.

Do not add a client-side gate to anything else on the way to open sourcing.
It would be removed by the first fork and read as bad faith by everyone else.

This file decides the repository/package boundary. `unpeel-link.md` owns the
service definition, seat rule, public contract, and service data boundary;
`account-backed-rooms.md` owns Host RoomFS data. None of those decisions make
RoomFS, RoomStore, the Unpeel Apps SDK and Apps UI SDK, client crypto, or wire
schemas proprietary.

## The boundary

| part | path | status | why |
| --- | --- | --- | --- |
| Rust crates (core, host, TUI) | `crates/` | **open** | the thing users run; auditability is the point |
| Mac app | `apps/native` | **open** | same |
| Shared Apple client/protocol code | `apps/shared` | **open** | crypto and wire behavior must be auditable |
| iPhone / iPad app | `apps/ios` | **open** | a first-party controller, not the paid boundary |
| Release Worker | `apps/releases` | **open** | public software distribution, not Link |
| Website, public docs, downloads, purchase UI | public parts of `apps/website` | **open** | product surface and client-facing contracts |
| Link account, seat, entitlement, and rendezvous backend | private sibling repo `~/Dev/unpeel-account` (`@unpeel/account-service`, a `file:` dep of `apps/website`) | **closed; extracted 2026-08-12** | operated paid service implementation |
| Link Relay + push backend | `apps/relay` | **open** (decided 2026-08-12) | the paid thing is the operated service — server-side entitlement verification and the APNs provider key — not the worker code; open relay code makes the E2E "the relay sees nothing" claim fully auditable (the open Swift KAT tests already consume `apps/relay/test/relay-kat-vectors.json`) |
| RoomFS, RoomStore, Unpeel Apps SDK, Apps UI SDK (`unpeel-apps-ui-sdk`), manifests, and protocols | current/future client + Host packages | **open** | standalone/local/direct Apps must never depend on closed code or an Unpeel-rendered terminal |

Consequences worth accepting deliberately:

- Someone can build Unpeel.app themselves and use it free on their own
  network. That is already true under the pricing decision; open sourcing
  costs nothing extra here.
- Someone can build or fork the first-party phone app and use it over direct
  networking. Fine — official off-LAN Link reach still needs that person's
  server-issued entitlement.
- Someone can self-host the relay from the published source (decided
  2026-08-12: `apps/relay` is open, not merely reimplementable). Embraced,
  not tolerated — it proves the E2E privacy claim and costs no real revenue:
  entitlement issuance stays in the closed account service, and push to the
  official iOS app requires the APNs provider key only the operated service
  holds, so self-hosted relays get transport but never push. Deployment
  secrets, abuse-control config, and provider credentials stay out of the
  repo as always.
- The moat is **the operated Link service + the trademark**, not the copyright
  license.

The source boundary is implementation, not documentation. Public clients must
not call an undocumented private protocol that only the closed service can
understand. Publish request/response schemas, entitlement-verification rules,
E2E framing, error vocabulary, and conformance fixtures. Keep deployment code,
abuse controls, provider credentials, operational tooling, and the service's
private data model behind the Link boundary.

## License choice

**Recommendation: permissive — MIT or Apache-2.0.** Apache-2.0 if the explicit
patent grant is wanted; MIT for minimum friction and maximum compatibility
(Ghostty, which is vendored here, is MIT).

Copyleft (AGPL) is the wrong tool for this shape: it would not prevent the
"build it yourself and use it on your LAN" case — which is *allowed* by
design — and would deter contributors and downstream packagers, which are two
of the three things open sourcing is meant to buy.

Trademark policy is what actually stops "Unpeel" clones. Decide the name/icon
position at the same time as the license.

## Publication shape (decided 2026-08-12): fresh-start repository

**Publish a NEW public repository whose history begins at a single "initial
public release" commit of the audited tree (taken from the `unpeel-tui`
branch state). This repository stays private forever as the full-history
archive.**

Why not the alternatives:

- **Publishing full history** would expose everything git ever recorded: the
  pre-extraction account/licensing backend, the whole Clarity era, candid
  incident history, and anything a secret scan misses across thousands of
  commits — an audit surface we cannot realistically clear, and permanent if
  wrong.
- **An `oss` branch in this repo** is structurally unsafe: flipping the repo
  public publishes *all* refs at once, GitHub keeps once-pushed commits
  fetchable by SHA even after branch deletion, and one accidental
  `git push origin main` publishes everything forever. The safe state must be
  the default, not discipline.
- **History rewriting** (git-filter-repo) changes every hash anyway, is
  error-prone across the rebrand-era renames, and still requires the full
  scan. Ruled out.

Consequences:

- Public blame/archaeology starts at the import commit. Context worth having
  public moves into `docs/` (which is already candid), not into git history.
- Day-to-day development moves to the public repo after cutover; this repo is
  then an archive, not a fork to keep in sync.
- The secret audit shrinks from "every commit ever" to "the published tree" —
  but that tree scan still has to happen, and secret scanning in CI keeps the
  new history clean from commit 1.
- The extracted service repo (`unpeel-com/unpeel-account`, private since
  2026-08-12) is unaffected: it was born after the extraction and never
  publishes.

## What it buys

1. **The core promise becomes auditable.** "Nothing leaves your machines" is
   currently trust-me. For a self-hosted product this is the single biggest
   credibility multiplier available, and it is the reason to do this at all.
2. **Provider integrations are the natural contribution surface.** AGENTS.md
   already documents *Adding a New Agent CLI* as a checklist across a handful
   of choke points (`integrations/*.rs`, `hook_assets.rs`, `Presets.swift`,
   `ResumeCommand.swift`, `SessionActivity.swift`, `transcripts.rs`). New CLIs
   appear faster than one team can chase. The future one-directory descriptor,
   Host-owned capability model, and trusted adapter boundary are planned in
   `docs/plans/agent-runtimes.md`; the current checklist remains authoritative
   until that migration lands.
3. **Distribution gets broader.** The signed/checksummed R2 installer already
   exists. Public source additionally makes a Homebrew formula, `cargo
   install`, and distro packaging natural community paths. Clean-container
   Linux build/install/basic runtime is now green; the real-machine matrix and
   published artifact validation remain the headless gate.

## Pre-publication checklist

Audited 2026-08-07; the repo is in better shape than expected.

**Already correct — keep it that way**

- [x] The client holds only a **public** verifying key (`LicenseManager.swift`,
      Curve25519, with an env override for tests). The signing key is
      server-side and has never been in the client.
- [x] The one tracked `.env` (`apps/website/.env`) contains a build flag
      (`VITE_UNPEEL_REMOTE=1`), not a secret.
- [x] The Relay implementation is already isolated under `apps/relay` (and
      per the 2026-08-12 decision it publishes with the initial import).
- [x] Nothing secret-shaped in tracked files: the four scanner hits are
      comments describing the *format* of Stripe/APNs keys.

**To do before publishing**

- [x] **Choose a license** — decided 2026-08-13: **MIT**, `LICENSE` added
      (Copyright UX Themes AS), paired with `TRADEMARK.md` (code is MIT; the
      name/logo/icon/mascot are not — distributed forks must rebrand).
- [x] **Extract the Link backend from `apps/website`.** Done 2026-08-12: the
      service implementation (auth/account/admin/license/subscribe routes,
      service libs, D1 migrations, service scripts) lives in the private
      sibling repo `~/Dev/unpeel-account`, consumed by `apps/website` as the
      `@unpeel/account-service` `file:` dependency. Pages/docs/purchase UI and
      the render-by-name Inertia seam stay open; the package imports no open
      code. **Stub done 2026-08-13:** the public stub at
      `apps/website/account-service-stub/` is the default `file:` dependency
      (same export surface, 501s + no-ops), and official builds overlay the
      private sibling via a vite alias + tsconfig-paths fallback when it
      exists (`UNPEEL_FORCE_ACCOUNT_STUB=1` tests the open shape). Verified:
      real-mode builds bundle zero stub code, stub-mode builds bundle zero
      closed code. Keep the stub's export surface in lockstep with the real
      package, and keep the two repos' hono versions identical (TS only
      unifies duplicate packages at equal versions). Still owed: publish the
      client-facing API schemas (`/api/validate`, activation, entitlement)
      as open documentation.
- [x] **Run a real secret scanner over the published tree.** Done 2026-08-13:
      gitleaks over a `git archive HEAD` export — six findings, all verified
      test fixtures (fake pairing token, RFC 6455 example nonce, sequential
      dummy key, a Swift type name), now allowlisted in `.gitleaks.toml` so CI
      only alerts on NEW findings. Still owed: wire gitleaks into the public
      repo's CI from commit 1, and re-run the tree scan at import time.
- [x] **Make `apps/native` buildable from a fresh clone.** Done 2026-08-13:
      the GhosttyKit xcframework zip (49 MB, ghostty commit `2da015cd`) is
      hosted at
      `unpeel.com/releases/stable/vendor/GhosttyKit-2da015cd.xcframework.zip`
      (immutable R2 key), and the vendored `Package.swift` now uses the
      upstream url+checksum binary-target mode (generated with
      `Script/build-manifest.sh`). Verified: SPM downloads, checksum-validates,
      and `swift build` passes. When rebuilding the framework, upload the new
      zip under a new commit-named key and regenerate the manifest.
- [x] **License provenance for vendored binaries.** Done 2026-08-13:
      `libghostty-spm` had its LICENSE; `ghostty-vt/` now carries upstream
      Ghostty's MIT LICENSE next to the prebuilt `.a`s (source commit pinned
      in its README). `agent-browser` is NOT vendored — `build-app.sh` bundles
      it at build time from the npm/managed install and copies its Apache-2.0
      LICENSE into the app bundle. Sparkle is a remote SPM dependency, not
      vendored; its license travels with source resolution.
- [x] **Decide on `RELEASE.md`** — decided 2026-08-13: **stays private**.
      Exclude it from the initial public import (it is the one tracked file
      deliberately stripped from the fresh-start tree). Consequences to
      accept: the public AGENTS.md reference to it dangles, and `release.sh`
      preflight — which only operators with local secrets can run anyway —
      requires the private ledger. The public repo can grow a sanitized
      release-process doc later if contributors need one.
- [x] **Decide about publishing `AGENTS.md` and `docs/`** — decided
      2026-08-13: **publish as-is**, incident history, unbuilt strategy,
      commercial reasoning and all. The candor is the differentiator, and the
      docs are load-bearing for contributors (AGENTS.md is the map the
      CONTRIBUTING.md points at). No pre-publication rewrite pass.
- [x] **Trademark position** — `TRADEMARK.md` added 2026-08-13 (nominative
      use free, distributed forks rebrand, official binaries only from
      unpeel.com/app stores). Formal trademark *registration* is a separate
      legal step, still open.
- [ ] **App Store boundary.** Publish `apps/ios` source while keeping Apple
      signing certificates, provisioning profiles, App Store Connect secrets,
      APNs provider keys, and official bundle/trademark distribution private.

**To keep true from now on**

- [ ] No closed-code dependencies in the open parts; optional use of the
      documented operated Link API is a service boundary, not an SDK dependency.
- [ ] Secret scanning in CI, so history stays clean by default.
- [ ] Every new vendored artifact arrives with its license.
- [ ] No new client-side entitlement checks (see the first section).
- [ ] No closed Link SDK in any client/Host/App path; open clients speak the
      documented protocol and local/direct behavior has no Link dependency.

## Related

- `docs/plans/master-plan-next.md` — canonical cross-project execution order
- `docs/plans/unpeel-link.md` — canonical Link service and its public/closed
  boundary
- `docs/agents/licensing.md` — the pricing model this depends on
- `docs/plans/headless-host.md` — distribution, which open sourcing unblocks
- `docs/plans/shared-core.md` — the codebase shape being published
- `docs/plans/agent-runtimes.md` — the open provider/runtime contribution
  surface and adapter trust boundary
- `AGENTS.md` ▸ Pricing, Unpeel Link, and Legacy Licensing
