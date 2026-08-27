//
//  GhosttyBridge.swift
//  UnpeelNative
//
//  The ONLY file in this target allowed to import GhosttyTerminal /
//  GhosttyKit (PRD §8: libghostty API is alpha; churn must be contained
//  here). Everything Ghostty-shaped is translated into plain AppKit /
//  Foundation types at this boundary.
//
//  What this bridge does:
//  - Owns a `TerminalController` (one ghostty_app_t + config per pane).
//  - Sets the surface command via the Ghostty config `command` key.
//    (The C API also supports per-surface `command` / `env_vars` on
//    `ghostty_surface_config_s`, but the GhosttyTerminal wrapper does not
//    expose those yet; one controller per pane gives us per-session
//    commands today. See report/PRD §11.1.)
//  - Hosts the wrapper's `TerminalView` (AppKit NSView, Metal-rendered,
//    full key/mouse/IME pipeline) with the EXEC io backend, so Ghostty
//    owns a real PTY running our command — exactly Strategy A.
//
//  Runtime callbacks: wakeup, clipboard read/write/confirm, config
//  reload and the action dispatch loop are implemented inside
//  GhosttyTerminal's TerminalController+Callbacks. We only consume the
//  high-level delegate protocols below; unhandled actions are logged by
//  the wrapper when debug logging is enabled.
//

import AppKit
import GhosttyKit
import GhosttyTerminal
import SwiftUI

/// Plain-Swift events the rest of the app may care about. No Ghostty types.
@MainActor
protocol GhosttyTerminalPaneDelegate: AnyObject {
    func terminalPane(_ pane: GhosttyTerminalPane, didChangeTitle title: String)
    /// The surface's child process exited (or the surface closed).
    func terminalPane(_ pane: GhosttyTerminalPane, didCloseProcessAlive processAlive: Bool)
}

/// An NSView containing one GPU-rendered Ghostty terminal surface running
/// a fixed command. Fills itself with the surface; resize is handled by
/// AppKit layout + the wrapper's `fitToSize()`.
@MainActor
final class GhosttyTerminalPane: NSView {
    weak var paneDelegate: GhosttyTerminalPaneDelegate?

    private let terminalView: TerminalView
    private let controller: TerminalController
    private var surface: TerminalSurface?
    private var didTearDown = false

    /// Most recent working directory, used to resolve relative file paths from
    /// cmd-click. Seeded with the spawn cwd; updated by OSC 7 if the shell
    /// reports it.
    private var currentWorkingDirectory: String?

    /// Scroll-to-bottom overlay (bottom-right, like the Tauri app). The
    /// pane owns it so per-session scroll state survives surface swaps in
    /// the cache. Visibility is driven by Ghostty's scrollbar action via
    /// `TerminalSurfaceScrollbarDelegate`, plus the TUI jump-hint scan below.
    private let scrollButtonModel = TerminalScrollButtonModel()

    /// True while Ghostty reports the viewport above the scrollback tail.
    private var scrolledUpInScrollback = false

    // MARK: - Full-screen TUI "jump to bottom" hint

    /// Claude Code's TUI keeps its own virtual scroll (drawn in the primary
    /// screen, repainted in place), so the terminal never reports
    /// "scrolled up" and the overlay button never shows. The TUI's own signal
    /// is a hint drawn on screen. While the pane is visible we scan the
    /// rendered viewport for that hint; when present the same overlay
    /// button appears, and pressing it fakes the ctrl+End keypress
    /// (CSI 1;5F — the bytes a real ctrl+End sends) instead of moving the
    /// viewport.
    ///
    /// Matches the hint chip in its two shapes — "Jump to bottom
    /// (ctrl+End)" and "3 new messages (ctrl+End) ↓" — and nothing looser,
    /// and only within the bottom rows of the viewport, where Claude pins
    /// the real chip (just above its composer). Quoted hint text higher up
    /// in the transcript must not match: a spurious ctrl+End at a TUI that
    /// is not scrolled up lands as literal "[1;5F" junk in the composer.
    /// Keep aligned with the iOS matcher in
    /// `RemoteGhosttyTerminalView.swift`.
    private static func viewportHasTuiJumpHint(_ text: String) -> Bool {
        let tail = text
            .split(separator: "\n", omittingEmptySubsequences: false)
            .suffix(15)
            .joined(separator: "\n")
        if tail.contains("Jump to bottom (ctrl+End)") { return true }
        return tail.firstMatch(of: /\d+ new messages? \(ctrl\+End\)/) != nil
    }
    /// macOS virtual keycode for End (kVK_End). The jump is sent as a real
    /// ctrl+End KEY EVENT, not raw CSI bytes: `sendText` bypasses ghostty's
    /// key encoder, so TUIs that negotiated the kitty keyboard protocol
    /// (Claude Code) received mangled text — a literal "[1;5F" in the
    /// composer — instead of the keypress.
    private static let endKeycode: UInt32 = 119
    private var tuiJumpHintActive = false
    private var tuiJumpHintTimer: Timer?
    /// Armed by a button press: the remote TUI repaints asynchronously, so
    /// one ctrl+End can land mid-stream and leave the hint up (users had to
    /// click repeatedly). While retries remain, the 0.5s hint poll re-sends
    /// the key instead of re-showing the button; a clean scan settles it.
    private var tuiJumpRetriesRemaining = 0
    /// Last scrollbar metrics (drives the scrolled-up state of the shared
    /// overlay button).
    private var lastScrollbarMetrics: TerminalScrollbarMetrics?

    // MARK: - Find (⌘F)

    /// Lazily built ⌘F bar (top-right overlay). libghostty runs the search;
    /// the pane relays query/navigation as binding actions and match counts
    /// back via `TerminalSurfaceSearchDelegate`. Owned per-pane so a
    /// session's find state survives surface-cache swaps like scroll state
    /// does.
    private var findBar: TerminalFindBar?
    private var findBarVisible = false
    private var searchTotal: Int?
    private var searchSelected: Int?
    private var findObservers: [NSObjectProtocol] = []

    /// Corner container for the overlay button: passes clicks through to
    /// the terminal whenever the button is not showing.
    private final class ScrollButtonContainer: NSView {
        var isInteractive: () -> Bool = { false }
        override func hitTest(_ point: NSPoint) -> NSView? {
            isInteractive() ? super.hitTest(point) : nil
        }
    }

    /// - Parameters:
    ///   - command: command line the surface executes (Ghostty splits args
    ///     itself; e.g. "/bin/zsh --login" or "unpeel-attach <session-id>").
    ///   - workingDirectory: initial cwd for the spawned process.
    ///   - style: plain-Swift terminal theme (DESIGN.md §3); translated into
    ///     Ghostty config keys here, at the bridge boundary.
    init(command: String, workingDirectory: String?, style: TerminalPaneStyle = .resolved()) {
        currentWorkingDirectory = workingDirectory
        // Per-pane controller = per-pane ghostty config = per-pane command.
        // Colors live in the TerminalTheme, NOT the base config: the wrapper
        // re-resolves the active variant whenever the view's effective
        // appearance changes (viewDidChangeEffectiveAppearance →
        // controller.setColorScheme), which is how the surface follows the
        // app's light/dark mode without rebuilding the pane.
        controller = TerminalController(theme: Self.terminalTheme(for: style)) { builder in
            builder.withCustom("command", command)
            // Don't keep dead surfaces around.
            builder.withCustom("wait-after-command", "false")
            builder.withCustom("shell-integration", "detect")
            Self.applySurfaceKeybinds(&builder)
            // The Swift frame bleeds to the window edges; per-provider
            // terminal styles decide whether Ghostty keeps any cell padding.
            builder.withCustom("window-padding-x", "\(style.windowPaddingX)")
            builder.withCustom("window-padding-y", "\(style.windowPaddingY)")
            builder.withCustom(
                "window-padding-balance",
                style.windowPaddingBalanced ? "true" : "false"
            )
            // Extend the terminal background into padding so empty rows /
            // residual padding match the TUI canvas (OpenCode/Grok) instead
            // of leaving a mismatched strip of the default theme.
            builder.withCustom("window-padding-color", "extend")
            // Discrete (wheel-tick) speed only. Precision must stay 1: the
            // multiplier scales trackpad deltas BEFORE mouse-report
            // conversion, so anything >1 makes mouse-captured TUIs (Claude's
            // virtual scroll) jump multiple lines per finger-travel line —
            // the "not as smooth as Ghostty" complaint. A bare value would
            // set both fields.
            builder.withCustom(
                "mouse-scroll-multiplier",
                "precision:1,discrete:\(style.mouseScrollMultiplier)"
            )

            builder.withCursorStyle(.block)
            builder.withCursorStyleBlink(true)
            builder.withFontSize(style.fontSize)
            if let family = style.fontFamily {
                builder.withFontFamily(family)
            }
        }

        // A config-build failure leaves the controller without a ghostty
        // app, and every later surface rebuild fails with no explanation —
        // surface it loudly instead.
        if let issue = controller.lastConfigurationIssue {
            NSLog("[UnpeelNative] ghostty configuration issue: %@", issue)
        }

        terminalView = TerminalView(frame: .zero)
        // Unbalanced padding pins the grid at the fixed top-left padding —
        // give the view the exact origin for point→cell mapping (cmd-click).
        terminalView.gridOrigin = CGPoint(
            x: CGFloat(style.windowPaddingX),
            y: CGFloat(style.windowPaddingY)
        )

        super.init(frame: .zero)

        // Metal surface is clear; paint this pane opaque with the frame bg so
        // any gap around the surface (letterbox, padding, resize) matches the
        // TUI canvas rather than showing through to the content dim.
        applyFrameLayerBackground(style)

        terminalView.configuration = TerminalSurfaceOptions(
            backend: .exec,
            workingDirectory: workingDirectory,
            context: .window
        )
        terminalView.delegate = self
        terminalView.controller = controller

        terminalView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(terminalView)
        NSLayoutConstraint.activate([
            terminalView.topAnchor.constraint(equalTo: topAnchor),
            terminalView.leadingAnchor.constraint(equalTo: leadingAnchor),
            terminalView.trailingAnchor.constraint(equalTo: trailingAnchor),
            terminalView.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])

        // The wrapper's TerminalView registers no drag types, so file/image
        // drags over the surface fall through to this pane.
        registerForDraggedTypes(Self.dropPasteboardTypes)

        scrollButtonModel.action = { [weak self] in self?.scrollButtonPressed() }
        let host = NSHostingView(
            rootView: TerminalScrollToBottomButton(model: scrollButtonModel)
        )
        host.translatesAutoresizingMaskIntoConstraints = false
        let buttonContainer = ScrollButtonContainer()
        buttonContainer.isInteractive = { [scrollButtonModel] in
            scrollButtonModel.visible
        }
        buttonContainer.translatesAutoresizingMaskIntoConstraints = false
        buttonContainer.addSubview(host)
        addSubview(buttonContainer, positioned: .above, relativeTo: terminalView)
        NSLayoutConstraint.activate([
            host.topAnchor.constraint(equalTo: buttonContainer.topAnchor),
            host.leadingAnchor.constraint(equalTo: buttonContainer.leadingAnchor),
            host.trailingAnchor.constraint(equalTo: buttonContainer.trailingAnchor),
            host.bottomAnchor.constraint(equalTo: buttonContainer.bottomAnchor),
            // The SwiftUI root carries 8pt of padding (animation headroom),
            // so -12/-8 here lands the 36pt button at 20/16 from the corner,
            // matching the Tauri app.
            buttonContainer.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            buttonContainer.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -8),
        ])

        // Edit ▸ Find menu commands. Every retained pane hears the post;
        // only the one actually on screen (in a window, not inside a hidden
        // warm-pane container, key window) acts.
        let center = NotificationCenter.default
        findObservers = [
            center.addObserver(
                forName: .unpeelTerminalFind, object: nil, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self, self.isDisplayedForFind else { return }
                    self.showFindBar()
                }
            },
            center.addObserver(
                forName: .unpeelTerminalFindNext, object: nil, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self, self.isDisplayedForFind else { return }
                    self.findNext()
                }
            },
            center.addObserver(
                forName: .unpeelTerminalFindPrevious, object: nil, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self, self.isDisplayedForFind else { return }
                    self.findPrevious()
                }
            },
        ]
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) { fatalError("not supported") }

    /// Push a new frame style into the live Ghostty controller (colors only).
    /// Used when OpenCode/Grok config changes while a pane is retained — no
    /// surface rebuild. Window padding stays at whatever was set at create
    /// (provider panes already zero it). Also updates this pane's opaque
    /// layer so letterbox/padding matches the new canvas.
    func applyPaneStyle(_ style: TerminalPaneStyle) {
        _ = controller.setTheme(Self.terminalTheme(for: style))
        applyFrameLayerBackground(style)
    }

    /// Explicitly detach/free the Ghostty surface before a cache eviction
    /// releases this pane. `TerminalView.controller = nil` is the wrapper's
    /// supported teardown path: its coordinator clears callbacks and queues
    /// `ghostty_surface_free`, which terminates the EXEC child
    /// (`unpeel-attach`) without waiting for ARC/deinit timing. Idempotent so
    /// a delayed final release remains harmless.
    func tearDown() {
        guard !didTearDown else { return }
        didTearDown = true
        stopTuiJumpHintPolling()
        terminalView.setSurfaceVisible(false)
        // Keep the delegate installed for the synchronous detach callback so
        // our cached `surface` pointer is cleared before severing the bridge.
        terminalView.controller = nil
        terminalView.delegate = nil
        paneDelegate = nil
    }

    private func applyFrameLayerBackground(_ style: TerminalPaneStyle) {
        wantsLayer = true
        // Match the currently effective appearance so light/dark configs
        // don't leave the wrong fixed layer color under a clear Metal surface.
        let appearance = window?.effectiveAppearance ?? effectiveAppearance
        let isDark = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        let hex = isDark ? style.dark.background : style.light.background
        layer?.backgroundColor = (Self.nsColor(fromHexString: hex) ?? Theme.terminalBackgroundNSColor)
            .cgColor
    }

    fileprivate static func nsColor(fromHexString value: String) -> NSColor? {
        var body = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if body.hasPrefix("#") { body.removeFirst() }
        guard body.count == 6, let rgb = UInt32(body, radix: 16) else { return nil }
        return NSColor(hex: rgb)
    }

    /// libghostty ships ~92 default keybinds and a focused surface consumes
    /// any chord it has a binding for BEFORE NSMenu key equivalents run
    /// (AppTerminalView.performKeyEquivalent → ghostty_surface_key_is_binding)
    /// — so defaults like super+w=close_surface silently eat app chords
    /// (⌘W Close Window, ⌘N, ⌘Q…). Clear them all; NSMenu is the single
    /// owner of app chords. The surface keeps only chords that must act on
    /// the terminal itself, re-added explicitly below. Copy stays
    /// `performable` (consumed only when a selection exists), matching
    /// ghostty's own default, so a bare ⌘C over an empty terminal still
    /// reaches the Edit menu.
    fileprivate static func applySurfaceKeybinds(
        _ builder: inout TerminalConfiguration.Builder
    ) {
        builder.withCustom("keybind", "clear")
        builder.withCustom("keybind", "performable:super+c=copy_to_clipboard")
        builder.withCustom("keybind", "super+v=paste_from_clipboard")
        // Scrollback navigation (same as ghostty's macOS defaults).
        builder.withCustom("keybind", "super+home=scroll_to_top")
        builder.withCustom("keybind", "super+end=scroll_to_bottom")
        builder.withCustom("keybind", "super+page_up=scroll_page_up")
        builder.withCustom("keybind", "super+page_down=scroll_page_down")
    }

    /// TerminalPaneStyle variants → the wrapper's light/dark TerminalTheme
    /// (matches the Svelte app's xterm themes exactly).
    fileprivate static func terminalTheme(for style: TerminalPaneStyle) -> TerminalTheme {
        TerminalTheme(
            light: themeConfiguration(style.light),
            dark: themeConfiguration(style.dark)
        )
    }

    private static func themeConfiguration(
        _ variant: TerminalPaneStyle.Variant
    ) -> TerminalConfiguration {
        TerminalConfiguration { builder in
            builder.withBackground(variant.background)
            builder.withForeground(variant.foreground)
            builder.withSelectionBackground(variant.selectionBackground)
            builder.withCursorColor(variant.cursorColor)
            for (index, color) in variant.palette.enumerated() {
                builder.withPalette(index, color: color)
            }
        }
    }

    deinit {
        MainActor.assumeIsolated {
            if let observer = occlusionObserver {
                NotificationCenter.default.removeObserver(observer)
            }
            for observer in findObservers {
                NotificationCenter.default.removeObserver(observer)
            }
            tuiJumpHintTimer?.invalidate()
        }
    }

    // MARK: - Render pause for hidden surfaces

    /// Retained-but-detached panes (SurfaceCache keeps every live session's
    /// pane alive; only the selected one is in the view hierarchy) and panes
    /// in an occluded/minimized window must not keep Ghostty's renderer
    /// drawing frames nobody sees. The wrapper exposes this as
    /// `TerminalView.setSurfaceVisible(_:)` → `ghostty_surface_set_occlusion`
    /// plus suspension of its wakeup→tick→draw loop, mirroring what Ghostty
    /// itself does on `NSWindow.occlusionState` changes.
    private var occlusionObserver: NSObjectProtocol?

    // NOTE for pre-warmed panes (WarmPaneHostView): they are mounted inside
    // a HIDDEN container but must NOT be paused via setSurfaceVisible(false)
    // — that suspends the wrapper's wakeup→tick loop, and a surface that
    // never ticks while its attach client floods the replay wedges its IO;
    // the next synchronous surface call from the main thread (adoption on
    // click) then deadlocks. Hidden-but-ticking is the safe state; the
    // hidden container already keeps them out of the compositor.
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let observer = occlusionObserver {
            NotificationCenter.default.removeObserver(observer)
            occlusionObserver = nil
        }
        if let window {
            occlusionObserver = NotificationCenter.default.addObserver(
                forName: NSWindow.didChangeOcclusionStateNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    self?.updateSurfaceVisibility()
                }
            }
        }
        updateSurfaceVisibility()
    }

    private func updateSurfaceVisibility() {
        let visible = window.map { $0.occlusionState.contains(.visible) } ?? false
        terminalView.setSurfaceVisible(visible)
        if visible {
            // Resume with a fresh frame so the viewport is current: the
            // pane may have been resized while detached, and output that
            // arrived while paused has not been drawn.
            terminalView.fitToSize()
            startTuiJumpHintPolling()
        } else {
            stopTuiJumpHintPolling()
        }
    }

    /// Only the pane the user is actually looking at scans for the hint;
    /// detached/occluded panes pay nothing.
    private func startTuiJumpHintPolling() {
        guard tuiJumpHintTimer == nil else { return }
        let timer = Timer(timeInterval: 0.5, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.scanForTuiJumpHint()
            }
        }
        timer.tolerance = 0.2
        RunLoop.main.add(timer, forMode: .common)
        tuiJumpHintTimer = timer
    }

    private func stopTuiJumpHintPolling() {
        tuiJumpHintTimer?.invalidate()
        tuiJumpHintTimer = nil
        tuiJumpRetriesRemaining = 0
        setTuiJumpHintActive(false)
    }

    private func scanForTuiJumpHint() {
        // No scrollback gate: Claude Code draws its virtual-scroll transcript
        // in the PRIMARY screen, so sessions with history would never show
        // the hint. Scanning unconditionally is safe because the button's
        // press does both — fakes ctrl+End (ignored by TUIs that aren't
        // scrolled up) and scrolls the local surface to the bottom — so even
        // a marker matched inside scrolled-back output yields a "jump to
        // bottom" outcome.
        guard let surface else {
            tuiJumpRetriesRemaining = 0
            setTuiJumpHintActive(false)
            return
        }
        let text = surface.readViewportText() ?? ""
        let active = Self.viewportHasTuiJumpHint(text)
        if active, tuiJumpRetriesRemaining > 0 {
            tuiJumpRetriesRemaining -= 1
            surface.sendKeyPress(keycode: Self.endKeycode, mods: GHOSTTY_MODS_CTRL)
            return
        }
        if !active {
            tuiJumpRetriesRemaining = 0
        }
        setTuiJumpHintActive(active)
    }

    private func setTuiJumpHintActive(_ active: Bool) {
        guard tuiJumpHintActive != active else { return }
        tuiJumpHintActive = active
        refreshScrollButtonVisibility()
    }

    private func refreshScrollButtonVisibility() {
        let visible = scrolledUpInScrollback || tuiJumpHintActive
        if scrollButtonModel.visible != visible {
            scrollButtonModel.visible = visible
        }
    }

    private func scrollButtonPressed() {
        if tuiJumpHintActive {
            // Hide optimistically and arm one retry: the poll re-sends the
            // key if the hint reappears, so one press settles at the bottom
            // even when the first ctrl+End lands mid-stream — while a
            // false-positive match leaks at most two "[1;5F" residues.
            tuiJumpRetriesRemaining = 1
            surface?.sendKeyPress(keycode: Self.endKeycode, mods: GHOSTTY_MODS_CTRL)
            setTuiJumpHintActive(false)
        }
        scrollToBottom()
    }

    // MARK: - Find bar behavior

    /// The pane the user is looking at: attached to the key window and not
    /// inside a hidden container (pre-warmed panes are mounted hidden).
    private var isDisplayedForFind: Bool {
        guard let window else { return false }
        return window.isKeyWindow && !isHiddenOrHasHiddenAncestor
    }

    func showFindBar() {
        let bar: TerminalFindBar
        if let existing = findBar {
            bar = existing
        } else {
            bar = TerminalFindBar()
            bar.onQueryChange = { [weak self] query in
                self?.applySearchQuery(query)
            }
            bar.onNext = { [weak self] in self?.findNext() }
            bar.onPrevious = { [weak self] in self?.findPrevious() }
            bar.onClose = { [weak self] in self?.hideFindBar() }
            bar.translatesAutoresizingMaskIntoConstraints = false
            addSubview(bar, positioned: .above, relativeTo: terminalView)
            NSLayoutConstraint.activate([
                bar.topAnchor.constraint(equalTo: topAnchor, constant: 10),
                bar.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -12),
            ])
            findBar = bar
        }
        bar.isHidden = false
        findBarVisible = true
        // Reopening with a retained query re-arms the highlights that
        // end_search cleared.
        if !bar.query.isEmpty {
            applySearchQuery(bar.query)
        }
        bar.focusField()
    }

    func hideFindBar() {
        guard findBarVisible else { return }
        findBarVisible = false
        findBar?.isHidden = true
        searchTotal = nil
        searchSelected = nil
        _ = surface?.performBindingAction("end_search")
        focus()
    }

    func findNext() {
        guard findBarVisible else {
            showFindBar()
            return
        }
        _ = surface?.performBindingAction("navigate_search:next")
    }

    func findPrevious() {
        guard findBarVisible else {
            showFindBar()
            return
        }
        _ = surface?.performBindingAction("navigate_search:previous")
    }

    private func applySearchQuery(_ query: String) {
        // Fresh query, stale counts: clear until render-driven updates land.
        searchTotal = nil
        searchSelected = nil
        findBar?.updateCounts(total: nil, selected: nil)
        // Empty text cancels the search (keeps the bar up; end_search on
        // close tears the highlights down).
        _ = surface?.performBindingAction("search:\(query)")
    }

    // NOTE: an earlier override here trailing-debounced `fitToSize()` by
    // 80ms to coalesce per-frame PTY resizes. It never worked: super.layout()
    // applies the edge constraints, which drives AppTerminalView.setFrameSize
    // → fitToSize SYNCHRONOUSLY in the same pass — the debounced fit only
    // ever fired as a same-size no-op 80ms later. Resize smoothness is now
    // handled at the source (synchronous render + no-stretch gravity in the
    // wrapper; warm panes frozen during live resize in WarmPaneHostView).

    /// Route keyboard focus into the surface.
    func focus() {
        window?.makeFirstResponder(terminalView)
    }

    /// Draws a frame synchronously (no-op while detached or occluded).
    /// Called after adopting + focusing the pane on a session switch so the
    /// swap's own CATransaction presents current content with the focused
    /// cursor, instead of the stale pre-detach drawable for a frame or two
    /// followed by a hollow→filled cursor pop.
    func renderNow() {
        terminalView.renderImmediately()
    }

    /// Re-runs the resize pipeline as if the user nudged the window edge a
    /// pixel and let go. Session switches sometimes land with a broken
    /// terminal layout that only a small manual window resize repairs —
    /// a plain `fitToSize` after re-attach is a same-size no-op to ghostty,
    /// so it cannot fix drift picked up while the pane was detached. Call
    /// after a swap, once layout has settled (next runloop turn).
    func refitNow() {
        terminalView.forceRefit()
    }

    // MARK: - Grid geometry (phone-resize letterbox)

    /// Grid of the most recently displayed full-bleed pane. New sessions
    /// open into the same terminal area, so this is the best estimate for
    /// their launch-time PTY size (`initial_cols`/`initial_rows`): the
    /// workload's first paint then matches the surface, instead of drawing
    /// at a guessed grid and depending on the attach client's corrective
    /// resize landing mid-startup (codex sometimes misses that SIGWINCH and
    /// keeps the wrong layout until the user nudges the window). nil until
    /// any pane has been displayed.
    static private(set) var lastDisplayedGrid: (cols: Int, rows: Int)?

    /// Set by the letterbox host while a phone-resize override constrains
    /// this pane: its grid then reflects the phone, not the terminal area,
    /// and must not feed `lastDisplayedGrid`.
    var isLetterboxed = false

    /// Latest surface grid + cell geometry in view points, and the view size
    /// it was measured against. Captured from the grid-resize delegate;
    /// backs `letterboxSize`.
    private var surfaceCellSize: CGSize?
    private var surfaceGridColumns = 0
    private var surfaceGridRows = 0
    private var surfaceBoundsAtSync = CGRect.zero

    /// Fired after the surface grid changes. The letterbox host uses it to
    /// re-fit once real cell metrics exist (a cold-mounted pane reports its
    /// first grid only after initial layout).
    var onSurfaceGridChanged: (() -> Void)?

    /// View size that renders exactly `cols`×`rows`: the target grid at the
    /// current cell size plus the surface chrome (window padding + sub-cell
    /// leftover) measured out of the last grid sync. Exact as long as the
    /// leftover stays under one cell, which the measurement guarantees.
    /// nil until the surface has reported a grid.
    func letterboxSize(cols: Int, rows: Int) -> CGSize? {
        guard let cell = surfaceCellSize,
              surfaceGridColumns > 0, surfaceGridRows > 0,
              surfaceBoundsAtSync.width > 1, surfaceBoundsAtSync.height > 1
        else { return nil }
        let chromeWidth = max(0, surfaceBoundsAtSync.width - CGFloat(surfaceGridColumns) * cell.width)
        let chromeHeight = max(0, surfaceBoundsAtSync.height - CGFloat(surfaceGridRows) * cell.height)
        return CGSize(
            width: CGFloat(cols) * cell.width + chromeWidth,
            height: CGFloat(rows) * cell.height + chromeHeight
        )
    }

    /// Rounds the pane's corners and draws a hairline bezel so a
    /// phone-letterboxed terminal reads as a phone screen. `masksToBounds`
    /// clips the Metal surface content to the rounded rect. Cleared (radius 0,
    /// no border) when the pane goes full-bleed again.
    func setPhoneScreenFraming(_ enabled: Bool) {
        wantsLayer = true
        guard let layer else { return }
        if enabled {
            layer.cornerRadius = 22
            layer.cornerCurve = .continuous
            layer.masksToBounds = true
            layer.borderWidth = 1
            layer.borderColor = NSColor.white.withAlphaComponent(0.14).cgColor
        } else {
            layer.cornerRadius = 0
            layer.masksToBounds = false
            layer.borderWidth = 0
            layer.borderColor = nil
        }
    }

    /// Injects text into the surface as if typed (used by the self-test).
    @discardableResult
    func sendText(_ text: String) -> Bool {
        surface?.sendText(text) ?? false
    }

    /// Snaps the viewport back to the live end of the screen, like the
    /// user pressing the scroll-to-bottom keybinding.
    func scrollToBottom() {
        surface?.performBindingAction("scroll_to_bottom")
    }

    /// Presses Return through the real AppKit key pipeline
    /// (keyDown -> ghostty_surface_key), exactly like a user keystroke.
    func pressReturn() {
        guard let event = NSEvent.keyEvent(
            with: .keyDown,
            location: .zero,
            modifierFlags: [],
            timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: window?.windowNumber ?? 0,
            context: nil,
            characters: "\r",
            charactersIgnoringModifiers: "\r",
            isARepeat: false,
            keyCode: 36 // kVK_Return
        ) else {
            NSLog("[UnpeelNative] pressReturn: could not synthesize event")
            return
        }
        terminalView.keyDown(with: event)
    }

    /// SPIKE-ONLY diagnostics: dumps the terminal screen as plain text via
    /// `ghostty_surface_read_text`. The GhosttyTerminal wrapper keeps the raw
    /// `ghostty_surface_t` internal, so we pull it out with reflection. Do
    /// not ship this; ask upstream for a public accessor instead.
    func dumpScreenText() -> String? {
        guard let surface else { return nil }
        guard let raw = Mirror(reflecting: surface).children
            .first(where: { $0.label == "surface" })?
            .value as? ghostty_surface_t
        else {
            NSLog("[UnpeelNative] dumpScreenText: raw surface not reachable")
            return nil
        }

        var selection = ghostty_selection_s()
        selection.top_left = ghostty_point_s(
            tag: GHOSTTY_POINT_SCREEN, coord: GHOSTTY_POINT_COORD_TOP_LEFT, x: 0, y: 0
        )
        selection.bottom_right = ghostty_point_s(
            tag: GHOSTTY_POINT_SCREEN, coord: GHOSTTY_POINT_COORD_BOTTOM_RIGHT, x: 0, y: 0
        )
        selection.rectangle = false

        var text = ghostty_text_s()
        guard ghostty_surface_read_text(raw, selection, &text) else { return nil }
        defer { ghostty_surface_free_text(raw, &text) }
        guard let bytes = text.text else { return nil }
        return String(
            decoding: UnsafeBufferPointer(
                start: UnsafeRawPointer(bytes).assumingMemoryBound(to: UInt8.self),
                count: Int(text.text_len)
            ),
            as: UTF8.self
        )
    }

    // MARK: - File drag-and-drop

    private static let imagePasteboardType = NSPasteboard.PasteboardType("public.image")
    private static let pngPasteboardType = NSPasteboard.PasteboardType("public.png")
    private static let dropPasteboardTypes: [NSPasteboard.PasteboardType] = [
        .fileURL,
        .URL,
        .tiff,
        pngPasteboardType,
        imagePasteboardType,
    ]

    /// Dropping files from Finder or image data from another app pastes
    /// attachable references into the terminal. Agent CLIs (e.g. Claude Code)
    /// detect single-quoted or backslash-escaped paths as attachable files, but
    /// not double-quoted ones. Multiple files are space-separated, no trailing
    /// newline so the user can keep typing.
    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        Self.canReadDropReferences(from: sender.draggingPasteboard) ? .copy : []
    }

    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        Self.canReadDropReferences(from: sender.draggingPasteboard) ? .copy : []
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        let references = Self.dropReferences(from: sender.draggingPasteboard)
        guard !references.isEmpty else { return false }
        let text = references.map(Self.quoteDropReference).joined(separator: " ")
        return insertDroppedText(text)
    }

    private static func canReadDropReferences(from pasteboard: NSPasteboard) -> Bool {
        if pasteboard.canReadObject(
            forClasses: [NSURL.self],
            options: [.urlReadingFileURLsOnly: true]
        ) {
            return true
        }
        if pasteboard.canReadObject(forClasses: [NSURL.self], options: nil) {
            return true
        }
        return pasteboard.canReadObject(forClasses: [NSImage.self], options: nil)
    }

    private static func dropReferences(from pasteboard: NSPasteboard) -> [String] {
        if let fileURLs = pasteboard.readObjects(
            forClasses: [NSURL.self],
            options: [.urlReadingFileURLsOnly: true]
        ) as? [URL], !fileURLs.isEmpty {
            return fileURLs.map(Self.stableDropPath)
        }

        if let urls = pasteboard.readObjects(
            forClasses: [NSURL.self],
            options: nil
        ) as? [URL] {
            let references = urls.map { url in
                url.isFileURL ? url.path : url.absoluteString
            }
            if !references.isEmpty { return references }
        }

        guard let images = pasteboard.readObjects(
            forClasses: [NSImage.self],
            options: nil
        ) as? [NSImage] else {
            return []
        }
        return images.compactMap(saveDroppedImage)
    }

    /// macOS screenshot thumbnails (and other drags from a temporary
    /// location) point at volatile files under `.../TemporaryItems/
    /// NSIRD_screencaptureui…` that the OS deletes the moment the screenshot
    /// UI finalizes — so pasting their raw path yields a dead reference.
    /// Copy such files into the stable `dropped-images` dir (same place the
    /// image-data drop and the phone attach flow use) and reference the copy.
    /// Files already in a durable location are referenced in place.
    private static func stableDropPath(_ url: URL) -> String {
        guard isVolatileLocation(url), let data = try? Data(contentsOf: url) else {
            return url.path
        }
        let dir = LaunchConfig.unpeelDir.appendingPathComponent(
            "dropped-images",
            isDirectory: true
        )
        do {
            try FileManager.default.createDirectory(
                at: dir,
                withIntermediateDirectories: true
            )
            let ext = url.pathExtension.isEmpty ? "png" : url.pathExtension
            let timestamp = UInt64(Date().timeIntervalSince1970 * 1000)
            let filename = "drop-\(timestamp)-\(UUID().uuidString).\(ext)"
            let dest = dir.appendingPathComponent(filename)
            try data.write(to: dest, options: .atomic)
            return dest.path
        } catch {
            NSLog("[UnpeelNative] failed to stabilize dropped file: \(error)")
            return url.path
        }
    }

    private static func isVolatileLocation(_ url: URL) -> Bool {
        let path = url.path
        if path.contains("/TemporaryItems/") { return true }
        if path.localizedCaseInsensitiveContains("screencaptureui") { return true }
        if path.hasPrefix(NSTemporaryDirectory()) { return true }
        // Per-user sandbox temp: /var/folders/<…>/T/
        if path.contains("/var/folders/"), path.contains("/T/") { return true }
        return false
    }

    private static func saveDroppedImage(_ image: NSImage) -> String? {
        guard let tiff = image.tiffRepresentation,
              let rep = NSBitmapImageRep(data: tiff),
              let data = rep.representation(using: .png, properties: [:])
        else { return nil }

        let dir = LaunchConfig.unpeelDir.appendingPathComponent(
            "dropped-images",
            isDirectory: true
        )
        do {
            try FileManager.default.createDirectory(
                at: dir,
                withIntermediateDirectories: true
            )
            let timestamp = UInt64(Date().timeIntervalSince1970 * 1000)
            let filename = "drop-\(timestamp)-\(UUID().uuidString).png"
            let url = dir.appendingPathComponent(filename)
            try data.write(to: url, options: .atomic)
            return url.path
        } catch {
            NSLog("[UnpeelNative] failed to save dropped image: \(error)")
            return nil
        }
    }

    /// Inserts a file path into the prompt the same way a Finder drop does:
    /// quoted so agent CLIs detect it as an attachable reference, then typed
    /// (or pasted) into the focused surface. Used by the session gallery's
    /// "Add to prompt".
    @discardableResult
    func insertAttachablePath(_ path: String) -> Bool {
        insertDroppedText(Self.quoteDropReference(path))
    }

    private func insertDroppedText(_ text: String) -> Bool {
        focus()
        // Paste, don't type: agent TUIs only recognize an image path (and
        // collapse it to their "[Image #N]" attachment chip) when it arrives
        // as a bracketed paste. Ghostty's paste binding wraps in the paste
        // markers exactly when the app has enabled them, so this is also
        // safe for plain shells. sendText stays as the no-clipboard
        // fallback — it types the raw path.
        if pasteText(text) {
            return true
        }
        return sendText(text)
    }

    private func pasteText(_ text: String) -> Bool {
        guard let surface else { return false }
        let pasteboard = NSPasteboard.general
        // NSPasteboardItem is not NSCopying — copy() throws. Snapshot the
        // clipboard by reading each item's data into a fresh item instead.
        let previousItems: [NSPasteboardItem] = (pasteboard.pasteboardItems ?? []).map { item in
            let snapshot = NSPasteboardItem()
            for type in item.types {
                if let data = item.data(forType: type) {
                    snapshot.setData(data, forType: type)
                }
            }
            return snapshot
        }

        pasteboard.clearContents()
        guard pasteboard.setString(text, forType: .string) else {
            restorePasteboardItems(previousItems)
            return false
        }
        let pasted = surface.performBindingAction("paste_from_clipboard")
        restorePasteboardItems(previousItems)
        return pasted || sendText(text)
    }

    private func restorePasteboardItems(_ items: [NSPasteboardItem]) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        if !items.isEmpty {
            pasteboard.writeObjects(items)
        }
    }

    /// Same safe-character set as the Svelte app's drop handler.
    nonisolated static func quoteDropReference(_ reference: String) -> String {
        guard reference.range(of: "[^\\w@%+=:,./-]", options: .regularExpression) != nil else {
            return reference
        }
        return "'" + reference.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    /// Enables the wrapper's stderr debug log (lifecycle, input, actions).
    static func enableDebugLogging() {
        TerminalDebugLog.enable(.standard)
    }
}

// MARK: - GhosttyTerminal delegates → plain delegate

extension GhosttyTerminalPane:
    TerminalSurfaceTitleDelegate,
    TerminalSurfaceGridResizeDelegate,
    TerminalSurfaceCloseDelegate,
    TerminalSurfaceBellDelegate,
    TerminalSurfacePwdDelegate,
    TerminalSurfaceOpenURLDelegate,
    TerminalSurfaceClickableFileDelegate,
    TerminalSurfaceScrollbarDelegate,
    TerminalSurfaceSearchDelegate,
    TerminalSurfaceLifecycleDelegate
{
    func terminalDidAttachSurface(_ surface: TerminalSurface) {
        self.surface = surface
        NSLog("[UnpeelNative] surface attached")
    }

    func terminalDidDetachSurface() {
        surface = nil
        scrolledUpInScrollback = false
        tuiJumpHintActive = false
        lastScrollbarMetrics = nil
        scrollButtonModel.visible = false
        NSLog("[UnpeelNative] surface detached")
    }

    func terminalDidUpdateScrollbar(_ metrics: TerminalScrollbarMetrics) {
        // In the alternate screen (full-screen TUIs) there is no
        // scrollback: total == viewport, so this path never fires "scrolled
        // up" — the TUI jump-hint scan covers that case instead.
        lastScrollbarMetrics = metrics
        scrolledUpInScrollback = !metrics.isAtBottom
        refreshScrollButtonVisibility()
    }

    // MARK: TerminalSurfaceSearchDelegate

    func terminalDidRequestStartSearch(needle _: String?) {
        // Keybinds are cleared, so today this only ever echoes our own
        // driving; keep it as the convergence point in case a future
        // binding or core path raises it.
        if !findBarVisible { showFindBar() }
    }

    func terminalDidRequestEndSearch() {
        // Core-initiated end: hide without re-sending end_search.
        guard findBarVisible else { return }
        findBarVisible = false
        findBar?.isHidden = true
        searchTotal = nil
        searchSelected = nil
    }

    func terminalDidUpdateSearchTotal(_ total: Int) {
        searchTotal = total
        findBar?.updateCounts(total: searchTotal, selected: searchSelected)
    }

    func terminalDidUpdateSearchSelected(_ selected: Int) {
        searchSelected = selected
        findBar?.updateCounts(total: searchTotal, selected: searchSelected)
    }

    func terminalDidChangeTitle(_ title: String) {
        paneDelegate?.terminalPane(self, didChangeTitle: title)
    }

    func terminalDidResize(_ size: TerminalGridMetrics) {
        let scale = max(window?.backingScaleFactor ?? 2, 1)
        surfaceGridColumns = Int(size.columns)
        surfaceGridRows = Int(size.rows)
        if size.cellWidthPixels > 0, size.cellHeightPixels > 0 {
            surfaceCellSize = CGSize(
                width: CGFloat(size.cellWidthPixels) / scale,
                height: CGFloat(size.cellHeightPixels) / scale
            )
        }
        surfaceBoundsAtSync = terminalView.bounds
        // Only a pane that is actually in the window reflects the terminal
        // area; pre-warmed/detached panes report layout-less default grids.
        if window != nil, !isLetterboxed, size.columns >= 20, size.rows >= 5 {
            Self.lastDisplayedGrid = (cols: Int(size.columns), rows: Int(size.rows))
        }
        // (No log here: this fires on every grid change during a drag, and
        // the wrapper already logs the same transition at .metrics level.)
        onSurfaceGridChanged?()
    }

    func terminalDidClose(processAlive: Bool) {
        paneDelegate?.terminalPane(self, didCloseProcessAlive: processAlive)
    }

    // Log-and-stub the rest for the spike.

    func terminalDidRingBell() {
        NSLog("[UnpeelNative] bell (stub)")
    }

    func terminalDidChangeWorkingDirectory(_ path: String) {
        // OSC 7 reports the shell's cwd; keep it so cmd-click resolves relative
        // paths against where the agent is actually working, not just the
        // spawn dir. Some agents/shells never emit it — the seed cwd remains.
        currentWorkingDirectory = path
    }

    /// Cmd-clicked a file path: resolve it against the session cwd and open it
    /// in the user's editor. Returns true only when it resolves to a real file
    /// (so a cmd-click on a URL or plain text falls through to normal handling).
    func terminalDidCommandClick(rowText: String, column: Int) -> Bool {
        guard let match = ClickablePath.match(inRow: rowText, column: column),
              let resolved = resolveClickedFile(match.path)
        else { return false }
        UnpeelStore.openFileInPreferredEditor(
            path: resolved,
            line: match.line,
            column: match.column
        )
        return true
    }

    /// Turns a clicked path token into an absolute path to an existing file, or
    /// nil. Absolute and `~` paths are used as-is; relative paths join the
    /// current working directory.
    private func resolveClickedFile(_ raw: String) -> String? {
        var path = raw
        if path.hasPrefix("~") {
            path = (path as NSString).expandingTildeInPath
        }
        if !path.hasPrefix("/") {
            guard let cwd = currentWorkingDirectory, !cwd.isEmpty else { return nil }
            path = (cwd as NSString).appendingPathComponent(path)
        }
        path = (path as NSString).standardizingPath

        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory),
              !isDirectory.boolValue
        else { return nil }
        return path
    }

    func terminalDidRequestOpenURL(_ url: String, kind _: TerminalOpenURLKind) {
        guard let parsed = Self.sanitizedURL(from: url) else {
            NSLog("[UnpeelNative] refusing malformed/unsupported url: \(url)")
            return
        }
        NSLog("[UnpeelNative] open url \(parsed.absoluteString)")
        if !NSWorkspace.shared.open(parsed) {
            NSLog("[UnpeelNative] NSWorkspace failed to open \(parsed.absoluteString)")
        }
    }

    /// Turn whatever the terminal handed us into something LaunchServices can
    /// actually open. Terminal-detected links routinely arrive wrapped across
    /// lines, padded with whitespace, or fenced in markdown punctuation like
    /// `(https://…)` / `<https://…>`; `URL(string:)` happily parses that junk
    /// into a URL that `NSWorkspace.open` then rejects with `-50` (paramErr).
    /// We strip the noise, re-encode if needed, and only allow safe schemes.
    nonisolated static func sanitizedURL(from raw: String) -> URL? {
        // Drop internal whitespace/newlines (a URL never legitimately contains
        // any — wrapped links pick these up from the terminal grid).
        var s = raw.components(separatedBy: .whitespacesAndNewlines).joined()
        guard !s.isEmpty else { return nil }

        // Peel matched wrapping brackets/quotes, then trailing punctuation that
        // commonly hugs an inline link (sentence periods, commas, etc.).
        let wrappers: [(Character, Character)] = [
            ("(", ")"), ("[", "]"), ("{", "}"), ("<", ">"),
            ("\"", "\""), ("'", "'"), ("`", "`"),
        ]
        for (open, close) in wrappers where s.first == open && s.last == close && s.count >= 2 {
            s = String(s.dropFirst().dropLast())
        }
        while let last = s.last, ".,;:!?\"')]}>".contains(last) {
            s = String(s.dropLast())
        }
        guard !s.isEmpty else { return nil }

        // Add a scheme for bare hosts so `www.example.com` / `example.com/x`
        // still open in the browser instead of being treated as a file path.
        if !s.contains("://"), !s.hasPrefix("mailto:"), !s.hasPrefix("tel:") {
            let host = s.split(separator: "/").first.map(String.init) ?? s
            if host.contains(".") {
                s = "https://" + s
            }
        }

        // Build the URL, percent-encoding leftover illegal characters if the
        // strict parse fails.
        let parsed = URL(string: s)
            ?? s.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed).flatMap(URL.init(string:))
        guard let parsed, let scheme = parsed.scheme?.lowercased() else { return nil }

        let allowed: Set<String> = ["http", "https", "mailto", "tel", "ftp", "ftps"]
        guard allowed.contains(scheme) else { return nil }
        return parsed
    }
}

// MARK: - Remote Host terminal surface

/// Ghostty-free viewport value delivered to the remote transport whenever
/// the Controller's pane changes size. The transport decides how and when to
/// send it to the Host; this bridge never reaches into local session state.
struct RemoteTerminalViewport: Equatable, Sendable {
    let columns: Int
    let rows: Int
    let widthPixels: Int
    let heightPixels: Int
    let cellWidthPixels: Int
    let cellHeightPixels: Int

    fileprivate init(_ viewport: InMemoryTerminalViewport) {
        columns = Int(viewport.columns)
        rows = Int(viewport.rows)
        widthPixels = Int(viewport.widthPixels)
        heightPixels = Int(viewport.heightPixels)
        cellWidthPixels = Int(viewport.cellWidthPixels)
        cellHeightPixels = Int(viewport.cellHeightPixels)
    }
}

typealias RemoteTerminalInputHandler = @MainActor @Sendable (Data) -> Void
typealias RemoteTerminalResizeHandler = @MainActor @Sendable (RemoteTerminalViewport) -> Void

/// Token captured when a terminal callback is enqueued. Rebinding or clearing
/// a pane advances the token, so already-queued input/resize is discarded
/// instead of crossing into the replacement transport.
struct RemoteTerminalCallbackEpoch: Equatable, Sendable {
    private(set) var revision: UInt64 = 0

    mutating func advance() {
        revision &+= 1
    }

    func accepts(_ queuedEpoch: RemoteTerminalCallbackEpoch) -> Bool {
        self == queuedEpoch
    }
}

/// Mutable callback indirection for a retained pane. A reconnect may replace
/// the transport while the Ghostty surface (and its last rendered frame)
/// stays alive, so the in-memory session must not permanently capture the
/// connection that happened to create it.
private final class RemoteTerminalCallbackRelay: @unchecked Sendable {
    private let lock = NSLock()
    private var inputHandler: RemoteTerminalInputHandler
    private var resizeHandler: RemoteTerminalResizeHandler
    private var epoch = RemoteTerminalCallbackEpoch()

    init(
        input: @escaping RemoteTerminalInputHandler,
        resize: @escaping RemoteTerminalResizeHandler
    ) {
        inputHandler = input
        resizeHandler = resize
    }

    func update(
        input: @escaping RemoteTerminalInputHandler,
        resize: @escaping RemoteTerminalResizeHandler
    ) {
        lock.lock()
        epoch.advance()
        inputHandler = input
        resizeHandler = resize
        lock.unlock()
    }

    func sendInput(_ data: Data) {
        let queuedEpoch = currentEpoch()
        deliverOnMain { relay in
            relay.deliverInput(data, queuedEpoch: queuedEpoch)
        }
    }

    func sendResize(_ viewport: InMemoryTerminalViewport) {
        let queuedEpoch = currentEpoch()
        let plainViewport = RemoteTerminalViewport(viewport)
        deliverOnMain { relay in
            relay.deliverResize(plainViewport, queuedEpoch: queuedEpoch)
        }
    }

    /// Break transport/runtime captures as soon as a pane is evicted. The
    /// Ghostty teardown itself is deferred by one main-queue turn, so relying
    /// on deinit alone would leave a brief window where input could still hit
    /// a retired connection.
    func clear() {
        lock.lock()
        epoch.advance()
        inputHandler = { _ in }
        resizeHandler = { _ in }
        lock.unlock()
    }

    private func currentEpoch() -> RemoteTerminalCallbackEpoch {
        lock.lock()
        let value = epoch
        lock.unlock()
        return value
    }

    private func deliverOnMain(
        _ body: @escaping @MainActor @Sendable (RemoteTerminalCallbackRelay) -> Void
    ) {
        if Thread.isMainThread {
            MainActor.assumeIsolated {
                body(self)
            }
        } else {
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                body(self)
            }
        }
    }

    @MainActor
    private func deliverInput(
        _ data: Data,
        queuedEpoch: RemoteTerminalCallbackEpoch
    ) {
        lock.lock()
        guard epoch.accepts(queuedEpoch) else {
            lock.unlock()
            return
        }
        let handler = inputHandler
        lock.unlock()
        handler(data)
    }

    @MainActor
    private func deliverResize(
        _ viewport: RemoteTerminalViewport,
        queuedEpoch: RemoteTerminalCallbackEpoch
    ) {
        lock.lock()
        guard epoch.accepts(queuedEpoch) else {
            lock.unlock()
            return
        }
        let handler = resizeHandler
        lock.unlock()
        handler(viewport)
    }
}

/// Bytes that are injected only into the Controller's local VT parser. They
/// are never routed through `InMemoryTerminalSession.sendInput`, so resetting
/// a retained frame cannot write escape sequences to the Host PTY.
struct RemoteTerminalLocalFeed: Equatable, Sendable {
    let bytes: Data

    /// CAN aborts an unterminated OSC/DCS before RIS. A retention rebase may
    /// deliberately cut such a pathological control string to keep an
    /// always-on journal bounded, so ESC c alone could be swallowed.
    private static let reset = Data([0x18, 0x1B, 0x63])
    private static let beginSynchronizedOutput = Data("\u{1B}[?2026h".utf8)
    private static let clearDisplayAndScrollback = Data(
        "\u{1B}[3J\u{1B}[2J\u{1B}[H".utf8
    )
    private static let endSynchronizedOutput = Data("\u{1B}[?2026l".utf8)

    /// Standalone reset: RIS clears terminal modes; CSI 3J/2J/H clears the
    /// retained screen, scrollback, and cursor. The synchronized-output pair
    /// prevents an intermediate blank frame from presenting.
    static let resetRetainedState = RemoteTerminalLocalFeed(
        bytes: reset
            + beginSynchronizedOutput
            + clearDisplayAndScrollback
            + endSynchronizedOutput
    )

    /// Atomic reset + replacement output. RIS must precede DEC 2026 because
    /// RIS itself resets synchronized-output mode.
    static func resettingBeforeFeeding(_ payload: Data) -> RemoteTerminalLocalFeed {
        RemoteTerminalLocalFeed(
            bytes: reset
                + beginSynchronizedOutput
                + clearDisplayAndScrollback
                + payload
                + endSynchronizedOutput
        )
    }
}

/// A retained, GPU-rendered terminal whose process and byte stream are owned
/// by a remote Host. This is intentionally a different type from
/// ``GhosttyTerminalPane``:
///
/// - its backend is `InMemoryTerminalSession`, never Ghostty's EXEC backend;
/// - it has no command or working directory and cannot launch `unpeel-attach`;
/// - host output enters only through ``receiveHostBytes(_:)``;
/// - keyboard/mouse input and viewport changes leave through plain-Swift
///   callbacks; and
/// - it does not implement URL/PWD delegates, and its file-click delegate is
///   deliberately inert, so a path printed by the Host is never resolved or
///   opened against the Controller's filesystem.
///
/// Removing this view from a hierarchy pauses rendering but deliberately
/// keeps both the surface and its last frame alive. Reattaching the same pane
/// therefore paints immediately and preserves scrollback and VT state.
@MainActor
final class RemoteGhosttyTerminalPane: NSView {
    private let terminalView: TerminalView
    private let controller: TerminalController
    private let memorySession: InMemoryTerminalSession
    private let callbackRelay: RemoteTerminalCallbackRelay
    private var surface: TerminalSurface?
    private var paneStyle: TerminalPaneStyle
    private var occlusionObserver: NSObjectProtocol?
    private var presentationEnabled = true
    private var needsRefitOnNextPresentation = true

    init(
        style: TerminalPaneStyle = .resolved(),
        onInput: @escaping RemoteTerminalInputHandler,
        onResize: @escaping RemoteTerminalResizeHandler
    ) {
        paneStyle = style

        let relay = RemoteTerminalCallbackRelay(input: onInput, resize: onResize)
        callbackRelay = relay
        memorySession = InMemoryTerminalSession(
            write: { data in relay.sendInput(data) },
            resize: { viewport in relay.sendResize(viewport) }
        )

        // There is deliberately no `command` config. The surface below uses
        // HOST_MANAGED IO, so Ghostty parses/renders bytes but never spawns a
        // local process for a remote session.
        controller = TerminalController(theme: GhosttyTerminalPane.terminalTheme(for: style)) {
            builder in
            GhosttyTerminalPane.applySurfaceKeybinds(&builder)
            builder.withCustom("window-padding-x", "\(style.windowPaddingX)")
            builder.withCustom("window-padding-y", "\(style.windowPaddingY)")
            builder.withCustom(
                "window-padding-balance",
                style.windowPaddingBalanced ? "true" : "false"
            )
            builder.withCustom("window-padding-color", "extend")
            builder.withCustom(
                "mouse-scroll-multiplier",
                "precision:1,discrete:\(style.mouseScrollMultiplier)"
            )
            builder.withCursorStyle(.block)
            builder.withCursorStyleBlink(true)
            builder.withFontSize(style.fontSize)
            if let family = style.fontFamily {
                builder.withFontFamily(family)
            }
        }

        if let issue = controller.lastConfigurationIssue {
            NSLog("[UnpeelNative] remote ghostty configuration issue: %@", issue)
        }

        terminalView = TerminalView(frame: .zero)
        terminalView.gridOrigin = CGPoint(
            x: CGFloat(style.windowPaddingX),
            y: CGFloat(style.windowPaddingY)
        )

        super.init(frame: .zero)

        applyFrameLayerBackground(style)
        terminalView.configuration = TerminalSurfaceOptions(
            backend: .inMemory(memorySession),
            workingDirectory: nil,
            context: .window
        )
        terminalView.delegate = self
        terminalView.controller = controller
        terminalView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(terminalView)
        NSLayoutConstraint.activate([
            terminalView.topAnchor.constraint(equalTo: topAnchor),
            terminalView.leadingAnchor.constraint(equalTo: leadingAnchor),
            terminalView.trailingAnchor.constraint(equalTo: trailingAnchor),
            terminalView.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) { fatalError("not supported") }

    deinit {
        MainActor.assumeIsolated {
            callbackRelay.clear()
            if let occlusionObserver {
                NotificationCenter.default.removeObserver(occlusionObserver)
            }
        }
    }

    /// Rebind a retained pane after transport reconnect without losing its
    /// terminal state. No bytes can escape to the superseded connection once
    /// this returns; the relay swaps both callbacks under one lock.
    func updateCallbacks(
        onInput: @escaping RemoteTerminalInputHandler,
        onResize: @escaping RemoteTerminalResizeHandler
    ) {
        callbackRelay.update(input: onInput, resize: onResize)
    }

    fileprivate func clearCallbacks() {
        callbackRelay.clear()
    }

    /// A remote output cursor may advance only after this pane accepts the
    /// corresponding bytes. Detached panes deliberately report not-ready:
    /// `InMemoryTerminalSession` can buffer pre-attach bytes, but doing so
    /// would make acceptance invisible to the runtime and could commit a Host
    /// cursor before any surface actually parsed the page.
    var isReadyForHostBytes: Bool {
        surface != nil
    }

    /// Feed an exact output page/chunk from the selected Host into Ghostty's
    /// VT engine. Returns true only after an attached surface synchronously
    /// accepts the whole feed. A detached pane refuses without invoking
    /// `memorySession.receive`, including when `resetBeforeFeed` is set, so
    /// callers must retain/retry the same uncommitted output page.
    @discardableResult
    func receiveHostBytes(_ data: Data, resetBeforeFeed: Bool = false) -> Bool {
        guard isReadyForHostBytes else { return false }
        let localFeed = resetBeforeFeed
            ? RemoteTerminalLocalFeed.resettingBeforeFeeding(data).bytes
            : data
        memorySession.receive(localFeed)
        return true
    }

    @discardableResult
    func receiveHostText(_ text: String) -> Bool {
        receiveHostBytes(Data(text.utf8))
    }

    /// Reset only the retained Controller-side VT state. Prefer the atomic
    /// `receiveHostBytes(_:resetBeforeFeed:)` path when replacement output is
    /// already available, so no blank reset frame can present between calls.
    @discardableResult
    func resetRetainedVTState() -> Bool {
        guard isReadyForHostBytes else { return false }
        memorySession.receive(RemoteTerminalLocalFeed.resetRetainedState.bytes)
        return true
    }

    /// Signal a Host-owned process exit to Ghostty. This never tears down or
    /// launches a Controller-side process.
    func finishHostProcess(exitCode: UInt32, runtimeMilliseconds: UInt64) {
        memorySession.finish(
            exitCode: exitCode,
            runtimeMilliseconds: runtimeMilliseconds
        )
    }

    /// Lets an owner keep a pane mounted during a transition without paying
    /// for hidden rendering. Host bytes and VT state continue to be retained.
    func setPresentationEnabled(_ enabled: Bool) {
        guard presentationEnabled != enabled else { return }
        presentationEnabled = enabled
        if !enabled {
            needsRefitOnNextPresentation = true
        }
        updateSurfaceVisibility()
    }

    func applyPaneStyle(_ style: TerminalPaneStyle) {
        guard !Self.hasSameTheme(paneStyle, style) else { return }
        paneStyle = style
        _ = controller.setTheme(GhosttyTerminalPane.terminalTheme(for: style))
        applyFrameLayerBackground(style)
    }

    /// Live style updates intentionally cover colors only. Font and padding
    /// are surface geometry established at construction; callers that change
    /// those should evict/recreate the cache entry instead of reflowing a
    /// retained remote screen during a session switch.
    private static func hasSameTheme(
        _ lhs: TerminalPaneStyle,
        _ rhs: TerminalPaneStyle
    ) -> Bool {
        func same(
            _ lhs: TerminalPaneStyle.Variant,
            _ rhs: TerminalPaneStyle.Variant
        ) -> Bool {
            lhs.background == rhs.background
                && lhs.foreground == rhs.foreground
                && lhs.selectionBackground == rhs.selectionBackground
                && lhs.cursorColor == rhs.cursorColor
                && lhs.palette == rhs.palette
        }
        return same(lhs.light, rhs.light) && same(lhs.dark, rhs.dark)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyFrameLayerBackground(paneStyle)
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let occlusionObserver {
            NotificationCenter.default.removeObserver(occlusionObserver)
            self.occlusionObserver = nil
        }
        if let window {
            occlusionObserver = NotificationCenter.default.addObserver(
                forName: NSWindow.didChangeOcclusionStateNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    self?.updateSurfaceVisibility()
                }
            }
        } else {
            needsRefitOnNextPresentation = true
        }
        updateSurfaceVisibility()
    }

    private func updateSurfaceVisibility() {
        let visible = presentationEnabled
            && (window?.occlusionState.contains(.visible) ?? false)
        terminalView.setSurfaceVisible(visible)
        guard visible else { return }

        terminalView.fitToSize()
        if needsRefitOnNextPresentation {
            needsRefitOnNextPresentation = false
            terminalView.forceRefit()
        }
        terminalView.renderImmediately()
    }

    private func applyFrameLayerBackground(_ style: TerminalPaneStyle) {
        wantsLayer = true
        let appearance = window?.effectiveAppearance ?? effectiveAppearance
        let isDark = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        let hex = isDark ? style.dark.background : style.light.background
        layer?.backgroundColor = (
            GhosttyTerminalPane.nsColor(fromHexString: hex)
                ?? Theme.terminalBackgroundNSColor
        ).cgColor
    }

    func focus() {
        window?.makeFirstResponder(terminalView)
    }

    func renderNow() {
        terminalView.renderImmediately()
    }

    func refitNow() {
        terminalView.forceRefit()
        terminalView.renderImmediately()
    }

    func scrollToBottom() {
        surface?.performBindingAction("scroll_to_bottom")
    }

    func readViewportText() -> String? {
        memorySession.readViewportText()
    }
}

extension RemoteGhosttyTerminalPane:
    TerminalSurfaceLifecycleDelegate,
    TerminalSurfaceClickableFileDelegate
{
    func terminalDidAttachSurface(_ surface: TerminalSurface) {
        self.surface = surface
    }

    func terminalDidDetachSurface() {
        surface = nil
    }

    /// Remote paths are meaningful only on the Host. Returning false keeps
    /// ordinary selection behavior and, crucially, performs no Controller
    /// filesystem lookup or editor launch.
    func terminalDidCommandClick(rowText _: String, column _: Int) -> Bool {
        false
    }
}

/// Stable identity for a retained remote pane. Session ids are Host-local,
/// so the Host id must be part of every cache lookup.
struct RemoteTerminalPaneKey: Hashable, Sendable {
    let hostID: String
    let sessionID: String
}

/// Pure LRU bookkeeping shared by the real pane cache and focused tests.
/// Protected entries (normally the selected pane) may take the set above the
/// nominal limit, but are never evicted out from under the visible surface.
struct RemoteTerminalPaneRetention {
    let limit: Int
    private(set) var mostRecent: [RemoteTerminalPaneKey] = []

    init(limit: Int = 8) {
        self.limit = max(1, limit)
    }

    mutating func noteUsed(_ key: RemoteTerminalPaneKey) {
        mostRecent.removeAll { $0 == key }
        mostRecent.append(key)
    }

    mutating func remove(_ key: RemoteTerminalPaneKey) {
        mostRecent.removeAll { $0 == key }
    }

    mutating func retained(
        from available: Set<RemoteTerminalPaneKey>,
        protecting protected: Set<RemoteTerminalPaneKey> = []
    ) -> Set<RemoteTerminalPaneKey> {
        mostRecent.removeAll { !available.contains($0) }
        var keep = protected.intersection(available)
        for key in mostRecent.reversed() where keep.count < limit {
            keep.insert(key)
        }
        return keep
    }
}

/// Remote-only pane retention. It never consults the Local `SurfaceCache`,
/// local `SessionEntry` values, launch commands, or filesystem paths.
@MainActor
final class RemoteGhosttyPaneCache {
    static let retainedPaneLimit = 8

    private var panes: [RemoteTerminalPaneKey: RemoteGhosttyTerminalPane] = [:]
    private var retention: RemoteTerminalPaneRetention

    init(retainedPaneLimit: Int = RemoteGhosttyPaneCache.retainedPaneLimit) {
        retention = RemoteTerminalPaneRetention(limit: retainedPaneLimit)
    }

    func pane(
        for key: RemoteTerminalPaneKey,
        style: TerminalPaneStyle = .resolved(),
        onInput: @escaping RemoteTerminalInputHandler,
        onResize: @escaping RemoteTerminalResizeHandler
    ) -> RemoteGhosttyTerminalPane {
        if let existing = panes[key] {
            existing.updateCallbacks(onInput: onInput, onResize: onResize)
            existing.applyPaneStyle(style)
            retention.noteUsed(key)
            return existing
        }

        let pane = RemoteGhosttyTerminalPane(
            style: style,
            onInput: onInput,
            onResize: onResize
        )
        panes[key] = pane
        retention.noteUsed(key)
        return pane
    }

    func existingPane(for key: RemoteTerminalPaneKey) -> RemoteGhosttyTerminalPane? {
        panes[key]
    }

    func noteShown(_ key: RemoteTerminalPaneKey) {
        guard panes[key] != nil else { return }
        retention.noteUsed(key)
    }

    func prune(
        keeping liveKeys: Set<RemoteTerminalPaneKey>,
        selectedKey: RemoteTerminalPaneKey?
    ) {
        let protected = selectedKey.map { Set([$0]) } ?? []
        let keep = retention.retained(
            from: Set(panes.keys).intersection(liveKeys),
            protecting: protected
        )
        for key in Array(panes.keys) where !keep.contains(key) {
            drop(key)
        }
    }

    func removeHost(_ hostID: String) {
        for key in Array(panes.keys) where key.hostID == hostID {
            drop(key)
        }
    }

    func removeAll() {
        for key in Array(panes.keys) {
            drop(key)
        }
    }

    private func drop(_ key: RemoteTerminalPaneKey) {
        guard let pane = panes.removeValue(forKey: key) else { return }
        retention.remove(key)
        pane.clearCallbacks()
        pane.setPresentationEnabled(false)

        // Surface teardown can release Metal resources. Move that work out
        // of the SwiftUI/layout pass that decided to prune, just like the
        // Local cache does, while preserving this cache as a separate owner.
        DispatchQueue.main.async {
            pane.removeFromSuperview()
        }
    }
}
