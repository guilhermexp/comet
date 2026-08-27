//
//  SurfaceCache.swift
//  UnpeelNative
//
//  Retains one GhosttyTerminalPane per session id. Surfaces are created on
//  first selection (or pre-warmed on hover intent) and then KEPT ALIVE and
//  swapped in/out of the view hierarchy — never destroyed on switch — so
//  switching sessions is instant and preserves viewport state (the native
//  advantage over the webview).
//
//  Panes are dropped when their session disappears from the store (host
//  died / GC'd), and beyond that the cache keeps only the most recently
//  shown panes (`retainedPaneLimit`) so memory doesn't grow with every
//  session ever viewed — the webview app caps retention at 2; with the
//  kqueue attach client a hidden pane costs ~0 CPU, so we can afford more.
//
//  Provider themes (OpenCode / Grok): config FSEvents + a short canvas
//  sample of output.bin (truecolor bg the TUI is painting). Sampling is
//  what makes in-TUI theme changes update the titlebar/Ghostty default bg
//  without a session switch — Grok often paints first and only later (or
//  never) writes config.toml.
//

import AppKit
import CoreServices
import Combine

/// Owns panes between cache eviction and their deferred main-loop teardown.
/// A pane requested again before teardown is reclaimed from here, so the
/// cache never starts a replacement `unpeel-attach` while the old one is
/// merely waiting for its staggered release turn.
struct DeferredSurfaceEvictions<Value> {
    struct Token: Equatable {
        fileprivate let rawValue: UInt64
    }

    private var nextToken: UInt64 = 0
    private var pending: [String: (token: Token, value: Value)] = [:]

    func contains(_ id: String) -> Bool {
        pending[id] != nil
    }

    mutating func schedule(_ value: Value, for id: String) -> Token {
        precondition(pending[id] == nil, "surface eviction already pending")
        nextToken &+= 1
        let token = Token(rawValue: nextToken)
        pending[id] = (token, value)
        return token
    }

    mutating func reclaim(_ id: String) -> Value? {
        pending.removeValue(forKey: id)?.value
    }

    /// Claim a pane for teardown only when this is still its current
    /// eviction. A reclaimed/rescheduled pane rejects stale queued work.
    mutating func take(_ id: String, token: Token) -> Value? {
        guard pending[id]?.token == token else { return nil }
        return pending.removeValue(forKey: id)?.value
    }
}

@MainActor
final class SurfaceCache: ObservableObject {
    /// Most-recently-shown panes kept beyond the selected/prewarmed set.
    static let retainedPaneLimit = 8

    /// Bumped whenever a provider frame style is reapplied. SwiftUI reads
    /// this so the titlebar and swap-container pick up the new background
    /// without a session switch.
    @Published private(set) var themeRevision: UInt = 0

    private struct PaneRecord {
        let pane: GhosttyTerminalPane
        var command: String
        var workingDirectory: String?
        /// Last applied style signature (config + optional canvas sample).
        var styleSignature: String
        /// Live canvas color sampled from output.bin (0xRRGGBB).
        var canvasSample: UInt32?
    }

    private var panes: [String: PaneRecord] = [:]
    private var deferredEvictions = DeferredSurfaceEvictions<PaneRecord>()

    /// Session ids in display order, most recent last (LRU bookkeeping).
    private var lastShown: [String] = []

    /// nonisolated so deinit can release it; only touched on the main queue
    /// while the cache is alive (FSEvents callback is main-queue too).
    nonisolated(unsafe) private var fsEventStream: FSEventStreamRef?
    private var watchedPaths: [String] = []
    private var themeReloadScheduled = false
    /// nonisolated so deinit can invalidate; only touched on the main queue.
    nonisolated(unsafe) private var canvasSampleTimer: Timer?

    init() {
        rebuildThemeWatcher()
    }

    deinit {
        if let stream = fsEventStream {
            FSEventStreamStop(stream)
            FSEventStreamInvalidate(stream)
            FSEventStreamRelease(stream)
        }
        canvasSampleTimer?.invalidate()
    }

    /// Returns the retained pane for a session, creating it on first use.
    func pane(for session: SessionEntry, workingDirectory: String?) -> GhosttyTerminalPane {
        restoreDeferredPane(session.id)
        if let existing = panes[session.id] {
            // Working directory / command can change across restart; keep
            // the record current so theme reloads resolve against the right
            // paths.
            if existing.command != session.command
                || existing.workingDirectory != workingDirectory {
                panes[session.id]?.command = session.command
                panes[session.id]?.workingDirectory = workingDirectory
                rebuildThemeWatcher()
                updateCanvasSampleTimer()
            }
            return existing.pane
        }
        let sample = TerminalFrameStyle.usesProviderTheme(command: session.command)
            ? ProviderCanvasSampler.dominantBackground(sessionID: session.id)
            : nil
        let frameStyle = TerminalFrameStyle.resolved(
            command: session.command,
            workingDirectory: workingDirectory,
            canvasOverride: sample
        )
        let pane = GhosttyTerminalPane(
            command: LaunchConfig.attachCommand(sessionID: session.id),
            workingDirectory: workingDirectory,
            style: frameStyle.paneStyle
        )
        panes[session.id] = PaneRecord(
            pane: pane,
            command: session.command,
            workingDirectory: workingDirectory,
            styleSignature: styleSignature(
                background: frameStyle.background,
                canvasSample: sample
            ),
            canvasSample: sample
        )
        rebuildThemeWatcher()
        updateCanvasSampleTimer()
        return pane
    }

    func existingPane(for sessionID: String) -> GhosttyTerminalPane? {
        panes[sessionID]?.pane
    }

    /// Live canvas sample for a session, if known. Used by the titlebar path
    /// so chrome matches the retained pane without re-reading output.bin.
    func canvasSample(for sessionID: String) -> UInt32? {
        panes[sessionID]?.canvasSample
    }

    /// Resolve the current frame style for a session (config + live sample).
    func frameStyle(for session: SessionEntry, workingDirectory: String?) -> TerminalFrameStyle {
        let sample = panes[session.id]?.canvasSample
            ?? (TerminalFrameStyle.usesProviderTheme(command: session.command)
                ? ProviderCanvasSampler.dominantBackground(sessionID: session.id)
                : nil)
        return TerminalFrameStyle.resolved(
            command: session.command,
            workingDirectory: workingDirectory,
            canvasOverride: sample
        )
    }

    /// Record that a pane was actually displayed (not just pre-warmed); the
    /// LRU keeps the most recently shown ones alive.
    func noteShown(_ sessionID: String) {
        restoreDeferredPane(sessionID)
        lastShown.removeAll { $0 == sessionID }
        lastShown.append(sessionID)
    }

    /// Drops panes whose sessions no longer exist (or exited), then trims
    /// the remainder to: the selected pane + pre-warmed panes + the most
    /// recently shown, up to `retainedPaneLimit`.
    func prune(keeping liveIDs: Set<String>, selectedID: String?, prewarmedIDs: [String]) {
        var protected = Set(prewarmedIDs)
        if let selectedID { protected.insert(selectedID) }
        // A pane can become selected/prewarmed during its stagger window.
        // Reclaim it before examining active entries so queued stale work
        // cannot tear down a pane the UI wants again.
        for id in protected {
            restoreDeferredPane(id)
        }

        var dropped = false
        for id in Array(panes.keys) where !liveIDs.contains(id) && !protected.contains(id) {
            drop(id)
            dropped = true
        }

        var keep = protected
        for id in lastShown.reversed() {
            if keep.count >= Self.retainedPaneLimit { break }
            keep.insert(id)
        }
        for id in Array(panes.keys) where !keep.contains(id) {
            drop(id)
            dropped = true
        }
        if dropped {
            rebuildThemeWatcher()
            updateCanvasSampleTimer()
        }
    }

    /// Outstanding deferred teardowns (spacing factor for the next one).
    private var pendingTeardowns = 0

    /// Remove a pane from the active cache immediately, then detach/free its
    /// Ghostty EXEC surface on a staggered main-loop turn. `prune` runs inside
    /// SwiftUI's publish/layout path, where synchronous surface destruction
    /// previously froze the app. Until the queued turn starts, `pane(for:)`
    /// can reclaim this exact record, preventing a second `unpeel-attach`
    /// child from being spawned for the same session.
    private func drop(_ sessionID: String) {
        guard let record = panes.removeValue(forKey: sessionID) else { return }
        lastShown.removeAll { $0 == sessionID }
        let token = deferredEvictions.schedule(record, for: sessionID)

        pendingTeardowns += 1
        let delay = 0.1 * Double(pendingTeardowns)
        let pane = record.pane
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
            guard let self else {
                // The cache died while this main-loop turn was pending. The
                // closure still owns the pane, so explicitly release its EXEC
                // surface before that final strong reference disappears.
                pane.tearDown()
                pane.removeFromSuperview()
                return
            }
            self.pendingTeardowns = max(0, self.pendingTeardowns - 1)
            guard let record = self.deferredEvictions.take(
                sessionID, token: token
            ) else {
                // Reclaimed or superseded: this queued eviction is stale.
                return
            }
            // This whole block is one main-actor turn: a replacement cannot
            // be created between claiming the record and detaching it.
            record.pane.tearDown()
            record.pane.removeFromSuperview()
        }
    }

    /// Cancel a not-yet-started deferred eviction and put the same live pane
    /// back in the active cache. The already queued closure is token-gated
    /// and becomes a harmless no-op when its turn arrives.
    private func restoreDeferredPane(_ sessionID: String) {
        guard let record = deferredEvictions.reclaim(sessionID) else { return }
        panes[sessionID] = record
        rebuildThemeWatcher()
        updateCanvasSampleTimer()
    }

    // MARK: - Live provider theme reload

    /// Re-resolve OpenCode/Grok frame styles for every cached pane (config
    /// files + optional live canvas sample) and push color changes into
    /// Ghostty. Bumps `themeRevision` when anything actually changed.
    @discardableResult
    func reloadProviderThemes(forceRevisionBump: Bool = false) -> Bool {
        var anyChanged = false
        for (id, record) in panes {
            guard TerminalFrameStyle.usesProviderTheme(command: record.command) else {
                continue
            }
            let sample = ProviderCanvasSampler.dominantBackground(sessionID: id)
            let frameStyle = TerminalFrameStyle.resolved(
                command: record.command,
                workingDirectory: record.workingDirectory,
                canvasOverride: sample
            )
            let signature = styleSignature(
                background: frameStyle.background,
                canvasSample: sample
            )
            if sample != record.canvasSample {
                panes[id]?.canvasSample = sample
            }
            guard signature != record.styleSignature else { continue }
            record.pane.applyPaneStyle(frameStyle.paneStyle)
            panes[id]?.styleSignature = signature
            anyChanged = true
        }
        if anyChanged || forceRevisionBump {
            themeRevision &+= 1
        }
        return anyChanged
    }

    private func styleSignature(
        background: TerminalFrameStyle.Background?,
        canvasSample: UInt32?
    ) -> String {
        let config = background?.signature ?? "default"
        let sample = canvasSample.map { String(format: "%06X", $0) } ?? "-"
        return "\(config)|\(sample)"
    }

    private func scheduleThemeReload() {
        guard !themeReloadScheduled else { return }
        themeReloadScheduled = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) { [weak self] in
            guard let self else { return }
            self.themeReloadScheduled = false
            // Config file changed — always bump so titlebar re-reads even
            // before the TUI has repainted enough for a canvas sample.
            self.reloadProviderThemes(forceRevisionBump: true)
        }
    }

    // MARK: - Canvas sampling (in-TUI theme changes)

    private func updateCanvasSampleTimer() {
        let needs = panes.values.contains {
            TerminalFrameStyle.usesProviderTheme(command: $0.command)
        }
        if needs {
            guard canvasSampleTimer == nil else { return }
            // ~3×/sec is plenty for a theme picker; cheap (96KB file tail).
            let timer = Timer(timeInterval: 0.35, repeats: true) { [weak self] _ in
                MainActor.assumeIsolated {
                    // Don't force a revision bump on a quiet poll — only when
                    // the sampled canvas (or config) actually moved.
                    _ = self?.reloadProviderThemes(forceRevisionBump: false)
                }
            }
            RunLoop.main.add(timer, forMode: .common)
            canvasSampleTimer = timer
            // Immediate pass so the first paint after open catches up.
            _ = reloadProviderThemes(forceRevisionBump: false)
        } else {
            canvasSampleTimer?.invalidate()
            canvasSampleTimer = nil
        }
    }

    // MARK: - Config FSEvents

    private func rebuildThemeWatcher() {
        let workingDirectories = panes.values.compactMap { record -> String? in
            guard TerminalFrameStyle.usesProviderTheme(command: record.command) else {
                return nil
            }
            return record.workingDirectory
        }
        let paths = ProviderThemeWatchPaths.roots(workingDirectories: workingDirectories)
        guard paths != watchedPaths || fsEventStream == nil else { return }
        watchedPaths = paths
        teardownThemeWatcher()
        guard !paths.isEmpty else { return }

        var context = FSEventStreamContext()
        context.info = Unmanaged.passUnretained(self).toOpaque()
        let callback: FSEventStreamCallback = { _, info, numEvents, eventPaths, _, _ in
            guard let info else { return }
            let cache = Unmanaged<SurfaceCache>.fromOpaque(info).takeUnretainedValue()
            // UseCFTypes → CFArray of CFString paths.
            let cfPaths = Unmanaged<CFArray>.fromOpaque(eventPaths).takeUnretainedValue()
            let paths = (cfPaths as NSArray) as? [String] ?? []
            let relevant = paths.isEmpty
                || paths.contains { ProviderThemeWatchPaths.isRelevantChange($0) }
            guard relevant else { return }
            MainActor.assumeIsolated {
                cache.scheduleThemeReload()
            }
        }
        let flags = FSEventStreamCreateFlags(
            kFSEventStreamCreateFlagFileEvents
                | kFSEventStreamCreateFlagUseCFTypes
        )
        guard let stream = FSEventStreamCreate(
            nil,
            callback,
            &context,
            paths as CFArray,
            FSEventStreamEventId(kFSEventStreamEventIdSinceNow),
            0.2,
            flags
        ) else {
            NSLog("[UnpeelNative] provider theme FSEvents stream creation failed")
            return
        }
        FSEventStreamSetDispatchQueue(stream, .main)
        FSEventStreamStart(stream)
        fsEventStream = stream
    }

    private func teardownThemeWatcher() {
        guard let stream = fsEventStream else { return }
        FSEventStreamStop(stream)
        FSEventStreamInvalidate(stream)
        FSEventStreamRelease(stream)
        fsEventStream = nil
    }
}
