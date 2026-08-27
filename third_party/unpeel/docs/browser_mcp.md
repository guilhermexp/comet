# Unpeel Browser MCP

> **Current implementation (2026-08-11) supersedes this original design
> sketch.** Unpeel authors the Browser domain in its built-in `unpeel` MCP
> server and drives the bundled `agent-browser` **native Rust CDP daemon**
> against system Chrome. Node/Playwright is ruled out, not a fallback. Browser
> access is a free local/Host capability on the planned public-source side of
> Unpeel; it must never be client-side license gated. When Controller access is
> exposed through the Host protocol, LAN/VPN/IP and SSH remain free. Only
> carrying that same protocol through the operated Unpeel Link
> rendezvous/Relay path requires Link. Current detail:
> [`docs/agents/browser-mcp.md`](agents/browser-mcp.md); original engine audit:
> [`docs/feature/browser-mcp-deep-check.md`](feature/browser-mcp-deep-check.md).

## Goal

Add a first-class browser automation integration for Unpeel sessions.

The feature should let agents operate a real browser through MCP while Unpeel
provides setup, defaults, session binding, artifacts, and diagnostics. The
initial engine should be `agent-browser`, bundled or managed by Unpeel, and
presented to users as "Browser MCP" or "Browser Access" rather than as an npm
package they have to understand.

The first version should answer:

- Can this agent open a page, inspect it, click, fill forms, and take
  screenshots?
- Can it run the same loop against Mobile Safari in iOS Simulator when the local
  machine has Xcode support?
- Which sessions have browser access enabled?
- Where did the screenshots, videos, logs, and traces go?
- Is the local machine correctly set up for desktop browser and iOS Simulator
  automation?

## Product Shape

This should be a Unpeel integration, not a browser tab inside Unpeel.

Unpeel remains the visual terminal-hosted agent product. The browser is a tool
the agent can call through MCP, with Unpeel handling setup and visibility. The
actual browser window can be Chrome/Chromium or Apple's Simulator app. Unpeel
can show status, recent artifacts, and configuration, but should not grow a DOM
inspector, browser viewport, test-runner dashboard, or IDE-style app surface.

User-facing language:

- "Browser MCP"
- "Browser Access"
- "Allow this session to use a browser"

Internally, `unpeel-core::browser_mcp` owns the MCP schema and translates it to
the native engine daemon. There is no `agent-browser mcp` mode.

Browser MCP is part of the local runtime, not Unpeel Link. Profiles, artifacts,
permissions, and browser state stay on the user-owned Host. Remote use does not
change this ownership or add an account requirement unless the user chooses
Link as the transport.

## MVP

- Bundle or locate a pinned `agent-browser` binary.
- Add a native settings page for Browser MCP.
- Add a global toggle for enabling Browser MCP in new sessions.
- Add per-session access control, following the existing Sessions MCP access
  mental model.
- Inject the Browser MCP server into enabled sessions.
- Support desktop browser automation against Chrome/Chromium.
- Support iOS Simulator Mobile Safari automation when Xcode/Appium requirements
  are available.
- Persist screenshots, videos, logs, and traces as session artifacts.
- Provide a health check that reports missing browser, missing Xcode tooling,
  missing iOS runtimes, unsupported architecture, or failed binary execution.
- Fail soft when the browser engine is unavailable.

## Non-Goals

- Do not embed a live browser viewport in Unpeel.
- Do not embed or rehost the iOS Simulator UI inside Unpeel.
- Do not add code-editor, diff-viewer, source-tree, or Xcode-like project UI.
- Do not position this as a web-development-only feature.
- Do not require users to manually install an npm package for the default path.
- Do not auto-update the browser engine independently of Unpeel releases.
- Do not promise arbitrary native iOS app automation in the first version.

## Engine

Use `agent-browser` as the first engine.

As of the checked package version, `agent-browser` is a Rust native CLI
distributed through npm. The npm package includes prebuilt native binaries,
including:

- `bin/agent-browser-darwin-arm64`
- `bin/agent-browser-darwin-x64`

The JavaScript entrypoint is mostly a platform launcher. For the native macOS
app, Unpeel should call the appropriate native binary directly and avoid adding
a Node runtime dependency.

Pin a known-good version in the release pipeline. Re-evaluate the license,
binary layout, and command surface when upgrading.

## Architecture

Implemented process tree:

```text
Unpeel session
  -> provider CLI with MCP config
    -> unpeel-host __mcp__ (browser domain)
      -> unpeel-core::browser_mcp
        -> bundled agent-browser --native daemon
          -> system Chrome over CDP
```

The Unpeel-owned Rust MCP layer is responsible for:

- Select the correct bundled `agent-browser` binary for the architecture.
- Set Unpeel-specific environment variables.
- Set artifact, profile, and trace directories.
- Apply the selected tool set.
- Optionally apply the selected desktop browser executable.
- Optionally apply the selected iOS Simulator device/runtime.
- Start and communicate with the native engine daemon; never require Node.

The wrapper gives Unpeel a stable product contract even if the underlying engine
changes.

## Settings Page

Add a native settings page:

```text
Settings
  Browser MCP
```

Use Codex's browser settings as the baseline shape: a simple enable card at the
top, a compact General section, explicit Permissions, site-specific overrides,
and a clearly separated Developer Mode area for elevated-risk controls.

Recommended sections:

- **Browser Access**
  - Top-level enable/disable toggle.
  - Description: "Let agents use a browser through MCP."
  - Status indicator if the engine is unavailable or needs setup.

- **Status**
  - Ready / Needs setup / Unavailable.
  - Bundled engine version.
  - Last health check result.

- **Access**
  - Enable Browser MCP for new sessions.
  - Default access level.
  - Allow project/session overrides.

- **General**
  - Default local URL open destination: Unpeel Browser, system browser, or ask.
  - Browsing data/profile data: clear current session, current project, or all
    Browser MCP data.
  - Visual context capture: never include screenshots, ask first, or include
    when useful.

- **Desktop Browser**
  - System Chrome/Chromium detection.
  - Custom executable path.
  - Install/setup action when browser support is missing.
  - Default profile mode: isolated per session vs shared user profile.

- **iOS Simulator**
  - Xcode detection.
  - iOS runtime detection.
  - Default simulator device.
  - Boot/focus behavior.
  - Status for Appium/XCUITest requirements if the selected engine needs them.

- **Tool Sets**
  - Core browser controls.
  - Network inspection.
  - React/tooling helpers when available.
  - Mobile/iOS controls.

- **Permissions**
  - Approval before opening websites: always ask, ask for external sites, or use
    site rules.
  - Site-specific allow/deny rules for domains.
  - Domain allowlist support when the engine exposes it.

- **Artifacts**
  - Open artifact folder.
  - Clear Browser MCP artifacts.
  - Retention policy.

- **Developer Mode**
  - Connect to live Chrome session.
  - Enable full CDP access.
  - Show elevated-risk warning before enabling.
  - Explain that full CDP access can inspect pages, cookies, storage, network
    requests, and browser internals.

- **Diagnostics**
  - Run health check.
  - Copy diagnostic report.
  - Reveal trace/log file.

The settings page should configure the integration. It should not become the
browser automation UI itself.

## Session UX

Each session should have a small Browser MCP access state, similar to Sessions
MCP access:

- Off
- This session
- This project, if project-level access is supported

When enabled, the session should know:

- engine version
- desktop/mobile mode
- selected browser or simulator
- artifact directory
- last screenshot or video artifact
- last health-check error, if any

If access changes require a restart to update the provider's MCP client config,
reuse the existing restart recommendation pattern instead of force-restarting
the session.

## MCP Surface

Expose the engine's MCP tools through the Unpeel-managed server. The exact tool
names can remain engine-owned, but the product should document capabilities in
human terms:

- open a URL
- inspect page snapshot
- click/tap an element
- type/fill form fields
- press keys
- take screenshot
- record video
- read console logs
- inspect network activity
- use Mobile Safari in iOS Simulator when available

Avoid designing a large Unpeel-specific tool schema until there is a strong
reason. Start by wrapping the engine process and adding Unpeel-specific setup,
paths, and safety around it.

## Agent Context And Instructions

Browser MCP should expose a small context/instructions surface so agents know
how to use the browser capability correctly in the current Unpeel session.
This should be the primary instruction path instead of forcing users or agents
to install a separate skill. A skill can still be useful as optional long-form
guidance, but the MCP server itself should provide the live, session-specific
contract for how browser access works.

Prefer a belt-and-suspenders approach because MCP client support for resources
and prompts varies by provider:

- MCP resource: `unpeel-browser://context`
- MCP resource: `unpeel-browser://instructions`
- MCP prompts for common workflows, if the client surfaces prompts.
- Read-only MCP tool fallback: `browser_context`

The context payload should be compact, stable, and session-specific. It should
include:

- Browser access state and selected mode.
- Engine name and version.
- Active session id and recommended `agent-browser` session key.
- Artifact directory for screenshots, videos, logs, traces, and downloads.
- Approval policy.
- Site allow/deny rules.
- Whether screenshots may be captured automatically.
- Whether profile data is isolated, copied from Chrome, persistent, or connected
  to a live Chrome/CDP session.
- Whether full CDP access is enabled.
- Default local URL handling.
- Mobile/iOS availability and selected simulator device, if enabled.
- Short safety rules for credentials, cookies, downloads, and external sites.

The instructions payload should be short enough to be read before tool use. It
should tell the agent:

- Use snapshots before clicking or filling forms.
- Prefer stable element refs from snapshots over brittle coordinates.
- Ask before using shared profile or live Chrome data when policy requires it.
- Save screenshots and videos into the provided artifact directory.
- Avoid leaking cookies, localStorage, tokens, or downloaded private files into
  chat unless the user explicitly asks.
- Re-check the browser state after navigation, form submission, or major UI
  changes.
- Use mobile/iOS mode only when the task asks for mobile Safari behavior.

This endpoint is also where Unpeel can give provider-specific hints without
hard-coding them into every provider integration. For example, Codex, Claude,
and Gemini may surface MCP prompts/resources differently; the read-only tool
keeps the instructions discoverable even when prompt/resource support is weak.

## Artifacts

Store artifacts under the Unpeel session where possible, for example:

```text
~/.unpeel/app-sessions/<session-id>/artifacts/browser/
  screenshots/
  videos/
  logs/
  traces/
```

Artifacts should be visible to both the human and the agent. The MCP server
should return stable file paths for screenshots, recordings, and traces so
agents can inspect or reference them later.

## Privacy And Safety

Browser automation touches sensitive data. The default should be conservative.

- Prefer isolated per-session browser profiles by default.
- Make shared profile access explicit.
- Make it clear when cookies, logged-in sessions, or website credentials may be
  available to an agent.
- Store artifacts locally.
- Do not upload screenshots, logs, traces, or page content.
- Allow users to clear Browser MCP profiles and artifacts.
- Avoid granting browser access to all sessions silently.

## Bundling And Release

Bundling checklist:

- Verify `agent-browser` license and include required Apache-2.0 notices.
- Include the selected macOS binaries in `Unpeel.app`.
- Mark bundled binaries executable.
- Sign bundled binaries as part of the native app signing pipeline.
- Ensure notarization covers the helper and engine binaries.
- Add a release-pipeline check that the expected binary exists for each shipped
  architecture.
- Decide whether to ship both Apple Silicon and Intel binaries, or only the
  supported architecture set for Unpeel.
- Pin the engine version and update intentionally.

Do not depend on npm postinstall behavior at runtime.

## Health Check

The health check should be available from Settings and useful to agents.

It should verify:

- bundled engine binary exists
- binary can execute
- engine version matches the pinned expected version
- desktop Chrome/Chromium is available or a custom path is valid
- artifact directory is writable
- MCP mode starts and responds
- Xcode command line tools are available, if mobile mode is enabled
- iOS Simulator runtime/device is available, if mobile mode is enabled
- Appium/XCUITest dependencies are available, if required by the selected engine

The report should be copyable and concise enough to paste into an agent session.

## Later

- Add a separate native iOS Simulator MCP for arbitrary `.app` install, launch,
  deep link, log, and XCTest/Appium native app automation.
- Add project-level defaults for browser profile, device, and tool sets.
- Add artifact previews in the session UI.
- Add a "capture current page state" action for humans.
- Add managed browser install support if relying on system Chrome is too brittle.
- Add engine abstraction if another browser automation backend becomes valuable.
- Add remote browser/device providers, if privacy and cost controls are clear.

## Open Questions

- Should Browser MCP be enabled by default for new sessions, or opt-in per
  project/session?
- Should mobile/iOS tools be a separate toggle from desktop browser tools?
- Should Unpeel ship Intel macOS support for this engine if the main app still
  supports Intel?
- Does the engine's iOS path require Appium installation, and if so should
  Unpeel manage that dependency or only diagnose it?
- Should artifacts be pruned by age, size, or only manually?
