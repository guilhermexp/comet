//
//  TerminalArea.swift
//  UnpeelNative
//
//  Main content pane: titlebar + the selected session's retained Ghostty
//  surface (DESIGN.md §6). Surfaces are swapped, never destroyed.
//
//  While settings is open the pane swaps to SettingsContentHost. The swap
//  is deliberately INSTANT (`.transition(.identity)` + `.animation(nil)`):
//  the terminal surface is a Metal-backed NSView, and animating opacity
//  or frame across a CAMetalLayer flashes (the layer is snapshotted, the
//  drawable goes black for a frame, vibrancy re-composites) — that was the
//  root cause of the old settings blink. Removing the terminal from the
//  hierarchy without animation is the same flash-free path used when
//  switching to a dead/empty session; the pane itself stays retained in
//  SurfaceCache and its renderer pauses via viewDidMoveToWindow(nil).
//

import AppKit
import SwiftUI

struct ContentArea: View {
    @ObservedObject var store: UnpeelStore
    @ObservedObject private var viewerPresence = ViewerPresenceStore.shared
    @ObservedObject var cache: SurfaceCache
    @State private var terminalSessionID: String?

    var body: some View {
        // Remote scope renders through the SAME workspace pane: the store's
        // display projection feeds it, and only the terminal byte transport
        // differs (the runtime's in-memory VT pane instead of a local
        // Ghostty surface).
        ZStack {
            if store.settingsVisible {
                SettingsContentHost(store: store)
                    .transition(.identity)
            } else {
                workspacePane
                    .transition(.identity)
            }
        }
        // Belt and braces: even if a future caller wraps settingsVisible in
        // withAnimation, this subtree must not animate the swap.
        .animation(nil, value: store.settingsVisible)
        // The shared content pane covers terminals, settings, archives, and
        // empty states, so rounding here keeps every right-hand view aligned.
        .clipShape(
            UnevenRoundedRectangle(
                topLeadingRadius: Theme.windowCornerRadius,
                bottomLeadingRadius: Theme.windowCornerRadius,
                style: .circular
            )
        )
        .overlay(alignment: .leading) {
            if !store.sidebarCollapsed {
                LeadingEdgeContour(radius: Theme.windowCornerRadius)
                    .stroke(
                        LinearGradient(
                            stops: [
                                .init(color: Theme.resizerLine, location: 0.0),
                                .init(color: Theme.paneEdgeHighlight, location: 0.5),
                                .init(color: Theme.resizerLine, location: 1.0)
                            ],
                            startPoint: .top,
                            endPoint: .bottom
                        ),
                        lineWidth: 1
                    )
                    .allowsHitTesting(false)
            }
        }
        .onAppear {
            terminalSessionID = store.selectedSessionID
        }
        .onChange(of: store.selectedSessionID) { newID in
            let targetID = newID
            DispatchQueue.main.async { [store] in
                guard store.selectedSessionID == targetID else { return }
                terminalSessionID = targetID
                normalizeShownTerminalSize()
            }
        }
        // Coming forward from the phone: re-assert the desktop grid on the
        // shown session (see normalizeShownTerminalSize).
        .onReceive(
            NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)
        ) { _ in
            normalizeShownTerminalSize()
        }
        // A phone/remote viewer leaving is the "no longer mobile controlled"
        // moment — snap the grid back once nothing else owns the shared PTY.
        .onChange(of: shownSessionHasViewers) { hasViewers in
            if !hasViewers { normalizeShownTerminalSize() }
        }
    }

    /// Whether the workspace is scoped to a remote Host: same chrome, but
    /// the terminal transport is the runtime's in-memory VT pane and the
    /// Controller-local titlebar utilities (gallery, local-site links, open
    /// in editor) hide.
    private var isRemoteScope: Bool {
        store.selectedHostScope != .local
    }

    /// True while a phone or remote client is currently viewing the shown
    /// session (so it may be driving the shared PTY size on purpose).
    private var shownSessionHasViewers: Bool {
        guard let id = terminalSessionID else { return false }
        return !(viewerPresence.viewers[id]?.isEmpty ?? true)
    }

    /// Snap the shown session's hosted PTY back to the desktop's full grid
    /// when nothing else is driving it. A phone in fit-to-screen mode resizes
    /// the *shared* PTY smaller; Ghostty's own desktop surface stays
    /// full-size, so no resize event fires and the terminal keeps rendering
    /// narrow (content wraps early, dead space on the right) until the user
    /// nudges the window. `refitNow` re-runs the resize path — the same
    /// automatic "nudge" a switch already does — re-asserting the full size
    /// on the moments the user opens the session on the Mac (selecting it,
    /// the app coming forward, a viewer leaving). No-op while phone-controlled
    /// (a live letterbox override or an active viewer owns the grid on
    /// purpose), so it never fights a phone that is still connected.
    private func normalizeShownTerminalSize() {
        guard !isRemoteScope else { return }
        guard !store.settingsVisible,
              store.archivedProjectID == nil,
              !store.recentActivityVisible,
              let id = terminalSessionID
        else { return }
        guard store.phoneResizeOverrides[id] == nil else { return }
        guard viewerPresence.viewers[id]?.isEmpty ?? true else { return }
        DispatchQueue.main.async {
            cache.existingPane(for: id)?.refitNow()
        }
    }

    private var workspacePane: some View {
        VStack(spacing: 0) {
            TitleBarView(
                segments: contentTitlebarSegments,
                branch: coversTerminal ? nil : store.titlebarBranchName,
                branchIsWorktree: !coversTerminal && store.titlebarBranchIsWorktree,
                showsTerminalBackground: showsTerminal,
                terminalBackgroundColor: selectedTerminalFrameBackground
            )
            .overlay(alignment: .trailing) {
                HStack(spacing: 10) {
                    // Gallery/screenshot chip only makes sense for agent
                    // sessions — a plain shell has no captures and no prompt
                    // to attach a screenshot to. Optional on desktop
                    // (Appearance ▸ "Session gallery", off by default); the
                    // chip also owns the ⌘⇧S capture flow, so disabling it
                    // disables screenshots too.
                    if store.showSessionGallery, !isRemoteScope,
                       !coversTerminal, let session = terminalSession,
                       SetupTool.detect(in: session.command) != nil {
                        SessionGalleryButton(sessionID: session.id, cache: cache)
                    }
                    // Local site(s) the shown session's project is serving
                    // right now (host-probed live URLs, aggregated across the
                    // project family — the dev server usually runs in a
                    // different session than the one being watched). Sits
                    // left of the Open-in menu: one URL opens directly,
                    // several get a dropdown.
                    if !coversTerminal, !isRemoteScope, let session = terminalSession {
                        let localURLs = store.localSiteURLs(
                            forProjectFamilyOf: session.projectID
                        )
                        if !localURLs.isEmpty {
                            LocalSiteMenu(urls: localURLs)
                        }
                    }
                    if let path = activeWorkspacePath {
                        WorkspaceOpenMenu(
                            preferredTarget: WorkspaceOpenTarget.preferred(
                                forEditor: store.codeEditor
                            ),
                            onOpen: { target in store.openWorkspace(path: path, in: target) }
                        )
                    }
                }
                .padding(.trailing, 10)
            }
            // Remote connection state (reconnecting / repair / offline)
            // surfaces as a banner in the same slot the local restart /
            // resume banners use.
            if isRemoteScope, let banner = RemoteConnectionPresentation(
                state: store.remoteHostRuntime.connectionState,
                hasSnapshot: store.remoteHostRuntime.snapshot != nil
            ).contentBanner {
                RemoteHostConnectionBanner(banner: banner)
            }
            // 1px of the same solid canvas under the titlebar edge covers the
            // sub-pixel seam that otherwise shows between the titlebar strip
            // and the Metal terminal (different color paths / antialiasing).
            if showsTerminal {
                Color(nsColor: selectedTerminalFrameBackground)
                    .frame(height: 1)
                    .allowsHitTesting(false)
            }
            if !coversTerminal, !isRemoteScope,
               let session = terminalSession,
               shouldMountTerminal(for: session),
               let recommendation = store.restartRecommendations[session.id] {
                RestartRecommendedBar(
                    recommendation: recommendation,
                    onRestart: {
                        switch recommendation.action {
                        case .resumeAgent:
                            store.resumeAgent(session.id)
                        case .reloadTerminal:
                            store.restartSession(session.id, stoppedOnly: false)
                        case nil:
                            break
                        }
                    },
                    onDismiss: { store.dismissRestartRecommendation(for: session.id) }
                )
            }
            if !coversTerminal, !isRemoteScope,
               let session = terminalSession,
               shouldMountTerminal(for: session),
               store.resumeFailures.contains(session.id) {
                ResumeFailedBar(
                    onStartFresh: { store.startFreshAfterResumeFailure(session.id) },
                    onDismiss: { store.dismissResumeFailure(for: session.id) }
                )
            }
            if !coversTerminal, !isRemoteScope,
               let session = terminalSession,
               shouldMountTerminal(for: session),
               let phoneResize = store.phoneResizeOverrides[session.id] {
                PhoneResizedBar(
                    grid: phoneResize,
                    onRevert: { store.clearPhoneResizeOverride(for: session.id) }
                )
            }
            ZStack {
                // Hidden, behind everything: panes for likely switch targets
                // attach + replay here before they are ever selected, so the
                // actual switch is instant. Sized like the real terminal
                // area so the replay renders at the right cols/rows.
                WarmPaneHostView(
                    sessions: prewarmSessions,
                    workingDirectories: prewarmWorkingDirectories,
                    cache: cache
                )
                if store.recentActivityVisible {
                    RecentActivityView(store: store)
                } else if let project = archivedProject {
                    ArchivedSessionsView(store: store, project: project)
                } else if let session = terminalSession {
                    if shouldMountTerminal(for: session) {
                        if isRemoteScope {
                            // Same chrome, remote byte transport: the
                            // runtime-owned in-memory Ghostty pane renders
                            // the Host's committed VT output. Never a local
                            // hosted session or attach client.
                            RemoteScopeTerminalMount(
                                store: store,
                                session: session,
                                backgroundColor: terminalFrameBackground(for: session)
                            )
                            .id("remote:\(session.id)")
                            .padding(0)
                            .padding(.top, showsTerminal ? -1 : 0)
                        } else {
                            TerminalHostView(
                                session: session,
                                workingDirectory: store.projectsByID[session.projectID]?.path,
                                frameBackgroundColor: terminalFrameBackground(for: session),
                                themeRevision: cache.themeRevision,
                                phoneResize: store.phoneResizeOverrides[session.id],
                                cache: cache
                            )
                            // Full bleed: the surface owns the whole content area;
                            // breathing room comes from Ghostty's own window padding.
                            .padding(0)
                            // Pull 1px under the seam-cap above so the join is
                            // covered from both sides without eating a TUI row.
                            .padding(.top, showsTerminal ? -1 : 0)
                        }
                    } else {
                        DeadSessionView(
                            session: session,
                            canResume: store.sessionCanRestart(session.id),
                            isRestarting: store.restartingSessionIDs.contains(session.id),
                            onRestart: { store.restartSession(session.id) }
                        )
                    }
                } else if let project = launcherTargetProject {
                    SessionLauncherView(store: store, project: project)
                } else {
                    EmptyStateView()
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        // Live terminal: solid provider canvas under the whole column so any
        // residual edge (titlebar join, bottom of Metal surface, rounded
        // corner bleed) matches the TUI. settingsShellDim is semi-transparent
        // over vibrancy and reads as the wrong tint in those 1px gaps.
        .background(
            showsTerminal
                ? Color(nsColor: selectedTerminalFrameBackground)
                : Theme.settingsShellDim
        )
    }

    /// A main-pane library (archived sessions or All recent) is covering the
    /// terminal area.
    private var coversTerminal: Bool {
        archivedProject != nil || store.recentActivityVisible
    }

    private var showsTerminal: Bool {
        !coversTerminal && terminalSession.map(shouldMountTerminal(for:)) == true
    }

    private var selectedTerminalFrameBackground: NSColor {
        guard let session = terminalSession, shouldMountTerminal(for: session) else {
            return Theme.terminalBackgroundNSColor
        }
        return terminalFrameBackground(for: session)
    }

    private var terminalSession: SessionEntry? {
        terminalSessionID.flatMap { store.displaySessionsByID[$0] }
    }

    private var archivedProject: Project? {
        guard let id = store.archivedProjectID else { return nil }
        return store.displayProjectsByID[id]
    }

    private var contentTitlebarSegments: [String] {
        if store.recentActivityVisible {
            return ["Recent"]
        }
        if let project = archivedProject {
            let count = store.archivedSessions(projectID: project.id).count
            return [project.name, "Archived (\(count))"]
        }
        return store.titlebarSegments
    }

    /// Project whose session launcher fills the content area. Only shown when
    /// explicitly invoked (the Finder "New Unpeel Session Here" service sets
    /// `launcherProjectID`); otherwise the plain EmptyStateView shows, so a
    /// cold launch never lands on a project's launcher.
    private var launcherTargetProject: Project? {
        guard let id = store.launcherProjectID else { return nil }
        return store.displayProjectsByID[id]
    }

    private var activeWorkspacePath: String? {
        // Remote paths belong to the Host; the Controller must never offer
        // to open them in a local editor.
        if isRemoteScope { return nil }
        // The All recent page spans every project — no single workspace to
        // offer in the open-in menu.
        if store.recentActivityVisible { return nil }
        if let archivedProject {
            return archivedProject.path
        }
        if let session = store.selectedSession {
            return session.worktreePath ?? store.projectsByID[session.projectID]?.path
        }
        return store.displayNodes.first?.project.path
    }

    private func terminalFrameBackground(for session: SessionEntry) -> NSColor {
        // Remote rows carry the Host-resolved canvas color; local styling
        // must never stat a Host-side working directory.
        if isRemoteScope {
            return store.remoteTerminalBackgroundColor(for: session.id)
                ?? Theme.terminalBackgroundNSColor
        }
        // Depend on themeRevision so OpenCode/Grok config + live canvas
        // samples repaint the titlebar / swap container without a switch.
        _ = cache.themeRevision
        return cache.frameStyle(
            for: session,
            workingDirectory: store.projectsByID[session.projectID]?.path
        ).backgroundColor
    }

    private func shouldMountTerminal(for session: SessionEntry) -> Bool {
        // A session being restarted stays selected (no empty-state flash), but
        // its host is being torn down — unmount the surface and show the
        // DeadSessionView/restart spinner until the replacement is live.
        session.status != .exited
            && !store.restartingSessionIDs.contains(session.id)
    }

    /// Pre-warm targets resolved to live sessions, excluding the one being
    /// displayed (the swap container owns that pane).
    private var prewarmSessions: [SessionEntry] {
        store.prewarmSessionIDs.compactMap { (id: String) -> SessionEntry? in
            guard id != store.selectedSessionID,
                  let session = store.sessionsByID[id], session.isAttachable,
                  // A warm pane mounts at the full terminal-area size, which
                  // would fight a phone-letterboxed session's grid.
                  store.phoneResizeOverrides[id] == nil
            else { return nil }
            return session
        }
    }

    private var prewarmWorkingDirectories: [String: String] {
        var dirs: [String: String] = [:]
        for session in prewarmSessions {
            if let path = store.projectsByID[session.projectID]?.path {
                dirs[session.id] = path
            }
        }
        return dirs
    }
}

// MARK: - Session recommendation

struct RestartRecommendedBar: View {
    let recommendation: SessionRestartRecommendation
    let onRestart: () -> Void
    let onDismiss: () -> Void
    @State private var restartHovering = false
    @State private var dismissHovering = false

    var body: some View {
        HStack(spacing: 10) {
            Text(recommendation.action.map { "\($0.label) recommended" } ?? "Context queued")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Theme.foreground)
                .lineLimit(1)
            Text(recommendation.message)
                .font(.system(size: 11))
                .foregroundStyle(Theme.mutedForeground)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 8)
            if let action = recommendation.action {
                Button(action: onRestart) {
                    HStack(spacing: 5) {
                        Image(systemName: "arrow.clockwise")
                            .font(.system(size: 10, weight: .semibold))
                        Text(action.label)
                            .font(.system(size: 11, weight: .semibold))
                    }
                    .foregroundStyle(Theme.foreground)
                    .padding(.horizontal, 9)
                    .frame(height: 24)
                    .background(
                        RoundedRectangle(cornerRadius: 6, style: .continuous)
                            .fill(Theme.foreground.opacity(restartHovering ? 0.13 : 0.08))
                    )
                    .overlay(
                        RoundedRectangle(cornerRadius: 6, style: .continuous)
                            .strokeBorder(Theme.foreground.opacity(0.10))
                    )
                }
                .buttonStyle(.plain)
                .onHover { restartHovering = $0 }
                .accessibilityLabel(action.label)
            }

            Button(action: onDismiss) {
                Image(systemName: "xmark")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(dismissHovering ? Theme.foreground : Theme.mutedForeground)
                    .frame(width: 24, height: 24)
                    .background(
                        RoundedRectangle(cornerRadius: 6, style: .continuous)
                            .fill(dismissHovering ? Theme.foreground.opacity(0.10) : .clear)
                    )
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .onHover { dismissHovering = $0 }
            .accessibilityLabel("Dismiss recommendation")
        }
        .padding(.horizontal, 12)
        .frame(height: 36)
        .background(Theme.attention.opacity(0.08))
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(Theme.foreground.opacity(0.08))
                .frame(height: 1)
        }
    }
}

// MARK: - Resume failed banner

/// Shown when a restart-with-resume relaunch died because the provider's
/// conversation storage no longer exists on disk (UnpeelStore.resumeFailures)
/// — the CLI printed "no conversation found" and exited to a bare shell.
/// Same anatomy as RestartRecommendedBar; the action relaunches fresh.
struct ResumeFailedBar: View {
    let onStartFresh: () -> Void
    let onDismiss: () -> Void
    @State private var freshHovering = false
    @State private var dismissHovering = false

    var body: some View {
        HStack(spacing: 10) {
            Text("Couldn't resume the conversation")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Theme.foreground)
                .lineLimit(1)
            Text("Its history no longer exists on disk, so the agent can only start over.")
                .font(.system(size: 11))
                .foregroundStyle(Theme.mutedForeground)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 8)
            Button(action: onStartFresh) {
                HStack(spacing: 5) {
                    Image(systemName: "plus.circle")
                        .font(.system(size: 10, weight: .semibold))
                    Text("Start fresh")
                        .font(.system(size: 11, weight: .semibold))
                }
                .foregroundStyle(Theme.foreground)
                .padding(.horizontal, 9)
                .frame(height: 24)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(Theme.foreground.opacity(freshHovering ? 0.13 : 0.08))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .strokeBorder(Theme.foreground.opacity(0.10))
                )
            }
            .buttonStyle(.plain)
            .onHover { freshHovering = $0 }
            .accessibilityLabel("Start a fresh session")

            Button(action: onDismiss) {
                Image(systemName: "xmark")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(dismissHovering ? Theme.foreground : Theme.mutedForeground)
                    .frame(width: 24, height: 24)
                    .background(
                        RoundedRectangle(cornerRadius: 6, style: .continuous)
                            .fill(dismissHovering ? Theme.foreground.opacity(0.10) : .clear)
                    )
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .onHover { dismissHovering = $0 }
            .accessibilityLabel("Dismiss resume failure notice")
        }
        .padding(.horizontal, 12)
        .frame(height: 36)
        .background(Theme.attention.opacity(0.08))
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(Theme.foreground.opacity(0.08))
                .frame(height: 1)
        }
    }
}

// MARK: - Phone resize banner

/// Compact bar under the titlebar while a phone temporarily drives this
/// session's terminal size (PhoneResizeOverride). Same anatomy as
/// RestartRecommendedBar; the X reverts to the desktop's natural size.
struct PhoneResizedBar: View {
    let grid: PhoneResizeOverride
    let onRevert: () -> Void
    @State private var dismissHovering = false

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "iphone")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(Theme.unread)
            Text("Resized for phone")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Theme.foreground)
                .lineLimit(1)
            Text("The terminal is \(grid.cols)×\(grid.rows) to match your phone.")
                .font(.system(size: 11))
                .foregroundStyle(Theme.mutedForeground)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 8)
            Button(action: onRevert) {
                Image(systemName: "xmark")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(dismissHovering ? Theme.foreground : Theme.mutedForeground)
                    .frame(width: 24, height: 24)
                    .background(
                        RoundedRectangle(cornerRadius: 6, style: .continuous)
                            .fill(dismissHovering ? Theme.foreground.opacity(0.10) : .clear)
                    )
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .onHover { dismissHovering = $0 }
            .accessibilityLabel("Revert phone resize")
        }
        .padding(.horizontal, 12)
        .frame(height: 36)
        .background(Theme.unread.opacity(0.08))
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(Theme.foreground.opacity(0.08))
                .frame(height: 1)
        }
    }
}

// MARK: - Warm pane host (hidden pre-attach mounting)

/// Mounts panes for pre-warm sessions into a hidden container that tracks
/// the terminal area's size. Being in the window (hidden is enough) is what
/// lets the Ghostty surface build and spawn its attach client; rendering
/// stays paused via `renderingPausedForPrewarm` until the pane is shown for
/// real. Never touches a pane that some other superview (the swap
/// container) currently owns.
struct WarmPaneHostView: NSViewRepresentable {
    let sessions: [SessionEntry]
    let workingDirectories: [String: String]
    let cache: SurfaceCache

    /// Sizes warm panes by hand instead of edge constraints, so tracking can
    /// pause during a live window resize: each hidden pane would otherwise
    /// run the full grid + PTY resize pipeline (socket roundtrip, host
    /// reflow, TUI repaint) on every drag frame — for panes nobody can see,
    /// multiplying the visible pane's cost by the prewarm count. One catch-up
    /// sync at didEndLiveResize preserves the "adoption needs no reflow"
    /// property.
    final class WarmPaneContainerView: NSView {
        override func layout() {
            super.layout()
            syncPaneFrames()
        }

        func syncPaneFrames(force: Bool = false) {
            if !force, window?.inLiveResize == true { return }
            for sub in subviews where sub.frame != bounds {
                sub.frame = bounds
            }
        }

        override func viewDidEndLiveResize() {
            super.viewDidEndLiveResize()
            syncPaneFrames(force: true)
        }
    }

    func makeNSView(context _: Context) -> WarmPaneContainerView {
        let view = WarmPaneContainerView()
        view.isHidden = true
        return view
    }

    func updateNSView(_ container: WarmPaneContainerView, context _: Context) {
        // Unmount warm panes that are no longer wanted (or were dropped from
        // the cache). Detached panes pause on their own (window == nil).
        for sub in container.subviews {
            guard let pane = sub as? GhosttyTerminalPane else { continue }
            let stillWanted = sessions.contains {
                cache.existingPane(for: $0.id) === pane
            }
            if !stillWanted {
                pane.removeFromSuperview()
            }
        }

        // Surface creation is synchronous main-thread work (process spawn +
        // Metal setup); cap it to ONE brand-new pane per update pass so a
        // hover sweep or ⌘-hold never stalls a single frame with several
        // creations. The remaining targets mount on a later pass (any store
        // publish re-runs this).
        var createdNewPane = false
        for session in sessions {
            let exists = cache.existingPane(for: session.id) != nil
            if !exists && createdNewPane { continue }

            let pane = cache.pane(
                for: session,
                workingDirectory: workingDirectories[session.id]
            )
            // Owned by the swap container (or anything else): hands off.
            if let superview = pane.superview, superview !== container { continue }
            guard pane.superview !== container else { continue }
            if !exists { createdNewPane = true }

            pane.translatesAutoresizingMaskIntoConstraints = true
            pane.autoresizingMask = []
            container.addSubview(pane)
        }
        container.syncPaneFrames()
    }
}

// MARK: - Surface host (retained swap container)

struct TerminalHostView: NSViewRepresentable {
    let session: SessionEntry
    let workingDirectory: String?
    let frameBackgroundColor: NSColor
    /// From SurfaceCache — changes when OpenCode/Grok theme (config or live
    /// canvas sample) moves, so updateNSView re-paints even if the session
    /// id is unchanged.
    let themeRevision: UInt
    let phoneResize: PhoneResizeOverride?
    let cache: SurfaceCache

    final class SwapContainer: NSView {
        var frameBackgroundColor = Theme.terminalBackgroundNSColor {
            didSet { updateLayer() }
        }
        var appliedThemeRevision: UInt = 0
        var representedSessionID: String?
        var pendingCreateSessionID: String?

        /// Phone-driven letterbox grid; nil = the pane fills the container.
        var phoneResize: PhoneResizeOverride? {
            didSet {
                guard phoneResize != oldValue else { return }
                scrollOffset = 0
                applyPaneLayout()
            }
        }

        private var laidOutPane: GhosttyTerminalPane?
        private var appliedLetterboxSize: CGSize?

        /// Vertical scroll position while a phone screen overflows the window
        /// (0 = bottom/prompt pinned; `letterboxOverflow` = top revealed).
        /// Adjusted by `scrollWheel`; reset when the override changes.
        private var scrollOffset: CGFloat = 0
        private var letterboxOverflow: CGFloat = 0

        /// Breathing room above and below the phone screen while letterboxed.
        private static let letterboxVerticalPadding: CGFloat = 20

        override var isOpaque: Bool { true }
        override var wantsUpdateLayer: Bool { true }

        /// Terminal background per appearance; AppKit re-runs updateLayer
        /// when the effective appearance flips, so the backdrop behind
        /// surface swaps always matches the Ghostty theme variant.
        override func updateLayer() {
            layer?.backgroundColor = frameBackgroundColor.cgColor
        }

        private var attachedPane: GhosttyTerminalPane? {
            subviews.lazy.compactMap { $0 as? GhosttyTerminalPane }.first
        }

        /// True when `pane` is a direct child here. The pane is ALWAYS a direct
        /// child — never reparented into a wrapper. A Ghostty Metal surface
        /// goes blank both inside an NSScrollView clip view and inside a nested
        /// clip NSView, so overflow/padding is done purely by framing the pane
        /// manually and clipping with the container's own `masksToBounds`.
        func hosts(_ pane: GhosttyTerminalPane) -> Bool {
            pane.superview === self
        }

        /// Removes any currently-hosted pane. The pane stays alive in the
        /// surface cache.
        func detachHostedPane() {
            for sub in subviews where sub is GhosttyTerminalPane {
                sub.removeFromSuperview()
            }
        }

        /// Vertical wheel scroll while a phone screen overflows the window.
        /// Best-effort: a full-screen TUI usually leaves vertical wheel events
        /// unconsumed, so they bubble up here; when it doesn't, the bottom
        /// stays pinned (the live prompt), which is the important part.
        override func scrollWheel(with event: NSEvent) {
            guard letterboxOverflow > 0 else {
                super.scrollWheel(with: event)
                return
            }
            let next = scrollOffset - event.scrollingDeltaY
            scrollOffset = min(max(next, 0), letterboxOverflow)
            layoutAttachedPane()
        }

        // The pane is framed manually, never with AutoLayout. A fixed-size
        // letterbox constraint exerts outward pressure on this container:
        // SwiftUI reads the fitting size as a layout minimum and the AppKit
        // engine can grow the container instead of clamping the pane (the
        // container is autoresizing-placed, which loses to a 999 size), so
        // shrinking the window below the phone grid shoved the whole content
        // column — titlebar and banner included — out of the window.
        override func setFrameSize(_ newSize: NSSize) {
            super.setFrameSize(newSize)
            layoutAttachedPane()
        }

        /// Frames the attached pane: full-bleed normally, or a fixed
        /// phone-shaped rectangle while a phone resize override is active. The
        /// pane is always a direct child; the letterbox keeps the screen's
        /// *full* grid height (aspect ratio forced, never shrunk to the
        /// window) and clips overflow to the container — the bottom (live
        /// prompt) stays pinned and `scrollWheel` reveals the top. The pane
        /// resize flows through the normal Ghostty→attach path, so the hosted
        /// PTY follows on its own.
        private func layoutAttachedPane() {
            guard let pane = attachedPane else { return }
            guard let target = phoneResize.flatMap({
                pane.letterboxSize(cols: $0.cols, rows: $0.rows)
            }) else {
                layer?.masksToBounds = false
                letterboxOverflow = 0
                pane.frame = bounds
                pane.setPhoneScreenFraming(false)
                return
            }
            // Reserve top+bottom padding out of the container height.
            let pad = min(Self.letterboxVerticalPadding, bounds.height / 4)
            let avail = max(0, bounds.height - pad * 2)

            // Width still clamps to the window (rare; windows are wide enough),
            // but height is forced to the full phone grid so the aspect holds.
            let width = min(ceil(target.width), bounds.width)
            let height = ceil(target.height)
            let overflow = max(0, height - avail)
            letterboxOverflow = overflow

            // Clip the overflow to the container instead of shrinking the grid.
            layer?.masksToBounds = true

            let x = ((bounds.width - width) / 2).rounded(.down)
            // Non-flipped coords: y is the pane's bottom edge. When it fits,
            // center with even gutters. When it overflows, pin the bottom a
            // `pad` above the container floor and let `scrollOffset` reveal the
            // top (0 = bottom/prompt visible, up to `overflow`).
            let y: CGFloat = overflow > 0
                ? pad + min(max(scrollOffset, 0), overflow)
                : ((bounds.height - height) / 2).rounded(.down)
            pane.frame = CGRect(x: x, y: y, width: width, height: height)
            pane.setPhoneScreenFraming(true)
        }

        /// Re-applies the pane layout after a state change (override set or
        /// cleared, grid metrics reported, pane attached).
        func applyPaneLayout() {
            guard let pane = attachedPane else { return }
            let target = phoneResize.flatMap {
                pane.letterboxSize(cols: $0.cols, rows: $0.rows)
            }
            // While letterboxed the pane's grid is phone-shaped and must not
            // become the launch-size estimate for new sessions. Keyed off the
            // override (not `target`): before cell metrics exist target is
            // still nil even though a letterbox is pending.
            pane.isLetterboxed = phoneResize != nil
            let changed = !(laidOutPane === pane && appliedLetterboxSize == target)
            laidOutPane = pane
            appliedLetterboxSize = target
            layoutAttachedPane()
            // Grid-metrics callbacks re-enter here after every sync; the
            // refit below only runs on a real change so a settled letterbox
            // stays settled (refit → metrics → refit would never converge).
            guard changed else { return }
            // A pane un-letterboxed while detached can carry grid drift the
            // debounced fit won't repair (same-size no-op); force the full
            // resize path once layout settles.
            DispatchQueue.main.async { [weak pane, weak self] in
                guard let pane, let self, self.hosts(pane) else { return }
                pane.refitNow()
            }
        }
    }

    func makeNSView(context _: Context) -> SwapContainer {
        let view = SwapContainer()
        view.wantsLayer = true
        return view
    }

    func updateNSView(_ container: SwapContainer, context _: Context) {
        // Always write the layer color — dynamic NSColor identity can stay
        // equal across resolution changes, and didSet would then no-op.
        container.frameBackgroundColor = frameBackgroundColor
        container.layer?.backgroundColor = frameBackgroundColor.cgColor
        container.representedSessionID = session.id
        container.phoneResize = phoneResize
        cache.noteShown(session.id)

        if let pane = cache.existingPane(for: session.id) {
            container.pendingCreateSessionID = nil
            // When the live theme revision moves, re-push Ghostty colors for
            // the already-attached pane (attach is a no-op if still hosted).
            if container.appliedThemeRevision != themeRevision {
                container.appliedThemeRevision = themeRevision
                let style = cache.frameStyle(
                    for: session,
                    workingDirectory: workingDirectory
                )
                pane.applyPaneStyle(style.paneStyle)
            }
            attach(pane, to: container)
            return
        }

        // Cold switch: paint the terminal background in this transaction,
        // then create the expensive Ghostty pane on the next main-loop turn.
        // Warm/existing panes still swap synchronously above.
        container.detachHostedPane()
        guard container.pendingCreateSessionID != session.id else { return }
        container.pendingCreateSessionID = session.id
        let session = session
        let workingDirectory = workingDirectory
        DispatchQueue.main.async { [cache, weak container] in
            guard let container,
                  container.representedSessionID == session.id,
                  container.pendingCreateSessionID == session.id
            else { return }
            container.pendingCreateSessionID = nil
            let pane = cache.pane(for: session, workingDirectory: workingDirectory)
            attach(pane, to: container)
        }
    }

    private func attach(_ pane: GhosttyTerminalPane, to container: SwapContainer) {
        guard !container.hosts(pane) else { return }

        // Detach whatever pane is currently shown (it stays alive in the
        // cache — viewport state survives the swap).
        container.detachHostedPane()

        // addSubview re-parents an adopted warm pane automatically. The swap
        // container frames the pane manually (see SwapContainer); a pane
        // adopted from the warm host arrives with the flag off, and leaving
        // it off with no constraints would let the engine zero its frame.
        // applyPaneLayout re-parents it into the letterbox scroll host when a
        // phone resize is active.
        pane.translatesAutoresizingMaskIntoConstraints = true
        container.addSubview(pane)
        container.applyPaneLayout()
        // Cold-mounted panes report their first grid (and cell metrics)
        // only after initial layout; re-fit the letterbox once they exist.
        pane.onSurfaceGridChanged = { [weak pane, weak container] in
            guard let pane, let container, container.hosts(pane),
                  container.phoneResize != nil
            else { return }
            container.applyPaneLayout()
        }

        // Focus and draw synchronously so the CATransaction committing this
        // swap already shows current content with a focused cursor. Without
        // this the first composited frame is the pane's stale pre-detach
        // drawable (the wrapper defers the post-attach draw one runloop
        // turn) and the cursor pops hollow→filled a frame later — together
        // they read as switch lag. window is nil on the very first mount
        // (makeNSView's view is not in the hierarchy yet during this pass);
        // the async fallback below covers that.
        if container.window != nil {
            pane.focus()
            pane.renderNow()
        }

        DispatchQueue.main.async { [weak pane, weak container] in
            // A rapid second switch may have already replaced this pane —
            // a stale focus would route keystrokes to a hidden session.
            guard let pane, let container, container.hosts(pane) else { return }
            pane.focus()
            // Layout has settled by now: force the full resize path so any
            // grid/layer drift the pane picked up while warm or detached is
            // repaired on every switch, instead of waiting for the user to
            // nudge the window (a same-size fit is a no-op to ghostty).
            pane.refitNow()
        }
    }
}

// MARK: - Scroll-to-bottom overlay

/// Visibility/action state for a pane's scroll-to-bottom button. Owned by
/// `GhosttyTerminalPane`; `visible` is flipped from Ghostty's scrollbar
/// metrics callback and the full-screen TUI jump-hint scan.
@MainActor
final class TerminalScrollButtonModel: ObservableObject {
    @Published var visible = false
    var action: () -> Void = {}
}

/// The button itself, mirroring the Tauri app's glass scroll-bottom button
/// (TerminalView.svelte): 36pt circle, chevron down, fades + slides in over
/// 0.2s, 0.88 opacity resting → 1 on hover. Native Liquid Glass on
/// macOS 26 (same `.glass` style as DeadSessionView's Restart button);
/// material-circle fallback on older systems. The 8pt padding gives the
/// slide-in headroom inside the hosting view.
struct TerminalScrollToBottomButton: View {
    @ObservedObject var model: TerminalScrollButtonModel
    @State private var hovering = false

    var body: some View {
        styledButton
            .onHover { hovering = $0 }
            .opacity(model.visible ? (hovering ? 1 : 0.88) : 0)
            .offset(y: model.visible ? 0 : 8)
            .allowsHitTesting(model.visible)
            .animation(.easeOut(duration: 0.2), value: model.visible)
            .padding(8)
            .accessibilityLabel("Scroll to bottom")
    }

    @ViewBuilder
    private var styledButton: some View {
        if #available(macOS 26.0, *) {
            Button(action: model.action) {
                chevron
                    // .glass pads the label to its own metrics; fix the
                    // label frame so the circle stays 36pt like the Tauri app.
                    .frame(width: 20, height: 20)
            }
            .buttonStyle(.glass)
            .buttonBorderShape(.circle)
            .controlSize(.large)
        } else {
            Button(action: model.action) {
                chevron
                    .frame(width: 36, height: 36)
                    .background(.ultraThinMaterial, in: Circle())
                    .overlay(Circle().strokeBorder(Theme.foreground.opacity(0.08)))
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
        }
    }

    private var chevron: some View {
        Image(systemName: "chevron.down")
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(Theme.foreground.opacity(0.9))
    }
}

// MARK: - Empty / starting / dead states

struct EmptyStateView: View {
    var body: some View {
        VStack(spacing: 16) {
            AppBrandLogo()
                .frame(width: 56, height: 56)
                .foregroundStyle(Theme.mutedForeground)
            VStack(spacing: 8) {
                Text("No session selected")
                    .font(.system(size: 14))
                    .foregroundStyle(Theme.mutedForeground)
                Text("Pick a session in the sidebar, or hit + on a project")
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.foreground.opacity(0.35))
            }
            Text(AppBrand.versionLabel)
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(Theme.foreground.opacity(0.25))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// The Unpeel "U" mark (apps/website/components/Logo.tsx): two halves on a shared
/// viewBox, each its own template image so SwiftUI can tint both with the
/// surrounding foregroundStyle and fade just the top half. NSImage's SVG
/// decoder ignores per-path `fill-opacity`, so the fade has to live here.
struct AppBrandLogo: View {
    var body: some View {
        ZStack {
            half(AppBrand.logoBottom)
            half(AppBrand.logoTop).opacity(0.2)
        }
    }

    private func half(_ image: NSImage?) -> some View {
        Group {
            if let image {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fit)
            }
        }
    }
}

/// App branding for in-window chrome (the empty-state logo + version).
enum AppBrand {
    // L-normalized panel paths (NSImage's SVG decoder only handles M/L/C/Z).
    // LOGO:START — generated by scripts/update-logo.mjs from scripts/logo-source.svg
    /// Artwork coordinate space, e.g. "0 0 446 446".
    static let markViewBox = "0 0 446 446"
    /// Solid lower panel of the mark.
    static let logoBottomPath = "M408.833 446C429.36 446 446 429.36 446 408.833L446 223C446 212.737 437.68 204.417 427.417 204.417L364.233 204.417C353.97 204.417 345.65 212.737 345.65 223L345.65 327.067C345.65 337.33 337.33 345.65 327.067 345.65L118.933 345.65C108.67 345.65 100.35 337.33 100.35 327.067L100.35 223C100.35 212.737 92.03 204.417 81.7667 204.417L18.5833 204.417C8.32004 204.417 0 212.737 0 223L0 408.833C0 429.36 16.6401 446 37.1667 446L408.833 446Z"
    /// Upper panel of the mark.
    static let logoTopPath = "M1.62461e-05 37.1667C1.80406e-05 16.6401 16.6401 -1.19583e-06 37.1667 0L408.833 3.24921e-05C429.36 3.42866e-05 446 16.6401 446 37.1667L446 223L345.65 223L345.65 118.933C345.65 108.67 337.33 100.35 327.067 100.35L118.933 100.35C108.67 100.35 100.35 108.67 100.35 118.933L100.35 223L0 223L1.62461e-05 37.1667Z"
    // LOGO:END

    /// Builds a template NSImage from one or more panel paths. Template so the
    /// surrounding `foregroundStyle` (or the menu bar) tints it for light/dark.
    private static func mark(_ paths: String...) -> NSImage? {
        let body = paths.map { ##"<path d="\##($0)"/>"## }.joined()
        let svg = ##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="\##(markViewBox)" fill="#FFFFFF">\##(body)</svg>"##
        guard let image = NSImage(data: Data(svg.utf8)) else { return nil }
        image.isTemplate = true
        return image
    }

    /// Solid lower panel of the mark.
    static let logoBottom: NSImage? = mark(logoBottomPath)

    /// Faded upper panel of the mark.
    static let logoTop: NSImage? = mark(logoTopPath)

    /// White composite of the mark (solid lower + faded upper), non-template.
    /// The two panels carry different opacities — that two-tone is what makes
    /// the mark recognizable — but NSImage's SVG decoder drops per-path
    /// `fill-opacity`, so we composite them at their real opacities and bake
    /// that into the alpha. Base for both menu-bar variants below.
    private static func compositeMark(side: CGFloat = 16) -> NSImage? {
        func panel(_ d: String) -> NSImage? {
            NSImage(data: Data(##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="\##(markViewBox)" fill="#FFFFFF"><path d="\##(d)"/></svg>"##.utf8))
        }
        guard let lower = panel(logoBottomPath), let upper = panel(logoTopPath) else { return nil }
        return NSImage(size: NSSize(width: side, height: side), flipped: false) { rect in
            upper.draw(in: rect, from: .zero, operation: .sourceOver, fraction: 0.4)
            lower.draw(in: rect, from: .zero, operation: .sourceOver, fraction: 1.0)
            return true
        }
    }

    /// Template menu-bar mark — the menu bar tints it (respecting alpha) for
    /// light and dark automatically. Used for the idle state.
    static let menuBarMark: NSImage? = {
        let image = compositeMark()
        image?.isTemplate = true
        return image
    }()

    /// Colored, NON-template menu-bar image: the mark tinted to `foreground`
    /// (resolved by the caller from the menu bar's light/dark appearance) with a
    /// filled `badge` dot at the top-right. Used for the "done jobs" state — a
    /// template would flatten the badge to monochrome, so this keeps its color.
    static func menuBarBadgedMark(foreground: NSColor, badge: NSColor) -> NSImage? {
        guard let base = compositeMark() else { return nil }
        // A hair wider than tall so the badge can sit just past the mark's
        // top-right corner without clipping.
        let canvas = NSSize(width: 19, height: 16)
        let image = NSImage(size: canvas, flipped: false) { _ in
            let markRect = NSRect(x: 0, y: 0, width: 16, height: 16)
            base.draw(in: markRect)
            foreground.set()
            markRect.fill(using: .sourceAtop)        // tint the white mark
            // Badge dot at the top-right, just past the mark's corner.
            let d: CGFloat = 7
            let badgeRect = NSRect(x: canvas.width - d, y: canvas.height - d, width: d, height: d)
            badge.setFill()
            NSBezierPath(ovalIn: badgeRect).fill()
            return true
        }
        image.isTemplate = false
        return image
    }

    /// "vX.Y.Z" from the bundle's CFBundleShortVersionString; "dev" when run
    /// as a bare executable with no Info.plist.
    static let versionLabel: String = {
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
        guard let version, !version.isEmpty else { return "dev" }
        return "v\(version)"
    }()
}

struct StartingSessionView: View {
    let session: SessionEntry

    var body: some View {
        VStack(spacing: 10) {
            ToolIconView(command: session.presentationCommand, size: 32)
                .foregroundStyle(Theme.toolColor(forCommand: session.presentationCommand))
            Text(session.label)
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(Theme.foreground)
                .lineLimit(1)
            BrailleSpinner(color: Theme.toolSpinnerColor(forCommand: session.presentationCommand))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct DeadSessionView: View {
    let session: SessionEntry
    /// Unknown non-empty commands have no trustworthy resume recipe. Keep
    /// their exited terminal/output visible without offering a button that
    /// would silently start unrelated fresh work.
    var canResume = true
    /// Restart in flight: the Svelte restart overlay swaps the button to a
    /// disabled in-flight state (App.svelte:1551-1554). Labelled "Resume":
    /// the session here is always stopped, and the relaunch continues its
    /// conversation.
    var isRestarting = false
    var onRestart: () -> Void = {}

    var body: some View {
        VStack(spacing: 10) {
            // The Svelte exited/restart overlays show the session's tool
            // icon at 32px (App.svelte:1521/1545, ToolIcon → commandIcon
            // in icons.ts, terminalIcon fallback for plain shells).
            ToolIconView(tool: QuickPresetTool.detect(in: session.command), size: 32)
                .foregroundStyle(Theme.toolColor(forCommand: session.command))
            Text(session.label)
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(Theme.foreground)
                .lineLimit(1)
            Text("Session exited")
                .font(.system(size: 11))
                .foregroundStyle(Theme.mutedForeground)
            // GlassButton under the label (App.svelte:1524-1528).
            // Same semantics as the Svelte app: the dead entry is removed
            // and a fresh session re-runs the original command in the
            // original cwd/worktree; the conversation itself is resumed
            // inside the CLI (e.g. `/resume`).
            if canResume {
                restartButton
                    .padding(.top, 2)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    @ViewBuilder
    private var restartButton: some View {
        let button = Button(action: onRestart) {
            if isRestarting {
                HStack(spacing: 6) {
                    BrailleSpinner(color: Theme.toolSpinnerColor(forCommand: session.command))
                        .frame(width: 14, height: 14)
                    Text("Resuming")
                }
            } else {
                Text("Resume")
            }
        }
        .disabled(isRestarting)
        if #available(macOS 26.0, *) {
            button.buttonStyle(.glass)
        } else {
            button.buttonStyle(.bordered)
        }
    }
}
