//
//  SettingsView.swift
//  UnpeelNative
//
//  Settings presentation, deliberately different from the Svelte
//  full-screen swap (designer's spec, 2026-06-12): the app layout never
//  hides. Instead:
//  - SettingsSidebarPanel: the Back row + tab nav. It slides into the
//    sidebar's list area using the exact worktrees-panel motion
//    (offset ±140 + fade, 0.2s cubicOut — SidebarMotion.slide); the
//    footer stays put, same as the worktrees slide.
//  - SettingsContentHost: the right-hand pane — centered
//    "Settings / <Tab>" titlebar (SettingsView.svelte:100-127) over the
//    active panel. ContentArea swaps it with the terminal INSTANTLY
//    (transition .identity, animation nil): the Ghostty surface is
//    Metal-backed, and animating opacity across a CAMetalLayer is what
//    produced the old full-window blink. Tab switching swaps panels
//    in place with no transition, like the Svelte tab switch.
//  Esc returns to the workspace (SettingsView.svelte:45-50).
//
//  Panel inventory vs the Svelte SETTINGS_TABS (settings/tabs.ts:9-35):
//  - Appearance → AppearanceSettingsPanel (mode only — light/dark/system;
//                 no ambience presets natively yet). Persists as a native
//                 UserDefaults overlay over the read-only app-state.json
//                 `theme` (the native app must never write that file).
//  - Presets   → PresetsSettingsPanel (full parity, native overlay storage)
//  - Advanced  → AdvancedSettingsPanel (resource diagnostics, old-session
//                cleanup, and the gear menu's folder/log utilities)
//  - General and Tags are omitted from the nav: their machinery
//    (code-editor preference, tag CRUD) does not exist in the native build
//    and both persist via app-state.json writes.
//

import AppKit
import CoreImage
import SwiftUI
import UnpeelShared

// MARK: - Tabs (settings/tabs.ts)

enum SettingsTab: String, CaseIterable, Identifiable {
    case presets
    case appearance
    case mobile
    case workspaces
    case transcripts
    case notifications
    case sessions
    case browser
    case computer
    case experimental
    case advanced
    // The standalone "Unpeel Link" license tab was merged into Remote
    // (2026-08-13): license + seat status now render as a section of the
    // Remote panel, next to the device enrollment list. Legacy deep links
    // ("license") are mapped to .mobile in Snapshot restore.

    var id: String { rawValue }

    /// `profiles` was the released developer deep-link spelling before the
    /// isolated-instance feature became Workspaces. It was never persisted as
    /// app state, but accepting it keeps existing snapshot/dev commands valid.
    static func compatibleRawValue(_ rawValue: String) -> SettingsTab? {
        SettingsTab(rawValue: rawValue == "profiles" ? "workspaces" : rawValue)
    }

    static var visibleCases: [SettingsTab] {
        allCases.filter { tab in
            switch tab {
            case .mobile: return UnpeelFeatureFlags.mobileRemoteControlEnabled
            // Sessions MCP is experimental (Settings ▸ Experimental); its
            // panel only exists while the feature is on.
            case .sessions: return UnpeelFeatureFlags.isEnabled(.sessionsMcp)
            // Browser and computer use are experimental too; their panels
            // only exist while the features are on.
            case .browser: return UnpeelFeatureFlags.isEnabled(.browserMcp)
            case .computer: return UnpeelFeatureFlags.isEnabled(.computerUse)
            case .workspaces: return UnpeelFeatureFlags.isEnabled(.workspaces)
            default: return true
            }
        }
    }

    var title: String {
        switch self {
        case .appearance: return "Appearance"
        case .presets: return "Presets"
        case .mobile: return "Remote"
        case .workspaces: return "Workspaces"
        case .transcripts: return "Transcripts"
        case .notifications: return "Notifications"
        case .sessions: return "Sessions use"
        case .browser: return "Browser use"
        case .computer: return "Computer use"
        case .experimental: return "Experimental"
        case .advanced: return "Advanced"
        }
    }

    /// The first-party MCP domain panels, grouped under an "Unpeel MCP"
    /// header at the bottom of the sidebar nav (one unified server, one
    /// domain per panel).
    var isBuiltInMCP: Bool {
        switch self {
        case .sessions, .browser, .computer: return true
        default: return false
        }
    }
}

// MARK: - Sidebar nav panel (.settings-sidebar nav)

/// The Back row + tab nav that slides into the sidebar's list area while
/// settings is open (SidebarView hosts it in the same ZStack as the
/// project tree, with the same ±140 slide). No background here — it
/// rides on the sidebar's existing chrome; list padding
/// `calc(titlebar + 2px) 8px 12px`, gap 2 (SettingsView.svelte:173-179).
struct SettingsSidebarPanel: View {
    @ObservedObject var store: UnpeelStore

    private static let feedbackURL = URL(string: "https://github.com/orgs/unpeel-com/discussions")!

    var body: some View {
        VStack(spacing: 0) {
            // The nav scrolls when the window is short, dissolving into the
            // top chrome / bottom footer through the same fade mask as the
            // main sidebar lists.
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    SettingsNavRow(
                        title: "Back",
                        leadingIcon: .back,
                        isActive: false,
                        action: { store.closeSettings() }
                    )
                    .padding(.bottom, 6) // .back-row margin-bottom 6

                    // .settings-nav-list, gap 1.5
                    VStack(alignment: .leading, spacing: 1.5) {
                        ForEach(SettingsTab.visibleCases.filter { !$0.isBuiltInMCP }) { tab in
                            SettingsNavRow(
                                title: tab.title,
                                isActive: tab == selectedTab,
                                action: { store.settingsTab = tab }
                            )
                        }

                        // Built-in MCP panels grouped under their own header at the
                        // bottom of the nav.
                        SettingsNavSectionHeader(title: "Unpeel MCP")
                            .padding(.top, 10)

                        ForEach(SettingsTab.visibleCases.filter(\.isBuiltInMCP)) { tab in
                            SettingsNavRow(
                                title: tab.title,
                                isActive: tab == selectedTab,
                                action: { store.settingsTab = tab }
                            )
                        }
                    }
                }
                // Bottom padding keeps the last row clear of the mask's
                // 26pt bottom fade when scrolled to the end.
                .padding(EdgeInsets(
                    top: Theme.titlebarHeight + 2, leading: 8, bottom: 26, trailing: 8
                ))
                .frame(maxWidth: .infinity, alignment: .topLeading)
            }
            .scrollIndicators(.hidden)
            .mask(SidebarListFadeMask())

            // Pinned at the bottom: opens GitHub Discussions (same as the
            // website footer's "Bugs & Feedback" link and the iOS sidebar).
            // Borderless like the main sidebar footer — the list's bottom
            // fade provides the separation.
            Link(destination: Self.feedbackURL) {
                HStack(spacing: 8) {
                    Image(systemName: "exclamationmark.bubble")
                        .font(.system(size: 12, weight: .medium))
                    Text("Feedback & bugs")
                        .font(.system(size: 12.5, weight: .medium))
                    Spacer(minLength: 4)
                    Image(systemName: "arrow.up.right")
                        .font(.system(size: 10, weight: .semibold))
                        .opacity(0.6)
                }
                .foregroundStyle(.secondary)
                .padding(.horizontal, 10)
                .padding(.vertical, 7)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .padding(EdgeInsets(top: 0, leading: 8, bottom: 12, trailing: 8))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .overlay(alignment: .top) {
            // .settings-sidebar-drag-region: titlebar-height drag strip.
            WindowDragArea().frame(height: Theme.titlebarHeight)
        }
    }

    private var selectedTab: SettingsTab {
        if SettingsTab.visibleCases.contains(store.settingsTab) {
            return store.settingsTab
        }
        // The selected tab's gate (Mobile dev flag, Sessions MCP experiment)
        // turned off — fall back to the first tab.
        return .presets
    }
}

// MARK: - Content host (.settings-main-shell)

/// The content-pane half of settings: "Settings / <Tab>" titlebar over the
/// active panel. ContentArea swaps this with the terminal without any
/// animation (see TerminalArea.swift) — only the sidebar nav animates.
struct SettingsContentHost: View {
    @ObservedObject var store: UnpeelStore

    var body: some View {
        VStack(spacing: 0) {
            settingsTitlebar
            // Each panel is a grouped Form (its own scroll view), System
            // Settings style — no outer ScrollView. The column is capped at
            // 740pt: macOS grouped Form caps its section cards at ~700pt,
            // so 740 leaves the standard 20pt gutters and lets the panels'
            // fixed chrome (pane header, CTA rows) align with the cards by
            // using the same 20pt horizontal padding.
            panelContent
                .frame(maxWidth: 740)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .mask(panelTopFade)
        }
        .background(SettingsMainBackground())
        .background(
            // Escape closes settings (SettingsView.svelte handleKeydown).
            Button("") { store.closeSettings() }
                .keyboardShortcut(.cancelAction)
                .opacity(0)
        )
    }

    /// "Settings / <Tab>" centered, 13px/600 muted, gap 8, separator at
    /// 0.54 opacity (SettingsView.svelte:300-324); the strip drags the
    /// window like the workspace titlebar.
    private var settingsTitlebar: some View {
        ZStack {
            WindowDragArea()
            HStack(spacing: 8) {
                Text("Settings")
                Text("/").opacity(0.54)
                Text(selectedTab.title)
            }
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(Theme.mutedForeground)
            // Share the content column's geometry (740pt cap + 20pt inset,
            // see panelContent) and left-align so the breadcrumb lines up
            // with the pane header below instead of centering on the window.
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 20)
            .frame(maxWidth: 740)
            .frame(maxWidth: .infinity)
            .allowsHitTesting(false)
        }
        .frame(height: Theme.titlebarHeight)
    }

    /// The breadcrumb is the only pinned chrome; the pane title, description
    /// and every section scroll under it. This top fade dissolves that content
    /// as it meets the breadcrumb instead of cutting it off with a hard edge.
    private var panelTopFade: some View {
        VStack(spacing: 0) {
            LinearGradient(colors: [.clear, .black], startPoint: .top, endPoint: .bottom)
                .frame(height: 22)
            Color.black
        }
    }

    @ViewBuilder
    private var panelContent: some View {
        switch selectedTab {
        case .appearance:
            AppearanceSettingsPanel(store: store)
        case .presets:
            PresetsSettingsPanel(store: store)
        case .transcripts:
            TranscriptsSettingsPanel(store: store)
        case .notifications:
            NotificationsSettingsPanel(store: store)
        case .sessions:
            UnpeelMCPSettingsPanel(store: store)
        case .browser:
            BrowserSettingsPanel(store: store)
        case .computer:
            ComputerSettingsPanel(store: store)
        case .experimental:
            ExperimentalSettingsPanel(store: store)
        case .mobile:
            RemoteSettingsPanel(store: store)
        case .workspaces:
            // Unpeel Link (license + enrollment) lives on the Remote tab.
            WorkspacesSettingsPanel(onOpenPro: { store.openSettings(tab: .mobile) })
        case .advanced:
            AdvancedSettingsPanel(store: store)
        }
    }

    private var selectedTab: SettingsTab {
        if SettingsTab.visibleCases.contains(store.settingsTab) {
            return store.settingsTab
        }
        // The selected tab's gate (Mobile dev flag, Sessions MCP experiment)
        // turned off — fall back to the first tab.
        return .presets
    }
}

/// `.settings-main-shell` background: the glass content tint layered over
/// an extra dimming wash — black 24% dark, white 36% light
/// (SettingsView.svelte:273-282 + the light-theme override below it).
struct SettingsMainBackground: View {
    var body: some View {
        ZStack {
            VisualEffectBackground(material: .underWindowBackground)
            Theme.settingsShellTint
        }
        .ignoresSafeArea()
    }
}

// MARK: - Appearance panel (settings/AppearancePanel.svelte, mode only)

/// Native Appearance panel: the theme mode picker. The Svelte panel's
/// second control (Ambience color schemes) has no native machinery yet, so
/// it is omitted rather than faked.
struct AppearanceSettingsPanel: View {
    @ObservedObject var store: UnpeelStore

    /// The same editor set the titlebar "open" dropdown offers, limited to
    /// installed apps — plus the current selection even if it isn't installed,
    /// so a saved default never silently disappears from the picker.
    private var editorOptions: [WorkspaceOpenTarget] {
        var options = WorkspaceOpenTarget.editorTargets.filter { $0.isAvailable }
        if !options.contains(where: { $0.codeEditorID == store.codeEditor }),
           let selected = WorkspaceOpenTarget.editorTargets
               .first(where: { $0.codeEditorID == store.codeEditor }) {
            options.insert(selected, at: 0)
        }
        return options
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Form {
                // Pane title/description ride along as a background-less
                // Section header so they scroll under the sticky breadcrumb,
                // unlike grouped-Form rows which always grow a card.
                Section {} header: {
                    SettingsPaneHeader(
                        title: "Appearance",
                        description: "How Unpeel looks. System follows your macOS appearance."
                    )
                    .padding(.bottom, 4)
                }

                Section {
                    Picker("Mode", selection: Binding(
                        get: { store.themePreference },
                        set: { store.setThemePreference($0) }
                    )) {
                        ForEach(ThemePreference.allCases) { preference in
                            Text(preference.title).tag(preference)
                        }
                    }
                    .pickerStyle(.segmented)
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.foreground)
                } header: {
                    SettingsSectionHeader(
                        title: "Mode",
                        description: "Applies to the window, sidebar and terminal colors. "
                            + "Claude Code has its own theme setting — run /config inside "
                            + "Claude Code and change Theme to match."
                    )
                }

                Section {
                    Picker("Editor", selection: Binding(
                        get: { store.codeEditor },
                        set: { store.setCodeEditor($0) }
                    )) {
                        ForEach(editorOptions, id: \.id) { target in
                            Label {
                                Text(target.title)
                            } icon: {
                                WorkspaceAppIconView(target: target, size: 16)
                            }
                            .tag(target.codeEditorID ?? "")
                        }
                    }
                    .pickerStyle(.menu)
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.foreground)
                } header: {
                    SettingsSectionHeader(
                        title: "Default editor",
                        description: "Used by \"Open in editor\" and the titlebar open button."
                    )
                }

                Section {
                    LabeledContent {
                        Toggle(
                            "",
                            isOn: Binding(
                                get: { store.showSessionToolIcons },
                                set: { store.showSessionToolIcons = $0 }
                            )
                        )
                        .toggleStyle(.switch)
                        .labelsHidden()
                        .controlSize(.small)
                    } label: {
                        VStack(alignment: .leading, spacing: 1) {
                            Text("Show agent logos")
                                .font(.system(size: 13))
                                .foregroundStyle(Theme.foreground)
                            Text("Mark each session with its agent's logo, to the "
                                + "right of the date.")
                                .font(.system(size: 11))
                                .foregroundStyle(Theme.mutedForeground)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                } header: {
                    SettingsSectionHeader(
                        title: "Sessions",
                        description: "How session rows appear in the sidebar."
                    )
                }

                Section {
                    LabeledContent {
                        Toggle(
                            "",
                            isOn: Binding(
                                get: { store.showSessionGallery },
                                set: { store.showSessionGallery = $0 }
                            )
                        )
                        .toggleStyle(.switch)
                        .labelsHidden()
                        .controlSize(.small)
                    } label: {
                        VStack(alignment: .leading, spacing: 1) {
                            Text("Session gallery")
                                .font(.system(size: 13))
                                .foregroundStyle(Theme.foreground)
                            Text("Photo chip in the terminal title bar with the "
                                + "session's captures, plus Take Screenshot (⇧⌘S) "
                                + "to shoot into the session and attach it to the "
                                + "prompt. Turn off if you use your own screenshot "
                                + "tools.")
                                .font(.system(size: 11))
                                .foregroundStyle(Theme.mutedForeground)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                } header: {
                    SettingsSectionHeader(
                        title: "Terminal",
                        description: "Extras around the terminal view."
                    )
                }
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
        }
    }
}

// MARK: - Remote access panel

struct RemoteSettingsPanel: View {
    @ObservedObject var store: UnpeelStore

    /// Install link for the iPhone app. Always unpeel.com/ios — the site 302s
    /// to the current TestFlight public link (and later the App Store page),
    /// so shipped desktop builds never hold a stale store URL.
    private static let iosAppURL = URL(string: "https://unpeel.com/ios")!

    @State private var sharePresented = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Form {
                Section {} header: {
                    SettingsPaneHeader(
                        title: "Remote",
                        description: RemoteHostFeature.pickerEnabled
                            ? "Control other machines from this app, and let other devices control this Mac."
                            : "Let another Unpeel device control this Mac."
                    )
                    .padding(.bottom, 4)
                }

                // Two symmetric lists, one pairing contract: outbound (Hosts
                // this app controls) then inbound (Controllers of this Mac).
                // Host scope switching lives here — the sidebar footer's
                // remote button only opens this screen.
                if RemoteHostFeature.pickerEnabled {
                    HostScopeSection(store: store, hosts: store.remoteHostStore)
                }

                controlsThisMacSection
                testflightSection
                // Unpeel Link: the enrollment list (which replaced the old
                // global relay toggle — the uplink runs whenever ≥1 inbound
                // device is on Link), then the subscription/seat block.
                LinkEnrollmentSection(store: store, hosts: store.remoteHostStore)
                LinkLicenseSections(store: store)
                securitySection
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
        }
        .onAppear {
            store.startHostRemoteServer()
            store.refreshPairedControllers()
        }
    }

    /// Banner pointing users at the iPhone app's TestFlight beta. Sits
    /// directly below the inbound list because installing the phone app is
    /// step zero of pairing it.
    private var testflightSection: some View {
        Section {
            HStack(alignment: .center, spacing: 16) {
                TestFlightIconView()
                    .frame(width: 84, height: 84)

                VStack(alignment: .leading, spacing: 6) {
                    Text("Unpeel for iPhone is in beta")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(Theme.foreground)
                    Text("Join the TestFlight beta to control this Mac from your phone. Open the invite link on your iPhone to install it with TestFlight.")
                        .font(.system(size: 12))
                        .foregroundStyle(Theme.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)

                    Button("Join the Beta") {
                        NSWorkspace.shared.open(Self.iosAppURL)
                    }
                    .controlSize(.small)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(.vertical, 6)
            .listRowBackground(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Theme.accent.opacity(0.09))
            )
        }
    }

    /// The inbound list — Controllers that drive this Mac. Mirrors the
    /// outbound "This App Controls" list above it: rows are paired devices,
    /// and the add verb is "Share This Mac…" (mint and show a one-time code).
    private var controlsThisMacSection: some View {
        Section {
            if let error = store.hostServerError {
                Text(error)
                    .font(.system(size: 13))
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }

            SettingsValueRow(
                label: "Local access",
                value: store.hostServerEndpoint == nil ? "Unavailable" : "Serving on this network"
            )

            if store.pairedControllers.isEmpty {
                Text("No paired devices.")
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.mutedForeground)
            } else {
                ForEach(store.pairedControllers) { device in
                    LabeledContent {
                        HStack(spacing: 12) {
                            // Link scope is managed in the Unpeel Link
                            // section below; this is just a reminder of the
                            // device's current reach.
                            Text(device.relayAllowed != false ? "Link" : "Direct only")
                                .font(.system(size: 11))
                                .foregroundStyle(Theme.mutedForeground)

                            Button("Revoke", role: .destructive) {
                                store.revokeMobileDevice(device.id)
                            }
                            .buttonStyle(.borderless)
                            .controlSize(.small)
                        }
                    } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(device.name)
                                .font(.system(size: 13, weight: .medium))
                                .foregroundStyle(Theme.foreground)
                                .lineLimit(1)
                            Text(deviceDetail(device))
                                .font(.system(size: 11))
                                .foregroundStyle(Theme.mutedForeground)
                                .lineLimit(1)
                        }
                    }
                }
            }

            Button {
                sharePresented = true
            } label: {
                Label("Share This Mac…", systemImage: "plus")
            }
            .sheet(isPresented: $sharePresented) {
                ShareThisMacSheet(store: store)
            }
        } header: {
            SettingsSectionHeader(
                title: "Controls This Mac",
                description: "Devices pair directly over your network and receive their own revocable credential. Revoking one immediately invalidates it."
            )
        }
    }

    private var securitySection: some View {
        Section {
            SettingsValueRow(label: "Authentication", value: "Per-device bearer token")
            SettingsValueRow(label: "Pairing code", value: "One-time, 5 minutes")
            SettingsValueRow(label: "Stored token", value: "SHA-256 hash")
        } header: {
            SettingsSectionHeader(
                title: "Security",
                description: "The hook and MCP servers stay localhost-only. Remote Controllers use a separate LAN server."
            )
        }
    }

}

/// "iOS 1.2 • last seen Aug 13, 09:41" — shared by the Controls This Mac
/// list and the Unpeel Link enrollment list so both describe a device the
/// same way.
private func deviceDetail(_ device: RemotePairedDeviceSummary) -> String {
    let lastSeen: String
    if let lastSeenAt = device.lastSeenAtUnixMs {
        let date = Date(timeIntervalSince1970: TimeInterval(lastSeenAt) / 1000)
        lastSeen = "last seen \(date.formatted(date: .abbreviated, time: .shortened))"
    } else {
        lastSeen = "never seen"
    }
    let version = device.appVersion.map { " \($0)" } ?? ""
    return "\(device.platform)\(version) • \(lastSeen)"
}

/// "Share This Mac…" sheet — mints a one-time pairing code on open and shows
/// the QR until a Controller consumes it. Minting on open (rather than on
/// every Remote-panel visit) keeps a live code off the screen until the user
/// actually intends to pair; re-minting is free — it just replaces the single
/// active one-time token. Dismissing keeps the code valid until its TTL so
/// copy-then-paste-on-another-Mac flows survive closing the sheet.
struct ShareThisMacSheet: View {
    @ObservedObject var store: UnpeelStore
    @Environment(\.dismiss) private var dismiss
    @State private var now = Date()
    @State private var cliCommandCopied = false

    /// This Mac's Bonjour/DNS name, ready to paste into an SSH target from
    /// another machine on the same network.
    private static let sshHostName = ProcessInfo.processInfo.hostName

    /// The terminal counterpart of the QR code: the `unpeel` CLI controls
    /// this Mac over the operator's existing SSH access (the Host-side
    /// stdio gateway). SSH is a transport for the same Host contract — no
    /// pairing code involved, so it sits alongside the QR, not inside it.
    private var cliCommand: String {
        "unpeel --host ssh://\(Self.sshHostName)"
    }

    private var expiry: Date? {
        guard let payload = store.hostPairingPayload else { return nil }
        return Date(timeIntervalSince1970: TimeInterval(payload.expiresAtUnixMs) / 1000)
    }

    private var expiresInText: String {
        guard let expiry else { return "" }
        let remaining = max(0, Int(expiry.timeIntervalSince(now).rounded(.down)))
        return String(format: "Expires in %d:%02d", remaining / 60, remaining % 60)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Share This Mac")
                        .font(.system(size: 20, weight: .semibold))
                    Text("Let another Unpeel device control this Mac.")
                        .font(.system(size: 12))
                        .foregroundStyle(Theme.mutedForeground)
                }
                Spacer()
                Button("Done") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }

            if let error = store.hostServerError {
                Text(error)
                    .font(.system(size: 13))
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }

            if store.hostPairingCompleted {
                HStack(spacing: 14) {
                    Image(systemName: "checkmark.circle.fill")
                        .font(.system(size: 34))
                        .foregroundStyle(.green)
                    VStack(alignment: .leading, spacing: 5) {
                        Text("Controller paired")
                            .font(.system(size: 13, weight: .semibold))
                        Text("The displayed one-time code has been consumed.")
                            .font(.system(size: 12))
                            .foregroundStyle(Theme.mutedForeground)
                        Button("Pair Another Controller") {
                            store.beginHostPairing()
                        }
                        .controlSize(.small)
                    }
                    Spacer()
                }
                .padding(.vertical, 12)
            } else {
                HStack(alignment: .top, spacing: 18) {
                    PairingQRCodeView(payload: store.hostPairingCode)
                        .frame(width: 184, height: 184)

                    VStack(alignment: .leading, spacing: 10) {
                        Text("Scan or paste this code into another Unpeel device.")
                            .font(.system(size: 13, weight: .semibold))
                            .foregroundStyle(Theme.foreground)
                        Text("The code expires in five minutes and can be used once. After pairing, that Controller receives its own revocable device token.")
                            .font(.system(size: 12))
                            .foregroundStyle(Theme.mutedForeground)
                            .fixedSize(horizontal: false, vertical: true)

                        if store.hostPairingPayload != nil {
                            Text(expiresInText)
                                .font(.system(size: 12, weight: .medium))
                                .monospacedDigit()
                                .foregroundStyle(Theme.mutedForeground)
                        }

                        HStack(spacing: 8) {
                            Button(store.hostPairingPayload == nil ? "Generate QR Code" : "Refresh QR Code") {
                                store.beginHostPairing()
                            }
                            .controlSize(.small)

                            if store.hostPairingPayload != nil {
                                Button("Copy Pairing Code") {
                                    guard let code = store.hostPairingCode else { return }
                                    NSPasteboard.general.clearContents()
                                    NSPasteboard.general.setString(code, forType: .string)
                                }
                                .controlSize(.small)
                            }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }

            Divider()

            // The CLI route: same Host, driven over SSH from any terminal.
            VStack(alignment: .leading, spacing: 6) {
                Text("Or connect from a terminal")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Theme.foreground)

                HStack(spacing: 8) {
                    Text(cliCommand)
                        .font(.system(size: 12, design: .monospaced))
                        .textSelection(.enabled)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 5)
                        .background(
                            RoundedRectangle(cornerRadius: 6, style: .continuous)
                                .fill(Theme.foreground.opacity(0.06))
                        )

                    Button(cliCommandCopied ? "Copied" : "Copy") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(cliCommand, forType: .string)
                        cliCommandCopied = true
                    }
                    .controlSize(.small)
                }

                Text("Run this on another machine with the Unpeel CLI installed "
                    + "(curl -fsSL https://unpeel.com/install.sh | sh). It rides your "
                    + "normal SSH access instead of a pairing code, so this Mac needs "
                    + "Remote Login on (System Settings ▸ General ▸ Sharing) and your "
                    + "SSH config must reach it — over a VPN or Tailscale too, but "
                    + "never through Unpeel Link.")
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.mutedForeground)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(24)
        .frame(width: 520)
        .onAppear { store.beginHostPairing() }
        // Live countdown, and never leave an expired QR on screen — an
        // expired code silently stops scanning, so re-mint at zero.
        .onReceive(Timer.publish(every: 1, on: .main, in: .common).autoconnect()) { tick in
            now = tick
            if let expiry, expiry <= tick, !store.hostPairingCompleted {
                store.beginHostPairing()
            }
        }
    }
}

/// Settings ▸ Remote ▸ On Unpeel Link — the enrollment list that replaced
/// the global relay toggle (2026-08-13). Devices listed here ride the
/// encrypted relay away from home; everything else stays Direct-only. Two
/// kinds of rows, one list: inbound paired Controllers (the per-device
/// `relayAllowed` flag) and outbound paired Hosts (the per-Host
/// `linkEnabled` flag). The uplink runs whenever ≥1 inbound device is
/// enrolled — the entitlement check itself stays server-side
/// (docs/plans/unpeel-link.md).
private struct LinkEnrollmentSection: View {
    @ObservedObject var store: UnpeelStore
    @ObservedObject var hosts: RemoteHostStore
    @ObservedObject private var uplink = RelayUplinkManager.shared

    private var enrolledDevices: [RemotePairedDeviceSummary] {
        store.pairedControllers.filter { $0.relayAllowed != false }
    }

    private var directOnlyDevices: [RemotePairedDeviceSummary] {
        store.pairedControllers.filter { $0.relayAllowed == false }
    }

    // Outbound Hosts appear only where the Host picker exists at all.
    private var enrolledHosts: [PairedHostRecord] {
        RemoteHostFeature.pickerEnabled ? hosts.records.filter(\.isLinkEnabled) : []
    }

    private var directOnlyHosts: [PairedHostRecord] {
        RemoteHostFeature.pickerEnabled ? hosts.records.filter { !$0.isLinkEnabled } : []
    }

    private var addCandidatesEmpty: Bool {
        directOnlyDevices.isEmpty && directOnlyHosts.isEmpty
    }

    var body: some View {
        Section {
            if enrolledDevices.isEmpty, enrolledHosts.isEmpty {
                Text("Nothing is on Link — every connection stays direct, on your own network.")
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.mutedForeground)
            }

            ForEach(enrolledDevices) { device in
                enrollmentRow(
                    icon: "iphone",
                    name: device.name,
                    detail: deviceDetail(device)
                ) {
                    store.setDeviceRelayAllowed(device.id, false)
                }
            }

            ForEach(enrolledHosts) { host in
                enrollmentRow(
                    icon: "server.rack",
                    name: host.name,
                    detail: host.hostID
                ) {
                    store.setHostLinkEnabled(host.hostID, false)
                }
            }

            // The uplink serves inbound devices; without one enrolled there
            // is nothing to report.
            if !enrolledDevices.isEmpty {
                SettingsValueRow(label: "Relay", value: uplink.status.label)
            }

            Menu {
                ForEach(directOnlyDevices) { device in
                    Button {
                        store.setDeviceRelayAllowed(device.id, true)
                    } label: {
                        Label(device.name, systemImage: "iphone")
                    }
                }
                ForEach(directOnlyHosts) { host in
                    Button {
                        store.setHostLinkEnabled(host.hostID, true)
                    } label: {
                        Label(host.name, systemImage: "server.rack")
                    }
                }
            } label: {
                Label("Add to Link…", systemImage: "plus")
            }
            .disabled(addCandidatesEmpty)
            .help(addCandidatesEmpty
                ? "Everything paired is already on Link. Pair a new device or Host from the lists above first."
                : "Enroll a paired device or Host on Unpeel Link.")
        } header: {
            SettingsSectionHeader(
                title: "Reachable outside your network (Unpeel Link)",
                description: "These devices reach this Mac — and these Hosts stay "
                    + "reachable — from any network, through the unpeel.com "
                    + "relay. Session traffic is end-to-end encrypted; notification "
                    + "titles pass through Unpeel and Apple Push. Everything not listed "
                    + "here connects direct-only, on your own network."
            )
        }
    }

    private func enrollmentRow(
        icon: String,
        name: String,
        detail: String,
        remove: @escaping () -> Void
    ) -> some View {
        LabeledContent {
            // A real bordered button: rendered borderless this read as a
            // status caption ("Direct Only") instead of the remove action,
            // which made enrollment look un-toggleable (2026-08-13).
            Button("Remove from Link", action: remove)
                .buttonStyle(.bordered)
                .controlSize(.small)
                .help("Take this device off Unpeel Link — it then connects only over your own network. Re-add it any time with \u{201C}Add to Link…\u{201D}.")
        } label: {
            HStack(spacing: 10) {
                Image(systemName: icon)
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.mutedForeground)
                    .frame(width: 18)
                VStack(alignment: .leading, spacing: 2) {
                    Text(name)
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(Theme.foreground)
                        .lineLimit(1)
                    Text(detail)
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .lineLimit(1)
                }
            }
        }
    }
}

private struct TestFlightIconView: View {
    private static let image: NSImage? = {
        guard let url = ModuleResources.url(forResource: "TestFlightIcon", withExtension: "png") else {
            return nil
        }
        return NSImage(contentsOf: url)
    }()

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .fill(Color.black.opacity(0.12))

            if let image = Self.image {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fit)
            } else {
                Image(systemName: "airplane")
                    .font(.system(size: 34, weight: .semibold))
                    .foregroundStyle(Theme.accent)
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .strokeBorder(Color.white.opacity(0.16))
        )
        .shadow(color: Color.black.opacity(0.18), radius: 8, y: 3)
        .accessibilityLabel("TestFlight")
    }
}

struct PairingQRCodeView: View {
    let payload: String?

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Color.white)
            if let image = qrImage {
                Image(nsImage: image)
                    .interpolation(.none)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .padding(12)
            } else {
                VStack(spacing: 6) {
                    Image(systemName: "qrcode")
                        .font(.system(size: 34, weight: .regular))
                    Text("No Code")
                        .font(.system(size: 12, weight: .medium))
                }
                .foregroundStyle(Color.black.opacity(0.38))
            }
        }
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(Theme.foreground.opacity(0.08))
        )
    }

    private var qrImage: NSImage? {
        guard let payload, let data = payload.data(using: .utf8) else { return nil }
        guard let filter = CIFilter(name: "CIQRCodeGenerator") else { return nil }
        filter.setValue(data, forKey: "inputMessage")
        // Lowest correction level: combined with the compact pairing code
        // this yields the coarsest (fastest-scanning) grid. The code sits on
        // a screen, not a scuffed sticker — damage tolerance buys nothing.
        filter.setValue("L", forKey: "inputCorrectionLevel")
        guard let output = filter.outputImage else { return nil }
        let image = output.transformed(by: CGAffineTransform(scaleX: 10, y: 10))
        let rep = NSCIImageRep(ciImage: image)
        let nsImage = NSImage(size: rep.size)
        nsImage.addRepresentation(rep)
        return nsImage
    }
}

// MARK: - Unpeel Sessions MCP panel


/// How Unpeel gets the user's attention: the menu-waiting attention badge and
/// the macOS/phone notification banners. General app behavior — nothing here
/// is tied to the Sessions MCP.
struct NotificationsSettingsPanel: View {
    @ObservedObject var store: UnpeelStore
    @ObservedObject private var uplink = RelayUplinkManager.shared

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Form {
                Section {} header: {
                    SettingsPaneHeader(
                        title: "Notifications",
                        description: "How Unpeel flags a session that needs you and "
                            + "when it sends a notification banner."
                    )
                    .padding(.bottom, 4)
                }

                menuAttentionSection
                notificationsSection
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
        }
    }

    /// Toggle for surfacing the attention badge when an agent draws a select
    /// menu (Claude/Codex numbered prompts). These fire no lifecycle hook, so
    /// the host detects them from the rendered screen; some users may prefer to
    /// keep the busy spinner instead.
    private var menuAttentionSection: some View {
        Section {
            LabeledContent {
                Toggle(
                    "",
                    isOn: Binding(
                        get: { store.menuAttentionDetectionEnabled },
                        set: { store.menuAttentionDetectionEnabled = $0 }
                    )
                )
                .toggleStyle(.switch)
                .labelsHidden()
                .controlSize(.small)
            } label: {
                VStack(alignment: .leading, spacing: 1) {
                    Text("Flag menus waiting for a choice")
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.foreground)
                    Text("Show the yellow attention dot when an agent draws a "
                        + "pick-an-option menu. These prompts send no signal on "
                        + "their own, so Unpeel reads them off the screen.")
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        } header: {
            SettingsSectionHeader(
                title: "Attention",
                description: "When a session is waiting for you to answer an "
                    + "on-screen menu."
            )
        }
    }

    private var notificationsSection: some View {
        Section {
            Button("Send a test Mac notification") {
                DesktopNotifier.shared.sendTestNotification()
            }

            Button("Send a test phone notification") {
                store.sendTestPhoneNotification()
            }
            .disabled(store.mobilePushTargetCount == 0)

            SettingsValueRow(
                label: "Paired phone tokens",
                value: store.mobilePushTargetCount == 0
                    ? "None registered"
                    : "\(store.mobilePushTargetCount) ready"
            )
            SettingsValueRow(label: "Unpeel Link", value: uplink.status.label)
            SettingsValueRow(label: "Last phone push", value: uplink.lastPushDiagnostic.label)
            if let attemptedAt = uplink.lastPushAttemptAt {
                SettingsValueRow(
                    label: "Last attempt",
                    value: attemptedAt.formatted(date: .abbreviated, time: .standard)
                )
            }
        } header: {
            SettingsSectionHeader(
                title: "Notifications",
                description: "A macOS banner (and a push to a paired iPhone) when a session "
                    + "needs input, or finishes if you turned on \u{201C}Notify when done\u{201D} "
                    + "for it. Phone alerts use Link/APNs even while terminal traffic stays "
                    + "Direct or SSH. Mac and phone tests exercise their respective delivery "
                    + "paths; phone diagnostics distinguish a missing APNs token, Link "
                    + "entitlement failure, and APNs rejection."
            )
        }
    }

}

// MARK: - Transcripts panel

/// Which content types the Markdown transcript includes and how much of it.
/// Shared by the session context menu's "Copy transcript" action (desktop and
/// phone) and the Sessions MCP `read_transcript` tool (as its defaults), so
/// all of them stay in sync.
struct TranscriptsSettingsPanel: View {
    @ObservedObject var store: UnpeelStore

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Form {
                Section {} header: {
                    SettingsPaneHeader(
                        title: "Transcripts",
                        description: "A session's conversation, rendered as Markdown — "
                            + "what \"Copy transcript\" copies and what agents read."
                    )
                    .padding(.bottom, 4)
                }

                transcriptSection
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
        }
    }

    private var transcriptSection: some View {
        Section {
            transcriptToggle(
                title: "Session info header",
                subtitle: "Start with the session's title, ID, CLI, and model. "
                    + "The ID lets another agent target this session with the "
                    + "Sessions MCP tools.",
                on: store.transcriptSettings.includeSessionInfo
            ) { $0.includeSessionInfo = $1 }
            transcriptToggle(
                title: "User messages",
                on: store.transcriptSettings.includeUser
            ) { $0.includeUser = $1 }
            transcriptToggle(
                title: "Assistant messages",
                on: store.transcriptSettings.includeAssistant
            ) { $0.includeAssistant = $1 }
            transcriptToggle(
                title: "Reasoning",
                subtitle: "The agent's thinking blocks.",
                on: store.transcriptSettings.includeReasoning
            ) { $0.includeReasoning = $1 }
            transcriptToggle(
                title: "Tool calls & results",
                subtitle: "Commands the agent ran and their output.",
                on: store.transcriptSettings.includeTools
            ) { $0.includeTools = $1 }
            transcriptToggle(
                title: "File changes & diffs",
                on: store.transcriptSettings.includeFileChanges
            ) { $0.includeFileChanges = $1 }
            transcriptToggle(
                title: "Plan updates",
                on: store.transcriptSettings.includePlanUpdates
            ) { $0.includePlanUpdates = $1 }

            LabeledContent {
                Picker(
                    "",
                    selection: Binding(
                        get: { store.transcriptSettings.maxEntries },
                        set: { value in
                            store.updateTranscriptSettings { $0.maxEntries = value }
                        }
                    )
                ) {
                    Text("Whole conversation").tag(0)
                    Text("Last 20 entries").tag(20)
                    Text("Last 50 entries").tag(50)
                    Text("Last 100 entries").tag(100)
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .fixedSize()
            } label: {
                VStack(alignment: .leading, spacing: 1) {
                    Text("Range")
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.foreground)
                    Text("How much of the conversation to include.")
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        } header: {
            SettingsSectionHeader(
                title: "Transcript content",
                description: "What \"Copy transcript\" (right-click a session) puts on "
                    + "the clipboard as Markdown. These options also drive the defaults "
                    + "for agents reading a session's transcript. Range is the default "
                    + "for agent reads; the Copy transcript menu picks its own range."
            )
        }
    }

    /// One transcript content toggle wired into the shared transcript settings.
    private func transcriptToggle(
        title: String,
        subtitle: String? = nil,
        on: Bool,
        set: @escaping (inout TranscriptSettings, Bool) -> Void
    ) -> some View {
        LabeledContent {
            Toggle(
                "",
                isOn: Binding(
                    get: { on },
                    set: { value in
                        store.updateTranscriptSettings { set(&$0, value) }
                    }
                )
            )
            .toggleStyle(.switch)
            .labelsHidden()
            .controlSize(.small)
        } label: {
            VStack(alignment: .leading, spacing: 1) {
                Text(title)
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.foreground)
                if let subtitle {
                    Text(subtitle)
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

}

// MARK: - Browser MCP panel (Browser Access)


/// min-height 28, padding 2px 12px, radius 9, title 13px/600;
/// muted → fg + fg-10% bg on hover; active = active-tint bg + fg.
/// Muted uppercase caption that labels the "Unpeel MCP" nav group,
/// aligned with the nav rows' 12pt leading inset.
private struct SettingsNavSectionHeader: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(Theme.mutedForeground.opacity(0.6))
            .lineLimit(1)
            .padding(EdgeInsets(top: 2, leading: 12, bottom: 2, trailing: 12))
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct SettingsNavRow: View {
    let title: String
    var leadingIcon: ChromeIcon?
    let isActive: Bool
    let action: () -> Void

    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 0) {
                if let leadingIcon {
                    // .settings-leading: 18×18 slot, margin-right 6.
                    ChromeIconView(icon: leadingIcon, size: 16)
                        .frame(width: 18, height: 18)
                        .padding(.trailing, 6)
                }
                Text(title)
                    .font(.system(size: 13, weight: .semibold))
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
            }
            .foregroundStyle(hovering || isActive ? Theme.foreground : Theme.mutedForeground)
            .padding(EdgeInsets(top: 2, leading: 12, bottom: 2, trailing: 12))
            .frame(maxWidth: .infinity, minHeight: 28, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(isActive ? Theme.activeRow : (hovering ? Theme.hoverRow : .clear))
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .animation(.easeInOut(duration: 0.12), value: hovering)
    }
}

// MARK: - System Settings grouped-form helpers
//
// Settings panels are grouped SwiftUI Forms (macOS 26 renders the System
// Settings inset-card anatomy natively: rounded section cards, hairline
// row separators inset to the labels, standard switches). The helpers
// below cover the pieces Form does not give us for free on a custom dark
// vibrancy background.

/// Large bold pane title + muted description (System Settings pane header
/// treatment). Used as the `header:` of a leading empty Section so it scrolls
/// with the content under the sticky breadcrumb — grouped-Form *rows* always
/// grow a card (`listRowBackground(.clear)` is ignored), but Section headers
/// are background-less, so the title/description live there instead.
struct SettingsPaneHeader: View {
    let title: String
    var description = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.system(size: 22, weight: .bold))
                .foregroundStyle(Theme.foreground)
            if !description.isEmpty {
                Text(description)
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.mutedForeground)
                    .lineSpacing(2.5)
                    .fixedSize(horizontal: false, vertical: true)
                    // Cap the measure: full-column description lines are hard
                    // to read at 700pt.
                    .frame(maxWidth: 560, alignment: .leading)
            }
        }
        .padding(.top, 4)
        .padding(.bottom, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Section header block: 13pt semibold title + muted multi-line 12pt
/// description, like the "Screen & System Audio Recording" header copy in
/// System Settings. Use as a Section's `header:`.
struct SettingsSectionHeader: View {
    let title: String
    var description = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Theme.foreground)
            if !description.isEmpty {
                Text(description)
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.mutedForeground)
                    .lineSpacing(2.5)
                    .fixedSize(horizontal: false, vertical: true)
                    // Same readable-measure cap as SettingsPaneHeader.
                    .frame(maxWidth: 560, alignment: .leading)
            }
        }
        .textCase(nil)
        // Grouped Form's own section gap is tight on the dark shell; the top
        // padding here is what separates a section from the card above it.
        .padding(.top, 14)
        .padding(.bottom, 6)
    }
}

/// Label + trailing value text row (System Settings "About"-style rows).
struct SettingsValueRow: View {
    let label: String
    let value: String

    var body: some View {
        LabeledContent {
            Text(value)
                .font(.system(size: 13))
                .monospacedDigit()
                .foregroundStyle(Theme.mutedForeground)
        } label: {
            Text(label)
                .font(.system(size: 13))
                .foregroundStyle(Theme.foreground)
        }
    }
}

// MARK: - Experimental panel

/// Data-driven list of experimental feature toggles. Every entry in
/// `ExperimentalFeature.all` renders one row here automatically, so adding a
/// future experiment needs no new UI — just a registry entry in
/// `FeatureFlags.swift`.
struct ExperimentalSettingsPanel: View {
    @ObservedObject var store: UnpeelStore

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Form {
                Section {} header: {
                    SettingsPaneHeader(
                        title: "Experimental",
                        description: "Early features that are still being shaped. They can "
                            + "change or disappear between releases. Turn one off here if it "
                            + "gets in the way — no restart needed."
                    )
                    .padding(.bottom, 4)
                }

                if UnpeelFeatureFlags.availableExperimentalFeatures.isEmpty {
                    Section {
                        Text("No experimental features right now. Check back after an update.")
                            .font(.system(size: 12))
                            .foregroundStyle(Theme.mutedForeground)
                    }
                } else {
                    Section {
                        ForEach(UnpeelFeatureFlags.availableExperimentalFeatures) { feature in
                            featureRow(feature)
                        }
                    }
                }
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
        }
    }

    private func featureRow(_ feature: ExperimentalFeature) -> some View {
        LabeledContent {
            Toggle("", isOn: Binding(
                get: { store.isExperimentalEnabled(feature) },
                set: { store.setExperimental($0, for: feature) }
            ))
            .labelsHidden()
            .toggleStyle(.switch)
        } label: {
            VStack(alignment: .leading, spacing: 2) {
                Text(feature.title)
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.foreground)
                Text(feature.summary)
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.mutedForeground)
                    .lineSpacing(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: 460, alignment: .leading)
            }
        }
    }
}

// MARK: - Advanced panel (settings/AdvancedPanel.svelte)

/// Native Advanced panel: memory usage, running terminal hosts (with
/// Stop and archive),
/// cleanup policy, and diagnostics utilities.
struct AdvancedSettingsPanel: View {
    @ObservedObject var store: UnpeelStore

    @State private var memory: MemorySnapshot?
    @State private var terminals: [RunningTerminal] = []
    @State private var loading = false
    @State private var loaded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Form {
                Section {} header: {
                    SettingsPaneHeader(
                        title: "Advanced",
                        description: "Resource usage, cleanup, and on-disk data for Unpeel's terminal hosts."
                    )
                    .padding(.bottom, 4)
                }

                cleanupSection
                memorySection
                terminalsSection
                diagnosticsSection
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
        }
        .onAppear {
            if !loaded { refresh() }
        }
        .onChange(of: store.removingSessionIDs) { ids in
            // Refresh counts after a kill finishes so memory + row list stay in sync.
            if ids.isEmpty, loaded { refresh() }
        }
    }

    // MARK: Old session cleanup

    private var cleanupSection: some View {
        Section {
            LabeledContent {
                Picker(
                    "Auto-stop and archive inactive terminals",
                    selection: Binding(
                        get: { store.autoStopArchiveMinutes },
                        set: { store.setAutoStopArchiveMinutes($0) }
                    )
                ) {
                    ForEach(UnpeelStore.autoStopArchiveMinuteOptions, id: \.self) { minutes in
                        Text(UnpeelStore.autoStopArchiveLabel(for: minutes)).tag(minutes)
                    }
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .frame(width: 150)
            } label: {
                Text("Auto-stop and archive inactive terminals")
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.foreground)
            }
        } header: {
            SettingsSectionHeader(
                title: "Cleanup",
                description: "Sessions that have stayed idle for the selected time are stopped and archived — the same as clicking \"Stop and archive\": the terminal stops and the session files away into the project's archive library, where Restore & Resume continues the conversation. Sessions that keep working (including loops — any activity resets the clock), or that are pinned, selected, unread, or waiting for input, are left alone; plain shell terminals are never touched. Nothing is deleted automatically."
            )
        }
    }

    // MARK: Memory (AdvancedPanel.svelte:214-244)

    private var memorySection: some View {
        Section {
            if let memory {
                SettingsValueRow(
                    label: "App memory (Unpeel Native)",
                    value: formatMB(memory.processFootprintBytes)
                )
                SettingsValueRow(
                    label: "Running terminal hosts",
                    value: "\(memory.runningHostCount)"
                )
                SettingsValueRow(
                    label: "Hosted sessions on disk",
                    value: "\(memory.hostedSessionCount)"
                )
            } else {
                Text(loading ? "Loading…" : "Unable to read memory usage")
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.mutedForeground.opacity(0.6))
            }
        } header: {
            SettingsSectionHeader(
                title: "Memory",
                description: "Current process memory usage and active session counts."
            )
        }
    }

    // MARK: Running terminals

    private var totalCpu: Double { terminals.reduce(0) { $0 + $1.cpuPercent } }
    private var totalRss: UInt64 { terminals.reduce(0) { $0 + $1.rssBytes } }

    private var summaryText: String {
        terminals.isEmpty
            ? "Live terminal hosts sorted by current CPU usage."
            : "\(terminals.count) running · \(formatCpu(totalCpu)) CPU · \(formatMB(totalRss)) memory. Sorted by current CPU usage."
    }

    private var terminalsSection: some View {
        Section {
            if terminals.isEmpty {
                Text(loading ? "Loading terminals…" : "No running terminals.")
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.mutedForeground)
            } else {
                ForEach(terminals) { terminal in
                    terminalRow(terminal)
                }
            }
        } header: {
            HStack(alignment: .top, spacing: 12) {
                SettingsSectionHeader(title: "Running Terminals", description: summaryText)
                Spacer(minLength: 8)
                Button(loading ? "Refreshing…" : "Refresh") { refresh() }
                    .controlSize(.small)
                    .disabled(loading)
                    // Track the header's built-in top padding so the button
                    // lines up with the title, not the section gap above it.
                    .padding(.top, 14)
            }
        }
    }

    /// System Settings login-items-style row: icon tile, label + sublabel,
    /// trailing CPU/Memory cells + Open / Stop and archive buttons.
    private func terminalRow(_ terminal: RunningTerminal) -> some View {
        let isRemoving = store.removingSessionIDs.contains(terminal.id)
        return HStack(alignment: .center, spacing: 12) {
            ToolIconView(tool: QuickPresetTool.detect(in: terminal.command), size: 16)
                .foregroundStyle(Theme.foreground)
                .frame(width: 28, height: 28)
                .background(
                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                        .fill(Theme.foreground.opacity(0.07))
                )

            VStack(alignment: .leading, spacing: 2) {
                Text(terminal.label)
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.foreground)
                    .lineLimit(1)
                    .truncationMode(.tail)
                HStack(spacing: 6) {
                    Text(terminal.commandLabel)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .frame(maxWidth: 200, alignment: .leading)
                        .fixedSize(horizontal: true, vertical: false)
                    Text("PID \(String(terminal.pid))")
                    Text("\(terminal.processCount) proc")
                    Text(compactPath(terminal.cwd))
                        .opacity(0.75)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .help(terminal.cwd)
                }
                .font(.system(size: 11))
                .foregroundStyle(Theme.mutedForeground)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            HStack(spacing: 10) {
                resourceCell(value: formatCpu(terminal.cpuPercent), label: "CPU")
                resourceCell(value: formatMB(terminal.rssBytes), label: "Memory")
            }

            HStack(spacing: 8) {
                Button("Open") {
                    store.revealSessionInSidebar(terminal.id)
                }
                .controlSize(.small)
                .disabled(isRemoving)

                // Non-destructive, like everywhere else: stop the host and
                // file the session into the archive (history stays on disk).
                // The old "Kill" here was secretly confirmRemoveSession —
                // it deleted the session dir outright.
                Button("Stop and archive") {
                    store.archiveSession(terminal.id)
                    terminals.removeAll { $0.id == terminal.id }
                }
                .controlSize(.small)
                .disabled(isRemoving)
            }
        }
        .padding(.vertical, 2)
        .opacity(isRemoving ? 0.5 : 1)
    }

    private func resourceCell(value: String, label: String) -> some View {
        VStack(alignment: .trailing, spacing: 1) {
            Text(value)
                .font(.system(size: 12, weight: .semibold))
                .monospacedDigit()
                .foregroundStyle(Theme.foreground)
            Text(label)
                .font(.system(size: 10))
                .foregroundStyle(Theme.mutedForeground)
        }
        .frame(minWidth: 56, alignment: .trailing)
    }

    // MARK: Diagnostics (home for the old gear-menu utilities)

    private var diagnosticsSection: some View {
        Section {
            LabeledContent {
                Button("Show in Finder") {
                    NSWorkspace.shared.open(LaunchConfig.appSessionsDir)
                }
                .controlSize(.small)
            } label: {
                Text("Sessions folder")
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.foreground)
            }
            LabeledContent {
                Button("Show in Finder") {
                    let trace = LaunchConfig.unpeelDir
                        .appendingPathComponent("hooks")
                        .appendingPathComponent("trace.log")
                    if FileManager.default.fileExists(atPath: trace.path) {
                        NSWorkspace.shared.activateFileViewerSelecting([trace])
                    } else {
                        NSWorkspace.shared.open(LaunchConfig.unpeelDir)
                    }
                }
                .controlSize(.small)
            } label: {
                Text("Hooks trace log")
                    .font(.system(size: 13))
                    .foregroundStyle(Theme.foreground)
            }
        } header: {
            SettingsSectionHeader(
                title: "Diagnostics",
                description: "Quick access to Unpeel's on-disk session data and hook trace log."
            )
        }
    }

    // MARK: Data collection

    private func refresh() {
        loading = true
        loaded = true
        Task.detached(priority: .userInitiated) {
            let snapshot = AdvancedDiagnostics.collect()
            await MainActor.run {
                memory = snapshot.memory
                terminals = snapshot.terminals
                loading = false
            }
        }
    }

    private func formatMB(_ bytes: UInt64) -> String {
        "\(Int((Double(bytes) / (1024 * 1024)).rounded())) MB"
    }

    private func formatCpu(_ value: Double) -> String {
        value >= 10 ? String(format: "%.0f%%", value) : String(format: "%.1f%%", value)
    }

    private func compactPath(_ path: String) -> String {
        guard !path.isEmpty else { return "No folder" }
        let parts = path.split(separator: "/").map(String.init)
        guard parts.count > 2 else { return path }
        return ".../" + parts.suffix(2).joined(separator: "/")
    }
}

// MARK: - Diagnostics data (read-only parity with get_memory_usage /
// list_running_terminals in pty_manager.rs)

struct MemorySnapshot: Sendable {
    let processFootprintBytes: UInt64
    let runningHostCount: Int
    let hostedSessionCount: Int
}

struct RunningTerminal: Identifiable, Sendable {
    let id: String // session id
    let projectID: String
    let label: String
    let command: String
    let cwd: String
    let pid: Int32
    let processCount: Int
    let cpuPercent: Double
    let rssBytes: UInt64

    var commandLabel: String {
        command.trimmingCharacters(in: .whitespaces).isEmpty ? "Blank shell" : command
    }
}

enum AdvancedDiagnostics {
    struct Snapshot: Sendable {
        let memory: MemorySnapshot
        let terminals: [RunningTerminal]
    }

    /// Manifest fields the advanced panel needs beyond what SessionEntry
    /// carries (cwd + pid live only in the manifest).
    private struct ManifestSlim: Decodable {
        struct Session: Decodable {
            let id: String
            let projectID: String
            let label: String?
            let command: String?

            enum CodingKeys: String, CodingKey {
                case id, label, command
                case projectID = "project_id"
            }
        }

        let session: Session
        let cwd: String?
        let state: String
        let pid: Int32?
    }

    static func collect() -> Snapshot {
        let fm = FileManager.default
        // Native renames win over the manifest label, same as UnpeelStore.
        let titleOverrides = (AppDefaults.shared.dictionary(
            forKey: NativeOverlay.sessionTitlesKey
        ) as? [String: String]) ?? [:]
        var hostedCount = 0
        var running: [(manifest: ManifestSlim, pid: Int32)] = []

        if let dirs = try? fm.contentsOfDirectory(
            at: LaunchConfig.appSessionsDir,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        ) {
            for dir in dirs {
                let url = dir.appendingPathComponent("manifest.json")
                guard let data = try? Data(contentsOf: url),
                      let manifest = try? JSONDecoder().decode(ManifestSlim.self, from: data)
                else { continue }
                hostedCount += 1
                if manifest.state == "running", let pid = manifest.pid, kill(pid, 0) == 0 {
                    running.append((manifest, pid))
                }
            }
        }

        // Whole-machine process table → per-host subtree rollup, the same
        // walk list_running_terminals does with sysinfo.
        let table = processTable()
        var children: [pid_t: [pid_t]] = [:]
        for (pid, info) in table {
            children[info.ppid, default: []].append(pid)
        }

        var terminals: [RunningTerminal] = []
        for (manifest, pid) in running {
            var processCount = 0
            var cpu = 0.0
            var rssKB: UInt64 = 0
            var stack: [pid_t] = [pid]
            var seen = Set<pid_t>()
            while let current = stack.popLast() {
                guard seen.insert(current).inserted else { continue }
                if let info = table[current] {
                    processCount += 1
                    cpu += info.cpu
                    rssKB += info.rssKB
                }
                stack.append(contentsOf: children[current] ?? [])
            }

            let command = manifest.session.command ?? ""
            let manifestLabel = (manifest.session.label?.isEmpty == false)
                ? manifest.session.label!
                : (command.isEmpty ? "Terminal" : command)
            let label = titleOverrides[manifest.session.id] ?? manifestLabel
            terminals.append(RunningTerminal(
                id: manifest.session.id,
                projectID: manifest.session.projectID,
                label: label,
                command: command,
                cwd: manifest.cwd ?? "",
                pid: pid,
                processCount: processCount,
                cpuPercent: cpu,
                rssBytes: rssKB * 1024
            ))
        }
        terminals.sort { $0.cpuPercent != $1.cpuPercent ? $0.cpuPercent > $1.cpuPercent : $0.id < $1.id }

        let memory = MemorySnapshot(
            processFootprintBytes: processFootprint(),
            runningHostCount: running.count,
            hostedSessionCount: hostedCount
        )
        return Snapshot(memory: memory, terminals: terminals)
    }

    /// `ps -axo pid=,ppid=,pcpu=,rss=` → pid-indexed cpu%/rss(KB)/ppid.
    private static func processTable() -> [pid_t: (ppid: pid_t, cpu: Double, rssKB: UInt64)] {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/ps")
        process.arguments = ["-axo", "pid=,ppid=,pcpu=,rss="]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        process.standardInput = FileHandle.nullDevice
        guard (try? process.run()) != nil else { return [:] }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()

        var table: [pid_t: (ppid: pid_t, cpu: Double, rssKB: UInt64)] = [:]
        for line in String(decoding: data, as: UTF8.self).split(separator: "\n") {
            let fields = line.split(separator: " ", omittingEmptySubsequences: true)
            guard fields.count >= 4,
                  let pid = pid_t(fields[0]),
                  let ppid = pid_t(fields[1]),
                  let cpu = Double(fields[2]),
                  let rss = UInt64(fields[3])
            else { continue }
            table[pid] = (ppid, cpu, rss)
        }
        return table
    }

    /// Own-process physical footprint (the figure Activity Monitor shows),
    /// the native equivalent of get_memory_usage's process RSS.
    private static func processFootprint() -> UInt64 {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(
            MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<integer_t>.size
        )
        let result = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
            }
        }
        guard result == KERN_SUCCESS else { return 0 }
        return UInt64(info.phys_footprint)
    }
}
