import SwiftUI
import UIKit
import UnpeelShared

public struct UnpeelIOSRootView: View {
    @State private var store: RemotePreviewStore
    @StateObject private var connection = RemoteConnectionStore()
    @Environment(\.horizontalSizeClass) private var horizontalSizeClass
    @Environment(\.scenePhase) private var scenePhase
    @State private var refreshLoopTask: Task<Void, Never>?
    @State private var refreshWatchdogTask: Task<Void, Never>?
    private let appLock = AppLockManager.shared

    public init(store: RemotePreviewStore = RemotePreviewStore()) {
        _store = State(initialValue: store)
    }

    public var body: some View {
        ZStack(alignment: .leading) {
            TerminalChrome.background.ignoresSafeArea()
            content
            SessionsDrawerOverlay(store: store)
                .zIndex(20)
            PresetDrawerOverlay(store: store)
                .zIndex(30)
            // MCP approval prompts pending on the Mac — an overlay (not a
            // `.sheet`, which doesn't present reliably over the Metal
            // surface), above the drawers because the asking agent is blocked
            // on the answer; below the app lock.
            if !store.pendingApprovals.isEmpty {
                ApprovalPromptOverlay(store: store)
                    .zIndex(50)
                    .transition(.opacity)
            }
            // App lock cover — above everything, including the drawers. Root
            // sheets are dismissed when the lock engages (below), so nothing
            // can present over it.
            if appLock.isLocked {
                AppLockOverlayView(lock: appLock)
                    .zIndex(100)
                    .transition(.opacity)
            }
        }
        .animation(.easeOut(duration: 0.18), value: appLock.isLocked)
        .animation(.easeOut(duration: 0.18), value: store.pendingApprovals.isEmpty)
        .environmentObject(connection)
        .sheet(isPresented: $connection.pairingSheetPresented) {
            PairingView(connection: connection)
        }
        // Bell (activity) + organize sheets live at the ROOT — the same level
        // as pairing, which is the only place a `.sheet` presents reliably
        // over the Metal terminal surface.
        .sheet(item: Binding(
            get: { store.topBarSheet },
            set: { store.topBarSheet = $0 }
        )) { sheet in
            switch sheet {
            case .activity:
                ActivitySessionsPanel(
                    blocked: store.bellBlockedSessions,
                    active: store.bellActiveSessions,
                    recent: store.bellRecentSessions,
                    projectsByID: store.projectsByID
                ) { selected in
                    store.topBarSheet = nil
                    store.select(selected)
                }
                .presentationDetents([.medium])
                .presentationDragIndicator(.visible)
            case .organize:
                if let session = store.organizeSheetSession {
                    SessionOrganizeSheet(store: store, session: session)
                        .presentationDetents([.medium])
                        .presentationDragIndicator(.visible)
                }
            case .organizeProject:
                if let project = store.organizeSheetProject {
                    ProjectOrganizeSheet(store: store, project: project)
                        .presentationDetents([.medium])
                        .presentationDragIndicator(.visible)
                }
            case .gallery:
                if let session = store.selectedSession {
                    BrowserGalleryPanel(
                        client: store.client,
                        sessionID: session.id,
                        supportsResumableUpload: store.supportsResumableArtifactUpload,
                        onApply: { data, contentType in
                            store.attachImageToComposer?(data, contentType)
                            store.topBarSheet = nil
                        }
                    )
                    .presentationDragIndicator(.visible)
                }
            case .textSelection:
                if let payload = store.terminalTextSelection {
                    TerminalTextSelectionSheet(payload: payload) {
                        store.topBarSheet = nil
                    }
                    .presentationDetents([.medium, .large])
                    .presentationDragIndicator(.visible)
                }
            case .archive:
                if let projectID = store.archiveLibraryProjectID,
                   let project = store.projectsByID[projectID] {
                    ArchivedSessionsSheet(store: store, project: project)
                        .presentationDetents([.medium, .large])
                        .presentationDragIndicator(.visible)
                }
            }
        }
        .onChange(of: store.topBarSheet != nil) { presented in
            if presented { dismissTerminalKeyboard() }
        }
        .onChange(of: connection.pairingSheetPresented) { presented in
            if presented { dismissTerminalKeyboard() }
        }
        .onAppear {
            // A background/locked cold launch may have preserved pairing
            // records while their WhenUnlocked Keychain items were hidden.
            // Rehydrate before exposing pairing state or adopting the client.
            connection.retryKeychainHydrationIfNeeded()
            store.adoptClient(
                connection.client,
                connectionEpoch: connection.epoch
            )
            // Lost Direct connections recover through the E2E Relay. Bonjour
            // remains an unauthenticated discovery hint and must never receive
            // the saved bearer; an address change without Relay requires an
            // explicit re-pair until proof-backed rediscovery exists.
            store.attemptRelayFallback = { [weak connection, weak store] in
                guard let connection, let store else { return false }
                let recovered = await connection.activateRelayFallback()
                if recovered {
                    store.adoptClient(
                        connection.client,
                        connectionEpoch: connection.epoch
                    )
                }
                return recovered
            }
            store.attemptDirectRestore = { [weak connection, weak store] in
                guard let connection, let store else { return false }
                let restored = await connection.restoreDirectConnection()
                if restored {
                    store.adoptClient(
                        connection.client,
                        connectionEpoch: connection.epoch
                    )
                }
                return restored
            }
            store.onDirectPollSucceeded = { [weak connection] proof in
                guard let connection else { return }
                await connection.ensureRelayCredentials(after: proof)
            }
            // A device build with no paired Mac has nothing to talk to —
            // land straight in pairing instead of an empty preview.
            if connection.needsPairing {
                connection.pairingSheetPresented = true
            }
            // Push: upload the APNs token to EVERY paired Mac whenever it
            // (re)arrives — notifications must work from non-active Macs
            // too — and route a tapped notification to its session.
            PushManager.shared.onTokenChange = { [weak connection] token, environment in
                connection?.registerPushTokenEverywhere(
                    apnsToken: token, environment: environment
                )
            }
            PushManager.shared.onOpenSession = { [weak store] sessionID in
                store?.selectSessionByID(sessionID)
            }
            PushManager.shared.uploadCachedToken()
        }
        .onChange(of: connection.epoch) { _ in
            store.adoptClient(
                connection.client,
                connectionEpoch: connection.epoch
            )
            Task { await store.loadFromBridge() }
            // New/re-paired Mac client — (re)register the push token with it.
            PushManager.shared.uploadCachedToken()
        }
        .task {
            startRefreshLoop()
        }
        .onChange(of: scenePhase) { phase in
            switch phase {
            case .background:
                // Polling while backgrounded just queues a request iOS will
                // suspend mid-flight; its failure on unlock used to flash the
                // disconnected state over a perfectly healthy connection.
                stopRefreshLoop()
                appLock.lockIfEnabled()
                if appLock.isLocked {
                    // Sheets present over the ZStack, so they'd float above
                    // the lock cover — drop them while locked. The keyboard
                    // would too.
                    store.topBarSheet = nil
                    connection.pairingSheetPresented = false
                    dismissTerminalKeyboard()
                }
            case .active:
                connection.retryKeychainHydrationIfNeeded()
                if connection.needsPairing {
                    connection.pairingSheetPresented = true
                }
                // Restart immediately (the loop's first step is a bootstrap
                // fetch) so the first visible frame paints from fresh data.
                startRefreshLoop(afterResume: true)
                // One automatic biometric prompt per foreground (covers cold
                // launch too); the overlay's button handles retries.
                Task { await appLock.autoUnlockOnForeground() }
            default:
                break
            }
        }
        .onReceive(NotificationCenter.default.publisher(
            for: UIApplication.protectedDataDidBecomeAvailableNotification
        )) { _ in
            connection.retryKeychainHydrationIfNeeded()
            if connection.needsPairing {
                connection.pairingSheetPresented = true
            }
        }
        .onDisappear {
            stopRefreshLoop()
        }
    }

    private func startRefreshLoop(afterResume: Bool = false) {
        guard refreshLoopTask == nil else { return }
        if afterResume {
            store.prepareForForegroundResume()
        }
        refreshLoopTask = Task {
            await store.runBridgeRefreshLoop()
        }
        startRefreshWatchdog()
    }

    private func stopRefreshLoop() {
        refreshLoopTask?.cancel()
        refreshLoopTask = nil
        refreshWatchdogTask?.cancel()
        refreshWatchdogTask = nil
    }

    /// The refresh loop is strictly sequential, so one poll hanging mid-await
    /// (a half-open connection across a Mac restart survives every request
    /// timeout miss) freezes the sidebar at the last applied snapshot while
    /// the banner keeps saying "Connected" — task cancellation can't help
    /// because the await never returns to observe it. Watch the store's
    /// poll-completion heartbeat instead and replace the whole loop task when
    /// it stops: the wedged task is abandoned (it dies with its connection),
    /// and the fresh one repolls within 2s.
    private func startRefreshWatchdog() {
        guard refreshWatchdogTask == nil else { return }
        refreshWatchdogTask = Task {
            while !Task.isCancelled {
                // Backgrounding cancels this task (stopRefreshLoop), so
                // reaching here means the app is foregrounded — or just woke
                // from suspension with a stale heartbeat, where a restart is
                // exactly right anyway.
                try? await Task.sleep(nanoseconds: 6_000_000_000)
                guard !Task.isCancelled else { break }
                if Date().timeIntervalSince(store.lastPollCompletedAt) > 20 {
                    RefreshDiagnostics.log("watchdog: poll heartbeat stale — restarting refresh loop")
                    refreshLoopTask?.cancel()
                    store.prepareForForegroundResume()
                    refreshLoopTask = Task {
                        await store.runBridgeRefreshLoop()
                    }
                }
            }
        }
    }

    @ViewBuilder
    private var content: some View {
        if horizontalSizeClass == .regular {
            NavigationSplitView {
                SessionSidebarView(store: store)
            } detail: {
                TerminalDetailView(store: store)
            }
        } else {
            TerminalDetailView(store: store)
        }
    }
}

private struct SessionsDrawerOverlay: View {
    var store: RemotePreviewStore
    @Environment(\.horizontalSizeClass) private var horizontalSizeClass
    /// The real session list mounts one tick AFTER the panel starts sliding:
    /// building it synchronously in the toggle's transaction is what made
    /// the drawer feel slow to open. Until then, skeleton rows.
    @State private var listReady = false
    /// Live leftward drag offset (≤ 0) while swiping the drawer closed.
    @State private var dragOffset: CGFloat = 0
    /// True while the current gesture is a horizontal drawer slide — the list
    /// scroll is locked out for its duration so a slide never scrolls the
    /// sidebar too (one direction at a time).
    @State private var horizontalSlideLock = false

    var body: some View {
        GeometryReader { geometry in
            let drawerWidth = drawerWidth(for: geometry.size)
            // Unified horizontal position of the drawer: 0 = fully open,
            // -drawerWidth = fully closed. Open state uses the close-drag
            // offset; an in-progress open-drag follows the finger (peek).
            let drawerX: CGFloat = {
                if store.sessionsDrawerPresented { return dragOffset }
                if let reveal = store.sidebarDragReveal { return min(0, reveal - drawerWidth) }
                return -drawerWidth
            }()
            // How far open it is (drives the backdrop fade), 0…1.
            let openFraction = max(0, min(1, (drawerWidth + drawerX) / drawerWidth))
            let mounted = store.sessionsDrawerPresented || store.sidebarDragReveal != nil

            ZStack(alignment: .leading) {
                if mounted {
                    Color.black.opacity(0.38 * openFraction)
                        .ignoresSafeArea()
                        .transition(.opacity)
                        .onTapGesture {
                            store.hideSessions()
                        }

                    drawerContent(topInset: geometry.safeAreaInsets.top)
                        .frame(width: drawerWidth)
                        .frame(maxHeight: .infinity)
                        .background(IOSSidebarTheme.background)
                        .overlay(alignment: .trailing) {
                            Rectangle()
                                .fill(.white.opacity(0.10))
                                .frame(width: 1)
                        }
                        // Rounded trailing edge like the desktop sidebar card;
                        // the leading edge stays flush with the screen.
                        .clipShape(UnevenRoundedRectangle(
                            topLeadingRadius: 0,
                            bottomLeadingRadius: 0,
                            bottomTrailingRadius: 16,
                            topTrailingRadius: 16,
                            style: .continuous
                        ))
                        // Flatten before the shadow: without this the
                        // offscreen shadow pass re-renders every row every
                        // frame of the slide-in.
                        .compositingGroup()
                        .shadow(color: .black.opacity(0.40), radius: 22, x: 8, y: 0)
                        .offset(x: drawerX)
                        .simultaneousGesture(closeDrag)
                        // Slide in/out for tap-open and close. During an
                        // open-drag the panel mounts already offscreen (drawerX
                        // ≈ -drawerWidth), so this transition is invisible and
                        // `drawerX` follows the finger; on commit it stays
                        // mounted and `drawerX` just animates to 0.
                        .transition(.move(edge: .leading).combined(with: .opacity))
                }
            }
            .frame(width: geometry.size.width, height: geometry.size.height)
            // ONLY hit-testable when actually presented. During an open-drag the
            // finger is already captured by the terminal's gesture; letting the
            // overlay grab touches while a peek is active is what froze the app
            // when a peek got stuck (the overlay swallowed every touch).
            .allowsHitTesting(store.sessionsDrawerPresented)
        }
        .ignoresSafeArea()
        .animation(.timingCurve(0.16, 1, 0.3, 1, duration: 0.28), value: store.sessionsDrawerPresented)
        // The drawer slides over the terminal; a lingering keyboard both
        // covers the session list and keeps typing routed at the terminal.
        .onChange(of: store.sessionsDrawerPresented) { presented in
            if presented {
                dragOffset = 0
                dismissTerminalKeyboard()
            }
        }
        .onChange(of: store.presetDrawerProjectID) { projectID in
            if projectID != nil { dismissTerminalKeyboard() }
        }
    }

    /// Swipe the drawer left to dismiss. `.simultaneousGesture` so the session
    /// list still scrolls vertically; we only take over once the drag is
    /// clearly horizontal + leftward. Measured in `.global` space because the
    /// drawer is itself translated by `dragOffset` (a `.local` read would be in
    /// a moving frame → oscillation, the same trap as the preset drawer).
    private var closeDrag: some Gesture {
        DragGesture(minimumDistance: 12, coordinateSpace: .global)
            .onChanged { value in
                guard abs(value.translation.width) > abs(value.translation.height) else { return }
                // Commit to a horizontal slide and lock the list scroll for the
                // rest of the gesture (stays locked even if it later wanders
                // vertical, so the two never fight).
                horizontalSlideLock = true
                dragOffset = min(0, value.translation.width)
            }
            .onEnded { value in
                horizontalSlideLock = false
                let horizontal = abs(value.translation.width) > abs(value.translation.height)
                let dismiss = horizontal
                    && (value.translation.width < -80 || value.predictedEndTranslation.width < -180)
                if dismiss {
                    store.hideSessions()
                } else {
                    withAnimation(.spring(response: 0.32, dampingFraction: 0.86)) {
                        dragOffset = 0
                    }
                }
            }
    }

    @ViewBuilder
    private func drawerContent(topInset: CGFloat) -> some View {
        ZStack {
            if listReady {
                SessionSidebarView(
                    store: store,
                    topContentInset: topInset,
                    scrollLocked: horizontalSlideLock
                )
                    .transition(.opacity)
            } else {
                SidebarSkeletonView(topInset: topInset)
            }
        }
        .animation(.easeOut(duration: 0.16), value: listReady)
        .task {
            // One frame on the skeleton lets the slide-in start immediately;
            // the real list mounts while the panel is already moving.
            try? await Task.sleep(nanoseconds: 30_000_000)
            listReady = true
        }
        .onDisappear {
            listReady = false
        }
    }

    private func drawerWidth(for size: CGSize) -> CGFloat {
        if horizontalSizeClass == .regular {
            return min(380, max(320, size.width * 0.40))
        }
        return min(360, max(286, size.width * 0.88))
    }
}

/// Sidebar-shaped shimmer shown for the drawer's first frames: a connection
/// row, project headers, and indented session rows — enough structure that
/// the swap to real content reads as fill-in, not replacement.
private struct SidebarSkeletonView: View {
    let topInset: CGFloat
    @State private var pulsing = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            skeletonRow(iconSize: 16, lineFraction: 0.55)
            ForEach(0 ..< 3, id: \.self) { group in
                VStack(alignment: .leading, spacing: 12) {
                    skeletonRow(iconSize: 14, lineFraction: 0.42)
                    ForEach(0 ..< 3, id: \.self) { row in
                        skeletonRow(
                            iconSize: 0,
                            lineFraction: [0.68, 0.5, 0.6][row]
                        )
                        .padding(.leading, 22)
                    }
                }
                .opacity(group == 2 ? 0.6 : 1)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .padding(.top, topInset + 16)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(IOSSidebarTheme.background)
        .opacity(pulsing ? 0.55 : 1)
        .animation(
            .easeInOut(duration: 0.8).repeatForever(autoreverses: true),
            value: pulsing
        )
        .onAppear { pulsing = true }
        .allowsHitTesting(false)
    }

    private func skeletonRow(iconSize: CGFloat, lineFraction: CGFloat) -> some View {
        GeometryReader { geometry in
            HStack(spacing: 8) {
                if iconSize > 0 {
                    RoundedRectangle(cornerRadius: 4, style: .continuous)
                        .fill(.white.opacity(0.08))
                        .frame(width: iconSize, height: iconSize)
                }
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(.white.opacity(0.08))
                    .frame(width: geometry.size.width * lineFraction, height: 11)
            }
        }
        .frame(height: 16)
    }
}

private struct PresetDrawerOverlay: View {
    var store: RemotePreviewStore

    /// Live drag offset while the user is swiping the sheet down to dismiss.
    @State private var dragOffset: CGFloat = 0

    /// Row height (`PresetDrawerRow` pins `minHeight: 52`) and inter-row spacing,
    /// used to size the list deterministically — no runtime GeometryReader
    /// measurement, which otherwise re-fires every drag frame and makes the
    /// dismiss drag stutter.
    private static let rowHeight: CGFloat = 52
    private static let rowSpacing: CGFloat = 7

    private var project: RemoteProjectSummary? {
        store.presetDrawerProject
    }

    private var presets: [RemotePresetSummary] {
        // Keep the snapshot's order: the Mac sends presets in the same order
        // the desktop "+" menu shows them, so the two menus stay identical.
        store.snapshot.presets
            .filter { $0.enabled && $0.supportsIOSSessionAPI }
    }

    var body: some View {
        GeometryReader { geometry in
            ZStack(alignment: .bottom) {
                if let project {
                    Color.black.opacity(0.30)
                        .ignoresSafeArea()
                        .onTapGesture {
                            store.hidePresetDrawer()
                        }
                        .transition(.opacity)

                    // Cap the list so the drawer can never grow past the screen;
                    // when the presets overflow this, the ScrollView scrolls.
                    let maxListHeight = max(
                        200,
                        geometry.size.height - geometry.safeAreaInsets.top - 150
                    )
                    let listBottomPadding = max(16, geometry.safeAreaInsets.bottom + 10)
                    let naturalListHeight = CGFloat(presets.count) * Self.rowHeight
                        + CGFloat(max(0, presets.count - 1)) * Self.rowSpacing
                        + listBottomPadding
                    let listHeight = min(naturalListHeight, maxListHeight)

                    VStack(spacing: 0) {
                        // Header (drag handle + title + close) — carries the
                        // swipe-down-to-dismiss gesture. Kept out of the
                        // ScrollView so the gesture never fights scrolling.
                        VStack(spacing: 0) {
                            Capsule()
                                .fill(Color.white.opacity(0.26))
                                .frame(width: 42, height: 4)
                                .padding(.top, 10)
                                .padding(.bottom, 12)

                            HStack(spacing: 10) {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text("New session")
                                        .font(.system(size: 17, weight: .semibold))
                                        .foregroundStyle(IOSSidebarTheme.foreground)
                                    Text(project.name)
                                        .font(.system(size: 12, weight: .medium))
                                        .foregroundStyle(IOSSidebarTheme.mutedForeground)
                                        .lineLimit(1)
                                }

                                Spacer(minLength: 0)

                                Button {
                                    store.hidePresetDrawer()
                                } label: {
                                    Image(systemName: "xmark")
                                        .font(.system(size: 13, weight: .semibold))
                                        .frame(width: 34, height: 34)
                                }
                                .foregroundStyle(IOSSidebarTheme.mutedForeground)
                                .iosGlassControl(cornerRadius: 11)
                                .accessibilityLabel("Close presets")
                            }
                            .padding(.horizontal, 18)
                            .padding(.bottom, 10)
                        }
                        .contentShape(Rectangle())
                        .gesture(dismissDrag)

                        ScrollView {
                            VStack(spacing: 7) {
                                ForEach(presets) { preset in
                                    PresetDrawerRow(
                                        preset: preset,
                                        launching: store.launchingPresetID == preset.id
                                    ) {
                                        store.startSession(projectID: project.id, preset: preset)
                                    }
                                }
                            }
                            .padding(.horizontal, 12)
                            .padding(.bottom, listBottomPadding)
                        }
                        .frame(height: listHeight)
                        .scrollBounceBehavior(.basedOnSize)
                    }
                    .frame(maxWidth: min(420, geometry.size.width - 18))
                    // Force dark so the glass buttons (the X) render dark-mode
                    // glass instead of following the system light appearance.
                    .environment(\.colorScheme, .dark)
                    // Real iOS 26 Liquid Glass (`.glassEffect`) — the old
                    // `.ultraThinMaterial` reads as a flat frosted panel;
                    // material fallback below iOS 26.
                    .glassSheetBackground(cornerRadius: 28)
                    .compositingGroup()
                    .shadow(color: .black.opacity(0.42), radius: 28, x: 0, y: -12)
                    .padding(.horizontal, 9)
                    .padding(.bottom, 8)
                    .offset(y: dragOffset)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .frame(width: geometry.size.width, height: geometry.size.height)
            .allowsHitTesting(project != nil)
        }
        .ignoresSafeArea()
        .animation(.timingCurve(0.16, 1, 0.3, 1, duration: 0.30), value: store.presetDrawerProjectID)
        // Reset the drag offset whenever the drawer (re)opens so a prior
        // swipe-dismiss can't leave it shifted the next time it appears.
        .onChange(of: store.presetDrawerProjectID) { _, newValue in
            if newValue != nil { dragOffset = 0 }
        }
    }

    /// Swipe the sheet down to dismiss, matching native sheet behavior.
    ///
    /// Measured in `.global` space on purpose: the gesture lives on the header,
    /// which is itself translated by `dragOffset`, so a `.local` translation
    /// would be read in a coordinate space that this gesture is actively
    /// moving — a feedback loop that makes the sheet jump/oscillate. Global
    /// (screen) space is unaffected by the sheet's own offset.
    private var dismissDrag: some Gesture {
        DragGesture(minimumDistance: 8, coordinateSpace: .global)
            .onChanged { value in
                dragOffset = max(0, value.translation.height)
            }
            .onEnded { value in
                let dismiss = value.translation.height > 90
                    || value.predictedEndTranslation.height > 200
                if dismiss {
                    store.hidePresetDrawer()
                } else {
                    withAnimation(.spring(response: 0.32, dampingFraction: 0.86)) {
                        dragOffset = 0
                    }
                }
            }
    }
}

private extension View {
    /// Clean solid dark sheet background. Liquid Glass (`.glassEffect`) was
    /// tried here and looked bad: over the near-black terminal it has nothing
    /// to refract, so `.regular` goes milky-light and a dark tint just reads as
    /// a flat panel that picks up the content behind it inconsistently. A solid
    /// dark fill with a hairline edge is consistent and legible.
    func glassSheetBackground(cornerRadius: CGFloat) -> some View {
        let shape = RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
        return background(shape.fill(Color(hex: 0x1B1C22)))
            .overlay(
                shape.strokeBorder(Color.white.opacity(0.08), lineWidth: 1)
            )
    }
}

/// Root-level Allow / Don't Allow prompt for a pending MCP approval on the
/// connected Mac (session write / browser / computer access). Renders the
/// Mac-resolved copy verbatim; answering races the desktop prompt and other
/// controllers — first answer wins, the rest dismiss on their own. No
/// tap-outside-to-dismiss: this is a privilege grant, so the only exits are
/// the two explicit answers.
private struct ApprovalPromptOverlay: View {
    var store: RemotePreviewStore

    var body: some View {
        if let approval = store.pendingApprovals.first {
            ZStack {
                Color.black.opacity(0.45)
                    .ignoresSafeArea()
                VStack(spacing: 14) {
                    Image(systemName: Self.icon(for: approval.kind))
                        .font(.system(size: 30, weight: .semibold))
                        .foregroundStyle(.yellow)
                    Text(approval.title)
                        .font(.system(size: 16, weight: .semibold))
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                    Text(approval.body)
                        .font(.system(size: 13))
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                    if store.pendingApprovals.count > 1 {
                        Text("\(store.pendingApprovals.count - 1) more waiting")
                            .font(.system(size: 11))
                            .foregroundStyle(.tertiary)
                    }
                    HStack(spacing: 10) {
                        Button {
                            Task { await store.answerApproval(id: approval.id, approved: false) }
                        } label: {
                            Text("Don't Allow")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.bordered)
                        Button {
                            Task { await store.answerApproval(id: approval.id, approved: true) }
                        } label: {
                            Text("Allow")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.borderedProminent)
                    }
                    .padding(.top, 4)
                }
                .padding(22)
                .frame(maxWidth: 340)
                .glassSheetBackground(cornerRadius: 22)
                .padding(.horizontal, 28)
            }
            .environment(\.colorScheme, .dark)
        }
    }

    private static func icon(for kind: String) -> String {
        switch kind {
        case "write": return "keyboard"
        case "browser": return "globe"
        case "computer": return "desktopcomputer"
        default: return "questionmark.circle"
        }
    }
}

private struct PresetDrawerRow: View {
    let preset: RemotePresetSummary
    let launching: Bool
    let onLaunch: () -> Void

    /// The CLI type as the title (so the command isn't repeated on both lines).
    /// A custom preset with its own label keeps that label instead.
    private var titleText: String {
        let label = preset.label.trimmingCharacters(in: .whitespacesAndNewlines)
        if !label.isEmpty && label != preset.command {
            return label
        }
        return preset.cliID ?? label
    }

    var body: some View {
        Button(action: onLaunch) {
            HStack(spacing: 11) {
                Group {
                    if launching {
                        Image(systemName: "circle.dotted")
                            .font(.system(size: 16, weight: .semibold))
                            .foregroundStyle(IOSSidebarTheme.toolColor(for: preset))
                    } else {
                        // The real CLI brand logo (same shared SVGs as the
                        // session rows and the desktop app), not an SF Symbol.
                        SharedToolIconView(
                            providerID: preset.cliID,
                            command: preset.command,
                            size: 18
                        )
                    }
                }
                .frame(width: 28, height: 28)
                .background(
                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .fill(IOSSidebarTheme.hoverRow)
                )

                VStack(alignment: .leading, spacing: 2) {
                    Text(titleText)
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(IOSSidebarTheme.foreground)
                        .lineLimit(1)
                    Text(preset.command)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(IOSSidebarTheme.mutedForeground)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }

                Spacer(minLength: 0)

                Image(systemName: "arrow.up.right")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(IOSSidebarTheme.mutedForeground)
            }
            .padding(EdgeInsets(top: 9, leading: 10, bottom: 9, trailing: 10))
            .frame(maxWidth: .infinity, minHeight: 52, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .fill(IOSSidebarTheme.hoverRow.opacity(0.72))
            )
        }
        .buttonStyle(.plain)
        .disabled(launching)
        .opacity(launching ? 0.72 : 1)
    }
}

struct SessionSidebarView: View {
    var store: RemotePreviewStore
    @EnvironmentObject private var connection: RemoteConnectionStore
    /// Phone-side push health, so a broken notification pipeline (permission
    /// denied, APNs registration failed) is visible where the user actually
    /// lives — otherwise needs-input alerts just silently stop arriving.
    @ObservedObject private var push = PushManager.shared
    var topContentInset: CGFloat = 0
    /// Disable list scrolling while the drawer is being slid horizontally, so a
    /// slide gesture never scrolls the sidebar at the same time.
    var scrollLocked: Bool = false

    /// Window top inset captured once per view identity — the previous
    /// computed property walked `UIApplication.connectedScenes` twice per
    /// body evaluation of the sidebar, its hottest view.
    @State private var deviceTopInset = Self.deviceWindowTopInset()
    /// Window bottom inset (home indicator), so the pinned feedback footer
    /// clears it — the drawer ignores safe areas.
    @State private var deviceBottomInset = Self.deviceWindowBottomInset()

    /// Extra top content padding while the push warning banner is shown:
    /// banner height (~26) + breathing room, so the connection row starts
    /// below it instead of underneath.
    private static let pushWarningInset: CGFloat = 40

    /// The drawer overlay ignores safe areas, so the passed inset can arrive
    /// as zero — fall back to the window's real top inset so the header (and
    /// its "+" buttons) never sit under the notch/Dynamic Island.
    private var effectiveTopInset: CGFloat {
        max(topContentInset, deviceTopInset)
    }

    private static func deviceWindowTopInset() -> CGFloat {
        #if os(iOS)
        UIApplication.shared.connectedScenes
            .compactMap { ($0 as? UIWindowScene)?.keyWindow }
            .first?.safeAreaInsets.top ?? 0
        #else
        0
        #endif
    }

    private static func deviceWindowBottomInset() -> CGFloat {
        #if os(iOS)
        UIApplication.shared.connectedScenes
            .compactMap { ($0 as? UIWindowScene)?.keyWindow }
            .first?.safeAreaInsets.bottom ?? 0
        #else
        0
        #endif
    }

    var body: some View {
        ZStack {
            IOSSidebarTheme.background.ignoresSafeArea()
            VStack(spacing: 0) {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 1) {
                            Button {
                                connection.pairingSheetPresented = true
                            } label: {
                                ConnectionStatusRow(
                                    store: store,
                                    switchableMacCount: connection.pairedMacs.count
                                )
                            }
                            .buttonStyle(.plain)
                            .padding(.bottom, 7)

                            if store.isDisconnected {
                                // No live connection means the session list
                                // is stale by definition — say so instead of
                                // rendering sessions the phone can't reach.
                                SidebarDisconnectedView()
                            } else {
                                if !store.blockedSessions.isEmpty {
                                    SidebarSectionTitle("Blocked")
                                    ForEach(store.blockedSessions.prefix(5)) { session in
                                        MacStyleSessionRow(
                                            session: session,
                                            project: store.projectsByID[session.projectID],
                                            selected: session.id == store.selectedSessionID,
                                            onOrganize: { store.presentSessionOrganize(for: session) }
                                        ) {
                                            store.select(session)
                                        }
                                        .id("blocked-\(session.id)")
                                    }
                                    .padding(.bottom, 6)
                                }

                                listContent
                                    .id("main")
                            }
                        }
                        // Content starts below the clock but the panel runs
                        // to the physical top — scrolled rows slide under
                        // the veil instead of a header pushing them down.
                        // A visible push warning is sticky in that header
                        // band, so the content drops below it.
                        .padding(EdgeInsets(
                            top: effectiveTopInset + 20
                                + (push.registrationState.sidebarWarning == nil
                                    ? 0 : Self.pushWarningInset),
                            leading: 8,
                            // Breathing room so the last folder/sessions aren't
                            // flush against the bottom edge (and the home
                            // indicator), and the scroll-reveal has room.
                            bottom: 40,
                            trailing: 8
                        ))
                    }
                    .scrollIndicators(.hidden)
                    .scrollDisabled(scrollLocked)
                    .mask(SidebarListFadeMask())
                    .overlay(alignment: .top) {
                        IOSSidebarTopGlassVeil(height: effectiveTopInset + 22)
                            .allowsHitTesting(false)
                    }
                    // Sticky, not a scrolling row: broken notifications stay
                    // visible however far the list is scrolled. Tap opens the
                    // settings sheet — its Notifications section has the
                    // fix-it buttons (Open iOS Settings / retry).
                    .overlay(alignment: .top) {
                        if let warning = push.registrationState.sidebarWarning {
                            SidebarPushWarningBanner(message: warning) {
                                connection.pairingSheetPresented = true
                            }
                            .padding(.horizontal, 8)
                            .padding(.top, effectiveTopInset + 14)
                        }
                    }
                    .onAppear {
                        // The drawer builds fresh on every open, so this
                        // covers presentation too: position the list without
                        // animation instead of animating a scroll into the
                        // middle of the slide-in.
                        revealSelectedSession(using: proxy, animated: false)
                    }
                    .onChange(of: store.selectedSessionID) { _ in
                        revealSelectedSession(using: proxy, animated: true)
                    }
                }
                feedbackFooter
            }
        }
        .environment(\.colorScheme, .dark)
        .onAppear {
            // Refresh in case the key window wasn't attached yet when the
            // @State initial value was captured.
            let inset = Self.deviceWindowTopInset()
            if deviceTopInset != inset { deviceTopInset = inset }
            let bottom = Self.deviceWindowBottomInset()
            if deviceBottomInset != bottom { deviceBottomInset = bottom }
        }
    }

    /// GitHub Discussions — same destination as the website footer's
    /// "Bugs & Feedback" link.
    private static let feedbackURL = URL(string: "https://github.com/orgs/unpeel-com/discussions")!

    /// Pinned at the very bottom of the sidebar: opens the GitHub discussion.
    private var feedbackFooter: some View {
        Link(destination: Self.feedbackURL) {
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.bubble")
                    .font(.system(size: 13, weight: .medium))
                Text("Feedback & bugs")
                    .font(.system(size: 13, weight: .medium))
                Spacer(minLength: 4)
                Image(systemName: "arrow.up.right")
                    .font(.system(size: 11, weight: .semibold))
                    .opacity(0.7)
            }
            .foregroundStyle(IOSSidebarTheme.mutedForeground)
            .padding(.horizontal, 14)
            .padding(.top, 12)
            .padding(.bottom, max(deviceBottomInset, 12))
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .overlay(alignment: .top) {
            Rectangle()
                .fill(.white.opacity(0.08))
                .frame(height: 1)
        }
    }

    @ViewBuilder
    private var listContent: some View {
        ForEach(folderGroups, id: \.folder.id) { group in
            let isOpen = store.expandedFolderID == group.folder.id
            FolderSectionRow(
                folder: group.folder,
                isExpanded: isOpen,
                sessionCount: group.projects.reduce(0) {
                    $0 + sessionCount(for: $1, includeChildren: true)
                },
                activity: isOpen ? (false, false) : folderActivity(group.projects)
            ) {
                withAnimation(.timingCurve(0.16, 1, 0.3, 1, duration: 0.28)) {
                    store.toggleFolder(group.folder.id)
                }
            }
            if isOpen {
                ForEach(group.projects) { project in
                    projectBlock(project)
                }
            }
        }

        ForEach(looseProjects) { project in
            projectBlock(project)
        }
    }

    @ViewBuilder
    private func projectBlock(
        _ project: RemoteProjectSummary,
        depth: Int = 0
    ) -> some View {
        let projectSessions = sessions(for: project)
        let children = inlineChildProjects(for: project)
        let descendantSessions = children.flatMap { sessions(for: $0) }
        let isExpanded = store.expandedProjectIDs.contains(project.id)
        Group {
            MacStyleProjectRow(
                project: project,
                depth: depth,
                isExpanded: isExpanded,
                hasBusySession: (projectSessions + descendantSessions).contains {
                    $0.activity == .working
                },
                hasAttentionSession: (projectSessions + descendantSessions).contains {
                    $0.activity == .blocked
                },
                sessionCount: sessionCount(for: project, includeChildren: true),
                canCreateSession: store.supportsSessionCreation,
                onAdd: {
                    store.showPresetDrawer(for: project.id)
                },
                onOrganize: { store.presentProjectOrganize(for: project) }
            ) {
                withAnimation(.timingCurve(0.16, 1, 0.3, 1, duration: 0.28)) {
                    store.toggleProject(project.id)
                }
            }
            if isExpanded {
                ForEach(children) { child in
                    childFolderBlock(child, depth: depth + 1)
                }
                projectSessionRows(for: project, depth: depth + 1)
            }
        }
    }

    @ViewBuilder
    private func childFolderBlock(
        _ project: RemoteProjectSummary,
        depth: Int
    ) -> some View {
        let projectSessions = sessions(for: project)
        let isExpanded = store.expandedProjectIDs.contains(project.id)
        MacStyleProjectRow(
            project: project,
            depth: depth,
            isExpanded: isExpanded,
            hasBusySession: projectSessions.contains { $0.activity == .working },
            hasAttentionSession: projectSessions.contains { $0.activity == .blocked },
            sessionCount: projectSessions.count,
            canCreateSession: store.supportsSessionCreation,
            onAdd: { store.showPresetDrawer(for: project.id) },
            onOrganize: { store.presentProjectOrganize(for: project) }
        ) {
            withAnimation(.timingCurve(0.16, 1, 0.3, 1, duration: 0.28)) {
                store.toggleProject(project.id)
            }
        }
        if isExpanded {
            projectSessionRows(for: project, depth: depth + 1)
        }
    }

    @ViewBuilder
    private func projectSessionRows(
        for project: RemoteProjectSummary,
        depth: Int
    ) -> some View {
        ForEach(pinnedSessions(for: project)) { session in
            MacStyleSessionRow(
                session: session,
                project: project,
                selected: session.id == store.selectedSessionID,
                depth: depth,
                onOrganize: { store.presentSessionOrganize(for: session) }
            ) {
                store.select(session)
            }
            .id(session.id)
            .transition(.opacity.combined(with: .move(edge: .top)))
        }
        ForEach(displayedRegularSessions(for: project)) { session in
            MacStyleSessionRow(
                session: session,
                project: project,
                selected: session.id == store.selectedSessionID,
                depth: depth,
                onOrganize: { store.presentSessionOrganize(for: session) }
            ) {
                store.select(session)
            }
            .id(session.id)
            .transition(.opacity.combined(with: .move(edge: .top)))
        }
    }

    private static let visibleSessionLimit = 5

    private var tree: IOSSidebarProjectTree {
        store.sidebarTree
    }

    private var looseProjects: [RemoteProjectSummary] {
        tree.looseProjects
    }

    private var folderGroups: [(folder: RemoteProjectFolderSummary, projects: [RemoteProjectSummary])] {
        tree.folderGroups
    }

    private func inlineChildProjects(for project: RemoteProjectSummary) -> [RemoteProjectSummary] {
        tree.childProjects(for: project).filter {
            $0.isGroup == true || store.worktreesEnabled
        }
    }

    private func sessionCount(
        for project: RemoteProjectSummary,
        includeChildren: Bool,
        visited: Set<String> = []
    ) -> Int {
        guard !visited.contains(project.id) else { return 0 }
        let direct = sessions(for: project).count
        guard includeChildren else { return direct }
        let nextVisited = visited.union([project.id])
        return direct + inlineChildProjects(for: project).reduce(0) { total, child in
            total + sessionCount(for: child, includeChildren: true, visited: nextVisited)
        }
    }

    private func folderActivity(
        _ projects: [RemoteProjectSummary]
    ) -> (busy: Bool, attention: Bool) {
        let visibleProjects = projects.flatMap { [$0] + inlineChildProjects(for: $0) }
        let visibleSessions = visibleProjects.flatMap { sessions(for: $0) }
        return (
            visibleSessions.contains { $0.activity == .working },
            visibleSessions.contains { $0.activity == .blocked }
        )
    }

    private func sessions(for project: RemoteProjectSummary) -> [RemoteSessionSummary] {
        tree.sessions(for: project)
    }

    private func pinnedSessions(for project: RemoteProjectSummary) -> [RemoteSessionSummary] {
        sessions(for: project).filter(\.pinned)
    }

    private func regularSessions(for project: RemoteProjectSummary) -> [RemoteSessionSummary] {
        sessions(for: project).filter { !$0.pinned }
    }

    /// The desktop sidebar model, mirrored: running sessions always render,
    /// then at most 5 recent stopped/archived sessions; selected/unread/
    /// working/blocked sessions always stay. A new Mac already sends the list partitioned and
    /// windowed, so this is a stable no-op there; against an older Mac
    /// (full interleaved list) it applies the same model phone-side.
    private func displayedRegularSessions(for project: RemoteProjectSummary) -> [RemoteSessionSummary] {
        let regular = regularSessions(for: project)
        let active = regular.filter { $0.status == .running }
        let stopped = regular.filter { $0.status != .running }
        return active + stopped.enumerated().compactMap { index, session in
            if index < Self.visibleSessionLimit
                || session.id == store.selectedSessionID || session.unread
                || session.activity == .working || session.activity == .blocked {
                return session
            }
            return nil
        }
    }

    private func revealSelectedSession(using proxy: ScrollViewProxy, animated: Bool) {
        guard let selected = store.selectedSession else { return }
        store.revealProject(selected.projectID)
        if animated {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.12) {
                withAnimation(.easeOut(duration: 0.22)) {
                    proxy.scrollTo(selected.id, anchor: .center)
                }
            }
        } else {
            // Next runloop tick: the expansion above has to land in the tree
            // before the target row exists to scroll to.
            DispatchQueue.main.async {
                proxy.scrollTo(selected.id, anchor: .center)
            }
        }
    }

}

/// Sidebar-shaped index of a bootstrap snapshot. Everything is precomputed in
/// the initializer — the sidebar reads these per row per render pass, and the
/// previous lazy filters were O(projects × sessions) each time.
struct IOSSidebarProjectTree {
    let looseProjects: [RemoteProjectSummary]
    let folderGroups: [(folder: RemoteProjectFolderSummary, projects: [RemoteProjectSummary])]
    private let childrenByParent: [String: [RemoteProjectSummary]]
    private let sessionsByProject: [String: [RemoteSessionSummary]]
    private let projectsByID: [String: RemoteProjectSummary]

    init(snapshot: RemoteBootstrapSnapshot) {
        let folderIDs = Set(snapshot.folders.map(\.id))

        func isMainTreeProject(
            _ project: RemoteProjectSummary,
            acceptingLegacyFolderParentID legacyFolderID: String? = nil
        ) -> Bool {
            guard !project.isInlineSidebarFolder else { return false }
            guard let parentID = project.parentProjectID else { return true }
            if parentID == legacyFolderID { return true }
            return folderIDs.contains(parentID)
        }

        looseProjects = snapshot.projects.filter { project in
            project.folderID == nil && isMainTreeProject(project)
        }
        folderGroups = snapshot.folders.map { folder in
            let projects = snapshot.projects.filter { project in
                project.folderID == folder.id
                    && isMainTreeProject(project, acceptingLegacyFolderParentID: folder.id)
            }
            return (folder, projects)
        }
        .filter { !$0.projects.isEmpty }
        childrenByParent = Dictionary(
            grouping: snapshot.projects.filter {
                $0.isInlineSidebarFolder
            },
            by: { $0.parentProjectID ?? "" }
        )
        sessionsByProject = Dictionary(
            grouping: snapshot.sessions.filter(\.supportsIOSSessionAPI),
            by: \.projectID
        )
        .mapValues { Self.treeOrdered($0) }
        projectsByID = Dictionary(uniqueKeysWithValues: snapshot.projects.map { ($0.id, $0) })
    }

    /// Current Macs already ship sessions in exact desktop order. Keep that
    /// order unchanged; session hierarchy no longer exists.
    private static func treeOrdered(
        _ sessions: [RemoteSessionSummary]
    ) -> [RemoteSessionSummary] {
        sessions
    }

    func worktreeProjects(for project: RemoteProjectSummary) -> [RemoteProjectSummary] {
        childProjects(for: project).filter { $0.worktreeBranch != nil }
    }

    func childProjects(for project: RemoteProjectSummary) -> [RemoteProjectSummary] {
        childrenByParent[project.id] ?? []
    }

    func sessions(for project: RemoteProjectSummary) -> [RemoteSessionSummary] {
        sessionsByProject[project.id] ?? []
    }

    func revealIDs(forProjectID projectID: String) -> Set<String> {
        var ids: Set<String> = [projectID]
        if let project = projectsByID[projectID],
           project.isInlineSidebarFolder,
           let parentID = project.parentProjectID {
            ids.insert(parentID)
        }
        return ids
    }
}

private extension RemoteProjectSummary {
    var isInlineSidebarFolder: Bool {
        parentProjectID != nil && (isGroup == true || worktreeBranch != nil)
    }
}

enum IOSSidebarTheme {
    static let foreground = Color(hex: 0xF3F5FB)
    static let mutedForeground = Color(hex: 0xF3F5FB, opacity: 0.66)
    static let hoverRow = Color(hex: 0xF3F5FB, opacity: 0.10)
    static let activeRow = Color(hex: 0xFFFFFF, opacity: 0.16)
    static let attention = Color(hex: 0xF59E0B)
    static let unread = Color(hex: 0x60A5FA)
    static let background = LinearGradient(
        colors: [
            Color(hex: 0x2B2E37),
            Color(hex: 0x1A1A1F),
        ],
        startPoint: .bottom,
        endPoint: .top
    )

    static func toolColor(for command: String) -> Color {
        if command.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return Color(hex: 0xD6D9E1)
        }
        if let hex = UnpeelRuntimeCatalog.runtime(command: command)?.tintColorHex {
            return Color(hex: UInt32(hex))
        }
        return Color(hex: 0xB9BDC9)
    }

    static func toolSpinnerColor(for command: String) -> Color {
        if let runtime = UnpeelRuntimeCatalog.runtime(command: command),
           let hex = runtime.spinnerTintColorHex ?? runtime.tintColorHex {
            return Color(hex: UInt32(hex))
        }
        return toolColor(for: command)
    }

    /// Mac-resolved spinner tint when the summary carries one (the single
    /// source of truth — new CLIs get their color with no phone update);
    /// the legacy command-prefix table above is the fallback for older Macs.
    static func toolSpinnerColor(for session: RemoteSessionSummary) -> Color {
        if let hex = session.spinnerColorHex { return Color(hex: UInt32(hex)) }
        return toolSpinnerColor(for: session.command)
    }

    /// Same Mac-first resolution for preset tints (drawer rows).
    static func toolColor(for preset: RemotePresetSummary) -> Color {
        if let hex = preset.tintColorHex { return Color(hex: UInt32(hex)) }
        return toolColor(for: preset.command)
    }

    /// Same dark-mode folder palette as the Mac app and TUI. A missing color
    /// stays neutral; only explicitly colored folders pick up a tint.
    static func folderColor(for id: String?) -> Color? {
        switch id {
        case "sky": return Color(hex: 0x7DD3FC)
        case "blue": return Color(hex: 0x7EA6FF)
        case "violet": return Color(hex: 0xB79CFF)
        case "rose": return Color(hex: 0xF79AC0)
        case "amber": return Color(hex: 0xF8C86A)
        case "moss": return Color(hex: 0x9DD67A)
        case "teal": return Color(hex: 0x64DCCB)
        case "graphite": return Color(hex: 0xB8BCC8)
        default: return nil
        }
    }
}

private struct SidebarListFadeMask: View {
    var body: some View {
        VStack(spacing: 0) {
            LinearGradient(
                stops: [
                    .init(color: .clear, location: 0),
                    .init(color: .black.opacity(0.35), location: 0.32),
                    .init(color: .black, location: 1),
                ],
                startPoint: .top,
                endPoint: .bottom
            )
            .frame(height: 28)
            Color.black
            LinearGradient(
                stops: [
                    .init(color: .black, location: 0),
                    .init(color: .black.opacity(0.65), location: 0.48),
                    .init(color: .clear, location: 1),
                ],
                startPoint: .top,
                endPoint: .bottom
            )
            .frame(height: 34)
        }
    }
}

private struct IOSSidebarTopGlassVeil: View {
    var height: CGFloat = 58

    var body: some View {
        Rectangle()
            .fill(.ultraThinMaterial)
            .overlay(
                LinearGradient(
                    stops: [
                        .init(color: Color(hex: 0x2B2E37, opacity: 0.88), location: 0),
                        .init(color: Color(hex: 0x2B2E37, opacity: 0.48), location: 0.48),
                        .init(color: .clear, location: 1),
                    ],
                    startPoint: .top,
                    endPoint: .bottom
                )
            )
            .mask(
                LinearGradient(
                    stops: [
                        .init(color: .black, location: 0),
                        .init(color: .black.opacity(0.88), location: 0.44),
                        .init(color: .black.opacity(0.34), location: 0.76),
                        .init(color: .clear, location: 1),
                    ],
                    startPoint: .top,
                    endPoint: .bottom
                )
            )
            .frame(height: height)
    }
}

private struct ConnectionStatusRow: View {
    var store: RemotePreviewStore
    /// Number of paired Macs; more than one shows the switcher hint.
    var switchableMacCount: Int = 1

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: store.isDisconnected ? "wifi.slash" : "dot.radiowaves.left.and.right")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(store.isDisconnected ? IOSSidebarTheme.attention : IOSSidebarTheme.foreground)
                .frame(width: 16, height: 16)
            VStack(alignment: .leading, spacing: 1) {
                Text(store.connectionStatus)
                    .font(.system(size: 13, weight: .medium))
                    .lineLimit(1)
                if let lastError = store.lastError {
                    Text(lastError)
                        .font(.system(size: 10))
                        .foregroundStyle(IOSSidebarTheme.mutedForeground)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 0)
            // More than one paired Mac: hint that tapping opens a switcher.
            if switchableMacCount > 1 {
                Image(systemName: "chevron.up.chevron.down")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(IOSSidebarTheme.mutedForeground)
            }
            // The session count is meaningless without a connection.
            if !store.isDisconnected {
                Text("\(store.snapshot.sessions.filter(\.supportsIOSSessionAPI).count)")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(IOSSidebarTheme.mutedForeground)
                    .padding(.horizontal, 6)
                    .frame(height: 18)
                    .background(IOSSidebarTheme.hoverRow, in: Capsule())
            }
        }
        .foregroundStyle(IOSSidebarTheme.foreground)
        .padding(EdgeInsets(top: 5, leading: 9, bottom: 5, trailing: 9))
        .background(
            RoundedRectangle(cornerRadius: 9, style: .continuous)
                .fill(IOSSidebarTheme.hoverRow)
        )
    }
}

/// Sticky under the sidebar's top veil whenever phone-side push delivery is
/// broken (permission denied or APNs registration failed). Opaque-ish so
/// rows scrolling underneath don't fight the label.
private struct SidebarPushWarningBanner: View {
    let message: String
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            HStack(spacing: 7) {
                Image(systemName: "bell.slash.fill")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(IOSSidebarTheme.attention)
                    .frame(width: 16, height: 16)
                Text(message)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(IOSSidebarTheme.foreground)
                    .lineLimit(1)
                Spacer(minLength: 4)
                Image(systemName: "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(IOSSidebarTheme.mutedForeground)
            }
            .padding(EdgeInsets(top: 5, leading: 9, bottom: 5, trailing: 9))
            .background(
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(.ultraThinMaterial)
                    .overlay(
                        RoundedRectangle(cornerRadius: 9, style: .continuous)
                            .fill(IOSSidebarTheme.attention.opacity(0.16))
                    )
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(message). Opens notification settings.")
    }
}

/// Replaces the session list while the Mac is unreachable: an honest "lost
/// connection" state instead of stale rows that can't be tapped into.
/// Reconnection retries the saved Direct endpoint and the E2E Relay, so this
/// only informs — the pairing sheet stays one tap away via the status row.
private struct SidebarDisconnectedView: View {
    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: "wifi.slash")
                .font(.system(size: 26, weight: .medium))
                .foregroundStyle(IOSSidebarTheme.mutedForeground)
            Text("Connection lost")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(IOSSidebarTheme.foreground)
            Text("Reconnecting automatically. Your sessions will reappear once your Mac is reachable.")
                .font(.system(size: 12))
                .foregroundStyle(IOSSidebarTheme.mutedForeground)
                .multilineTextAlignment(.center)
            ProgressView()
                .controlSize(.small)
                .tint(IOSSidebarTheme.mutedForeground)
                .padding(.top, 2)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 22)
        .padding(.vertical, 36)
    }
}

private struct SidebarSectionTitle: View {
    let title: String

    init(_ title: String) {
        self.title = title
    }

    var body: some View {
            Text(title)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(IOSSidebarTheme.mutedForeground)
            .textCase(.uppercase)
            .padding(EdgeInsets(top: 10, leading: 9, bottom: 3, trailing: 9))
    }
}

/// Folder accordion header: tap to open this folder (closing the previous
/// one). Collapsed headers keep the fleet glanceable with aggregate
/// busy/attention dots and a session count.
private struct FolderSectionRow: View {
    let folder: RemoteProjectFolderSummary
    let isExpanded: Bool
    let sessionCount: Int
    let activity: (busy: Bool, attention: Bool)
    let onToggle: () -> Void

    private var folderTint: Color {
        IOSSidebarTheme.folderColor(for: folder.colorID) ?? IOSSidebarTheme.mutedForeground
    }

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "chevron.right")
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(folderTint)
                .rotationEffect(.degrees(isExpanded ? 90 : 0))
                .frame(width: 12)

            Text(folder.name)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(IOSSidebarTheme.foreground)
                .textCase(.uppercase)
                .lineLimit(1)

            // Attention only — no green "busy" dot; the desktop shows no
            // aggregate busy marker on folder rows either.
            if activity.attention {
                Circle()
                    .fill(IOSSidebarTheme.attention)
                    .frame(width: 6, height: 6)
            }

            Spacer(minLength: 4)

            if !isExpanded && sessionCount > 0 {
                Text("\(sessionCount)")
                    .font(.system(size: 11))
                    .foregroundStyle(IOSSidebarTheme.mutedForeground)
                    .padding(.horizontal, 6)
                    .frame(height: 18)
                    .background(Capsule().fill(IOSSidebarTheme.hoverRow))
            }
        }
        .padding(EdgeInsets(top: 8, leading: 6, bottom: 5, trailing: 9))
        .frame(minHeight: 30)
        .contentShape(Rectangle())
        .onTapGesture(perform: onToggle)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel("\(folder.name), \(isExpanded ? "expanded" : "collapsed")")
    }
}

private struct MacStyleProjectRow: View {
    let project: RemoteProjectSummary
    let depth: Int
    let isExpanded: Bool
    let hasBusySession: Bool
    let hasAttentionSession: Bool
    let sessionCount: Int
    let canCreateSession: Bool
    let onAdd: () -> Void
    /// Long-press: the folder organize sheet (rename/sort/color/archive) —
    /// the phone's stand-in for the desktop project context menu.
    var onOrganize: (() -> Void)? = nil
    let onToggle: () -> Void

    private var isChildFolder: Bool { project.isInlineSidebarFolder }
    private var folderTint: Color {
        IOSSidebarTheme.folderColor(for: project.colorID) ?? IOSSidebarTheme.mutedForeground
    }

    var body: some View {
        HStack(spacing: 7) {
            Group {
                if isChildFolder {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(folderTint)
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))
                        .animation(.easeInOut(duration: 0.15), value: isExpanded)
                } else {
                    SharedChromeIconView(
                        icon: isExpanded ? .folderOpen : .folderClosed,
                        size: 18
                    )
                    .foregroundStyle(folderTint)
                }
            }
                .frame(width: isChildFolder ? 12 : 22, height: 22)
                .overlay(alignment: .topTrailing) {
                    // Attention only — the green "busy" dot was an old
                    // design; the desktop has no busy marker on project rows.
                    if hasAttentionSession {
                        Circle()
                            .fill(IOSSidebarTheme.attention)
                            .frame(width: 6, height: 6)
                            .background(
                                Circle()
                                    .fill(IOSSidebarTheme.attention.opacity(0.20))
                                    .frame(width: 14, height: 14)
                            )
                            .padding(.trailing, -1)
                    }
                }

            Text(project.name)
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(IOSSidebarTheme.foreground.opacity(0.62))
                .lineLimit(1)
                .truncationMode(.tail)

            if let branch = project.worktreeBranch {
                HStack(spacing: 3) {
                    SharedChromeIconView(icon: .branch, size: 12)
                    if branch != project.name {
                        Text(branch)
                            .font(.system(size: 10, design: .monospaced))
                            .lineLimit(1)
                            .truncationMode(.tail)
                            .frame(maxWidth: 104, alignment: .leading)
                    }
                }
                .foregroundStyle(IOSSidebarTheme.mutedForeground.opacity(0.55))
            }

            Spacer(minLength: 4)

            if !isExpanded, sessionCount > 0 {
                Text("\(sessionCount)")
                    .font(.system(size: 11))
                    .foregroundStyle(IOSSidebarTheme.mutedForeground)
                    .frame(minHeight: 18)
                    .padding(.horizontal, 6)
                    .background(Capsule().fill(IOSSidebarTheme.hoverRow))
            }

            if canCreateSession {
                Button(action: onAdd) {
                    SharedChromeIconView(icon: .plus, size: 17)
                        .foregroundStyle(IOSSidebarTheme.mutedForeground)
                        .frame(width: 28, height: 28)
                        .background(IOSSidebarTheme.hoverRow.opacity(0.44), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("New session")
            }
        }
        .padding(EdgeInsets(
            top: 2,
            leading: isChildFolder
                // 18 + chevron 12 + spacing 7 = the 37pt text column used
                // by a normal session under this parent project.
                ? 18 + CGFloat(max(0, depth - 1)) * 14
                : 7 + CGFloat(depth) * 14,
            bottom: 2,
            trailing: 7
        ))
        .frame(minHeight: 32)
        .contentShape(Rectangle())
        .onTapGesture(perform: onToggle)
        .onLongPressGesture {
            onOrganize?()
        }
        .accessibilityHint(onOrganize == nil ? "" : "Hold to rename, sort, or color this folder")
    }
}

private extension View {
    /// Selected session-row background: real Liquid Glass on iOS 26 (matching
    /// the desktop app's selected row), the flat `activeRow` wash otherwise.
    /// Unselected rows get no background.
    @ViewBuilder
    func selectedRowBackground(_ selected: Bool) -> some View {
        if #available(iOS 26.0, *), selected {
            glassEffect(.regular, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
        } else {
            background(
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(selected ? IOSSidebarTheme.activeRow : .clear)
            )
        }
    }
}

private struct MacStyleSessionRow: View {
    let session: RemoteSessionSummary
    let project: RemoteProjectSummary?
    let selected: Bool
    var depth = 0
    /// Long-press ("deep press") the row to open the rename/pin sheet for this
    /// session — the same sheet a long-press on the terminal title opens.
    var onOrganize: (() -> Void)? = nil
    /// Trailing (tap) action — kept last so existing trailing-closure call
    /// sites bind to it, not `onOrganize`.
    let onSelect: () -> Void

    var body: some View {
        rowContent
            .onLongPressGesture {
                onOrganize?()
            }
            .accessibilityLabel("\(session.title), \(project?.name ?? "No project")")
            .accessibilityHint(onOrganize == nil ? "" : "Hold to rename or pin this session")
    }

    private var rowContent: some View {
        HStack(spacing: 8) {
            leadingSlot
                .frame(width: 18, height: 18)

            Text(session.title)
                .font(.system(size: 14, weight: .regular))
                .foregroundStyle(session.status == .exited ? IOSSidebarTheme.mutedForeground : IOSSidebarTheme.foreground)
                .lineLimit(1)
                .truncationMode(.tail)

            Spacer(minLength: 4)

            HStack(spacing: 4) {
                // Notify-when-done is on for this session (same flag the
                // organize sheet toggles) — mirrors the pin treatment.
                if session.notifyWhenDone {
                    SharedChromeIconView(icon: .bell, size: 11)
                        .foregroundStyle(IOSSidebarTheme.mutedForeground)
                }
                if session.pinned {
                    SharedChromeIconView(icon: .pin, size: 11)
                        .foregroundStyle(IOSSidebarTheme.mutedForeground)
                }
                // TimelineView keeps the relative age ticking: with the poll
                // equality gate in place, an idle fleet never re-renders, so
                // a plain Date()-at-render label froze at "now"/"5m" forever.
                TimelineView(.periodic(from: .now, by: 60)) { context in
                    Text(meta(at: context.date))
                        .font(.system(size: 10, weight: selected ? .medium : .regular))
                        .foregroundStyle(IOSSidebarTheme.mutedForeground)
                        .frame(minWidth: 24, alignment: .trailing)
                }

                // Agent-CLI mark right of the date, matching the desktop
                // sidebar's SessionCommandIcon (12pt in a fixed 14×14 slot).
                SharedToolIconView(
                    providerID: session.presentationProviderID,
                    command: session.command,
                    size: 12
                )
                    .opacity(0.82)
                    .frame(width: 14, height: 14)
                    .padding(.leading, 1)
            }
        }
        .opacity(session.status == .exited ? 0.82 : 1)
        // Align the session LABEL with the parent project/folder NAME: indent
        // at the parent's depth (depth-1) and offset by (projectLeading 7 +
        // folderIconFrame 22 − sessionSlotFrame 18) = 11, so the text columns
        // line up rather than the session sitting one level deeper.
        .padding(EdgeInsets(
            top: 2,
            leading: 11 + CGFloat(max(depth - 1, 0)) * 14,
            bottom: 2,
            trailing: 9
        ))
        .frame(minHeight: 32)
        .selectedRowBackground(selected)
        .contentShape(Rectangle())
        .onTapGesture(perform: onSelect)
    }

    // All status indicators share this one leading column so they line up
    // vertically across rows: working → spinner, blocked → attention dot,
    // settled-unread → the blue "unread" dot (previously rendered after the
    // title, which broke the column). Precedence: work > attention > unread.
    @ViewBuilder
    private var leadingSlot: some View {
        if session.status == .running && session.activity == .working {
            TitlebarBrailleSpinner(color: IOSSidebarTheme.toolSpinnerColor(for: session))
                .scaleEffect(0.82)
        } else if session.activity == .blocked {
            Circle()
                .fill(IOSSidebarTheme.attention)
                .frame(width: 6, height: 6)
                .background(
                    Circle()
                        .fill(IOSSidebarTheme.attention.opacity(0.20))
                        .frame(width: 14, height: 14)
                )
        } else if session.unread {
            Circle()
                .fill(IOSSidebarTheme.unread)
                .frame(width: 7, height: 7)
        } else {
            Color.clear
        }
    }

    private func meta(at date: Date) -> String {
        if session.activity == .blocked { return "blocked" }
        return RelativeAge.shortString(
            fromUnixMs: session.updatedAtUnixMs ?? session.createdAtUnixMs,
            at: date
        )
    }
}


enum RelativeAge {
    static func shortString(fromUnixMs unixMs: Int64, at date: Date = Date()) -> String {
        let age = max(0, date.timeIntervalSince1970 - TimeInterval(unixMs) / 1000)
        if age < 60 { return "now" }
        if age < 3600 { return "\(Int(age / 60))m" }
        if age < 86_400 { return "\(Int(age / 3600))h" }
        return "\(Int(age / 86_400))d"
    }
}

extension Color {
    init(hex: UInt32, opacity: Double = 1) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: opacity
        )
    }
}

struct ActivityPill: View {
    let state: RemoteActivityState
    let status: RemoteSessionStatus

    var body: some View {
        Text(label)
            .font(.caption2.weight(.semibold))
            .foregroundStyle(color)
            .padding(.horizontal, 7)
            .padding(.vertical, 4)
            .background(color.opacity(0.12), in: Capsule())
    }

    private var label: String {
        if status == .exited { return "Done" }
        switch state {
        case .blocked: return "Blocked"
        case .working: return "Working"
        case .done: return "Done"
        case .starting: return "Starting"
        case .idle: return "Idle"
        case .unknown: return "Start"
        }
    }

    private var color: Color {
        if status == .exited { return .secondary }
        switch state {
        case .blocked: return .orange
        case .working: return .blue
        case .done: return .green
        case .starting: return .purple
        case .idle: return .secondary
        case .unknown: return .gray
        }
    }
}
