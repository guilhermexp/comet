//
//  BrowserSettingsPanel.swift
//  UnpeelNative
//
//  Extracted from SettingsView.swift — Settings ▸ Browser use panel.
//

import SwiftUI

/// Settings home for the Unpeel Browser MCP: engine status, options, and the
/// app-wide Browser Access. Access is the single `browser_default_access` field
/// in app-state.json (read per call by the host's `browser` domain gate) — one
/// global on/off, no per-session override.
struct BrowserSettingsPanel: View {
    @ObservedObject var store: UnpeelStore

    /// Resolved engine path, mirroring browser_mcp.rs resolve_engine_binary:
    /// env override → bundled next to unpeel-host → managed dir → login-shell
    /// PATH. nil while probing; empty string when nothing was found.
    @State private var enginePath: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Form {
                Section {} header: {
                    SettingsPaneHeader(
                        title: "Browser MCP",
                        description: "Browser MCP lets an agent session drive a real browser — "
                            + "open pages, click, fill forms, and take screenshots. Each session "
                            + "gets its own isolated browser (own profile and cookies, no access "
                            + "to your logins) that closes with the session."
                    )
                    .padding(.bottom, 4)
                }

                statusSection
                defaultSection
                approvalsSection
                optionsSection
                siteRulesSection
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
        }
        .task { await resolveEngine() }
        .onAppear {
            allowedDomainsDraft = store.browserSettings.allowedDomains
            executablePathDraft = store.browserSettings.executablePath
        }
    }

    /// Text-field drafts commit on submit/focus-loss instead of per keystroke,
    /// so we don't rewrite app-state.json on every character.
    @State private var allowedDomainsDraft = ""
    @State private var executablePathDraft = ""

    private var optionsSection: some View {
        Section {
            LabeledContent {
                Toggle(
                    "",
                    isOn: Binding(
                        get: { store.browserSettings.headed },
                        set: { value in store.updateBrowserSettings { $0.headed = value } }
                    )
                )
                .toggleStyle(.switch)
                .labelsHidden()
                .controlSize(.small)
            } label: {
                VStack(alignment: .leading, spacing: 1) {
                    Text("Show browser window")
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.foreground)
                    Text(store.browserSettings.headed
                        ? "You see what the agent does, live."
                        : "The browser runs in the background — screenshots still work.")
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            if store.browserSettings.headed {
                LabeledContent {
                    Toggle(
                        "",
                        isOn: Binding(
                            get: { store.browserSettings.showCursor },
                            set: { value in
                                store.updateBrowserSettings { $0.showCursor = value }
                            }
                        )
                    )
                    .toggleStyle(.switch)
                    .labelsHidden()
                    .controlSize(.small)
                } label: {
                    VStack(alignment: .leading, spacing: 1) {
                        Text("Show agent cursor")
                            .font(.system(size: 13))
                            .foregroundStyle(Theme.foreground)
                        Text(store.browserSettings.showCursor
                            ? "A pointer glides to whatever the agent clicks or fills, so "
                                + "you can follow along. Adds a short beat before each action."
                            : "Actions happen instantly, with no visible pointer.")
                            .font(.system(size: 11))
                            .foregroundStyle(Theme.mutedForeground)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }

            LabeledContent {
                Picker(
                    "",
                    selection: Binding(
                        get: { store.browserSettings.keepsProjectProfile },
                        set: { keep in
                            store.updateBrowserSettings {
                                $0.profileMode = keep ? "project" : "session"
                            }
                        }
                    )
                ) {
                    Text("Fresh each session").tag(false)
                    Text("Kept per project").tag(true)
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .fixedSize()
            } label: {
                VStack(alignment: .leading, spacing: 1) {
                    Text("Browsing data")
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.foreground)
                    Text(store.browserSettings.keepsProjectProfile
                        ? "Cookies and logins persist across a project's sessions — sign in "
                            + "once, every agent in the project stays signed in."
                        : "Cookies and logins vanish when a session's browser closes.")
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            if store.browserSettings.keepsProjectProfile {
                LabeledContent {
                    Button("Clear…") { store.clearBrowserProfiles() }
                        .controlSize(.small)
                } label: {
                    VStack(alignment: .leading, spacing: 1) {
                        Text("Clear kept browsing data")
                            .font(.system(size: 13))
                            .foregroundStyle(Theme.foreground)
                        Text("Deletes the saved project profiles (logins, cookies). "
                            + "Already-open browsers keep theirs until closed.")
                            .font(.system(size: 11))
                            .foregroundStyle(Theme.mutedForeground)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }

            LabeledContent {
                TextField("Auto-detect Chrome", text: $executablePathDraft)
                    .textFieldStyle(.roundedBorder)
                    .font(.system(size: 11, design: .monospaced))
                    .frame(width: 220)
                    .onSubmit { commitExecutablePath() }
            } label: {
                VStack(alignment: .leading, spacing: 1) {
                    Text("Browser app")
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.foreground)
                    Text("Path to a Chromium-based browser (Chrome, Brave, Edge, Arc). "
                        + "Leave empty to auto-detect. Press Return to apply.")
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        } header: {
            SettingsSectionHeader(
                title: "Options",
                description: "Applied to the agent's next browser action — no restart needed."
            )
        }
    }

    private var siteRulesSection: some View {
        Section {
            VStack(alignment: .leading, spacing: 6) {
                TextField("example.com, *.example.com", text: $allowedDomainsDraft)
                    .textFieldStyle(.roundedBorder)
                    .font(.system(size: 12, design: .monospaced))
                    .onSubmit { commitAllowedDomains() }
                Text(store.browserSettings.allowedDomains.trimmingCharacters(in: .whitespaces).isEmpty
                    ? "All sites allowed. Add comma-separated domains to restrict browsing — "
                        + "wildcards like *.example.com work, and the browser itself blocks "
                        + "everything else (pages, scripts, requests). Press Return to apply."
                    : "Browsing is restricted to the listed domains, enforced inside the "
                        + "browser engine. Clear the field and press Return to allow all sites.")
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.mutedForeground)
                    .fixedSize(horizontal: false, vertical: true)
            }
        } header: {
            SettingsSectionHeader(
                title: "Site access",
                description: "Limit which websites agents can reach."
            )
        }
    }

    private func commitAllowedDomains() {
        let value = allowedDomainsDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        store.updateBrowserSettings { $0.allowedDomains = value }
    }

    private func commitExecutablePath() {
        let value = executablePathDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        store.updateBrowserSettings { $0.executablePath = value }
    }


    /// The engine ships inside the app, so there is nothing to configure and
    /// nothing to show when it's healthy. This section renders ONLY when the
    /// engine genuinely can't be resolved (a broken/hand-modified install) —
    /// "it should simply just work" is the product bar.
    @ViewBuilder
    private var statusSection: some View {
        if enginePath == "" {
            Section {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Browser engine missing")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(.orange)
                    Text("This Unpeel install is missing its bundled browser engine, so "
                        + "browser tools are unavailable. Reinstall Unpeel from "
                        + "unpeel.com/download/mac to fix it. (Dev builds can also use "
                        + "npm install -g agent-browser or "
                        + "~/.unpeel/browser/bin/agent-browser.)")
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

    /// The app-wide default every session gets unless individually overridden.
    /// Default off: browser automation is opt-in.
    private var defaultSection: some View {
        Section {
            LabeledContent {
                Picker(
                    "",
                    selection: Binding(
                        get: { store.browserDefaultAccess },
                        set: { store.setDefaultBrowserAccess($0) }
                    )
                ) {
                    ForEach(BrowserAccess.allCases) { access in
                        Text(access.label).tag(access)
                    }
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .fixedSize()
            } label: {
                VStack(alignment: .leading, spacing: 1) {
                    Text("Browser access")
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.foreground)
                    Text(store.browserDefaultAccess.detail)
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        } footer: {
            Text("Off applies immediately to running sessions. Enabling reaches an agent "
                + "when it starts in a newly configured terminal.")
                .font(.system(size: 11))
                .foregroundStyle(Theme.mutedForeground)
        }
    }

    /// Remembered per-session approvals, shown only in Ask mode (mirrors
    /// Settings ▸ Computer).
    @ViewBuilder
    private var approvalsSection: some View {
        if store.browserDefaultAccess == .ask, !store.browserApprovals.isEmpty {
            Section("Approved sessions") {
                ForEach(store.browserApprovals, id: \.self) { sessionID in
                    LabeledContent {
                        Button("Revoke") {
                            store.revokeBrowserApproval(sessionID: sessionID)
                        }
                        .controlSize(.small)
                    } label: {
                        Text(store.sessionDisplayName(sessionID))
                            .font(.system(size: 13))
                            .foregroundStyle(Theme.foreground)
                    }
                }
            }
        }
    }

    /// Same candidate order the MCP host uses, so the status row reflects what
    /// a session would actually launch. The PATH probe runs through a login
    /// shell because GUI apps get a bare PATH.
    private func resolveEngine() async {
        if let override = ProcessInfo.processInfo.environment["UNPEEL_BROWSER_BIN"],
           FileManager.default.isExecutableFile(atPath: override) {
            enginePath = override
            return
        }
        let hostSibling = URL(fileURLWithPath: LaunchConfig.hostBinary)
            .deletingLastPathComponent()
            .appendingPathComponent("agent-browser").path
        if FileManager.default.isExecutableFile(atPath: hostSibling) {
            enginePath = hostSibling
            return
        }
        let managed = LaunchConfig.unpeelDir
            .appendingPathComponent("browser/bin/agent-browser").path
        if FileManager.default.isExecutableFile(atPath: managed) {
            enginePath = managed
            return
        }
        let fromPath = await Task.detached(priority: .utility) { () -> String? in
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/bin/zsh")
            process.arguments = ["-lc", "command -v agent-browser"]
            let pipe = Pipe()
            process.standardOutput = pipe
            process.standardError = FileHandle.nullDevice
            guard (try? process.run()) != nil else { return nil }
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            guard process.terminationStatus == 0 else { return nil }
            let path = String(decoding: data, as: UTF8.self)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return path.isEmpty ? nil : path
        }.value
        enginePath = fromPath ?? ""
    }
}

// MARK: - Nav row (.settings-row, SettingsView.svelte:211-271)
