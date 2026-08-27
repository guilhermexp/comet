import Foundation
import Combine
import GhosttyTerminal
import PhotosUI
import SwiftUI
import UnpeelShared
#if os(iOS)
import UIKit
#endif

struct RemoteTerminalCanvasLayout {
    static func canvasSize(
        columns: Int,
        rows: Int,
        cellSize: CGSize,
        horizontalPadding: CGFloat,
        verticalPadding: CGFloat
    ) -> CGSize {
        CGSize(
            width: CGFloat(max(columns, 1)) * cellSize.width + horizontalPadding * 2,
            height: CGFloat(max(rows, 1)) * cellSize.height + verticalPadding * 2
        )
    }

    static func visibleViewport(in viewport: CGSize) -> CGSize {
        CGSize(width: viewport.width, height: viewport.height)
    }

    static func baseScale(in viewport: CGSize, canvasSize: CGSize) -> CGFloat {
        guard viewport.width > 0, viewport.height > 0, canvasSize.width > 0, canvasSize.height > 0 else { return 1 }
        return 1
    }

    /// Fit-to-screen scale: snap the canvas to exactly the viewport width so
    /// there is zero horizontal pan (the canvas runs a few points over the
    /// viewport from padding + alignment slop). Only applies once the canvas
    /// is already phone-shaped — mid-transition it can still be desktop-wide,
    /// and shrinking that to fit would flash unreadably tiny text.
    static func fitWidthScale(in viewport: CGSize, canvasSize: CGSize) -> CGFloat {
        guard viewport.width > 0, canvasSize.width > 0 else { return 1 }
        let scale = viewport.width / canvasSize.width
        guard scale > 0.9, scale < 1.1 else { return 1 }
        return scale
    }

    /// `bottomSlack` is the dead region at the canvas bottom (alignment
    /// slop + grid-alignment extra) in unscaled canvas points: the canvas
    /// frame runs that much past the last content row, and anchoring the
    /// RAW canvas bottom to the viewport pushed the content — and its top
    /// rows — up under the title chrome by exactly that amount. The
    /// resting pan shifts the canvas down so the CONTENT bottom sits on
    /// the viewport bottom; the blank slack hangs invisibly below.
    static func defaultPan(
        in viewport: CGSize,
        scale: CGFloat,
        canvasSize: CGSize,
        bottomSlack: CGFloat = 0
    ) -> CGSize {
        let scaled = scaledCanvasSize(scale: scale, canvasSize: canvasSize)
        return CGSize(
            width: (scaled.width - viewport.width) / 2,
            height: (viewport.height - scaled.height) / 2 + max(bottomSlack, 0) * scale
        )
    }

    static func panBounds(
        in viewport: CGSize,
        scale: CGFloat,
        canvasSize: CGSize,
        bottomSlack: CGFloat = 0
    ) -> CGSize {
        let scaled = scaledCanvasSize(scale: scale, canvasSize: canvasSize)
        let defaultPan = defaultPan(
            in: viewport, scale: scale, canvasSize: canvasSize, bottomSlack: bottomSlack
        )
        return CGSize(
            width: max(abs(defaultPan.width), max(0, (scaled.width - viewport.width) / 2)),
            height: max(abs(defaultPan.height), max(0, (scaled.height - viewport.height) / 2))
        )
    }

    static func scaledCanvasSize(scale: CGFloat, canvasSize: CGSize) -> CGSize {
        CGSize(width: canvasSize.width * scale, height: canvasSize.height * scale)
    }

    static func clamped(
        _ pan: CGSize,
        in viewport: CGSize,
        scale: CGFloat,
        canvasSize: CGSize,
        bottomSlack: CGFloat = 0
    ) -> CGSize {
        let bounds = panBounds(
            in: viewport, scale: scale, canvasSize: canvasSize, bottomSlack: bottomSlack
        )
        return CGSize(
            width: min(max(pan.width, -bounds.width), bounds.width),
            height: min(max(pan.height, -bounds.height), bounds.height)
        )
    }

    static func clampedOffset(
        _ pan: CGSize,
        in viewport: CGSize,
        scale: CGFloat,
        canvasSize: CGSize,
        bottomSlack: CGFloat = 0
    ) -> CGSize {
        let defaultPan = defaultPan(
            in: viewport, scale: scale, canvasSize: canvasSize, bottomSlack: bottomSlack
        )
        let absolute = clamped(
            CGSize(width: pan.width + defaultPan.width, height: pan.height + defaultPan.height),
            in: viewport,
            scale: scale,
            canvasSize: canvasSize,
            bottomSlack: bottomSlack
        )
        return CGSize(width: absolute.width - defaultPan.width, height: absolute.height - defaultPan.height)
    }

    static func terminalFrame(
        viewport: CGSize,
        canvasSize: CGSize,
        scale: CGFloat,
        pan: CGSize
    ) -> CGRect {
        let scaled = scaledCanvasSize(scale: scale, canvasSize: canvasSize)
        let center = CGPoint(
            x: viewport.width / 2 + pan.width,
            y: viewport.height / 2 + pan.height
        )
        return CGRect(
            x: center.x - scaled.width / 2,
            y: center.y - scaled.height / 2,
            width: scaled.width,
            height: scaled.height
        )
    }
}

/// Publishes the software keyboard's on-screen height. The surface ignores
/// the keyboard safe area (avoidance would shove the whole terminal up), and
/// shrinks its visible viewport by this instead — the terminal stays put and
/// the live tail rides just above the keyboard.
@MainActor
private final class KeyboardHeightObserver: ObservableObject {
    @Published private(set) var height: CGFloat = 0
    /// Written once in init (main), read in deinit — safe without isolation.
    private nonisolated(unsafe) var observers: [NSObjectProtocol] = []
    private var pendingHeight: CGFloat?
    private var settleTask: Task<Void, Never>?

    #if os(iOS)
    init() {
        // A single focus produces a burst of frame notifications (accessory
        // bar attaching, predictive row, final keyboard frame), each with a
        // different height — reacting to every one made the canvas bounce.
        // Debounce the burst and publish ONE settled, animated change.
        let center = NotificationCenter.default
        observers.append(center.addObserver(
            forName: UIResponder.keyboardWillShowNotification,
            object: nil,
            queue: .main
        ) { [weak self] note in
            // Extract before the actor hop: Notification is not Sendable.
            let frame = note.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? CGRect
            MainActor.assumeIsolated {
                guard let self, let frame else { return }
                let bounds = UIScreen.main.bounds
                self.settle(to: max(0, bounds.maxY - frame.minY))
            }
        })
        observers.append(center.addObserver(
            forName: UIResponder.keyboardWillHideNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.settle(to: 0)
            }
        })
    }
    #endif

    private func settle(to next: CGFloat) {
        pendingHeight = next
        settleTask?.cancel()
        settleTask = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: 120_000_000)
            guard let self, !Task.isCancelled, let target = self.pendingHeight else { return }
            self.pendingHeight = nil
            guard self.height != target else { return }
            withAnimation(.easeOut(duration: 0.25)) {
                self.height = target
            }
        }
    }

    deinit {
        for observer in observers {
            NotificationCenter.default.removeObserver(observer)
        }
        settleTask?.cancel()
    }
}

/// The one phone terminal size (was "Small" of a three-way picker; Tommy
/// chose a single size, 2026-07-02). Denser than the desktop's 13pt so more
/// content fits; the auto-fit column math scales from it via cell metrics.
let phoneTerminalFontSize: CGFloat = 11

struct RemoteGhosttyTerminalSurface: View {
    /// Bottom space reserved for the floating menu control bar so the menu's
    /// own options aren't hidden behind it (bar circle + gaps above/below).
    private static let menuControlBarReservedHeight: CGFloat = 66

    let session: RemoteSessionSummary
    let topContentInset: CGFloat
    let landscapeTopContentInset: CGFloat
    let bottomContentInset: CGFloat
    /// True while an overlay (drawer, sheet) owns the screen: the terminal
    /// unfocuses so the keyboard + accessory bar get out of the way. Driving
    /// the focus STATE (not the responder chain) avoids the race where the
    /// next view update re-summons the keyboard before an async resign
    /// lands.
    var suppressFocus: Bool = false
    /// Renderer + platform view are cache-owned (`TerminalSessionCache`),
    /// not view-owned: switching back to a recently viewed session reuses
    /// the live terminal (held offset, painted surface) instead of paying a
    /// rebuild + grid fit + full tail replay behind the skeleton.
    private let cacheEntry: TerminalSessionCacheEntry
    /// Owns `topBarSheet` (to open the browser gallery) and the composer
    /// attach hook (so the gallery can apply a screenshot to the message).
    /// Optional so previews/tests can build the surface bare.
    private let store: RemotePreviewStore?
    @ObservedObject private var renderer: RemoteGhosttyRenderer
    @StateObject private var keyboard = KeyboardHeightObserver()
    @Environment(\.scenePhase) private var scenePhase

    /// The terminal should stream (reconnect + replay) only when the app is
    /// active AND not covered by the app-lock cover — see the `onChange` note.
    /// `AppLockManager` is `@Observable`, so reading `isLocked` here tracks it.
    private var shouldStream: Bool {
        scenePhase == .active && !AppLockManager.shared.isLocked
    }
    /// Starts UNfocused: opening or switching sessions is for reading —
    /// the keyboard comes up when the user taps the terminal.
    @State private var terminalFocused = false
    @State private var canvasPan: CGSize = .zero
    @State private var photoPickerPresented = false
    @State private var selectedPhotoItem: PhotosPickerItem?
    /// False when the TUI's input line already sits above the keyboard —
    /// then the terminal stays put instead of shifting up (fresh sessions
    /// draw their composer near the top). Re-evaluated on keyboard events
    /// and while typing.
    @State private var keyboardAvoidanceNeeded = true
    /// Latch: the on-screen keyboard was summoned by the terminal. Gates
    /// the keyboard inset instead of `terminalFocused` directly — focus
    /// flips synchronously at responder resign, and zeroing the inset in
    /// that same frame snapped the canvas down while the keyboard was
    /// still sliding away. The latch holds until the keyboard actually
    /// hides, so the inset decays with the animated `keyboard.height` and
    /// the canvas rides the keyboard's own slide in both directions.
    @State private var keyboardOwnsInset = false
    /// Grid-sizing viewport captured while no keyboard is on screen. While
    /// one is up the sizing viewport reuses it WHOLESALE (both axes), so a
    /// focus change can never reach the renderer as a viewport change — in
    /// fit mode that becomes a desktop PTY resize (TUI reflow + replay
    /// flash) if the keyboard leaks into geometry or safe-area insets
    /// anywhere. Width matters as much as height: a sub-point width wobble
    /// on focus flips the column count and rewraps the whole TUI.
    @State private var restingSizingSize: CGSize = .zero
    @StateObject private var voice = VoiceDictationController()
    /// Pulses the gallery button briefly when a new browser screenshot lands.
    @State private var galleryPulse = false

    init(
        session: RemoteSessionSummary,
        client: RemoteMacClient = RemoteMacClient(),
        connectionEpoch: Int = 0,
        topContentInset: CGFloat = 0,
        landscapeTopContentInset: CGFloat? = nil,
        bottomContentInset: CGFloat = 0,
        suppressFocus: Bool = false,
        store: RemotePreviewStore? = nil
    ) {
        self.session = session
        self.topContentInset = topContentInset
        self.landscapeTopContentInset = landscapeTopContentInset ?? topContentInset
        self.bottomContentInset = bottomContentInset
        self.suppressFocus = suppressFocus
        self.store = store
        let entry = TerminalSessionCache.shared.entry(
            for: session,
            client: client,
            epoch: connectionEpoch
        )
        cacheEntry = entry
        _renderer = ObservedObject(wrappedValue: entry.renderer)
    }

    /// The chrome color behind + around the terminal: the provider TUI's
    /// resolved background (opencode/grok, sent by the Mac) so the title-bar
    /// area and letterbox match the terminal, else the default.
    private var chromeBackground: Color {
        session.terminalBackgroundHex
            .map { Color(hex: UInt32(truncatingIfNeeded: $0)) }
            ?? TerminalChrome.background
    }

    private var connectionNotice: TerminalConnectionNotice? {
        if let store, store.isDisconnected {
            return TerminalConnectionNotice(
                title: "Reconnecting to Mac",
                message: store.lastError ?? "Looking for your Mac...",
                symbolName: "wifi.slash",
                tone: .reconnecting
            )
        }

        guard let message = renderer.lastError else { return nil }
        let retrying = Self.messageLooksRetrying(message)
        return TerminalConnectionNotice(
            title: retrying ? "Reconnecting terminal" : "Terminal notice",
            message: Self.friendlyConnectionMessage(message),
            symbolName: retrying ? "dot.radiowaves.left.and.right" : "exclamationmark.triangle.fill",
            tone: retrying ? .reconnecting : .warning
        )
    }

    var body: some View {
        GeometryReader { geometry in
            let canvasSize = renderer.canvasSize
            let isLandscape = geometry.size.width > geometry.size.height
            let effectiveTopInset = isLandscape ? landscapeTopContentInset : topContentInset
            let topSafePadding: CGFloat = isLandscape ? 54 : 78
            let reservedTop = max(effectiveTopInset, geometry.safeAreaInsets.top + topSafePadding)
            let effectiveBottomInset = isLandscape ? min(bottomContentInset, 14) : bottomContentInset
            let bottomSafePadding: CGFloat = isLandscape ? 6 : 12
            let baseReservedBottom = max(
                effectiveBottomInset,
                geometry.safeAreaInsets.bottom + bottomSafePadding
            )
            // Scale is keyboard-independent (fit-to-screen is width-driven,
            // base scale is 1), so derive it from the keyboard-free rest
            // viewport. That breaks the cycle where the keyboard inset needs
            // the scale (to place the caret on screen) while the scale would
            // otherwise need the inset (via the shrunken viewport).
            let restViewport = visibleViewport(in: geometry.size)
                .subtractingInsets(top: reservedTop, bottom: baseReservedBottom)
            let scale = renderer.desktopFitActive
                ? RemoteTerminalCanvasLayout.fitWidthScale(
                    in: restViewport, canvasSize: canvasSize
                )
                : baseScale(in: restViewport, canvasSize: canvasSize)
            // Keyboard inset counts only while the keyboard is TERMINAL-
            // owned (latched on show while focused, cleared on hide): the
            // observer is app-global, so other keyboards (photo picker's
            // search field, sheets) would otherwise leave a stale inset and
            // a dead gap under the terminal. The magnitude is the MINIMAL
            // lift that clears the caret — not the full keyboard height — so
            // a composer already sitting above the keyboard (cursor-agent
            // parks its caret several rows above its last line) barely moves
            // instead of flying off the top under the title bar. +4 only —
            // the canvas already carries its own bottom padding and slop.
            let keyboardInset = (keyboardOwnsInset && keyboardAvoidanceNeeded)
                ? keyboardAvoidanceInset(
                    keyboardHeight: keyboard.height,
                    screenHeight: geometry.size.height,
                    reservedTop: reservedTop,
                    baseReservedBottom: baseReservedBottom,
                    canvasSize: canvasSize,
                    scale: scale
                )
                : 0
            // Lift the content above the floating menu control bar the same
            // way the keyboard inset lifts it — into the CONTENT reserve only
            // (visibleViewport/pan), never the grid, so no reflow. The bar
            // itself stays anchored to `baseReservedBottom`, so it doesn't ride
            // up with its own reserve.
            let menuBarInset: CGFloat = (renderer.menuPromptActive && !terminalFocused)
                ? Self.menuControlBarReservedHeight
                : 0
            let reservedBottom = max(baseReservedBottom, keyboardInset + 4) + menuBarInset
            let visibleViewport = visibleViewport(in: geometry.size)
                .subtractingInsets(top: reservedTop, bottom: reservedBottom)
            // Grid sizing deliberately IGNORES the keyboard: re-fitting rows
            // on every focus made the desktop PTY resize + reflow + replay —
            // constant blinking. The grid fits the full viewport once; when
            // the keyboard opens, the canvas just repositions locally (the
            // bottom-anchor keeps the prompt above the keyboard). Belt and
            // suspenders: while a keyboard is on screen, the sizing height
            // is FROZEN at its last resting value, so even a keyboard that
            // leaks into geometry.size or safeAreaInsets (ignoresSafeArea
            // has hierarchy-dependent gaps) cannot change the grid.
            let rawSizingViewport = RemoteTerminalCanvasLayout
                .visibleViewport(in: geometry.size)
                .subtractingInsets(top: reservedTop, bottom: baseReservedBottom)
            let sizingViewport = (keyboard.height > 0 && restingSizingSize.width > 0)
                ? restingSizingSize
                : rawSizingViewport
            let defaultPan = defaultPan(in: visibleViewport, scale: scale, canvasSize: canvasSize)
            let clampedPan = clamped(
                CGSize(
                    width: canvasPan.width + defaultPan.width,
                    height: canvasPan.height + defaultPan.height
                ),
                in: visibleViewport,
                scale: scale,
                canvasSize: canvasSize
            )
            let panBounds = panBounds(in: visibleViewport, scale: scale, canvasSize: canvasSize)

            ZStack {
                chromeBackground

                RemoteTerminalSurfaceBridge(
                    entry: cacheEntry,
                    context: renderer.terminal,
                    isFocused: $terminalFocused,
                    onScrolledUpChange: renderer.updateScrolledUp,
                    onTouchScroll: { location, delta, velocity, phase in
                        // While the sidebar is being dragged in, drop the
                        // terminal's own scroll (claim the sequence, do nothing)
                        // so the slide isn't fighting a vertical scroll.
                        if store?.sidebarDragReveal != nil { return true }
                        return renderer.handleTouchScroll(
                            location: location, delta: delta, velocity: velocity, phase: phase
                        )
                    },
                    onCanvasPanDelta: { deltaX in
                        panCanvasHorizontally(
                            by: deltaX,
                            in: visibleViewport,
                            scale: scale,
                            canvasSize: canvasSize
                        )
                    },
                    onTextSelection: { text, anchorRange in
                        store?.presentTerminalTextSelection(
                            text: text,
                            anchorRange: anchorRange
                        )
                    },
                    onHostAction: { id in
                        switch id {
                        case "paste":
                            renderer.pasteClipboard()
                        case "attach-image":
                            photoPickerPresented = true
                        case "voice":
                            voice.toggle { text in
                                renderer.insertTranscribedText(text)
                            }
                        default:
                            break
                        }
                    }
                )
                    .environment(\.colorScheme, .dark)
                    .background(chromeBackground)
                    .frame(width: canvasSize.width, height: canvasSize.height)
                    // Dev toggle (Your Mac ▸ Developer ▸ Show terminal bounds).
                    .border(
                        DevSettings.shared.showTerminalBounds ? Color.red : Color.clear,
                        width: 1.5
                    )
                    .scaleEffect(scale, anchor: .center)
                    .position(
                        x: geometry.size.width / 2 + clampedPan.width,
                        y: reservedTop + visibleViewport.height / 2 + clampedPan.height
                    )

                // Progressive veil under the notch/title bar: canvas content
                // panned above the visible viewport blurs and fades out
                // instead of colliding with the floating title chrome.
                TerminalTopGlassVeil(height: reservedTop + 8, background: chromeBackground)

                // Session-switch skeleton: covers the surface until the first
                // replay paints, so a first open reads as "loading" instead
                // of a flash of empty/partial grid. Cache-hit switches skip
                // it entirely — the reused renderer's `initialReplayDone` is
                // already true and its retained surface still shows the last
                // frame while the stream catches up. Error capsule renders
                // on top.
                if !renderer.initialReplayDone {
                    TerminalSkeletonView(topInset: reservedTop + 6)
                        .transition(.opacity)
                }

                // Top-trailing control cluster. The fit button appears above
                // the gallery ONLY while
                // unfitted — which persists only while the Mac is actively
                // viewing this session (its revert wins) or against an older
                // Mac with no viewing signal. Otherwise the unwatched
                // auto-refit re-asserts the fit and the button never shows.
                VStack(spacing: 8) {
                    if !renderer.desktopFitActive {
                        Button {
                            renderer.toggleDesktopFit()
                        } label: {
                            Image(systemName: "rectangle.arrowtriangle.2.inward")
                                .font(.system(size: 14, weight: .semibold))
                                .rotationEffect(.degrees(90))
                                .frame(width: 40, height: 40)
                        }
                        .foregroundStyle(.primary)
                        .iosCircularGlassControl()
                        .accessibilityLabel("Fit terminal to screen")
                    }

                    Button {
                        galleryPulse = false
                        store?.topBarSheet = .gallery
                    } label: {
                        // Glass-gradient glyph like the sidebar/bell chrome
                        // icons (shared SVG). It opts out of hit testing, so the
                        // label needs its own shape or the button won't tap.
                        SharedChromeIconView(icon: .gallery, size: 19)
                            .frame(width: 40, height: 40)
                            .contentShape(Rectangle())
                    }
                    .iosCircularGlassControl()
                    .scaleEffect(galleryPulse ? 1.08 : 1)
                    .animation(.spring(response: 0.3, dampingFraction: 0.5), value: galleryPulse)
                    .accessibilityLabel("Session gallery")
                }
                .padding(.trailing, 12)
                .padding(.top, reservedTop + 4)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)


                TerminalScrollToBottomButton(
                    visible: renderer.scrollToBottomVisible || renderer.tuiJumpHintVisible,
                    action: {
                        scrollToLiveBottom(in: visibleViewport, scale: scale, canvasSize: canvasSize)
                    }
                )
                .padding(.trailing, 24)
                .padding(.bottom, reservedBottom + 12)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)

                // Menu control bar: appears when an agent-rendered select menu
                // is on screen and the keyboard is down, so a choice can be
                // answered with ↑/↓ · Enter · Esc · number keys — no keyboard.
                if renderer.menuPromptActive, !terminalFocused {
                    TerminalMenuControlBar(
                        optionCount: renderer.menuOptionCount,
                        onKey: { renderer.sendMenuControlKey($0) }
                    )
                    .padding(.horizontal, 10)
                    .padding(.bottom, baseReservedBottom + 12)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                }

                if let connectionNotice {
                    TerminalConnectionNoticeOverlay(notice: connectionNotice)
                        .padding(.horizontal, 18)
                        .padding(.top, reservedTop + 18)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                        .transition(
                            .move(edge: .top)
                                .combined(with: .opacity)
                                .combined(with: .scale(scale: 0.98, anchor: .top))
                        )
                        .zIndex(3)
                }

                // Live dictation pill: transcript grows while recording;
                // Discard drops it, Paste inserts it into the terminal. While
                // the reflection pass polishes the stopped transcript the pill
                // stays up (sparkles + spinner); X aborts the commit.
                if voice.isRecording || voice.isRefining || voice.errorMessage != nil {
                    let hasText = !voice.transcript.isEmpty
                    HStack(spacing: 10) {
                        Image(
                            systemName: voice.isRecording
                                ? "mic.fill"
                                : (voice.isRefining ? "sparkles" : "mic.slash")
                        )
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(
                            voice.isRecording ? .red : (voice.isRefining ? .cyan : .orange)
                        )
                        Text(
                            voice.errorMessage
                                ?? (voice.transcript.isEmpty ? "Listening…" : voice.transcript)
                        )
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(0.9))
                        .lineLimit(2)
                        .frame(maxWidth: 200, alignment: .leading)

                        // Discard — stop listening, drop the transcript.
                        Button {
                            voice.cancel()
                        } label: {
                            Image(systemName: "xmark")
                                .font(.system(size: 12, weight: .bold))
                                .foregroundStyle(.white.opacity(0.85))
                                .frame(width: 30, height: 30)
                                .background(.white.opacity(0.14), in: Circle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Discard dictation")

                        // Paste — commit the transcript into the terminal
                        // input; a spinner replaces it while the model pass
                        // runs (toggle no-ops during refine).
                        if voice.isRefining {
                            ProgressView()
                                .controlSize(.small)
                                .tint(.white.opacity(0.85))
                                .frame(width: 30, height: 30)
                        } else {
                            Button {
                                voice.toggle { text in
                                    renderer.insertTranscribedText(text)
                                }
                            } label: {
                                Image(systemName: "doc.on.clipboard")
                                    .font(.system(size: 12, weight: .bold))
                                    .foregroundStyle(.white.opacity(0.85))
                                    .frame(width: 30, height: 30)
                                    .background(.white.opacity(0.14), in: Circle())
                            }
                            .buttonStyle(.plain)
                            .disabled(!hasText)
                            .opacity(hasText ? 1 : 0.45)
                            .accessibilityLabel("Paste dictation")
                        }
                    }
                    .padding(.leading, 14)
                    .padding(.trailing, 6)
                    .padding(.vertical, 6)
                    .liquidGlassPill()
                    .padding(.horizontal, 24)
                    .padding(.bottom, reservedBottom + 14)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
                    .transition(
                        .move(edge: .bottom)
                            .combined(with: .opacity)
                            .combined(with: .scale(scale: 0.96, anchor: .bottom))
                    )
                }
            }
            .frame(width: geometry.size.width, height: geometry.size.height)
            .clipped()
            .frame(width: geometry.size.width, height: geometry.size.height)
            .contentShape(Rectangle())
            .animation(.easeOut(duration: 0.22), value: renderer.initialReplayDone)
            .animation(.easeOut(duration: 0.2), value: renderer.menuPromptActive)
            .animation(.spring(response: 0.34, dampingFraction: 0.82), value: connectionNotice)
            .animation(.spring(response: 0.32, dampingFraction: 0.8), value: voice.isRecording)
            .animation(.spring(response: 0.32, dampingFraction: 0.8), value: voice.isRefining)
            .animation(.spring(response: 0.32, dampingFraction: 0.8), value: voice.errorMessage)
            .onAppear {
                if keyboard.height == 0 {
                    restingSizingSize = rawSizingViewport
                }
                renderer.updateVisibleViewport(sizingViewport)
                // The visible surface owns the composer attach hook; the
                // gallery (presented at root) applies screenshots through it.
                // Switching sessions re-appears a new surface, which rebinds
                // the hook to that session's renderer.
                store?.attachImageToComposer = { [weak renderer, weak store] data, contentType in
                    renderer?.attachImage(
                        data,
                        contentType: contentType,
                        resumable: store?.supportsResumableArtifactUpload == true
                    )
                }
            }
            // Watch this session's browser gallery for new screenshots and
            // pulse the gallery button when one lands. Lightweight metadata
            // poll; empty (no browser use) is a cheap no-op. Bound to the
            // session id so it cancels/reset on switch.
            .task(id: session.id) { await watchForNewScreenshots() }
            .onChange(of: sizingViewport) { next in
                renderer.updateVisibleViewport(next)
            }
            .onChange(of: rawSizingViewport) { next in
                // Track the resting size only while no keyboard is up;
                // keyboard-driven fluctuations must never be adopted.
                if keyboard.height == 0 {
                    restingSizingSize = next
                }
            }
            .onChange(of: terminalFocused) { focused in
                // Suppress desktop grid resizes for the whole focus lifetime
                // (and until the keyboard finishes dropping) so a focus cycle
                // can never round-trip a resize + replay blink.
                renderer.keyboardActive = focused || keyboard.height > 0
                // Keyboard handed over without a hide/show cycle (focus
                // moved from another field straight to the terminal):
                // adopt it so the inset appears.
                if focused, keyboard.height > 0, !keyboardOwnsInset {
                    withAnimation(.easeOut(duration: 0.25)) {
                        keyboardOwnsInset = true
                    }
                }
            }
            .onChange(of: renderer.inputFollowSerial) { _ in
                renderer.scrollToBottom()
                canvasPan = CGSize(width: canvasPan.width, height: 0)
                // The TUI can move its input line lower as the conversation
                // grows — start avoiding mid-typing if the caret would sink
                // under the keyboard.
                if keyboard.height > 0, !keyboardAvoidanceNeeded,
                   cursorNeedsKeyboardRoom(
                       keyboardHeight: keyboard.height,
                       screenHeight: geometry.size.height,
                       reservedTop: reservedTop,
                       baseReservedBottom: baseReservedBottom,
                       canvasSize: canvasSize,
                       scale: scale
                   ) {
                    withAnimation(.easeOut(duration: 0.25)) {
                        keyboardAvoidanceNeeded = true
                    }
                }
            }
            .onChange(of: geometry.size.width) { _ in
                renderer.scrollToBottom()
                canvasPan = clampedOffset(
                    CGSize(width: canvasPan.width, height: 0),
                    in: visibleViewport,
                    scale: scale,
                    canvasSize: canvasSize
                )
            }
            .onChange(of: geometry.size.height) { _ in
                renderer.scrollToBottom()
                canvasPan = clampedOffset(
                    CGSize(width: canvasPan.width, height: 0),
                    in: visibleViewport,
                    scale: scale,
                    canvasSize: canvasSize
                )
            }
            .onChange(of: keyboard.height) { newHeight in
                // Keep the resize-suppression flag current across the whole
                // show/hide animation (stays set while still focused, so the
                // resize only resumes once the keyboard is truly gone).
                renderer.keyboardActive = terminalFocused || newHeight > 0
                if newHeight > 0 {
                    // Adopt the keyboard only when the terminal summoned
                    // it; other fields' keyboards never inset the canvas.
                    if terminalFocused, !keyboardOwnsInset {
                        withAnimation(.easeOut(duration: 0.25)) {
                            keyboardOwnsInset = true
                        }
                    }
                    // Skip the shift entirely when the TUI's input line
                    // already clears the keyboard — common in fresh
                    // sessions where the composer sits near the top of the
                    // grid. Animated: this runs in its own transaction, and
                    // an instant flip snapped the canvas mid-slide.
                    let needed = cursorNeedsKeyboardRoom(
                        keyboardHeight: newHeight,
                        screenHeight: geometry.size.height,
                        reservedTop: reservedTop,
                        baseReservedBottom: baseReservedBottom,
                        canvasSize: canvasSize,
                        scale: scale
                    )
                    if needed != keyboardAvoidanceNeeded {
                        withAnimation(.easeOut(duration: 0.25)) {
                            keyboardAvoidanceNeeded = needed
                        }
                    }
                } else {
                    // Fully hidden: release the latch (the inset is already
                    // zero through `keyboard.height`) and refresh the
                    // resting sizing height — covers a rotation that
                    // happened while the keyboard was up, whose capture
                    // was deferred.
                    keyboardOwnsInset = false
                    restingSizingSize = rawSizingViewport
                }
                // Snap only when already at the tail: layering an instant
                // viewport jump on the keyboard slide read as a visible
                // jump on focus, and a reader scrolled up must not be
                // yanked to the bottom by the keyboard.
                if !renderer.scrollToBottomVisible {
                    renderer.scrollToBottom()
                }
                canvasPan = clampedOffset(
                    CGSize(width: canvasPan.width, height: 0),
                    in: visibleViewport,
                    scale: scale,
                    canvasSize: canvasSize
                )
            }
            .onChange(of: renderer.desktopFitActive) { _ in
                // Fit-to-screen must read as one motion: the canvas snaps
                // back to its unpanned frame while the grid change arrives.
                withAnimation(.easeOut(duration: 0.18)) {
                    canvasPan = .zero
                }
            }
            .onChange(of: renderer.canvasSize) { _ in
                renderer.scrollToBottom()
                canvasPan = .zero
            }
            .onChange(of: session.id) { _ in
                resetCanvas()
            }
            // Mac re-resolves OpenCode/Grok themes on config change and sends
            // the dark hex on every bootstrap; apply into the retained
            // renderer so chrome + Ghostty default bg match without rebuild.
            .onChange(of: session.terminalBackgroundHex) { hex in
                renderer.applyProviderBackground(hex)
            }
        }
        .contentShape(Rectangle())
        // No SwiftUI tap-to-focus: the terminal view itself owns tap
        // semantics (tap summons the keyboard, tap-while-visible dismisses,
        // scrolls don't count) and the onFocusChange mirror keeps our
        // binding in sync. A gesture here fought that logic — the vendor
        // resigned on tap and the gesture immediately re-focused.
        .onAppear {
            TerminalSessionCache.shared.noteVisible(session.id)
            // Cache hits keep the old renderer — re-apply if the Mac's
            // resolved theme moved while this surface was off-screen.
            renderer.applyProviderBackground(session.terminalBackgroundHex)
            if shouldStream { renderer.start() }
        }
        .onDisappear {
            TerminalSessionCache.shared.noteHidden(session.id)
            renderer.stop()
        }
        // The binding is the single source of truth the bridge syncs from,
        // so flipping it here resigns deterministically — no responder-chain
        // race with the next updateUIView pass.
        .onChange(of: suppressFocus) { suppressed in
            if suppressed {
                terminalFocused = false
            }
        }
        // Stream only while the app is active AND unlocked. Backgrounding stops
        // the poll loops (radio/battery, and a suspended in-flight request used
        // to wedge the stream). App-lock matters too: while the lock cover is up
        // the terminal is hidden, so reconnecting/replaying then paints an
        // offscreen Metal surface that stays STALE after unlock (the "session
        // behind won't open" bug). Gating on `!isLocked` defers the fresh
        // reconnect + tail replay to the instant the terminal is visible again.
        .onChange(of: shouldStream) { streaming in
            if streaming {
                renderer.start()
            } else {
                renderer.stop()
            }
        }
        .photosPicker(
            isPresented: $photoPickerPresented,
            selection: $selectedPhotoItem,
            matching: .images
        )
        .onChange(of: selectedPhotoItem) { item in
            guard let item else { return }
            selectedPhotoItem = nil
            Task { @MainActor in
                let raw: Data?
                do {
                    raw = try await item.loadTransferable(type: Data.self)
                } catch {
                    NSLog("[UnpeelIOS] photo load failed: \(error)")
                    raw = nil
                }
                guard let raw else {
                    renderer.noteAttachmentIssue("Couldn't read that photo")
                    return
                }
                // HEIC photos are huge and agents downscale anyway; a capped
                // JPEG uploads instantly and every model accepts it.
                let jpeg = Self.compressedJPEG(from: raw) ?? raw
                renderer.attachImage(
                    jpeg,
                    resumable: store?.supportsResumableArtifactUpload == true
                )
            }
        }
    }

    private static func messageLooksRetrying(_ message: String) -> Bool {
        let lowercased = message.lowercased()
        return lowercased.contains("retrying")
            || lowercased.contains("connecting")
            || lowercased.contains("unavailable")
    }

    private static func friendlyConnectionMessage(_ message: String) -> String {
        if message == "Terminal stream unavailable — retrying" {
            return "Restoring the live stream..."
        }
        return message
    }

    /// Kinds that are agent captures (browser/computer screenshots) — the
    /// ones whose arrival pulses the gallery button. Uploads/downloads are
    /// excluded: the user put those there themselves. Keep in step with the
    /// desktop twin (`SessionArtifactStore.captureKinds` +
    /// `SessionGalleryButton.watchForNewCaptures`).
    private static let captureArtifactKinds: Set<String> = ["screenshots", "computer"]

    /// Poll the session's gallery; pulse the gallery button when a capture
    /// newer than the last-seen one appears. The first sample only
    /// establishes the floor (pre-existing screenshots never pulse). Runs on
    /// the main actor so the @State write is safe; cancels when the surface
    /// disappears or the session id changes.
    @MainActor
    private func watchForNewScreenshots() async {
        guard let client = store?.client else { return }
        var baseline: Int64 = -1
        while !Task.isCancelled {
            if let list = try? await client.browserArtifacts(sessionID: session.id) {
                let captures = list.artifacts
                    .filter { Self.captureArtifactKinds.contains($0.kind) }
                let newest = captures.map(\.modifiedAtUnixMs).max() ?? 0
                if baseline < 0 {
                    baseline = newest
                } else if newest > baseline {
                    baseline = newest
                    galleryPulse = true
                    try? await Task.sleep(nanoseconds: 2_200_000_000)
                    galleryPulse = false
                }
            }
            try? await Task.sleep(nanoseconds: 5_000_000_000)
        }
    }

    /// Longest edge capped at 2048px, JPEG 0.85 — ~500KB instead of a
    /// multi-MB HEIC, well inside the server's 4MB body cap.
    private static func compressedJPEG(from data: Data) -> Data? {
        #if canImport(UIKit)
        guard let image = UIImage(data: data) else { return nil }
        let maxEdge: CGFloat = 2048
        let largest = max(image.size.width, image.size.height)
        let scale = largest > maxEdge ? maxEdge / largest : 1
        let target = CGSize(
            width: (image.size.width * scale).rounded(),
            height: (image.size.height * scale).rounded()
        )
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let rendered = UIGraphicsImageRenderer(size: target, format: format).image { _ in
            image.draw(in: CGRect(origin: .zero, size: target))
        }
        return rendered.jpegData(compressionQuality: 0.85)
        #else
        return nil
        #endif
    }

    /// Whether the caret would sit under the keyboard in the RESTING layout
    /// (no keyboard inset, no user pan): canvas bottom-anchored to the full
    /// viewport, caret rect scaled into screen coordinates. A 32pt margin
    /// keeps it comfortably clear of the accessory bar.
    private func cursorNeedsKeyboardRoom(
        keyboardHeight: CGFloat,
        screenHeight: CGFloat,
        reservedTop: CGFloat,
        baseReservedBottom: CGFloat,
        canvasSize: CGSize,
        scale: CGFloat
    ) -> Bool {
        keyboardAvoidanceInset(
            keyboardHeight: keyboardHeight,
            screenHeight: screenHeight,
            reservedTop: reservedTop,
            baseReservedBottom: baseReservedBottom,
            canvasSize: canvasSize,
            scale: scale
        ) > 0
    }

    /// How far to lift the canvas so the live caret clears the software
    /// keyboard — the MINIMAL overlap, never the whole keyboard. Zero when the
    /// TUI's input line already sits above the keyboard (fresh sessions, and
    /// TUIs like cursor-agent that park the caret well above the last row);
    /// grows toward the full keyboard height only as the caret nears the very
    /// bottom (Claude's composer), or when the caret position is unknown.
    /// Reads the caret straight from the ghostty surface
    /// (`cursorCanvasRect` → `imePoint`).
    private func keyboardAvoidanceInset(
        keyboardHeight: CGFloat,
        screenHeight: CGFloat,
        reservedTop: CGFloat,
        baseReservedBottom: CGFloat,
        canvasSize: CGSize,
        scale: CGFloat
    ) -> CGFloat {
        guard keyboardHeight > 0 else { return 0 }
        guard let cursor = renderer.cursorCanvasRect() else { return keyboardHeight }
        let viewportHeight = screenHeight - reservedTop - baseReservedBottom
        let scaledHeight = canvasSize.height * scale
        // Mirrors defaultPan: the canvas rests shifted down by the bottom
        // slack so the content, not the blank slack, bottom-anchors.
        let canvasTop = reservedTop + viewportHeight - scaledHeight
            + renderer.canvasBottomSlack * scale
        let cursorBottomOnScreen = canvasTop + cursor.maxY * scale
        let keyboardTop = screenHeight - keyboardHeight
        // Clearance above the keyboard top. Generous (not just the caret line)
        // because centered-input TUIs like opencode park the caret at the TOP
        // of a multi-line input box, so a small margin leaves the box's lower
        // rows (placeholder + status line) hidden behind the keyboard/accessory
        // bar. Bottom-anchored composers (Claude) just get a comfortable gap.
        // Still capped at the keyboard height so a high caret isn't shoved
        // under the title chrome.
        let overlap = cursorBottomOnScreen + 104 - keyboardTop
        return max(0, min(overlap, keyboardHeight))
    }

    private func resetCanvas() {
        withAnimation(.easeOut(duration: 0.18)) {
            canvasPan = .zero
        }
    }


    private func visibleViewport(in viewport: CGSize) -> CGSize {
        RemoteTerminalCanvasLayout.visibleViewport(in: viewport)
    }

    private func baseScale(in viewport: CGSize, canvasSize: CGSize) -> CGFloat {
        RemoteTerminalCanvasLayout.baseScale(in: viewport, canvasSize: canvasSize)
    }

    private func defaultPan(in viewport: CGSize, scale: CGFloat, canvasSize: CGSize) -> CGSize {
        RemoteTerminalCanvasLayout.defaultPan(
            in: viewport,
            scale: scale,
            canvasSize: canvasSize,
            bottomSlack: renderer.canvasBottomSlack
        )
    }

    private func panBounds(in viewport: CGSize, scale: CGFloat, canvasSize: CGSize) -> CGSize {
        RemoteTerminalCanvasLayout.panBounds(
            in: viewport,
            scale: scale,
            canvasSize: canvasSize,
            bottomSlack: renderer.canvasBottomSlack
        )
    }

    private func scaledCanvasSize(scale: CGFloat, canvasSize: CGSize) -> CGSize {
        RemoteTerminalCanvasLayout.scaledCanvasSize(scale: scale, canvasSize: canvasSize)
    }

    private func clamped(_ pan: CGSize, in viewport: CGSize, scale: CGFloat, canvasSize: CGSize) -> CGSize {
        RemoteTerminalCanvasLayout.clamped(
            pan,
            in: viewport,
            scale: scale,
            canvasSize: canvasSize,
            bottomSlack: renderer.canvasBottomSlack
        )
    }

    private func clampedOffset(_ pan: CGSize, in viewport: CGSize, scale: CGFloat, canvasSize: CGSize) -> CGSize {
        RemoteTerminalCanvasLayout.clampedOffset(
            pan,
            in: viewport,
            scale: scale,
            canvasSize: canvasSize,
            bottomSlack: renderer.canvasBottomSlack
        )
    }

    private func panCanvasHorizontally(
        by deltaX: CGFloat,
        in viewport: CGSize,
        scale: CGFloat,
        canvasSize: CGSize
    ) {
        let bounds = panBounds(in: viewport, scale: scale, canvasSize: canvasSize)
        guard bounds.width > 1 else { return }
        canvasPan = clampedOffset(
            CGSize(width: canvasPan.width + deltaX, height: canvasPan.height),
            in: viewport,
            scale: scale,
            canvasSize: canvasSize
        )
    }

    private func followTypingColumn(
        in viewport: CGSize,
        scale: CGFloat,
        defaultPan: CGSize,
        canvasSize: CGSize
    ) {
        let bounds = panBounds(in: viewport, scale: scale, canvasSize: canvasSize)
        guard bounds.width > 1 || bounds.height > 1 else { return }
        let columns = max(Int(renderer.terminal.surfaceSize?.columns ?? 120), 1)
        let column = max(renderer.inputFollowColumn, 0)
        let columnX = (CGFloat(column) / CGFloat(columns)) * canvasSize.width
        let targetPanX = ((canvasSize.width / 2) - columnX) * scale
        let absolute = clamped(
            CGSize(width: targetPanX, height: canvasPan.height + defaultPan.height),
            in: viewport,
            scale: scale,
            canvasSize: canvasSize
        )
        withAnimation(.easeOut(duration: 0.14)) {
            canvasPan = CGSize(
                width: absolute.width - defaultPan.width,
                height: absolute.height - defaultPan.height
            )
        }
    }

    private func scrollToLiveBottom(in viewport: CGSize, scale: CGFloat, canvasSize: CGSize) {
        renderer.jumpToTuiBottomIfNeeded()
        renderer.scrollToBottom()
        withAnimation(.easeOut(duration: 0.18)) {
            canvasPan = clampedOffset(.zero, in: viewport, scale: scale, canvasSize: canvasSize)
        }
    }
}

private extension CGSize {
    func subtractingInsets(top: CGFloat, bottom: CGFloat) -> CGSize {
        CGSize(width: width, height: max(height - max(top, 0) - max(bottom, 0), 1))
    }
}

private struct TerminalScrollToBottomButton: View {
    let visible: Bool
    let action: () -> Void
    @State private var pressed = false

    var body: some View {
        Button(action: action) {
            Image(systemName: "chevron.down")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.white.opacity(0.9))
                .frame(width: 36, height: 36)
                .background(.ultraThinMaterial, in: Circle())
                .overlay(Circle().strokeBorder(.white.opacity(0.10), lineWidth: 1))
                .shadow(color: .black.opacity(0.28), radius: 12, y: 5)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .scaleEffect(pressed ? 0.95 : 1)
        .opacity(visible ? 0.92 : 0)
        .offset(y: visible ? 0 : 8)
        .allowsHitTesting(visible)
        .animation(.easeOut(duration: 0.2), value: visible)
        .simultaneousGesture(
            DragGesture(minimumDistance: 0)
                .onChanged { _ in pressed = true }
                .onEnded { _ in pressed = false }
        )
        .accessibilityLabel("Scroll to bottom")
    }
}

/// Floating control bar for an agent-rendered select menu: ↑/↓ to move the
/// highlight, Enter to select, Esc to cancel, and direct number shortcuts for
/// the options — the exact affordances such menus advertise. Appears only
/// while a menu is detected (`RemoteGhosttyRenderer.menuPromptActive`) and the
/// keyboard is down, so a choice can be answered without one.
private struct TerminalMenuControlBar: View {
    let optionCount: Int
    let onKey: (RemoteGhosttyRenderer.MenuControlKey) -> Void

    /// Match the accessory bar's circular keys.
    private var diameter: CGFloat { 38 }

    var body: some View {
        HStack(spacing: 5) {
            keyButton(systemImage: "arrowtriangle.up.fill", accessibility: "Previous option") {
                onKey(.up)
            }
            keyButton(systemImage: "arrowtriangle.down.fill", accessibility: "Next option") {
                onKey(.down)
            }
            dot
            keyButton(label: "esc", accessibility: "Cancel") { onKey(.escape) }
            keyButton(
                systemImage: "return",
                accessibility: "Select",
                prominent: true
            ) {
                onKey(.enter)
            }
        }
        .environment(\.colorScheme, .dark)
    }

    /// The small separator dot the accessory bar uses between key groups.
    private var dot: some View {
        Circle()
            .fill(.white.opacity(0.28))
            .frame(width: 3, height: 3)
            .padding(.horizontal, 2)
    }

    @ViewBuilder
    private func keyButton(
        label: String? = nil,
        systemImage: String? = nil,
        accessibility: String,
        prominent: Bool = false,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Group {
                if let systemImage {
                    Image(systemName: systemImage)
                        .font(.system(size: diameter * 0.38, weight: .semibold))
                } else if let label {
                    Text(label)
                        .font(.system(size: diameter * 0.36, weight: .semibold, design: .rounded))
                }
            }
            .foregroundStyle(prominent ? Color.black : .white.opacity(0.92))
            .frame(width: diameter, height: diameter)
            .background {
                if prominent {
                    Circle().fill(Color.accentColor)
                } else {
                    Circle().fill(.ultraThinMaterial)
                    Circle().fill(.white.opacity(0.06))
                }
            }
            .overlay(Circle().strokeBorder(.white.opacity(prominent ? 0 : 0.10), lineWidth: 1))
            .shadow(color: .black.opacity(0.28), radius: 10, y: 4)
            .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accessibility)
    }
}

/// Progressive material fade for the terminal's top band (notch + floating
/// title): strongest at the very top, dissolving to clear, so panned canvas
/// content reads as receding under the chrome rather than clipping.
private struct TerminalTopGlassVeil: View {
    let height: CGFloat
    /// The terminal's chrome color so the veil dissolves into the session's
    /// background (opencode/grok themes) rather than the default.
    var background: Color = TerminalChrome.background

    var body: some View {
        Rectangle()
            .fill(.ultraThinMaterial)
            .overlay(
                LinearGradient(
                    stops: [
                        .init(color: background.opacity(0.92), location: 0),
                        .init(color: background.opacity(0.5), location: 0.5),
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
                        .init(color: .black.opacity(0.86), location: 0.45),
                        .init(color: .black.opacity(0.32), location: 0.78),
                        .init(color: .clear, location: 1),
                    ],
                    startPoint: .top,
                    endPoint: .bottom
                )
            )
            .frame(height: height)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .allowsHitTesting(false)
    }
}

/// Shimmering placeholder lines while a freshly-switched session loads its
/// replay — terminal-shaped (mono-ish rows of varying width), opaque over
/// the not-yet-painted canvas, hit-test transparent.
private struct TerminalSkeletonView: View {
    let topInset: CGFloat
    @State private var pulsing = false

    private static let lineFractions: [CGFloat] = [
        0.82, 0.58, 0.71, 0.36, 0.88, 0.52, 0.28, 0.64, 0.47, 0.76, 0.41, 0.6,
    ]

    var body: some View {
        GeometryReader { geometry in
            VStack(alignment: .leading, spacing: 13) {
                ForEach(Self.lineFractions.indices, id: \.self) { index in
                    Rectangle()
                        .fill(.white.opacity(0.07))
                        .frame(
                            width: max(geometry.size.width - 64, 0)
                                * Self.lineFractions[index],
                            height: 12
                        )
                }
            }
            .padding(.horizontal, 32)
            .padding(.top, topInset + 22)
            .opacity(pulsing ? 0.55 : 1)
            .animation(
                .easeInOut(duration: 0.9).repeatForever(autoreverses: true),
                value: pulsing
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .background(TerminalChrome.background)
            .onAppear { pulsing = true }
        }
        .allowsHitTesting(false)
    }
}

private struct TerminalConnectionNotice: Equatable {
    enum Tone: Equatable {
        case reconnecting
        case warning
    }

    let title: String
    let message: String
    let symbolName: String
    let tone: Tone
}

private struct TerminalConnectionNoticeOverlay: View {
    let notice: TerminalConnectionNotice
    private static let cornerRadius: CGFloat = 16

    private var accent: Color {
        switch notice.tone {
        case .reconnecting:
            return Color(hex: 0x67E8F9)
        case .warning:
            return Color(hex: 0xF59E0B)
        }
    }

    var body: some View {
        HStack(spacing: 11) {
            icon

            VStack(alignment: .leading, spacing: 2) {
                Text(notice.title)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.94))
                    .lineLimit(1)
                Text(notice.message)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(.white.opacity(0.68))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer(minLength: 0)
        }
        .padding(.leading, 10)
        .padding(.trailing, 13)
        .padding(.vertical, 10)
        .frame(maxWidth: 330)
        .terminalNoticeGlassBackground(
            cornerRadius: Self.cornerRadius,
            accent: accent
        )
        .shadow(color: .black.opacity(0.32), radius: 20, y: 10)
        .allowsHitTesting(false)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(notice.title). \(notice.message)")
    }

    @ViewBuilder
    private var icon: some View {
        ZStack {
            Circle().fill(accent.opacity(0.15))
            Circle().strokeBorder(accent.opacity(0.28), lineWidth: 1)
            if notice.tone == .reconnecting {
                ProgressView()
                    .controlSize(.small)
                    .tint(accent)
            } else {
                Image(systemName: notice.symbolName)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(accent)
            }
        }
        .frame(width: 34, height: 34)
    }
}

private extension View {
    @ViewBuilder
    func terminalNoticeGlassBackground(cornerRadius: CGFloat, accent: Color) -> some View {
        let shape = RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)

        if #available(iOS 26.0, macOS 26.0, *) {
            self
                .padding(.horizontal, 1)
                .padding(.vertical, 1)
                .glassEffect(.regular, in: shape)
                .overlay(shape.fill(accent.opacity(0.05)))
                .overlay(terminalNoticeGlassStroke(shape: shape, accent: accent))
        } else {
            self
                .background(.ultraThinMaterial, in: shape)
                .overlay(shape.fill(.white.opacity(0.035)))
                .overlay(terminalNoticeGlassStroke(shape: shape, accent: accent))
        }
    }

    private func terminalNoticeGlassStroke(
        shape: RoundedRectangle,
        accent: Color
    ) -> some View {
        shape.strokeBorder(
            LinearGradient(
                colors: [
                    .white.opacity(0.26),
                    accent.opacity(0.28),
                    .white.opacity(0.08),
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            ),
            lineWidth: 1
        )
    }
}

private struct RemoteTerminalGrid: Equatable {
    var columns: Int
    var rows: Int
}

/// Serializes terminal input to the Mac. Ghostty emits a keystroke as up to
/// three write-closure calls (bracketed-paste begin, text, end), and a
/// fire-and-forget `Task` per chunk gives no ordering guarantee — scrambled
/// markers render as literal `[200~` in the remote TUI. One drain loop sends
/// strictly in order and coalesces bursts into fewer HTTP requests.
private final class RemoteTerminalWriteQueue: @unchecked Sendable {
    private let lock = NSLock()
    private var pending = Data()
    private var draining = false
    private let send: @Sendable (String) async throws -> Void
    private var failureHandler: (@Sendable (Error) -> Void)?

    init(send: @escaping @Sendable (String) async throws -> Void) {
        self.send = send
    }

    /// Failed sends are input the Mac never saw — report them instead of
    /// silently dropping keystrokes.
    func setOnSendFailure(_ handler: @escaping @Sendable (Error) -> Void) {
        lock.lock()
        failureHandler = handler
        lock.unlock()
    }

    private func currentFailureHandler() -> (@Sendable (Error) -> Void)? {
        lock.lock()
        defer { lock.unlock() }
        return failureHandler
    }

    func enqueue(_ data: Data) {
        guard !data.isEmpty else { return }
        lock.lock()
        pending.append(data)
        let shouldStart = !draining
        if shouldStart { draining = true }
        lock.unlock()
        guard shouldStart else { return }
        Task { await drain() }
    }

    /// Grabs everything queued so far; flips `draining` off when empty so
    /// the next enqueue restarts the loop.
    private func takeBatch() -> Data {
        lock.lock()
        defer { lock.unlock() }
        let batch = pending
        pending = Data()
        if batch.isEmpty { draining = false }
        return batch
    }

    private func drain() async {
        while true {
            let batch = takeBatch()
            if batch.isEmpty { return }
            do {
                try await send(String(decoding: batch, as: UTF8.self))
            } catch {
                currentFailureHandler()?(error)
            }
        }
    }
}

@MainActor
final class RemoteGhosttyRenderer: ObservableObject {
    let terminal: TerminalViewState
    @Published private(set) var lastError: String?
    @Published private(set) var inputFollowColumn = 0
    @Published private(set) var inputFollowSerial = 0
    @Published private(set) var scrollToBottomVisible = false
    /// Claude Code's full-screen TUI keeps its own virtual scroll (alternate
    /// screen, no scrollback), so the local viewport never reads "scrolled
    /// up" — the TUI's signal is a "Jump to bottom (ctrl+End)" hint drawn on
    /// screen. When a post-feed scan finds that marker, the scroll-to-bottom
    /// button shows and tapping it fakes the ctrl+End press remotely.
    @Published private(set) var tuiJumpHintVisible = false
    @Published private(set) var canvasSize = CGSize(width: 724, height: 590)
    /// Dead region at the canvas bottom in unscaled canvas points: canvas
    /// height minus the target grid's real content (rows × cell height +
    /// padding). Alignment slop and the sticky `gridAlignmentExtra` live
    /// there as blank rows; the layout shifts the canvas down by this so
    /// the content — not the blank slack — bottom-anchors to the viewport.
    @Published private(set) var canvasBottomSlack: CGFloat = 0
    /// Phone-fit mode: the desktop pane is letterboxed to this phone's grid
    /// (with a revert banner on the Mac), so both render identical cells.
    /// While active, grid requests go through the desktop-resize endpoint
    /// instead of raw PTY resizes, so the two paths never fight.
    @Published private(set) var desktopFitActive = false
    /// True while the software keyboard is up (terminal focused). Set by the
    /// view. Desktop grid resizes are suppressed for its duration: focus can
    /// nudge the ghostty surface's reported cell metrics (Metal re-measure on
    /// becoming first responder), and in fit mode that would fire a
    /// desktop-resize → tail replay = a visible blink/reflow a second or so
    /// after focus. The viewport itself is already frozen view-side; this
    /// closes the surface-metric path. A genuine geometry change (rotation)
    /// re-fires the resize once the keyboard drops.
    var keyboardActive = false
    /// Flips once the first replay has painted — drives the session-switch
    /// skeleton. Stays true across reconnects (stale content beats shimmer).
    @Published private(set) var initialReplayDone = false

    private let sessionID: String
    /// Live presentation identity from the Host. Unlike the Session launch
    /// command, this can change while a cached blank terminal stays open.
    private var providerID: String?
    private let command: String
    private let client: RemoteMacClient
    private let memorySession: InMemoryTerminalSession
    private let inputTracker: RemoteTerminalInputTracker
    private let writeQueue: RemoteTerminalWriteQueue
    /// Last provider dark-bg hex applied to Ghostty (from Mac bootstrap).
    /// Tracks so OpenCode/Grok theme edits can hot-update without rebuild.
    private var appliedBackgroundHex: Int?
    /// Transport switch for the write queue: WS text frames while an output
    /// WebSocket is healthy, the HTTP write endpoint otherwise.
    private let inputRouter: RemoteTerminalInputRouter
    private let mouseModeTracker = RemoteTerminalMouseModeTracker()
    /// Stateful across chunks (carries split trailing sequences) so a query
    /// split at a chunk boundary can never reassemble inside the surface's
    /// parser and get answered as typed input. Reset alongside the mouse
    /// tracker on reset/clear replays.
    private let queryFilter = TerminalQueryFilter()
    private var streamTask: Task<Void, Never>?
    private var outputOffset: UInt64?
    private var surfaceSizeCancellable: AnyCancellable?
    private var targetGrid: RemoteTerminalGrid
    private var localGrid = RemoteTerminalGrid(columns: 0, rows: 0)
    private var lastMetricsRefresh = ContinuousClock.now
    private var scrollToBottomTask: Task<Void, Never>?
    private var tuiJumpHintScanTask: Task<Void, Never>?
    private var tuiJumpRetryTask: Task<Void, Never>?
    /// True from the first touch of a remote-wheel scroll gesture until its
    /// momentum glide fully decays; hides the jump-to-bottom button for the
    /// duration so it never fights the easing.
    private var remoteWheelSettling = false
    private var resizeTask: Task<Void, Never>?
    /// Single-flight background metrics refresh, kicked from the HTTP output
    /// loop so the stream never waits on a metrics round-trip.
    private var gridRefreshTask: Task<Void, Never>?
    private var remoteGrid: RemoteTerminalGrid
    private var hasRemoteGrid = false
    private var lastRequestedRemoteGrid: RemoteTerminalGrid?
    /// Column count the screen was last rebuilt at. Only a column change
    /// breaks wrapping; row-only grid changes re-flow locally with no replay.
    private var lastReplayedColumns = 0
    /// Monotonic token so a cancelled resize task can never clear (or act
    /// on) the handle of the task that replaced it.
    private var resizeGeneration = 0
    /// Monotonic token that makes replays single-flight: bumped at every
    /// `replayTail` entry. In-flight polls and older replays re-check it
    /// after their awaits and discard their bytes when superseded — the
    /// newest replay repaints everything anyway. Kills the two verified
    /// double-content races (stale poll chunk fed mid-replay; two concurrent
    /// replays each painting the full tail).
    private var replayGeneration = 0
    /// Throttle for surfacing keystroke-send failures — a burst of failed
    /// writes is one problem, not one error flash per key.
    private var lastInputFailureNote: ContinuousClock.Instant?
    /// Monotonic feed id + the id currently holding the DEC 2026 bracket
    /// open. A superseded feed only closes the bracket it still owns, so it
    /// can never end a newer feed's in-progress frame.
    private var feedSerial = 0
    private var syncBracketOwner = 0
    /// Latches once the remote TUI is seen to drive DEC 2026 synchronized
    /// output itself (e.g. Grok, Claude Code). From then on the client stops
    /// wrapping chunks in its *own* 2026 bracket: a second, chunk-aligned
    /// bracket nested around the TUI's own frames collides with them and
    /// leaks sequence fragments (a stray `026l` on screen). The desktop
    /// never injects a bracket at all, which is why this was mobile-only.
    /// Simple TUIs that never emit 2026 keep the client bracket for flash
    /// suppression.
    private var remoteManagesSyncOutput = false
    private var visibleViewportSize: CGSize = .zero
    private var measuredCellSize = CGSize(width: fallbackCellWidth, height: fallbackCellHeight)
    private var gridAlignmentExtra = CGSize.zero
    private var remoteMouseWheelAccumulator: CGFloat = 0
    private var wheelMomentumTask: Task<Void, Never>?
    /// One auto-fit per attach. If the desktop takes the size back (banner
    /// X) while it is VIEWING this session, the phone respects it — but the
    /// moment the Mac stops showing the session, `autoRefitIfUnwatched`
    /// re-asserts the fit automatically (nobody is looking at the desktop
    /// pane, so there is no size to fight over).
    private var autoFitAttempted = false
    /// Latest desktop-viewing signal from the metrics endpoint: true = the
    /// Mac is showing this session's terminal right now (selected + app
    /// frontmost). nil = no signal yet, or an older Mac that doesn't report
    /// it — the phone then keeps the manual fit button behavior.
    private var desktopViewingSameSession: Bool?
    /// Throttle for automatic fit re-assertion, so a stale viewing signal
    /// (metrics cadence is ~1s, and the flag is a poll-time snapshot) can
    /// never lock a real desktop user into a resize fight.
    private var lastAutoRefit: ContinuousClock.Instant?
    /// One-shot guard for the post-replay fit correction. The initial
    /// `autoFitToPhone` runs before the ghostty surface reports its real cell
    /// metrics and before the safe-area-driven viewport settles, so a cold
    /// open can letterbox the desktop to a slightly-wrong column count that
    /// nothing later re-fits — the user had to rotate the phone to force a
    /// re-fit. `settleInitialFit` does that automatically, once, after the
    /// first replay lands (real metrics + a settled viewport guaranteed by
    /// then).
    private var initialFitSettled = false

    init(
        session: RemoteSessionSummary,
        client: RemoteMacClient = RemoteMacClient(),
        fontSize: CGFloat = phoneTerminalFontSize
    ) {
        self.sessionID = session.id
        self.providerID = session.presentationProviderID
        self.command = session.command
        self.client = client
        targetGrid = Self.fallbackGrid
        remoteGrid = Self.fallbackGrid
        // Scale the pre-metrics cell estimate with the font so the first
        // canvas/fit computations are close before real metrics arrive
        // (the fallback constants were measured at 13pt).
        let fontRatio = fontSize / 13
        measuredCellSize = CGSize(
            width: Self.fallbackCellWidth * fontRatio,
            height: Self.fallbackCellHeight * fontRatio
        )

        let inputClient = client
        let inputSessionID = session.id
        let inputTracker = RemoteTerminalInputTracker()
        self.inputTracker = inputTracker
        // Input rides the output WebSocket when one is connected (raw PTY
        // passthrough, echo returns via output) and the proven HTTP write
        // otherwise — the same ordered queue drains into both.
        let inputRouter = RemoteTerminalInputRouter { text, writeID in
            try await inputClient.write(sessionID: inputSessionID, data: text, writeID: writeID)
        }
        self.inputRouter = inputRouter
        let writeQueue = RemoteTerminalWriteQueue { text in
            try await inputRouter.send(text)
        }
        self.writeQueue = writeQueue
        memorySession = Self.makeMemorySession(
            inputTracker: inputTracker,
            writeQueue: writeQueue
        )

        appliedBackgroundHex = session.terminalBackgroundHex
        terminal = TerminalViewState(
            theme: Self.terminalTheme(
                darkBackground: Self.hexString(from: session.terminalBackgroundHex)
            ),
            terminalConfiguration: Self.terminalConfiguration()
        )
        terminal.configuration = TerminalSurfaceOptions(
            backend: .inMemory(memorySession),
            fontSize: Float(fontSize),
            context: .window
        )
        terminal.adopt(terminalColorScheme: .dark)
        surfaceSizeCancellable = terminal.$surfaceSize.sink { [weak self] size in
            Task { @MainActor in
                self?.handleLocalSurfaceSize(size)
            }
        }
        inputTracker.onFollow = { [weak self] column in
            Task { @MainActor in
                guard let self else { return }
                self.inputFollowColumn = column
                self.inputFollowSerial &+= 1
            }
        }
        writeQueue.setOnSendFailure { [weak self] error in
            Task { @MainActor in
                self?.noteInputSendFailure(error)
            }
        }
    }

    /// Refresh provider-specific terminal behavior without replacing the
    /// cached renderer, Ghostty surface, scrollback, or output cursor.
    func updatePresentationProviderID(_ providerID: String?) {
        self.providerID = providerID
    }

    /// Push a new provider TUI background from a later bootstrap poll.
    /// No-op when the hex is unchanged. Colors only — padding stays as built.
    func applyProviderBackground(_ hex: Int?) {
        guard hex != appliedBackgroundHex else { return }
        appliedBackgroundHex = hex
        _ = terminal.setTheme(
            Self.terminalTheme(darkBackground: Self.hexString(from: hex))
        )
    }

    func start() {
        guard streamTask == nil else { return }
        streamTask = Task { @MainActor [weak self] in
            await self?.runOutputLoop()
        }
    }

    func stop() {
        streamTask?.cancel()
        streamTask = nil
        scrollToBottomTask?.cancel()
        scrollToBottomTask = nil
        tuiJumpHintScanTask?.cancel()
        tuiJumpHintScanTask = nil
        tuiJumpRetryTask?.cancel()
        tuiJumpRetryTask = nil
        resizeTask?.cancel()
        resizeTask = nil
        gridRefreshTask?.cancel()
        gridRefreshTask = nil
        wheelMomentumTask?.cancel()
        wheelMomentumTask = nil
        remoteWheelSettling = false
        // The fit is "while the phone is watching": leaving the session
        // (switch, background, close) hands the size back to the desktop so
        // letterboxes never pile up on the Mac. Detached — this renderer may
        // be torn down before the request completes.
        initialFitSettled = false
        desktopViewingSameSession = nil
        lastAutoRefit = nil
        if desktopFitActive {
            desktopFitActive = false
            lastRequestedRemoteGrid = nil
            autoFitAttempted = false
            let client = client
            let sessionID = sessionID
            Task.detached {
                try? await client.revertDesktopTerminal(sessionID: sessionID)
            }
        } else {
            autoFitAttempted = false
        }
    }

    func updateScrolledUp(_ scrolledUp: Bool) {
        if scrollToBottomVisible != scrolledUp {
            scrollToBottomVisible = scrolledUp
        }
    }

    func scrollToBottom() {
        terminal.surface?.performBindingAction("scroll_to_bottom")
        scrollToBottomVisible = false
    }

    /// Live caret cell rect in CANVAS coordinates (unscaled points), from
    /// the local surface's IME caret. Lets the view skip keyboard avoidance
    /// when the TUI's input line is already high on screen — fresh sessions
    /// draw their composer near the top, and shifting them is pure churn.
    func cursorCanvasRect() -> CGRect? {
        guard let surface = terminal.surface else { return nil }
        let point = surface.imePoint()
        guard point.height > 0 else { return nil }
        return CGRect(x: point.x, y: point.y, width: point.width, height: point.height)
    }

    func handleTouchScroll(
        location: CGPoint,
        delta: CGSize,
        velocity: CGSize,
        phase: TerminalTouchScrollPhase
    ) -> Bool {
        if phase == .began {
            // An intentional scroll cancels any pending jump-to-bottom
            // retries so they never yank the user back down.
            tuiJumpRetryTask?.cancel()
            tuiJumpRetryTask = nil
        }
        guard shouldSendRemoteMouseWheel else {
            remoteMouseWheelAccumulator = 0
            wheelMomentumTask?.cancel()
            remoteWheelSettling = false
            return false
        }

        switch phase {
        case .began:
            remoteMouseWheelAccumulator = 0
            wheelMomentumTask?.cancel()
            remoteWheelSettling = true
            return true

        case .changed:
            guard abs(delta.height) >= max(2, abs(delta.width) * 0.8) else {
                return false
            }

            let stepSize = wheelStepSize
            remoteMouseWheelAccumulator += delta.height / stepSize
            let stepCount = min(Int(abs(remoteMouseWheelAccumulator)), Self.maximumMouseWheelEventsPerGestureTick)
            guard stepCount > 0 else { return true }

            let direction = remoteMouseWheelAccumulator < 0
                ? RemoteTerminalMouseEventEncoder.WheelDirection.down
                : .up
            remoteMouseWheelAccumulator -= CGFloat(stepCount) * (remoteMouseWheelAccumulator < 0 ? -1 : 1)

            sendWheel(direction, steps: stepCount, at: location)
            return true

        case .ended:
            remoteMouseWheelAccumulator = 0
            // Native scrolling is mostly glide — without momentum, TUIs like
            // Claude's virtual list only move while the finger drags, which
            // reads as "super slow". Decay the release velocity into wheel
            // bursts until friction wins or a new touch lands.
            startWheelMomentum(velocity: velocity, location: location)
            if wheelMomentumTask == nil {
                // No glide (slow drag release): the gesture settles here.
                remoteWheelSettling = false
                scheduleTuiJumpHintScan()
            }
            return true

        case .cancelled:
            remoteMouseWheelAccumulator = 0
            wheelMomentumTask?.cancel()
            remoteWheelSettling = false
            return true
        }
    }

    /// Finger travel per wheel event. A full cell height means the content
    /// tracks the finger 1:1 (one row per row of travel), matching the
    /// desktop after the trackpad-multiplier fix — a fraction of a cell
    /// over-scrolls every swipe. Momentum divides velocity by this same
    /// step, so glide speed scales with it automatically.
    private var wheelStepSize: CGFloat {
        max(measuredCellSize.height, 7)
    }

    private func sendWheel(
        _ direction: RemoteTerminalMouseEventEncoder.WheelDirection,
        steps: Int,
        at location: CGPoint
    ) {
        let cell = terminalCell(at: location)
        let payload = RemoteTerminalMouseEventEncoder.sgrWheelSequence(
            direction: direction,
            column: cell.column,
            row: cell.row,
            repeats: steps
        )
        sendRemoteMousePayload(payload)
    }

    private func startWheelMomentum(velocity: CGSize, location: CGPoint) {
        wheelMomentumTask?.cancel()
        wheelMomentumTask = nil
        let initialRate = velocity.height / wheelStepSize
        // Below ~6 steps/s a "flick" was really a positioned drag — no glide.
        guard abs(initialRate) > 6 else { return }
        let cappedRate = max(min(initialRate, 90), -90)
        wheelMomentumTask = Task { @MainActor [weak self] in
            var rate = cappedRate
            var carry: CGFloat = 0
            while !Task.isCancelled, abs(rate) > 3 {
                carry += rate * 0.05
                let steps = Int(abs(carry))
                if steps > 0 {
                    carry -= CGFloat(steps) * (carry < 0 ? -1 : 1)
                    guard let self else { return }
                    self.sendWheel(
                        rate < 0 ? .down : .up,
                        steps: min(steps, 24),
                        at: location
                    )
                }
                // Exponential friction: ~0.6s glide from a hard flick.
                rate *= 0.92
                try? await Task.sleep(nanoseconds: 50_000_000)
            }
            // Natural decay ends the scroll interaction; the settle scan can
            // now show the jump-to-bottom button without fighting easing. A
            // cancelled glide skips this — the cancelling gesture owns the
            // flag from here.
            guard let self, !Task.isCancelled else { return }
            self.remoteWheelSettling = false
            self.scheduleTuiJumpHintScan()
        }
    }

    func updateVisibleViewport(_ size: CGSize) {
        guard size.width > 0, size.height > 0 else { return }
        guard abs(visibleViewportSize.width - size.width) > 1
            || abs(visibleViewportSize.height - size.height) > 1
        else { return }
        visibleViewportSize = size
        requestRemoteGridForVisibleViewport()
    }

    private var shouldSendRemoteMouseWheel: Bool {
        mouseModeTracker.mouseTrackingEnabled
            || (providerPrefersRemoteMouseWheel && !mouseModeTracker.sawMouseOrAlternateDisable)
    }

    private var providerPrefersRemoteMouseWheel: Bool {
        let provider = (providerID ?? "").lowercased()
        if Self.remoteMouseWheelProviders.contains(provider) {
            return true
        }
        guard let executable = command
            .split(whereSeparator: \.isWhitespace)
            .first
            .map({ URL(fileURLWithPath: String($0)).lastPathComponent.lowercased() })
        else { return false }
        return Self.remoteMouseWheelProviders.contains(executable)
    }

    private func terminalCell(at location: CGPoint) -> (column: Int, row: Int) {
        let cellWidth = max(measuredCellSize.width, Self.fallbackCellWidth)
        let cellHeight = max(measuredCellSize.height, Self.fallbackCellHeight)
        let column = Int(floor((location.x - Self.horizontalPadding) / cellWidth)) + 1
        let row = Int(floor((location.y - Self.verticalPadding) / cellHeight)) + 1
        return (
            min(max(column, 1), max(targetGrid.columns, 1)),
            min(max(row, 1), max(targetGrid.rows, 1))
        )
    }

    private func sendRemoteMousePayload(_ payload: String) {
        guard !payload.isEmpty else { return }
        // Same ordered pipeline as keystrokes: interleaving mouse sequences
        // with a bracketed-paste wrap mid-flight would corrupt both.
        writeQueue.enqueue(Data(payload.utf8))
    }

    private func runOutputLoop() async {
        while terminal.surfaceSize == nil && !Task.isCancelled {
            try? await Task.sleep(nanoseconds: 50_000_000)
        }
        await waitForReadableSurface()

        // One transient failure must never kill the stream (Wi-Fi blip, Mac
        // asleep, app suspension): reconnect with backoff. Reconnects resume
        // incrementally from the held offset; streamOutput only pays a full
        // tail replay on first attach or a column change.
        var reconnectDelay: UInt64 = 500_000_000
        while !Task.isCancelled {
            do {
                try await streamOutput()
            } catch is CancellationError {
                return
            } catch {
                guard !Task.isCancelled else { return }
                lastError = "Terminal stream unavailable — retrying"
                try? await Task.sleep(nanoseconds: reconnectDelay)
                reconnectDelay = min(reconnectDelay * 2, 8_000_000_000)
            }
        }
    }

    /// One streaming attempt: shared prologue (grid + auto-fit), then the
    /// transport decision — the `unpeel-host __remote__` WebSocket when the
    /// Mac advertises it, with a silent per-attempt fallback to the HTTP
    /// long-poll (dev bridge, server down, pre-WS build, exited session, or
    /// a mid-stream drop). Throws on the first network failure;
    /// `runOutputLoop` owns the retry policy, and each new attempt re-reads
    /// discovery so WS is retried on the next cycle.
    private func streamOutput() async throws {
        _ = await refreshRemoteGrid(force: true)
        // Phone policy: viewing a session on the phone fits it to this
        // screen automatically; the desktop shows the banner and can take
        // the size back with its X. Runs before the initial replay so the
        // first paint is already phone-shaped.
        if !autoFitAttempted {
            autoFitAttempted = true
            await autoFitToPhone()
            // The fit is applied by the DESKTOP (letterbox + PTY resize);
            // replaying before it lands paints at the old desktop width and
            // is immediately replaced by the phone-width replay — a visible
            // "two terminals" layout shift on open. Bounded settle window so
            // the first paint is already phone-shaped; skipped when the grid
            // already matches (or no fit was requested).
            if let want = lastRequestedRemoteGrid, remoteGrid != want {
                for _ in 0 ..< 6 {
                    try? await Task.sleep(nanoseconds: 200_000_000)
                    _ = await refreshRemoteGrid(force: true)
                    if remoteGrid == want { break }
                }
            }
        }
        if desktopFitActive {
            requestRemoteGridForVisibleViewport()
        }
        // On Unpeel Remote the advertised WSS endpoint is a LAN address the
        // phone can't reach — but the relay has its own fast path: host
        // PUSH over the tunnel (one-way latency per update instead of a
        // long-poll round-trip). Falls back to the long-poll below against
        // older Macs or on any stream drop.
        if client.isRelay {
            switch await streamOutputOverRelayPush() {
            case .restart:
                return
            case .fallback:
                break
            }
            guard !Task.isCancelled else { return }
        }
        if !client.isRelay, let candidate = RemoteTerminalTransportSelector.candidate(
            endpoint: RemoteServerDiscovery.shared.endpoint,
            baseURL: client.baseURL,
            authToken: client.authToken
        ) {
            switch await streamOutputOverWebSocket(candidate) {
            case .restart:
                // Column change / offset moved: return cleanly so
                // `runOutputLoop` starts a fresh attempt immediately (no
                // backoff, no error) — it reconnects WS with a tail replay.
                return
            case .fallback:
                // Silent downgrade: the HTTP long-poll below owns the rest
                // of this attempt. Never surfaced as an error.
                break
            }
            guard !Task.isCancelled else { return }
        }
        try await streamOutputOverHTTP()
    }

    /// The HTTP long-poll transport: initial replay (or incremental resume),
    /// then the incremental poll loop. Also the fallback when the WebSocket
    /// is unavailable — offsets are shared output.bin offsets, so a WS
    /// session hands over mid-stream with no replay.
    private func streamOutputOverHTTP() async throws {
        // Reconnects (Wi-Fi blip, lock/unlock) resume incrementally:
        // output.bin is append-only, so a held offset almost always still
        // points at the tail — the server rebases a stale offset and the
        // poll loop's clear-and-feed handles that rare truncation. A full
        // replay is only needed on first attach or when the column count
        // changed (wrapping broke).
        let needsReplay = outputOffset == nil
            || lastReplayedColumns == 0
            || remoteGrid.columns != lastReplayedColumns
        if needsReplay {
            await waitForGridAlignment()
            await waitForReadableSurface()
            try await replayTail()
        }
        lastError = nil
        markInitialReplayDone()

        while !Task.isCancelled {
            // Brief yield only — pacing comes from the long-poll below, which
            // holds server-side until output exists. New bytes render ~20ms
            // after the TUI draws them instead of a poll interval later.
            try await Task.sleep(nanoseconds: 10_000_000)
            let offsetAtRequest = outputOffset
            let generationAtRequest = replayGeneration
            let next = try await client.outputChunk(
                sessionID: sessionID,
                offset: offsetAtRequest,
                limit: 512 * 1024,
                waitMs: Self.outputLongPollMs
            )
            // A concurrent replay started (or moved the offset) while this
            // request was in flight; feeding the stale chunk would paint the
            // same bytes twice over the fresh replay.
            guard outputOffset == offsetAtRequest,
                  replayGeneration == generationAtRequest
            else { continue }
            // Feed FIRST. This used to await the 1/sec metrics refresh
            // before painting, which serialized a full extra round-trip into
            // the stream — on the relay (~200-400ms RTT) a visible stutter
            // every second and ~30% throughput loss. Fresh bytes never wait
            // on metrics now; the refresh runs concurrently below.
            //
            // A rebased offset means the server replayed from a new position
            // (truncated log or stale offset): clear before feeding, inside
            // the chunk's synchronized-output bracket.
            let rebased = offsetAtRequest.map { next.offset != $0 } ?? true
            if await feed(next, clearFirst: rebased) {
                outputOffset = next.nextOffset
                lastError = nil
            }
            // Metrics refresh rides on output instead of a wall clock: a
            // desktop resize or revert always repaints, so grid changes
            // produce bytes — and an idle session stops costing the Mac a
            // per-second viewport snapshot. Runs OFF the stream's critical
            // path; a detected column change replays the tail through the
            // same generation supersession the resize task uses (in-flight
            // poll chunks re-check the generation and discard themselves).
            if !next.dataBase64.isEmpty, gridRefreshTask == nil {
                gridRefreshTask = Task { @MainActor [weak self] in
                    guard let self else { return }
                    defer { self.gridRefreshTask = nil }
                    guard await self.refreshRemoteGrid(force: false) else { return }
                    if self.desktopFitActive {
                        self.requestRemoteGridForVisibleViewport()
                    }
                    // Row-count changes re-flow locally; only a column change
                    // breaks wrapping and needs a full tail replay. A resize
                    // task in flight owns its own replay — never double-reset.
                    if self.remoteGrid.columns != self.lastReplayedColumns,
                       self.resizeTask == nil {
                        await self.waitForGridAlignment()
                        try? await self.replayTail()
                        self.lastError = nil
                    }
                }
            }
        }
    }

    private enum WebSocketAttemptOutcome {
        /// Use the HTTP long-poll for the rest of this attempt (connect
        /// failure, 409 exited session, incompatible server, pin mismatch,
        /// clean close, or a mid-stream drop). Deliberately silent — WS is
        /// an upgrade, never an error surface; the next reconnect cycle
        /// retries WS with fresh discovery.
        case fallback
        /// End this attempt cleanly so `runOutputLoop` immediately starts a
        /// fresh one: the held offset/columns changed under the stream
        /// (column change, resize replay) and the reconnect replays the
        /// tail at the new position — over WS again.
        case restart
    }

    /// One WebSocket streaming attempt against `unpeel-host __remote__`.
    /// Replay discipline mirrors the HTTP path exactly: a fresh attach or a
    /// column change resets + repaints from the server's tail replay; a
    /// resume (offset in the URL) feeds incrementally from the held offset;
    /// a `rebased` hello is the gap case and clears before feeding. Every
    /// payload goes through the same DEC-2026-bracketed feed and the same
    /// output.bin offsets, so WS and HTTP hand over mid-session either way.
    /// Fetch `[from, upTo)` over HTTP and feed it, catching the terminal up
    /// after it fell behind the live WS broadcaster — so a byte gap is closed
    /// without tearing the connection down. Loops because the gap can exceed
    /// one chunk (and the host withholds a trailing partial escape sequence,
    /// which the next round re-fetches). Advances `outputOffset` as it feeds;
    /// stops on no forward progress (a superseding replay, or a withheld
    /// boundary) and lets the caller decide.
    private func fillOutputGap(from: UInt64, upTo: UInt64) async {
        var cursor = from
        var rounds = 0
        while cursor < upTo {
            rounds += 1
            if rounds > 32 { return } // bounded catch-up per frame
            let want = Int(min(upTo - cursor, 512 * 1024))
            guard want > 0,
                  let chunk = try? await client.outputChunk(
                      sessionID: sessionID, offset: cursor, limit: want
                  ),
                  chunk.offset == cursor,
                  !chunk.truncated,
                  chunk.nextOffset > cursor
            else { return }
            guard await feed(chunk, clearFirst: false) else { return }
            cursor = chunk.nextOffset
            outputOffset = cursor
        }
    }

    /// Initial send budget granted at subscribe; replenished frame-by-frame
    /// as bytes are fed. ~4 max-size push frames in flight.
    private static let relayPushInitialCredit = 384 * 1024

    /// Host-push transport over the relay: subscribe once, then the Mac
    /// streams output frames as they are produced — per-update cost is
    /// one-way delivery instead of the long-poll's full round-trip, which
    /// is the difference between ~3-6 updates/sec and display-rate on
    /// cellular. Ends (or 404s on an older Mac) → silent fallback to the
    /// HTTP long-poll, mirroring the LAN WS transport's contract.
    private func streamOutputOverRelayPush() async -> WebSocketAttemptOutcome {
        let needsReplay = outputOffset == nil
            || lastReplayedColumns == 0
            || remoteGrid.columns != lastReplayedColumns
        let resumeOffset = needsReplay ? nil : outputOffset
        guard let frames = await client.relayOutputSubscribe(
            sessionID: sessionID,
            offset: resumeOffset,
            credit: Self.relayPushInitialCredit
        ) else { return .fallback }
        defer { client.relayOutputStop(sessionID: sessionID) }

        var pendingReset = needsReplay
        if pendingReset {
            await waitForGridAlignment()
            await waitForReadableSurface()
            // Single-flight, same as replayTail: this stream's replay
            // supersedes any older in-flight replay or poll chunk.
            replayGeneration &+= 1
        }
        var bootstrapBytes = Data()
        var bootstrapNextIndex = 0
        var bootstrapEncoding: RelayBootstrapEncoding?
        var bootstrapUncompressedBytes = 0
        var bootstrapEndOffset: UInt64 = 0
        var bootstrapGridChanged = false

        for await frame in frames {
            guard !Task.isCancelled else { return .restart }
            guard frame.stream == sessionID else { continue }
            // Grid rides the frames (first frame + on change) — no separate
            // metrics round-trips while streaming.
            var gridChanged = false
            if let cols = frame.cols, let rows = frame.rows {
                gridChanged = adoptRemoteGrid(columns: cols, rows: rows)
                if desktopFitActive {
                    requestRemoteGridForVisibleViewport()
                }
                if !pendingReset, remoteGrid.columns != lastReplayedColumns,
                   resizeTask == nil {
                    // Wrapping broke — resubscribe with a tail replay.
                    return .restart
                }
            }
            let payload = frame.data
            if let part = frame.bootstrap {
                guard pendingReset,
                      part.index == bootstrapNextIndex,
                      part.uncompressedBytes >= 0,
                      part.uncompressedBytes <= RelayBootstrapCodec.maximumUncompressedBytes,
                      bootstrapBytes.count + payload.count
                        <= RelayBootstrapCodec.maximumUncompressedBytes
                else { return .restart }
                if part.index == 0 {
                    bootstrapEncoding = part.encoding
                    bootstrapUncompressedBytes = part.uncompressedBytes
                    bootstrapEndOffset = part.endOffset
                } else {
                    guard part.encoding == bootstrapEncoding,
                          part.uncompressedBytes == bootstrapUncompressedBytes,
                          part.endOffset == bootstrapEndOffset
                    else { return .restart }
                }
                bootstrapGridChanged = bootstrapGridChanged || gridChanged
                bootstrapBytes.append(payload)
                bootstrapNextIndex += 1
                guard part.final else { continue }
                guard let encoding = bootstrapEncoding,
                      let replay = RelayBootstrapCodec.decode(
                          bootstrapBytes,
                          encoding: encoding,
                          uncompressedBytes: bootstrapUncompressedBytes
                      )
                else { return .restart }
                if bootstrapGridChanged {
                    await waitForGridAlignment()
                }
                guard await feedRaw(replay, resetFirst: true) else { return .restart }
                pendingReset = false
                outputOffset = bootstrapEndOffset
                lastReplayedColumns = targetGrid.columns
                lastError = nil
                markInitialReplayDone()
                continue
            }
            if pendingReset {
                // A fresh strict relay subscription must start with the
                // complete bootstrap envelope; painting an arbitrary live
                // tail recreates the blank-grid bug this protocol prevents.
                return .restart
            }
            if frame.rebased == true {
                // Server replayed from a new position (truncated log or a
                // stale resume offset): clear before feeding.
                guard await feedRaw(payload, clearFirst: true) else { return .restart }
                outputOffset = frame.offset + UInt64(payload.count)
                lastReplayedColumns = targetGrid.columns
                lastError = nil
                client.relayOutputCredit(sessionID: sessionID, bytes: max(payload.count, 1))
                continue
            }
            // Steady state: reconcile the frame's offset against ours —
            // fill small gaps over the tunnel, trim overlaps, drop stale
            // duplicates; never tear the stream down on a mismatch.
            if let held = outputOffset {
                switch StreamFrameReconciler.action(
                    held: held,
                    frameOffset: frame.offset,
                    frameLength: payload.count
                ) {
                case .feed:
                    break
                case .skip:
                    client.relayOutputCredit(sessionID: sessionID, bytes: max(payload.count, 1))
                    continue
                case .feedSuffix(let dropLeading):
                    let tail = Data(payload.dropFirst(dropLeading))
                    guard await feedRaw(tail) else { return .restart }
                    outputOffset = held + UInt64(tail.count)
                    lastError = nil
                    client.relayOutputCredit(sessionID: sessionID, bytes: max(payload.count, 1))
                    continue
                case .fillGap(let from, let upTo):
                    await fillOutputGap(from: from, upTo: upTo)
                    guard outputOffset == frame.offset else { return .restart }
                }
            }
            guard await feedRaw(payload) else { return .restart }
            outputOffset = frame.offset + UInt64(payload.count)
            lastError = nil
            client.relayOutputCredit(sessionID: sessionID, bytes: max(payload.count, 1))
        }
        // Stream ended: relay connection torn down, host ended the stream,
        // or the session exited. Silent downgrade; the next attempt
        // re-subscribes (or long-polls).
        return .fallback
    }

    private func streamOutputOverWebSocket(
        _ candidate: RemoteTerminalWebSocketCandidate
    ) async -> WebSocketAttemptOutcome {
        let needsReplay = outputOffset == nil
            || lastReplayedColumns == 0
            || remoteGrid.columns != lastReplayedColumns
        let resumeOffset = needsReplay ? nil : outputOffset
        guard let url = RemoteTerminalTransportSelector.webSocketOutputURL(
            host: candidate.host,
            port: candidate.port,
            sessionID: sessionID,
            token: candidate.token,
            offset: resumeOffset
        ) else { return .fallback }

        let connection = RemoteTerminalWebSocketConnection(
            url: url,
            pinnedFingerprint: candidate.certificateFingerprint
        )
        defer {
            inputRouter.retireWebSocket(connection)
            connection.close()
        }

        guard let hello = try? await connection.receiveHello(),
              hello.sessionID == sessionID
        else { return .fallback }

        // The hello carries the PTY grid — same handling as a metrics poll,
        // and it lands before the replay so grid alignment can use it.
        if let cols = hello.cols, let rows = hello.rows {
            _ = adoptRemoteGrid(columns: cols, rows: rows)
            if desktopFitActive {
                requestRemoteGridForVisibleViewport()
            }
        }
        // A column change surfaced by the hello itself: the resume offset we
        // connected with is useless (wrapping broke) — reconnect fresh so
        // the next attempt asks for a tail replay.
        if !needsReplay, remoteGrid.columns != lastReplayedColumns {
            return .restart
        }

        var pendingReset = needsReplay
        var pendingClear = !needsReplay
            && (hello.rebased || resumeOffset.map { hello.startOffset != $0 } ?? false)
        if pendingReset || pendingClear {
            await waitForGridAlignment()
            await waitForReadableSurface()
            // Single-flight, same as replayTail: this connection's replay
            // supersedes any older in-flight replay or poll chunk.
            replayGeneration &+= 1
            // Nothing buffered to replay (fresh/empty log): present the
            // clear immediately so the skeleton lifts without waiting for
            // the first output byte. Otherwise the clear stays deferred into
            // the first frame's synchronized bracket — no blank flash.
            if hello.outputSize <= hello.startOffset {
                guard await feedRaw(nil, clearFirst: pendingClear, resetFirst: pendingReset) else {
                    return .restart
                }
                pendingReset = false
                pendingClear = false
                outputOffset = hello.startOffset
                lastReplayedColumns = targetGrid.columns
            }
        }

        inputRouter.adoptWebSocket(connection)
        if !pendingReset, !pendingClear {
            lastError = nil
            markInitialReplayDone()
        }

        while !Task.isCancelled {
            let message: URLSessionWebSocketTask.Message
            do {
                message = try await connection.receive()
            } catch {
                // Includes the clean closes (1000 "session exited" — the
                // HTTP fallback serves exited sessions, matching the HTTP
                // path's behavior — and "output stream closed") as well as
                // 1011/timeouts. Always the silent downgrade.
                return .fallback
            }
            switch message {
            case .string(let text):
                // In-stream error frames are non-fatal; the stream continues.
                if case .error(let detail) = RemoteTerminalWSServerMessage.parse(text: text) {
                    NSLog("[UnpeelIOS] ws output stream error: \(detail)")
                }
            case .data(let raw):
                guard let frame = RemoteTerminalWSBinaryFrame.parse(raw) else {
                    return .fallback
                }
                let reset = pendingReset
                let clear = pendingClear
                // On a live frame (not the reset/clear baseline), reconcile
                // its offset against ours instead of tearing the whole
                // connection down on any mismatch — the old behavior churned
                // forever on a fast, large session (the "disconnected, can't
                // reconnect" loop). See StreamFrameReconciler.
                if !reset, !clear, let held = outputOffset {
                    switch StreamFrameReconciler.action(
                        held: held,
                        frameOffset: frame.offset,
                        frameLength: frame.payload.count
                    ) {
                    case .feed:
                        break // contiguous — feed the whole frame below
                    case .skip:
                        // Stale/replayed frame — drop it, stay connected.
                        continue
                    case .feedSuffix(let dropLeading):
                        // Overlaps our position — feed only the new tail.
                        let tail = Data(frame.payload.dropFirst(dropLeading))
                        guard await feedRaw(tail, clearFirst: false, resetFirst: false) else {
                            return .restart
                        }
                        outputOffset = held + UInt64(tail.count)
                        continue
                    case .fillGap(let from, let upTo):
                        // We fell behind the live broadcaster: fetch the
                        // missing bytes over HTTP and feed them, keeping the
                        // WS connection alive (no reconnect, no skeleton).
                        await fillOutputGap(from: from, upTo: upTo)
                        if outputOffset == frame.offset {
                            break // caught up — feed this frame contiguously
                        } else if let now = outputOffset, now > from {
                            // Partial catch-up; the reconciler resolves the
                            // next frame from the advanced offset.
                            continue
                        } else {
                            return .restart
                        }
                    }
                }
                pendingReset = false
                pendingClear = false
                guard await feedRaw(frame.payload, clearFirst: clear, resetFirst: reset) else {
                    // Superseded by a newer replay mid-feed — realign.
                    return .restart
                }
                outputOffset = frame.offset + UInt64(frame.payload.count)
                if reset || clear {
                    lastReplayedColumns = targetGrid.columns
                }
                lastError = nil
                markInitialReplayDone()
                // Metrics ride on output, same as the HTTP loop: grid
                // changes always repaint (produce bytes), and an idle
                // session costs the Mac nothing.
                if await refreshRemoteGrid(force: false) {
                    if desktopFitActive {
                        requestRemoteGridForVisibleViewport()
                    }
                    if remoteGrid.columns != lastReplayedColumns, resizeTask == nil {
                        // Column change broke wrapping — reconnect for a
                        // tail replay at the new width.
                        return .restart
                    }
                }
            @unknown default:
                break
            }
        }
        return .fallback
    }

    private func replayTail() async throws {
        // Single-flight: this replay supersedes any older in-flight replay
        // and any in-flight poll chunk (they re-check the generation after
        // their awaits and discard). Nothing touches the terminal until the
        // fetch lands, so racing replays can never interleave resets — the
        // newest one paints exactly once.
        replayGeneration &+= 1
        let generation = replayGeneration
        let initial = try await client.outputChunk(
            sessionID: sessionID,
            offset: nil,
            limit: Self.initialReplayLimit
        )
        // A newer replay started while this one was fetching; its feed owns
        // the screen now — feeding this tail too would paint it twice.
        guard replayGeneration == generation else { return }
        guard await feed(initial, resetFirst: true) else { return }
        outputOffset = initial.nextOffset
        lastReplayedColumns = targetGrid.columns
    }

    /// Feeds one output chunk into the terminal as a single synchronized-
    /// output frame (DEC 2026), so mid-repaint intermediate states from the
    /// remote TUI never present as a visible flash. The base64 decode and
    /// mouse-mode scan run off the main actor, and large payloads feed in
    /// bounded slices with yields between them — safe inside the bracket,
    /// nothing presents mid-feed. `resetFirst` is the full replay prelude
    /// (CAN + ESC c + clear); `clearFirst` requests the same parser-safe reset
    /// for a rebase/truncation. Returns
    /// false when a newer replay superseded this feed mid-flight (its bytes
    /// were discarded; the superseding replay repaints everything).
    @discardableResult
    private func feed(
        _ chunk: RemoteTerminalOutputChunk,
        clearFirst: Bool = false,
        resetFirst: Bool = false
    ) async -> Bool {
        let generation = replayGeneration
        let needsClear = clearFirst || resetFirst || chunk.truncated
        let needsReset = needsClear
        if needsClear {
            mouseModeTracker.reset()
            queryFilter.reset()
        }
        let data = await Self.decodeChunk(
            chunk.dataBase64, scanningInto: mouseModeTracker, filteringWith: queryFilter
        )
        guard replayGeneration == generation else { return false }
        return await feedPrepared(
            data,
            needsClear: needsClear,
            resetFirst: needsReset,
            generation: generation
        )
    }

    /// WS variant of `feed`: payloads arrive as already-decoded raw bytes
    /// (no base64 step), so only the mouse-mode scan runs off-main before
    /// the same bracketed feed. Same supersession semantics.
    @discardableResult
    private func feedRaw(
        _ payload: Data?,
        clearFirst: Bool = false,
        resetFirst: Bool = false
    ) async -> Bool {
        let generation = replayGeneration
        let needsClear = clearFirst || resetFirst
        let needsReset = needsClear
        if needsClear {
            mouseModeTracker.reset()
            queryFilter.reset()
        }
        let data = await Self.scanChunk(
            payload, scanningInto: mouseModeTracker, filteringWith: queryFilter
        )
        guard replayGeneration == generation else { return false }
        return await feedPrepared(
            data,
            needsClear: needsClear,
            resetFirst: needsReset,
            generation: generation
        )
    }

    /// Shared tail of both feed paths (HTTP base64 chunks and WS raw
    /// frames): the DEC 2026 bracket, reset/clear prelude, and bounded
    /// slice feeding with generation re-checks between slices.
    private func feedPrepared(
        _ data: Data?,
        needsClear: Bool,
        resetFirst: Bool,
        generation: Int
    ) async -> Bool {
        guard needsClear || data != nil else { return true }

        // Once the remote TUI drives DEC 2026 itself, stop nesting the
        // client's own bracket around its frames (it collides and leaks
        // fragments like `026l`). Latches for the session on first sight.
        if let data, !remoteManagesSyncOutput, Self.containsSyncOutput(data) {
            remoteManagesSyncOutput = true
        }
        // Reset/clear feeds are ALWAYS bracketed, latch or not: the render
        // pump draws at display rate, so an unshielded ESC c + clear
        // presents as a visible blank flash before the replay tail lands
        // (cached frame → blank → content). A one-shot bracket around a
        // full replay can't collide with the TUI's own mid-stream brackets
        // the way per-chunk nesting did.
        let injectBracket = !remoteManagesSyncOutput || needsClear

        feedSerial &+= 1
        let serial = feedSerial
        // One atomic prelude write, with the reset BEFORE the bracket: RIS
        // (ESC c) resets all modes including DEC 2026, so a bracket opened
        // first was silently dropped and the clear + tail presented
        // unshielded — the residual one-frame blink on session open. The
        // single write also gives the core no gap to snapshot mid-prelude.
        var prelude = Data()
        if resetFirst {
            prelude.append(Self.resetTerminalState)
        }
        if injectBracket {
            prelude.append(Self.beginSynchronizedOutput)
            syncBracketOwner = serial
        }
        if needsClear {
            // Belt and braces alongside ESC c: ghostty's RIS-vs-scrollback
            // semantics are unverifiable (binary xcframework), and every
            // other clear path uses the CSI 3J-carrying constant.
            prelude.append(Self.clearScreen)
        }
        if !prelude.isEmpty {
            memorySession.receive(prelude)
        }
        // The bracket must always close — but only by its current owner: if
        // a superseding feed already opened its own bracket, closing here
        // would present that feed's half-painted frame.
        defer {
            if injectBracket, syncBracketOwner == serial {
                memorySession.receive(Self.endSynchronizedOutput)
                syncBracketOwner = 0
            }
        }
        if let data {
            var index = data.startIndex
            while index < data.endIndex {
                let end = data.index(
                    index, offsetBy: Self.feedSliceBytes, limitedBy: data.endIndex
                ) ?? data.endIndex
                memorySession.receive(data[index..<end])
                index = end
                if index < data.endIndex {
                    await Task.yield()
                    guard replayGeneration == generation else { return false }
                }
            }
        }
        // Snap-to-tail only on full repaints (replay/rebase/truncation);
        // ghostty already follows the tail at bottom for ordinary chunks,
        // and a per-chunk snap could yank a user who just scrolled up.
        if needsClear {
            scrollToBottomSoon()
        }
        scheduleTuiJumpHintScan()
        return true
    }

    /// Off-main-actor chunk work: the base64 decode plus the mouse-mode byte
    /// scan (both O(n) over the payload), so a large replay never stalls the
    /// main thread in one turn.
    private nonisolated static func decodeChunk(
        _ base64: String,
        scanningInto tracker: RemoteTerminalMouseModeTracker,
        filteringWith filter: TerminalQueryFilter
    ) async -> Data? {
        await Task.detached(priority: .userInitiated) {
            guard let data = Data(base64Encoded: base64), !data.isEmpty else { return nil }
            tracker.feed(data)
            return filter.stripRequests(data)
        }.value
    }

    /// Raw-bytes counterpart of `decodeChunk`: the mouse-mode scan (O(n)
    /// over the payload) still runs off the main actor so a large WS replay
    /// frame never stalls the main thread in one turn.
    private nonisolated static func scanChunk(
        _ payload: Data?,
        scanningInto tracker: RemoteTerminalMouseModeTracker,
        filteringWith filter: TerminalQueryFilter
    ) async -> Data? {
        guard let payload, !payload.isEmpty else { return nil }
        return await Task.detached(priority: .userInitiated) {
            tracker.feed(payload)
            return filter.stripRequests(payload)
        }.value
    }

    private func scrollToBottomSoon() {
        guard scrollToBottomTask == nil else { return }
        scrollToBottomTask = Task { @MainActor [weak self] in
            guard let self else { return }
            try? await Task.sleep(nanoseconds: 50_000_000)
            defer { scrollToBottomTask = nil }
            guard !Task.isCancelled else { return }
            terminal.surface?.performBindingAction("scroll_to_bottom")
            scrollToBottomVisible = false
        }
    }

    /// Matches Claude Code's virtual-scroll hint chip in its two shapes —
    /// "Jump to bottom (ctrl+End)" and "3 new messages (ctrl+End) ↓" — and
    /// nothing looser, and only within the bottom rows of the viewport,
    /// where Claude pins the real chip (just above its composer). Quoted
    /// hint text higher up in the transcript must not match: a spurious
    /// ctrl+End at a TUI that is not scrolled up lands as literal "[1;5F"
    /// junk in the composer. Keep aligned with the desktop matcher in
    /// `GhosttyBridge.swift`.
    static func viewportHasTuiJumpHint(_ text: String) -> Bool {
        let tail = text
            .split(separator: "\n", omittingEmptySubsequences: false)
            .suffix(15)
            .joined(separator: "\n")
        if tail.contains("Jump to bottom (ctrl+End)") { return true }
        return tail.firstMatch(of: /\d+ new messages? \(ctrl\+End\)/) != nil
    }
    /// macOS virtual keycode for End (kVK_End; ghostty's iOS keycode table
    /// uses AppKit codes). The jump is sent as a real ctrl+End KEY EVENT
    /// through the local surface's key encoder, whose output rides the
    /// host-managed write path to the remote PTY exactly like typing.
    /// Sending raw CSI bytes broke TUIs that negotiated the kitty keyboard
    /// protocol (Claude Code): they showed a literal "[1;5F" in the
    /// composer instead of jumping.
    private static let endKeycode: UInt32 = 119

    /// An agent-rendered select menu is on screen (Codex/Claude "pick an
    /// option" prompts). These fire NO hook — no Stop, no PermissionRequest —
    /// so the activity engine keeps showing "busy" and nothing else flags
    /// "waiting for a choice". The menu always declares itself in the viewport
    /// footer, though, so we detect it by scanning the rendered text. Drives
    /// the on-screen menu control bar (↑/↓/Enter/Esc + number keys), so the
    /// user can answer without summoning the keyboard.
    @Published private(set) var menuPromptActive = false
    /// Highest leading option number found in the menu ("1." … "N."), so the
    /// control bar can offer direct number shortcuts. 0 when unknown.
    @Published private(set) var menuOptionCount = 0

    /// Footer phrases a select menu prints to advertise its keys. Detection
    /// requires a marker pair on the same or adjacent rows — nav + select, or
    /// confirm + cancel — so ordinary transcript prose mentioning "select"
    /// can't trip it. Kept in sync with the host-side twin
    /// (`crates/unpeel-core/src/menu_prompt.rs`).
    private static let menuNavMarkers = ["to navigate", "↑/↓", "↑ ↓", "▲/▼", "arrow keys"]
    private static let menuSelectMarkers = [
        "to select", "to confirm", "to choose", "enter to", "esc to cancel", "return to",
    ]
    /// Confirm-key phrases for footers that name a confirm and a cancel key
    /// but no navigation hint at all — Codex's approval menu prints only
    /// "Press enter to confirm or esc to cancel" under its numbered options.
    private static let menuConfirmMarkers = ["enter to confirm", "return to confirm"]
    /// Cancel-key phrases paired with `menuConfirmMarkers`.
    private static let menuCancelMarkers = ["esc to cancel", "escape to cancel"]
    /// Phrases marking a hint row as a passive status footer, not an
    /// answerable menu — Claude Code's subagent list pins
    /// "↑/↓ to select · Enter to view" for the whole run.
    private static let menuPassiveMarkers = ["to view"]

    /// Debounced post-feed scan for the TUI's jump hint: at most one scan
    /// per 250ms while output streams, and the last chunk always gets a
    /// scan after it, so the hint settles with the screen.
    private func scheduleTuiJumpHintScan() {
        guard tuiJumpHintScanTask == nil else { return }
        tuiJumpHintScanTask = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: 250_000_000)
            guard let self else { return }
            defer { self.tuiJumpHintScanTask = nil }
            guard !Task.isCancelled else { return }
            self.scanForTuiJumpHint()
            self.scanForMenuPrompt()
        }
    }

    /// Scan the rendered viewport for a select-menu footer (see
    /// `menuPromptActive`). Provider-agnostic and hook-free: it only trusts
    /// the on-screen text, so it works for agent-drawn menus that emit no
    /// lifecycle event. Not gated on the alternate screen — Claude/Codex draw
    /// these inline in the primary screen.
    private func scanForMenuPrompt() {
        guard let text = terminal.surface?.readViewportText() else {
            if menuPromptActive { menuPromptActive = false }
            if menuOptionCount != 0 { menuOptionCount = 0 }
            return
        }
        let active = Self.viewportHasMenuPrompt(text)
        if menuPromptActive != active {
            menuPromptActive = active
        }
        let count = active ? Self.highestMenuOptionNumber(in: text) : 0
        if menuOptionCount != count {
            menuOptionCount = count
        }
    }

    /// True when the viewport text contains a select-menu footer on the same
    /// or adjacent rows (menu footers are one hint line, two when wrapped),
    /// excluding passive status footers whose Enter action is "view". Two
    /// footer shapes qualify: a nav hint plus a select hint (Claude-style),
    /// or a confirm key named next to a cancel key (Codex-style, which
    /// prints no nav hint). Twin of the Rust `viewport_has_menu_prompt` in
    /// `menu_prompt.rs` — keep aligned.
    static func viewportHasMenuPrompt(_ text: String) -> Bool {
        let lines = text
            .split(separator: "\n", omittingEmptySubsequences: false)
            .map { $0.lowercased() }
        for index in lines.indices {
            let rawWindow = index + 1 < lines.endIndex
                ? lines[index] + "\n" + lines[index + 1]
                : lines[index]
            // A narrow terminal (phone fit) can wrap a footer mid-phrase,
            // e.g. "… · Enter to\n   view" — collapse whitespace runs so
            // multi-word markers still match across the wrap. Without this
            // the passive "to view" guard misses and the pinned subagent
            // footer reads as an answerable menu.
            let window = rawWindow.split(whereSeparator: \.isWhitespace).joined(separator: " ")
            let hasNav = menuNavMarkers.contains { window.contains($0) }
            let hasSelect = menuSelectMarkers.contains { window.contains($0) }
            let hasConfirm = menuConfirmMarkers.contains { window.contains($0) }
            let hasCancel = menuCancelMarkers.contains { window.contains($0) }
            let passive = menuPassiveMarkers.contains { window.contains($0) }
            if ((hasNav && hasSelect) || (hasConfirm && hasCancel)) && !passive { return true }
        }
        return false
    }

    /// Highest leading "N." option number in the viewport (menu items look
    /// like "❯ 1. …" / "2. …"). Caps at 9 — number shortcuts past that stop
    /// being single keypresses. 0 when none is found.
    private static func highestMenuOptionNumber(in text: String) -> Int {
        var highest = 0
        for rawLine in text.split(whereSeparator: \.isNewline) {
            var line = Substring(rawLine)
            while let first = line.first,
                  first == " " || first == "\t" || first == "❯" || first == ">" || first == "•" {
                line = line.dropFirst()
            }
            let digits = line.prefix(while: \.isNumber)
            guard !digits.isEmpty,
                  line.dropFirst(digits.count).first == ".",
                  let value = Int(digits), value >= 1, value <= 9
            else { continue }
            highest = max(highest, value)
        }
        return highest
    }

    /// A key the menu control bar can send. Arrows honor DECCKM so they land
    /// in the form the remote TUI expects; the rest are fixed bytes.
    enum MenuControlKey {
        case up
        case down
        case enter
        case escape
        case digit(Int)
    }

    /// Send a menu control key straight to the remote PTY through the same
    /// ordered write queue the jump-key uses (proven remote-input path).
    func sendMenuControlKey(_ key: MenuControlKey) {
        let appCursor = mouseModeTracker.applicationCursorKeysEnabled
        let bytes: [UInt8]
        switch key {
        case .up:
            bytes = appCursor ? [0x1b, 0x4f, 0x41] : [0x1b, 0x5b, 0x41]
        case .down:
            bytes = appCursor ? [0x1b, 0x4f, 0x42] : [0x1b, 0x5b, 0x42]
        case .enter:
            bytes = [0x0d]
        case .escape:
            bytes = [0x1b]
        case let .digit(value):
            bytes = Array(String(value).utf8)
        }
        writeQueue.enqueue(Data(bytes))
    }

    private func scanForTuiJumpHint() {
        // While a jump retry loop is in flight the button stays hidden; the
        // loop checks the screen itself and runs a settle scan when done.
        guard tuiJumpRetryTask == nil else { return }
        // Same while a remote-wheel scroll is dragging or gliding: showing
        // the button mid-easing invited taps whose ctrl+End fought the
        // still-decaying scroll-up wheel events. The gesture/momentum end
        // triggers a settle scan that re-evaluates.
        if remoteWheelSettling {
            if tuiJumpHintVisible { tuiJumpHintVisible = false }
            return
        }
        // No alternate-screen gate: Claude Code draws its virtual-scroll
        // transcript in the PRIMARY screen, so gating on alt-screen meant the
        // hint never fired for it on the phone. Scanning the viewport
        // unconditionally is safe because the shared button's action both
        // fakes ctrl+End and scrolls the local surface to the bottom — even a
        // marker matched inside old scrolled-back output still yields a
        // "jump to bottom" outcome.
        guard let text = terminal.surface?.readViewportText() else {
            if tuiJumpHintVisible { tuiJumpHintVisible = false }
            return
        }
        let active = Self.viewportHasTuiJumpHint(text)
        if tuiJumpHintVisible != active {
            tuiJumpHintVisible = active
        }
    }

    /// Fakes the ctrl+End press the hint names (CSI 1;5F, the bytes a real
    /// keyboard sends) through the ordered input pipeline — then verifies.
    /// The remote TUI receives the key and repaints asynchronously, and a
    /// single press can land mid-stream and leave the hint up (users had to
    /// tap repeatedly), so this re-scans and re-sends until the hint is gone,
    /// bounded to a few attempts. A user touch-scroll cancels the loop so it
    /// never fights an intentional scroll-up.
    func jumpToTuiBottomIfNeeded() {
        guard tuiJumpHintVisible else { return }
        tuiJumpRetryTask?.cancel()
        // The button hides on the first press and stays hidden for the whole
        // retry loop (the ambient scan is suppressed too) — re-showing it
        // between attempts made it flicker. The loop checks the screen text
        // directly; only a hint that survives every retry re-shows the
        // button, via the final settle scan.
        tuiJumpHintVisible = false
        tuiJumpRetryTask = Task { @MainActor [weak self] in
            // Two attempts max: a press that lands mid-stream gets one
            // follow-up, but a false-positive match (quoted hint text in
            // the transcript) dumps at most two "[1;5F" residues into the
            // composer instead of five.
            for _ in 0..<2 {
                guard let self, !Task.isCancelled else { return }
                self.terminal.surface?.sendControlKeyPress(keycode: Self.endKeycode)
                try? await Task.sleep(nanoseconds: 600_000_000)
                guard !Task.isCancelled else { return }
                let stillUp = (self.terminal.surface?.readViewportText())
                    .map(Self.viewportHasTuiJumpHint) ?? false
                if !stillUp {
                    break
                }
            }
            guard let self, !Task.isCancelled else { return }
            self.tuiJumpRetryTask = nil
            self.scanForTuiJumpHint()
        }
    }

    private func updateCanvasSize(grid: RemoteTerminalGrid) -> Bool {
        if targetGrid != grid {
            gridAlignmentExtra = .zero
        }
        targetGrid = grid
        return recalculateCanvasSize()
    }

    private func handleLocalSurfaceSize(_ size: TerminalGridMetrics?) {
        guard let size else { return }
        let nextGrid = RemoteTerminalGrid(columns: Int(size.columns), rows: Int(size.rows))
        localGrid = nextGrid
        var needsRecalculate = false
        if let nextCellSize = Self.cellSizePoints(from: size),
           Self.cellSize(nextCellSize, differsFrom: measuredCellSize)
        {
            measuredCellSize = nextCellSize
            needsRecalculate = true
        }
        if adjustCanvasForGrid(nextGrid) {
            needsRecalculate = true
        }
        if needsRecalculate {
            _ = recalculateCanvasSize()
        }
        if needsRecalculate {
            requestRemoteGridForVisibleViewport()
        }
    }

    private func recalculateCanvasSize() -> Bool {
        var next = RemoteTerminalCanvasLayout.canvasSize(
            columns: targetGrid.columns,
            rows: targetGrid.rows,
            cellSize: measuredCellSize,
            horizontalPadding: Self.horizontalPadding,
            verticalPadding: Self.verticalPadding
        )
        next.width += Self.gridAlignmentSlop
        next.height += Self.gridAlignmentSlop
        next.width += gridAlignmentExtra.width
        next.height += gridAlignmentExtra.height

        let clamped = CGSize(
            width: min(max(next.width, 320), 2_800),
            height: min(max(next.height, 260), 2_400)
        )
        // Slack updates even when the canvas size itself is unchanged
        // (grid/cell-metric changes move the content height within the
        // same clamped canvas).
        let contentHeight = CGFloat(max(targetGrid.rows, 1)) * measuredCellSize.height
            + Self.verticalPadding * 2
        let slack = max(0, clamped.height - contentHeight)
        if abs(canvasBottomSlack - slack) > 0.5 {
            canvasBottomSlack = slack
        }
        guard abs(canvasSize.width - clamped.width) > 2 || abs(canvasSize.height - clamped.height) > 2 else {
            return false
        }
        canvasSize = clamped
        return true
    }

    private var isGridAligned: Bool {
        localGrid.columns == targetGrid.columns && localGrid.rows >= targetGrid.rows
    }

    private func adjustCanvasForGrid(_ grid: RemoteTerminalGrid) -> Bool {
        guard targetGrid.columns > 0, targetGrid.rows > 0 else { return false }
        var nextExtra = gridAlignmentExtra

        if grid.columns < targetGrid.columns {
            let deficit = targetGrid.columns - grid.columns
            nextExtra.width += CGFloat(deficit) * measuredCellSize.width + Self.gridAlignmentSlop
        }
        if grid.rows < targetGrid.rows {
            let deficit = targetGrid.rows - grid.rows
            nextExtra.height += CGFloat(deficit) * measuredCellSize.height + Self.gridAlignmentSlop
        }

        nextExtra.width = min(max(nextExtra.width, 0), measuredCellSize.width * 4)
        nextExtra.height = min(max(nextExtra.height, 0), measuredCellSize.height * 4)
        guard abs(nextExtra.width - gridAlignmentExtra.width) > 0.5
            || abs(nextExtra.height - gridAlignmentExtra.height) > 0.5
        else { return false }
        gridAlignmentExtra = nextExtra
        return true
    }

    private func refreshRemoteGrid(force: Bool) async -> Bool {
        let now = ContinuousClock.now
        if !force, now - lastMetricsRefresh < .seconds(1) {
            return false
        }
        lastMetricsRefresh = now
        do {
            let metrics = try await client.terminalMetrics(sessionID: sessionID)
            desktopViewingSameSession = metrics.desktopViewing
            let changed = adoptRemoteGrid(columns: metrics.columns, rows: metrics.rows)
            // Unfitted but nobody at the Mac is looking: fit silently
            // instead of leaving the manual button up.
            if !desktopFitActive { autoRefitIfUnwatched() }
            return changed
        } catch {
            if force {
                gridAlignmentExtra = .zero
                return updateCanvasSize(grid: Self.fallbackGrid)
            }
            return false
        }
    }

    /// Applies a freshly observed remote grid (metrics poll or WS hello):
    /// clamping, phone-fit revert detection, canvas update. Returns whether
    /// anything the layout depends on changed.
    private func adoptRemoteGrid(columns: Int, rows: Int) -> Bool {
        let nextGrid = RemoteTerminalGrid(
            columns: min(max(columns, Self.minimumColumns), Self.maximumColumns),
            rows: min(max(rows, Self.minimumRows), Self.maximumRows)
        )
        let gridChanged = nextGrid != remoteGrid || nextGrid != targetGrid
        remoteGrid = nextGrid
        hasRemoteGrid = true
        // The observed grid no longer resembles the phone-fit request.
        // If the Mac is showing this session, that's the user reverting
        // (the banner's X) — drop fit mode instead of fighting them over
        // the PTY size. If the Mac ISN'T showing it, nobody there asked
        // for this size (PTY restart, stale resize, window churn), so the
        // phone re-asserts its fit instead of falling back to the button.
        if desktopFitActive, resizeTask == nil,
           let requested = lastRequestedRemoteGrid,
           abs(nextGrid.columns - requested.columns) > 2 {
            if desktopViewingSameSession == false {
                autoRefitIfUnwatched()
            } else {
                desktopFitActive = false
                lastRequestedRemoteGrid = nil
            }
        }
        let canvasChanged = updateCanvasSize(grid: nextGrid)
        return gridChanged || canvasChanged
    }

    private func waitForGridAlignment() async {
        for _ in 0..<12 {
            if isGridAligned { return }
            _ = updateCanvasSize(grid: targetGrid)
            do {
                try await Task.sleep(nanoseconds: 60_000_000)
            } catch {
                return
            }
        }
        try? await Task.sleep(nanoseconds: 140_000_000)
    }

    private func waitForReadableSurface() async {
        for _ in 0..<20 {
            if memorySession.readViewportText() != nil { return }
            do {
                try await Task.sleep(nanoseconds: 50_000_000)
            } catch {
                return
            }
        }
    }

    /// `force` re-issues the letterbox even when the desired grid already
    /// matches what we last requested/observed — used by the cold-open
    /// settle, where the desktop may have silently dropped the first resize
    /// (its PTY/TUI hadn't attached yet), so re-applying the identical grid
    /// is the correction, not a no-op. The resize is idempotent server-side.
    private func requestRemoteGridForVisibleViewport(force: Bool = false) {
        // Normal mode is a pure viewer: it renders the remote grid as-is and
        // never perturbs the desktop PTY (silent row-resizes on every phone
        // keyboard show/hide made desktop TUIs repaint and forced replay
        // flashes here). Remote resizes are exclusive to fit-to-screen mode,
        // which carries the desktop banner/revert contract.
        guard desktopFitActive else { return }
        // A focus/keyboard cycle must never perturb the desktop grid — the
        // viewport is frozen while the keyboard is up, but a surface-metric
        // re-report could still slip a resize through and blink the terminal.
        guard !keyboardActive else { return }
        guard hasRemoteGrid, visibleViewportSize.width > 0, visibleViewportSize.height > 0 else { return }
        let desiredGrid = desiredRemoteGrid(for: visibleViewportSize)
        if !force {
            guard desiredGrid != lastRequestedRemoteGrid else { return }
            guard desiredGrid.rows != remoteGrid.rows || desiredGrid.columns != remoteGrid.columns else { return }
        }

        lastRequestedRemoteGrid = desiredGrid
        resizeTask?.cancel()
        resizeGeneration &+= 1
        let generation = resizeGeneration
        resizeTask = Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                try await client.resizeDesktopTerminal(
                    sessionID: sessionID,
                    columns: desiredGrid.columns,
                    rows: desiredGrid.rows
                )
                guard !Task.isCancelled else { return }
                remoteGrid = desiredGrid
                _ = updateCanvasSize(grid: desiredGrid)
                lastError = nil
                _ = await refreshRemoteGrid(force: true)
                try? await replayTail()
            } catch is CancellationError {
            } catch {
                if lastRequestedRemoteGrid == desiredGrid {
                    lastRequestedRemoteGrid = nil
                }
            }
            if resizeGeneration == generation {
                resizeTask = nil
            }
        }
    }

    /// The phone-fit grid (fit-to-screen mode only): both axes derived from
    /// the visible viewport so the desktop matches this phone exactly.
    private func desiredRemoteGrid(for viewportSize: CGSize) -> RemoteTerminalGrid {
        let rowSpace = max(1, viewportSize.height - Self.verticalPadding * 2)
        let cellHeight = max(measuredCellSize.height, Self.fallbackCellHeight)
        let rows = min(
            max(Int(floor(rowSpace / cellHeight)), Self.minimumRows),
            Self.maximumRows
        )
        return RemoteTerminalGrid(columns: phoneFitColumns(for: viewportSize), rows: rows)
    }

    private func phoneFitColumns(for viewportSize: CGSize) -> Int {
        let columnSpace = max(1, viewportSize.width - Self.horizontalPadding * 2)
        let cellWidth = max(measuredCellSize.width, Self.fallbackCellWidth)
        return min(
            max(Int(floor(columnSpace / cellWidth)), Self.minimumColumns),
            Self.maximumColumns
        )
    }

    /// Upload an image to the Host and paste its quoted artifact path into
    /// the composer — indistinguishable from a desktop drag-and-drop by the
    /// time the agent sees it.
    func attachImage(
        _ data: Data,
        contentType: String = "image/jpeg",
        resumable: Bool = false
    ) {
        Task { @MainActor in
            do {
                let path = try await client.uploadImage(
                    sessionID: sessionID,
                    data: data,
                    contentType: contentType,
                    resumable: resumable
                )
                let quoted = "'" + path.replacingOccurrences(of: "'", with: "'\\''") + "' "
                var payload = Self.beginBracketedPaste
                payload.append(Data(quoted.utf8))
                payload.append(Self.endBracketedPaste)
                writeQueue.enqueue(payload)
                lastError = nil
                NSLog("[UnpeelIOS] attached image at \(path)")
            } catch {
                NSLog("[UnpeelIOS] image upload failed: \(error)")
                lastError = "Couldn't upload the image (\((error as NSError).code))"
            }
        }
    }

    /// Attachment pipeline problems surface in the same error capsule as
    /// stream failures — never silently.
    func noteAttachmentIssue(_ message: String) {
        lastError = message
    }

    /// A keystroke batch the Mac never received. Throttled: a burst of
    /// failures (typing while the link drops) surfaces once, not per key.
    private func noteInputSendFailure(_ error: Error) {
        let now = ContinuousClock.now
        if let last = lastInputFailureNote, now - last < .seconds(2) { return }
        lastInputFailureNote = now
        if let clientError = error as? RemoteMacClientError {
            lastError = "Couldn't send input (\(clientError.description))"
        } else {
            lastError = "Couldn't send input — check the connection"
        }
    }

    /// Commit dictated text into the composer through the same bracketed
    /// paste + ordered queue as typing — arrives whole, no submit.
    func insertTranscribedText(_ text: String) {
        guard !text.isEmpty else { return }
        var payload = Self.beginBracketedPaste
        payload.append(Data(text.utf8))
        payload.append(Self.endBracketedPaste)
        writeQueue.enqueue(payload)
    }

    /// Paste the system clipboard into the remote composer via bracketed paste
    /// (the reliable app-level path). Ghostty's own UITextInput paste over the
    /// remote in-memory surface doesn't reach the PTY — typing rides the
    /// key-event path, but `insertText` (paste) doesn't — which is why the
    /// system paste looked like it only dropped focus.
    func pasteClipboard() {
        guard let text = UIPasteboard.general.string, !text.isEmpty else { return }
        insertTranscribedText(text)
    }

    /// The automatic fit at session attach: waits for the first layout pass
    /// (viewport size), asks the desktop to letterbox to this phone's grid,
    /// and waits for the letterbox to land so the initial replay renders at
    /// the fitted size. Failures (older desktop build) leave the session in
    /// follower mode.
    private func autoFitToPhone() async {
        for _ in 0 ..< 20 where visibleViewportSize == .zero {
            try? await Task.sleep(nanoseconds: 50_000_000)
        }
        guard hasRemoteGrid, visibleViewportSize != .zero, !Task.isCancelled else { return }
        let grid = desiredRemoteGrid(for: visibleViewportSize)
        // Always (re-)issue the resize, even when the observed grid already
        // matches: stop() reverts via a detached task, so a foreground right
        // behind it could otherwise adopt the still-letterboxed grid just
        // before the revert lands on the Mac — and lose the fit the moment
        // revert-detection sees it. The resize is idempotent server-side.
        do {
            try await client.resizeDesktopTerminal(
                sessionID: sessionID,
                columns: grid.columns,
                rows: grid.rows
            )
            desktopFitActive = true
            lastRequestedRemoteGrid = grid
            remoteGrid = grid
            _ = updateCanvasSize(grid: grid)
            await pollUntilRemoteGridSettles(upTo: .milliseconds(700)) { observed in
                abs(observed.columns - grid.columns) <= 2
            }
        } catch {
            NSLog("[UnpeelIOS] auto-fit failed: \(error)")
        }
    }

    /// Latches the first-replay flag and, on its cold-open transition, kicks
    /// off the one-shot fit correction. Called from every transport's replay
    /// path so WS and HTTP behave identically.
    private func markInitialReplayDone() {
        let wasFirstReplay = !initialReplayDone
        if !initialReplayDone {
            initialReplayDone = true
        }
        guard wasFirstReplay else { return }
        Task { @MainActor [weak self] in
            await self?.settleInitialFit()
        }
    }

    /// The automatic re-fit that spares the user a manual phone rotation on a
    /// cold open. `autoFitToPhone` computes its grid before the surface has
    /// reported real cell metrics and before the safe-area viewport settles,
    /// so the first letterbox can land a column or two off — or be dropped
    /// entirely by a desktop PTY that hadn't attached yet — with nothing to
    /// correct it. Once the first replay is in, both inputs are final, so this
    /// force-re-issues the fit (idempotent server-side) and lets the shared
    /// resize path repaint at the confirmed grid. Runs once per attach (reset
    /// in `stop`); reuses `requestRemoteGridForVisibleViewport` so all the
    /// resize/replay/revert bookkeeping stays in one place.
    private func settleInitialFit() async {
        guard !initialFitSettled else { return }
        initialFitSettled = true
        // Let a couple of layout passes land the final safe-area viewport and
        // the surface's real cell metrics before recomputing.
        for _ in 0 ..< 6 {
            if Task.isCancelled { return }
            try? await Task.sleep(nanoseconds: 50_000_000)
        }
        guard desktopFitActive, !keyboardActive,
              visibleViewportSize.width > 0, visibleViewportSize.height > 0,
              !Task.isCancelled
        else { return }
        requestRemoteGridForVisibleViewport(force: true)
    }

    /// Polls terminal metrics (~100ms cadence) until the Mac reports a grid
    /// matching `settled`, capped at `maxWait` — falls through at the cap,
    /// like the flat sleeps this replaces, so fit transitions settle in one
    /// or two polls instead of a fixed 0.7-0.8s blank. Transitional grids
    /// are NOT adopted mid-settle (the canvas would flip desktop-wide for a
    /// beat); one final `refreshRemoteGrid` applies whatever the Mac
    /// settled on.
    private func pollUntilRemoteGridSettles(
        upTo maxWait: Duration,
        matches settled: (RemoteTerminalGrid) -> Bool
    ) async {
        let deadline = ContinuousClock.now + maxWait
        while !Task.isCancelled {
            if let metrics = try? await client.terminalMetrics(sessionID: sessionID) {
                desktopViewingSameSession = metrics.desktopViewing
                let observed = RemoteTerminalGrid(
                    columns: min(max(metrics.columns, Self.minimumColumns), Self.maximumColumns),
                    rows: min(max(metrics.rows, Self.minimumRows), Self.maximumRows)
                )
                if settled(observed) { break }
            }
            guard ContinuousClock.now + .milliseconds(100) <= deadline else { break }
            try? await Task.sleep(for: .milliseconds(100))
        }
        _ = await refreshRemoteGrid(force: true)
    }

    /// Automatic fit assertion for an unwatched session: when the Mac isn't
    /// showing this terminal (per the metrics `desktopViewing` signal), the
    /// phone owns the size — it (re-)fits silently instead of surfacing the
    /// manual button or honoring a spurious size change. Requires the
    /// attach-time auto-fit to have run (so a session the user is only
    /// peeking at through a stale renderer never letterboxes the Mac), and
    /// is throttled because the viewing flag is a ~1s-old snapshot.
    private func autoRefitIfUnwatched() {
        guard desktopViewingSameSession == false, autoFitAttempted else { return }
        guard !keyboardActive, resizeTask == nil else { return }
        guard hasRemoteGrid, visibleViewportSize.width > 0, visibleViewportSize.height > 0
        else { return }
        let now = ContinuousClock.now
        if let last = lastAutoRefit, now - last < .seconds(3) { return }
        lastAutoRefit = now
        if desktopFitActive {
            requestRemoteGridForVisibleViewport(force: true)
        } else {
            activateDesktopFit()
        }
    }

    func toggleDesktopFit() {
        if desktopFitActive {
            revertDesktopFit()
        } else {
            activateDesktopFit()
        }
    }

    private func activateDesktopFit() {
        guard hasRemoteGrid, visibleViewportSize.width > 0, visibleViewportSize.height > 0
        else {
            // Never fail silently: the grid isn't known yet (stream still
            // connecting), so tell the user instead of eating the tap.
            lastError = "Terminal is still connecting"
            return
        }
        desktopFitActive = true
        let grid = desiredRemoteGrid(for: visibleViewportSize)
        lastRequestedRemoteGrid = grid
        resizeTask?.cancel()
        resizeGeneration &+= 1
        let generation = resizeGeneration
        resizeTask = Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                try await client.resizeDesktopTerminal(
                    sessionID: sessionID,
                    columns: grid.columns,
                    rows: grid.rows
                )
                guard !Task.isCancelled else { return }
                remoteGrid = grid
                _ = updateCanvasSize(grid: grid)
                lastError = nil
                // The desktop letterboxes asynchronously (pane refit → attach
                // resize); poll until it lands, then adopt whatever grid it
                // settled on (it may clamp rows to the Mac window height).
                await pollUntilRemoteGridSettles(upTo: .milliseconds(800)) { observed in
                    abs(observed.columns - grid.columns) <= 2
                }
                guard !Task.isCancelled else { return }
                await waitForGridAlignment()
                try? await replayTail()
            } catch is CancellationError {
            } catch {
                desktopFitActive = false
                lastRequestedRemoteGrid = nil
                lastError = "Couldn't resize the desktop terminal"
            }
            if resizeGeneration == generation {
                resizeTask = nil
            }
        }
    }

    private func revertDesktopFit() {
        desktopFitActive = false
        lastRequestedRemoteGrid = nil
        resizeTask?.cancel()
        resizeGeneration &+= 1
        let generation = resizeGeneration
        let letterboxed = remoteGrid
        resizeTask = Task { @MainActor [weak self] in
            guard let self else { return }
            try? await client.revertDesktopTerminal(sessionID: sessionID)
            // Poll until the Mac reports its natural size back (any grid
            // other than the letterboxed one), capped like the old sleep.
            await pollUntilRemoteGridSettles(upTo: .milliseconds(800)) { observed in
                observed != letterboxed
            }
            guard !Task.isCancelled else { return }
            if remoteGrid != letterboxed {
                try? await replayTail()
            }
            if resizeGeneration == generation {
                resizeTask = nil
            }
        }
    }

    private static func makeMemorySession(
        inputTracker: RemoteTerminalInputTracker,
        writeQueue: RemoteTerminalWriteQueue
    ) -> InMemoryTerminalSession {
        InMemoryTerminalSession(
            write: { data in
                inputTracker.record(data)
                writeQueue.enqueue(data)
            },
            resize: { _ in
                // iOS is a remote display. Do not resize the Mac-owned PTY to
                // the phone viewport; render a desktop-width surface and pan it.
            }
        )
    }

    /// CAN aborts an unterminated OSC/DCS before RIS. A retained-journal
    /// rebase may intentionally cut that malformed sequence at the hard cap.
    private static let resetTerminalState = Data([0x18, 0x1B, 0x63])
    /// CSI ?2026h / CSI ?2026l — DEC synchronized output brackets.
    private static let beginSynchronizedOutput = Data("\u{1B}[?2026h".utf8)
    private static let endSynchronizedOutput = Data("\u{1B}[?2026l".utf8)
    /// The shared prefix of both DEC 2026 brackets (`ESC [ ? 2 0 2 6`), used
    /// to detect a remote TUI managing synchronized output on its own.
    private static let syncOutputPrefix = Data([0x1B, 0x5B, 0x3F, 0x32, 0x30, 0x32, 0x36])

    /// True when the chunk contains a DEC 2026 synchronized-output sequence
    /// (either `h` or `l`) — i.e. the remote TUI is driving it itself.
    static func containsSyncOutput(_ data: Data) -> Bool {
        data.range(of: syncOutputPrefix) != nil
    }
    /// ESC[200~ / ESC[201~ — bracketed paste markers; agent CLIs treat a
    /// pasted quoted path as an attachment (same recipe as the MCP send_text).
    private static let beginBracketedPaste = Data("\u{1B}[200~".utf8)
    private static let endBracketedPaste = Data("\u{1B}[201~".utf8)
    private static let clearScreen = Data([0x1B, 0x5B, 0x33, 0x4A, 0x1B, 0x5B, 0x32, 0x4A, 0x1B, 0x5B, 0x48])
    private static let initialReplayLimit = 768 * 1024
    /// Large feeds go in at this granularity with a yield between slices so
    /// the main actor stays responsive during a replay.
    private static let feedSliceBytes = 64 * 1024
    /// Server-side long-poll hold for the output loop. Idle sessions cost
    /// ~1 request per 8s instead of per second; the output request's URL
    /// timeout is raised to cover it (see RemoteMacClient.outputChunk).
    private static let outputLongPollMs = 8_000
    private static let fallbackCellWidth: CGFloat = 8.45
    private static let fallbackCellHeight: CGFloat = 18.65
    private static let horizontalPadding: CGFloat = 10
    private static let verticalPadding: CGFloat = 6
    private static let gridAlignmentSlop: CGFloat = 2
    private static let fallbackColumns = 83
    private static let fallbackRows = 31
    private static let minimumColumns = 2
    private static let maximumColumns = 300
    private static let minimumRows = 2
    private static let maximumRows = 120
    private static let maximumMouseWheelEventsPerGestureTick = 8
    private static let remoteMouseWheelProviders: Set<String> = [
        "claude",
        "grok",
        "opencode",
    ]

    private static var fallbackGrid: RemoteTerminalGrid {
        RemoteTerminalGrid(columns: fallbackColumns, rows: fallbackRows)
    }

    private static var displayScale: CGFloat {
        #if os(iOS)
        UIScreen.main.scale
        #else
        2
        #endif
    }

    private static func cellSizePoints(from metrics: TerminalGridMetrics) -> CGSize? {
        guard metrics.cellWidthPixels > 0, metrics.cellHeightPixels > 0 else { return nil }
        let scale = max(displayScale, 1)
        let size = CGSize(
            width: max(CGFloat(metrics.cellWidthPixels) / scale, 1),
            height: max(CGFloat(metrics.cellHeightPixels) / scale, 1)
        )
        guard size.width.isFinite, size.height.isFinite else { return nil }
        return size
    }

    private static func cellSize(_ lhs: CGSize, differsFrom rhs: CGSize) -> Bool {
        abs(lhs.width - rhs.width) > 0.05 || abs(lhs.height - rhs.height) > 0.05
    }

    /// Build the renderer theme. When the session carries a resolved provider
    /// background (opencode/grok paint their own bg; the Mac resolves it from
    /// its config files and sends the hex), override the dark theme background
    /// with it so ghostty's own blank/slack rows below the TUI's content match
    /// instead of showing the default `#1A1A1F`. The phone always renders dark.
    private static func terminalTheme(darkBackground: String? = nil) -> TerminalTheme {
        var dark = Self.darkTheme
        if let darkBackground {
            dark.background = darkBackground
        }
        return TerminalTheme(
            light: themeConfiguration(Self.lightTheme),
            dark: themeConfiguration(dark)
        )
    }

    /// `#rrggbb` string for a `RemoteSessionSummary.terminalBackgroundHex`
    /// (0xRRGGBB), or nil when the session has no resolved provider bg.
    private static func hexString(from value: Int?) -> String? {
        guard let value else { return nil }
        return String(format: "#%06x", value & 0xFFFFFF)
    }

    private static func terminalConfiguration() -> TerminalConfiguration {
        TerminalConfiguration { builder in
            builder.withCursorStyle(.block)
            builder.withCursorStyleBlink(true)
            builder.withFontSize(13)
            builder.withCustom("shell-integration", "detect")
            builder.withCustom("window-padding-balance", "true")
            // Extend the terminal's own background into the padding so a TUI
            // that paints its bg to the edges (opencode/grok) doesn't show the
            // app's base color as a mismatched border — while keeping the
            // padding so Claude etc. still get breathing room.
            builder.withCustom("window-padding-color", "extend")
            builder.withWindowPaddingX(8)
            builder.withWindowPaddingY(6)
        }
    }

    private static func themeConfiguration(_ theme: TerminalThemeVariant) -> TerminalConfiguration {
        TerminalConfiguration { builder in
            builder.withBackground(theme.background)
            builder.withForeground(theme.foreground)
            builder.withSelectionBackground(theme.selectionBackground)
            builder.withCursorColor(theme.cursorColor)
            for (index, color) in theme.palette.enumerated() {
                builder.withPalette(index, color: color)
            }
        }
    }

    private struct TerminalThemeVariant {
        var background: String
        var foreground: String
        var selectionBackground: String
        var cursorColor: String
        var palette: [String]
    }

    private static let darkTheme = TerminalThemeVariant(
        background: "#1A1A1F",
        foreground: "#fafafa",
        selectionBackground: "#3a3a40",
        cursorColor: "#fafafa",
        palette: [
            "#1c1c22", "#ef4444", "#22c55e", "#eab308",
            "#3b82f6", "#a855f7", "#06b6d4", "#a1a1aa",
            "#6e6e76", "#f87171", "#4ade80", "#facc15",
            "#60a5fa", "#c084fc", "#22d3ee", "#fafafa",
        ]
    )

    private static let lightTheme = TerminalThemeVariant(
        background: "#ffffff",
        foreground: "#09090b",
        selectionBackground: "#d4d4d8",
        cursorColor: "#09090b",
        palette: [
            "#09090b", "#dc2626", "#16a34a", "#ca8a04",
            "#2563eb", "#9333ea", "#0891b2", "#e4e4e7",
            "#71717a", "#ef4444", "#22c55e", "#eab308",
            "#3b82f6", "#a855f7", "#06b6d4", "#fafafa",
        ]
    )
}

/// Thread-safe: fed off the main actor (chunk decode helper) and read from
/// the main actor, serialized by an internal lock.
final class RemoteTerminalMouseModeTracker: @unchecked Sendable {
    private static let alternateScreenModes: Set<Int> = [47, 1047, 1049]
    private static let mouseTrackingModes: Set<Int> = [9, 1000, 1002, 1003]
    /// DECCKM (mode 1): while enabled, arrow keys must be sent as `ESC O A/B`
    /// rather than `ESC [ A/B`. The menu control bar reads this so its ↑/↓
    /// reach the remote TUI in whichever form it expects.
    private static let applicationCursorKeyModes: Set<Int> = [1]
    /// A pending (unterminated) escape prefix longer than this is not a
    /// real CSI — drop it entirely rather than truncating it into a byte
    /// soup that could parse as a different sequence.
    private static let maximumPendingBytes = 96

    private let lock = NSLock()
    private var alternateScreenStack: Set<Int> = []
    private var mouseTrackingStack: Set<Int> = []
    private var applicationCursorKeysOn = false
    private var pending = Data()
    private var sawDisable = false

    var alternateScreenEnabled: Bool {
        lock.lock()
        defer { lock.unlock() }
        return !alternateScreenStack.isEmpty
    }

    /// DECCKM state: true once the remote enabled application cursor keys.
    var applicationCursorKeysEnabled: Bool {
        lock.lock()
        defer { lock.unlock() }
        return applicationCursorKeysOn
    }

    var mouseTrackingEnabled: Bool {
        lock.lock()
        defer { lock.unlock() }
        return !mouseTrackingStack.isEmpty
    }

    var sawMouseOrAlternateDisable: Bool {
        lock.lock()
        defer { lock.unlock() }
        return sawDisable
    }

    func reset() {
        lock.lock()
        defer { lock.unlock() }
        alternateScreenStack.removeAll()
        mouseTrackingStack.removeAll()
        applicationCursorKeysOn = false
        sawDisable = false
        pending.removeAll(keepingCapacity: true)
    }

    func feed(_ data: Data) {
        guard !data.isEmpty else { return }
        lock.lock()
        defer { lock.unlock() }
        // Common case: no carried prefix — scan the chunk in place, no copy.
        if pending.isEmpty {
            scanLocked(data)
        } else {
            var bytes = pending
            bytes.append(data)
            pending.removeAll(keepingCapacity: true)
            scanLocked(bytes)
        }
        if pending.count > Self.maximumPendingBytes {
            pending.removeAll(keepingCapacity: false)
        }
    }

    /// Caller must hold `lock`. Scans the raw bytes without copying them
    /// into an intermediate array; an unterminated trailing escape prefix
    /// is carried in `pending` for the next feed.
    private func scanLocked(_ bytes: Data) {
        bytes.withUnsafeBytes { (buffer: UnsafeRawBufferPointer) in
            guard let base = buffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return
            }
            let count = buffer.count
            var index = 0
            while index < count {
                guard base[index] == 0x1B else {
                    index += 1
                    continue
                }
                guard index + 1 < count else {
                    pending.append(base[index])
                    break
                }
                let next = base[index + 1]
                if next == 0x63 {
                    resetModesLocked()
                    sawDisable = true
                    index += 2
                    continue
                }
                guard next == 0x5B else {
                    index += 2
                    continue
                }
                guard index + 2 < count else {
                    pending.append(UnsafeBufferPointer(start: base + index, count: count - index))
                    break
                }

                var cursor = index + 2
                let privateMode = base[cursor] == 0x3F
                if privateMode {
                    cursor += 1
                }

                let paramsStart = cursor
                while cursor < count, !Self.isFinalByte(base[cursor]) {
                    cursor += 1
                }
                guard cursor < count else {
                    pending.append(UnsafeBufferPointer(start: base + index, count: count - index))
                    break
                }

                if privateMode && (base[cursor] == 0x68 || base[cursor] == 0x6C) {
                    let params = Self.parseParams(
                        UnsafeBufferPointer(start: base + paramsStart, count: cursor - paramsStart)
                    )
                    updateModesLocked(params, enabled: base[cursor] == 0x68)
                }
                index = cursor + 1
            }
        }
    }

    private func updateModesLocked(_ params: [Int], enabled: Bool) {
        for param in params {
            if Self.alternateScreenModes.contains(param) {
                update(&alternateScreenStack, param, enabled: enabled)
                sawDisable = !enabled
            }
            if Self.mouseTrackingModes.contains(param) {
                update(&mouseTrackingStack, param, enabled: enabled)
                sawDisable = !enabled
            }
            if Self.applicationCursorKeyModes.contains(param) {
                applicationCursorKeysOn = enabled
            }
        }
    }

    private func resetModesLocked() {
        alternateScreenStack.removeAll()
        mouseTrackingStack.removeAll()
        applicationCursorKeysOn = false
    }

    private func update(_ modes: inout Set<Int>, _ mode: Int, enabled: Bool) {
        if enabled {
            modes.insert(mode)
        } else {
            modes.remove(mode)
        }
    }

    private static func parseParams(_ bytes: UnsafeBufferPointer<UInt8>) -> [Int] {
        let string = String(decoding: bytes, as: UTF8.self)
        return string
            .split(separator: ";")
            .compactMap { Int($0) }
    }

    private static func isFinalByte(_ byte: UInt8) -> Bool {
        (0x40...0x7E).contains(byte)
    }
}

enum RemoteTerminalMouseEventEncoder {
    enum WheelDirection {
        case up
        case down
    }

    static func sgrWheelSequence(
        direction: WheelDirection,
        column: Int,
        row: Int,
        repeats: Int = 1
    ) -> String {
        let button = direction == .up ? 64 : 65
        let safeColumn = max(column, 1)
        let safeRow = max(row, 1)
        return String(
            repeating: "\u{1B}[<\(button);\(safeColumn);\(safeRow)M",
            count: max(repeats, 0)
        )
    }
}

#if os(iOS)
private struct RemoteTerminalSurfaceBridge: UIViewRepresentable {
    @Environment(\.colorScheme) private var colorScheme

    /// Owns the platform view across SwiftUI teardowns: the ghostty surface
    /// (the terminal's actual screen state) lives inside the view's
    /// coordinator, so reusing the cached view is what makes a switched-back
    /// session paint its last frame instantly.
    let entry: TerminalSessionCacheEntry
    let context: TerminalViewState
    @Binding var isFocused: Bool
    let onScrolledUpChange: (Bool) -> Void
    let onTouchScroll: (CGPoint, CGSize, CGSize, TerminalTouchScrollPhase) -> Bool
    let onCanvasPanDelta: (CGFloat) -> Void
    /// Long-press text selection: viewport text snapshot + pre-selection
    /// range of the pressed word. The host presents the selection sheet at
    /// the root (never over the Metal surface).
    let onTextSelection: (String, NSRange?) -> Void
    /// Accessory-bar host actions ("attach-image", future "voice", …).
    var onHostAction: (String) -> Void = { _ in }

    func makeCoordinator() -> Coordinator {
        Coordinator(
            context: context,
            onScrolledUpChange: onScrolledUpChange,
            onTouchScroll: onTouchScroll,
            onCanvasPanDelta: onCanvasPanDelta,
            onTextSelection: onTextSelection
        )
    }

    /// Extra-keys row above the software keyboard, curated for agent CLIs
    /// (not general shell work): Esc interrupts generation, sticky Ctrl+C
    /// kills a process, arrows drive permission menus and pickers, `/`
    /// starts slash commands, `@` mentions files. One row, no paging.
    private static let accessoryItems: [TerminalInputAccessoryItem] = [
        // Arrows first so they never scroll off — they sit at the leading edge
        // and are always visible (they're the most-used keys for menus/history).
        // No backspace: the system keyboard's delete key covers it (and now
        // repeats on hold via the phantom-document fix). Paste lives here so the
        // pinned cluster stays small and the scrolling row shows more.
        .arrowLeft, .arrowUp, .arrowDown, .arrowRight,
        .divider,
        .hostAction(id: "paste", systemImage: "doc.on.clipboard", label: "Paste"),
        .divider,
        .esc, .tab, .ctrl,
        .divider,
        .symbol("/"), .symbol("@"),
    ]

    /// Pinned at the trailing edge, always visible while the key row scrolls:
    /// voice + attach image. Kept to two so the scrolling row (arrows first)
    /// has room. No dismiss key — tapping the terminal or swiping down hides
    /// the keyboard. (Backspace still repeats-on-hold from the scrolling row.)
    private static let accessoryTrailingItems: [TerminalInputAccessoryItem] = [
        .hostAction(id: "voice", systemImage: "mic", label: "Dictate"),
        .hostAction(id: "attach-image", systemImage: "photo", label: "Attach Image"),
    ]

    /// Match the app's dark terminal chrome: quiet white-alpha keys on the
    /// bar's glass/blur, cyan for armed/locked sticky modifiers (same accent
    /// as the fit-to-screen button).
    private static let accessoryStyle = TerminalInputAccessoryStyle(
        regularBackground: UIColor.white.withAlphaComponent(0.10),
        regularForeground: UIColor.white.withAlphaComponent(0.92),
        activeBackground: .systemCyan,
        activeForeground: .black
    )

    func makeUIView(context viewContext: Context) -> TerminalView {
        let view = entry.makeOrReuseTerminalView()
        configure(view, coordinator: viewContext.coordinator)
        view.inputAccessoryStyle = Self.accessoryStyle
        viewContext.coordinator.attach(
            to: view,
            onScrolledUpChange: onScrolledUpChange,
            onTouchScroll: onTouchScroll,
            onCanvasPanDelta: onCanvasPanDelta,
            onTextSelection: onTextSelection
        )
        synchronizeFocus(view)
        return view
    }

    func updateUIView(_ view: TerminalView, context viewContext: Context) {
        configure(view, coordinator: viewContext.coordinator)
        viewContext.coordinator.attach(
            to: view,
            onScrolledUpChange: onScrolledUpChange,
            onTouchScroll: onTouchScroll,
            onCanvasPanDelta: onCanvasPanDelta,
            onTextSelection: onTextSelection
        )
        self.context.adopt(colorScheme: colorScheme)
        synchronizeFocus(view)
    }

    static func dismantleUIView(_ view: TerminalView, coordinator: Coordinator) {
        coordinator.detach()
        view.delegate = nil
    }

    private func configure(_ view: TerminalView, coordinator: Coordinator) {
        view.delegate = coordinator
        if view.controller !== context.controller {
            view.controller = context.controller
        }
        view.configuration = context.configuration
        // Guarded: assigning inputAccessoryItems rebuilds the bar and
        // reloads input views, and updateUIView runs on every store publish.
        if view.inputAccessoryItems != Self.accessoryItems {
            view.inputAccessoryItems = Self.accessoryItems
        }
        if view.inputAccessoryTrailingItems != Self.accessoryTrailingItems {
            view.inputAccessoryTrailingItems = Self.accessoryTrailingItems
        }
        // Mirror responder changes back into SwiftUI so every dismissal path
        // (the bar's hide key, tap-to-dismiss, swipe-down) sticks instead of
        // being re-summoned by the next synchronizeFocus. Synchronous on
        // purpose: an async hop leaves a window where updateUIView reads the
        // stale value and re-asserts focus — the "keyboard pops back" race.
        // Responder transitions fire from touch handling, not mid-render, so
        // the direct write is safe.
        view.onFocusChange = { [binding = _isFocused] focused in
            if binding.wrappedValue != focused {
                binding.wrappedValue = focused
            }
        }
        view.onAccessoryHostAction = onHostAction
        // Pinch changes the local font size, which breaks the fit math
        // (cell metrics drive the grid) — one fixed size on the phone.
        view.pinchZoomEnabled = false
        // Hide the system keyboard dictation mic: inline dictation cannot work
        // against a terminal byte-stream (it refines via ranged replacements on
        // a text document that doesn't exist), so the redundant mic only
        // confuses. We ship our own push-to-talk voice key in the accessory
        // bar. The only mechanism iOS offers is `isSecureTextEntry`, which also
        // disables backspace hold-to-delete (key auto-repeat) — accepted cost.
        view.systemDictationDisabled = true
    }

    private func synchronizeFocus(_ view: TerminalView) {
        DispatchQueue.main.async {
            guard view.window != nil else { return }
            if isFocused {
                if !view.isFirstResponder {
                    view.becomeFirstResponder()
                }
            } else if view.isFirstResponder {
                view.resignFirstResponder()
            }
        }
    }

    @MainActor
    final class Coordinator: NSObject,
        TerminalSurfaceTitleDelegate,
        TerminalSurfaceGridResizeDelegate,
        TerminalSurfaceFocusDelegate,
        TerminalSurfaceCloseDelegate,
        TerminalSurfaceBellDelegate,
        TerminalSurfaceDesktopNotificationDelegate,
        TerminalSurfacePwdDelegate,
        TerminalSurfaceCommandFinishedDelegate,
        TerminalSurfaceLifecycleDelegate,
        TerminalSurfaceScrollbarDelegate,
        TerminalSurfaceTouchScrollDelegate,
        TerminalSurfaceTextSelectionRequestDelegate,
        UIGestureRecognizerDelegate
    {
        private enum PanAxis {
            case undecided
            case horizontal
            case vertical
        }

        private let terminalState: TerminalViewState
        private var onScrolledUpChange: (Bool) -> Void
        private var onTouchScroll: (CGPoint, CGSize, CGSize, TerminalTouchScrollPhase) -> Bool
        private var onCanvasPanDelta: (CGFloat) -> Void
        private var onTextSelection: (String, NSRange?) -> Void
        private weak var view: TerminalView?
        private weak var canvasPanRecognizer: UIPanGestureRecognizer?
        private var canvasPanAxis: PanAxis = .undecided

        init(
            context: TerminalViewState,
            onScrolledUpChange: @escaping (Bool) -> Void,
            onTouchScroll: @escaping (CGPoint, CGSize, CGSize, TerminalTouchScrollPhase) -> Bool,
            onCanvasPanDelta: @escaping (CGFloat) -> Void,
            onTextSelection: @escaping (String, NSRange?) -> Void
        ) {
            terminalState = context
            self.onScrolledUpChange = onScrolledUpChange
            self.onTouchScroll = onTouchScroll
            self.onCanvasPanDelta = onCanvasPanDelta
            self.onTextSelection = onTextSelection
        }

        func attach(
            to view: TerminalView,
            onScrolledUpChange: @escaping (Bool) -> Void,
            onTouchScroll: @escaping (CGPoint, CGSize, CGSize, TerminalTouchScrollPhase) -> Bool,
            onCanvasPanDelta: @escaping (CGFloat) -> Void,
            onTextSelection: @escaping (String, NSRange?) -> Void
        ) {
            self.view = view
            self.onScrolledUpChange = onScrolledUpChange
            self.onTouchScroll = onTouchScroll
            self.onCanvasPanDelta = onCanvasPanDelta
            self.onTextSelection = onTextSelection
            installCanvasPanRecognizer(on: view)
        }

        func detach() {
            if let recognizer = canvasPanRecognizer {
                recognizer.view?.removeGestureRecognizer(recognizer)
            }
            canvasPanRecognizer = nil
            canvasPanAxis = .undecided
            view = nil
        }

        private func installCanvasPanRecognizer(on view: TerminalView) {
            if canvasPanRecognizer?.view === view {
                return
            }
            if let recognizer = canvasPanRecognizer {
                recognizer.view?.removeGestureRecognizer(recognizer)
            }
            let recognizer = UIPanGestureRecognizer(target: self, action: #selector(handleCanvasPan(_:)))
            recognizer.allowedTouchTypes = [NSNumber(value: UITouch.TouchType.direct.rawValue)]
            recognizer.maximumNumberOfTouches = 1
            recognizer.cancelsTouchesInView = false
            recognizer.delaysTouchesBegan = false
            recognizer.delaysTouchesEnded = false
            recognizer.delegate = self
            view.addGestureRecognizer(recognizer)
            canvasPanRecognizer = recognizer
        }

        @objc private func handleCanvasPan(_ gesture: UIPanGestureRecognizer) {
            guard let view = gesture.view else { return }
            switch gesture.state {
            case .began:
                canvasPanAxis = .undecided
                gesture.setTranslation(.zero, in: view)
            case .changed:
                let translation = gesture.translation(in: view)
                if canvasPanAxis == .undecided {
                    let horizontal = abs(translation.x)
                    let vertical = abs(translation.y)
                    guard max(horizontal, vertical) > 6 else { return }
                    canvasPanAxis = horizontal > vertical * 1.2 ? .horizontal : .vertical
                }
                if canvasPanAxis == .horizontal {
                    onCanvasPanDelta(translation.x)
                }
                gesture.setTranslation(.zero, in: view)
            case .ended, .cancelled, .failed:
                canvasPanAxis = .undecided
            default:
                break
            }
        }

        func gestureRecognizer(
            _ gestureRecognizer: UIGestureRecognizer,
            shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer
        ) -> Bool {
            true
        }

        func terminalDidChangeTitle(_ title: String) {
            terminalState.terminalDidChangeTitle(title)
        }

        func terminalDidResize(_ size: TerminalGridMetrics) {
            terminalState.terminalDidResize(size)
        }

        func terminalDidChangeFocus(_ focused: Bool) {
            terminalState.terminalDidChangeFocus(focused)
        }

        func terminalDidClose(processAlive: Bool) {
            terminalState.terminalDidClose(processAlive: processAlive)
        }

        func terminalDidRingBell() {
            terminalState.terminalDidRingBell()
        }

        func terminalDidRequestDesktopNotification(title: String, body: String) {
            terminalState.terminalDidRequestDesktopNotification(title: title, body: body)
        }

        func terminalDidChangeWorkingDirectory(_ path: String) {
            terminalState.terminalDidChangeWorkingDirectory(path)
        }

        func terminalDidFinishCommand(exitCode: Int?, durationNanos: UInt64) {
            terminalState.terminalDidFinishCommand(exitCode: exitCode, durationNanos: durationNanos)
        }

        func terminalDidAttachSurface(_ surface: TerminalSurface) {
            terminalState.terminalDidAttachSurface(surface)
        }

        func terminalDidDetachSurface() {
            terminalState.terminalDidDetachSurface()
            onScrolledUpChange(false)
        }

        func terminalDidUpdateScrollbar(_ metrics: TerminalScrollbarMetrics) {
            onScrolledUpChange(!metrics.isAtBottom)
        }

        /// Long-press on the terminal: the vendored view snapshots the
        /// viewport text and resolves the pressed word's range; presenting
        /// the selection UI is the host's job.
        func terminalDidRequestTextSelection(_ request: TerminalTextSelectionRequest) {
            onTextSelection(request.text, request.anchorRange)
        }

        func terminalDidReceiveTouchScroll(
            location: CGPoint,
            delta: CGSize,
            velocity: CGSize,
            phase: TerminalTouchScrollPhase
        ) -> Bool {
            onTouchScroll(location, delta, velocity, phase)
        }
    }
}
#else
private struct RemoteTerminalSurfaceBridge: View {
    let entry: TerminalSessionCacheEntry
    let context: TerminalViewState
    @Binding var isFocused: Bool
    let onScrolledUpChange: (Bool) -> Void
    let onTouchScroll: (CGPoint, CGSize, CGSize, TerminalTouchScrollPhase) -> Bool
    let onCanvasPanDelta: (CGFloat) -> Void
    var onHostAction: (String) -> Void = { _ in }

    var body: some View {
        TerminalSurfaceView(context: context)
            .onAppear {
                isFocused = true
            }
    }
}
#endif

final class RemoteTerminalInputTracker: @unchecked Sendable {
    var onFollow: (@Sendable (Int) -> Void)?

    private let lock = NSLock()
    private var estimatedColumn = 0

    func record(_ data: Data) {
        var shouldNotify = false
        lock.lock()
        for byte in data {
            switch byte {
            case 10, 13:
                estimatedColumn = 0
                shouldNotify = true
            case 8, 127:
                estimatedColumn = max(0, estimatedColumn - 1)
                shouldNotify = true
            case 0..<32:
                break
            default:
                estimatedColumn += 1
                shouldNotify = estimatedColumn >= 48
            }
        }
        let column = estimatedColumn
        lock.unlock()

        if shouldNotify {
            onFollow?(column)
        }
    }
}
