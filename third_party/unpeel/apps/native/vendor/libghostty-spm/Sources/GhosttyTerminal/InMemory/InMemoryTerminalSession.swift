//
//  InMemoryTerminalSession.swift
//  libghostty-spm
//
//  Created by Lakr233 on 2026/3/16.
//

import Foundation
import GhosttyKit

public final class InMemoryTerminalSession: @unchecked Sendable {
    /// Bytes received while no surface is attached are queued (instead of
    /// silently dropped) and flushed on attach, so hosts can start feeding
    /// before the surface exists. Bounded; oldest bytes drop on overflow.
    private static let maximumPendingBytes = 8 * 1024 * 1024

    private let lock = NSLock()
    private var surface: ghostty_surface_t?
    private var lastResize: InMemoryTerminalViewport?
    private var pendingReceiveBuffer = Data()
    private let writeHandler: @Sendable (Data) -> Void
    private let resizeHandler: @Sendable (InMemoryTerminalViewport) -> Void

    public init(
        write: @escaping @Sendable (Data) -> Void,
        resize: @escaping @Sendable (InMemoryTerminalViewport) -> Void
    ) {
        writeHandler = write
        resizeHandler = resize
    }

    // MARK: - Surface Lifecycle

    func setSurface(_ surface: ghostty_surface_t?) {
        lock.lock()
        self.surface = surface
        TerminalDebugLog.log(
            .lifecycle,
            "in-memory session surface=\(surface == nil ? "nil" : "set")"
        )
        var notify: (@Sendable () -> Void)?
        if surface != nil {
            flushPendingReceiveBufferLocked()
            // Arm the render pump for the attach itself: a freshly attached
            // (or re-attached, e.g. cache remount) surface must present its
            // replayed content without waiting for the next byte.
            notify = hostBytesHandler
        }
        lock.unlock()
        notify?()
    }

    func clearSurface(ifMatches expectedSurface: ghostty_surface_t?) {
        lock.lock()
        defer { lock.unlock() }

        guard surface == expectedSurface else {
            TerminalDebugLog.log(
                .lifecycle,
                "in-memory session clear skipped expected=\(expectedSurface == nil ? "nil" : "set") current=\(surface == nil ? "nil" : "set")"
            )
            return
        }

        surface = nil
        TerminalDebugLog.log(.lifecycle, "in-memory session surface=nil matched")
    }

    var currentSurface: ghostty_surface_t? {
        lock.lock()
        defer { lock.unlock() }
        return surface
    }

    // MARK: - Viewport Read

    /// Returns the active viewport as a UTF-8 string, or `nil` if no surface
    /// is attached. Lines are separated by `\n`. The `ghostty_text_s`
    /// lifecycle (allocate via `ghostty_surface_read_text`, free via
    /// `ghostty_surface_free_text`) is fully encapsulated — callers never
    /// touch the C buffer.
    ///
    /// Selection grammar: `(VIEWPORT, TOP_LEFT)` to `(VIEWPORT, BOTTOM_RIGHT)`
    /// with `rectangle: false` (linear flow). This reads exactly the visible
    /// rows and ignores scrollback. Empty viewports return an empty string.
    ///
    /// Thread-safe: acquires the same `NSLock` as `receive(_:)` and
    /// `setSurface(_:)`, preventing reads against a surface mid-replacement.
    public func readViewportText() -> String? {
        lock.lock()
        defer { lock.unlock() }
        guard let surface else { return nil }

        let topLeft = ghostty_point_s(
            tag: GHOSTTY_POINT_VIEWPORT,
            coord: GHOSTTY_POINT_COORD_TOP_LEFT,
            x: 0,
            y: 0
        )
        let bottomRight = ghostty_point_s(
            tag: GHOSTTY_POINT_VIEWPORT,
            coord: GHOSTTY_POINT_COORD_BOTTOM_RIGHT,
            x: 0,
            y: 0
        )
        let selection = ghostty_selection_s(
            top_left: topLeft,
            bottom_right: bottomRight,
            rectangle: false
        )

        var out = ghostty_text_s()
        guard ghostty_surface_read_text(surface, selection, &out) else {
            return nil
        }
        defer { ghostty_surface_free_text(surface, &out) }

        guard let textPtr = out.text, out.text_len > 0 else {
            return ""
        }
        let bytes = UnsafeBufferPointer(start: textPtr, count: Int(out.text_len))
            .map { UInt8(bitPattern: $0) }
        return String(decoding: bytes, as: UTF8.self)
    }

    func updateViewport(_ size: TerminalGridMetrics) {
        TerminalDebugLog.log(.metrics, "in-memory viewport update \(size.debugSummary)")
        dispatchResize(InMemoryTerminalViewport(
            columns: size.columns,
            rows: size.rows,
            widthPixels: size.widthPixels,
            heightPixels: size.heightPixels,
            cellWidthPixels: size.cellWidthPixels,
            cellHeightPixels: size.cellHeightPixels
        ))
    }

    // MARK: - Receiving Data

    /// Fired after host bytes are written into the surface. The surface
    /// coordinator wires this to its render pump: the embedded core
    /// coalesces render wakeups under IO load, so a purely host-fed surface
    /// (no PTY, no local input) can hold freshly parsed terminal state on a
    /// stale frame — blank or partial regions until a resize forces a draw.
    /// Arming the pump on every host write makes remote-fed output present
    /// like local IO. Invoked outside the session lock.
    var onHostBytes: (@Sendable () -> Void)? {
        get {
            lock.lock()
            defer { lock.unlock() }
            return hostBytesHandler
        }
        set {
            lock.lock()
            defer { lock.unlock() }
            hostBytesHandler = newValue
        }
    }

    private var hostBytesHandler: (@Sendable () -> Void)?

    /// Test seam: replaces the raw `ghostty_surface_write_buffer` call so
    /// unit tests can observe receive/flush/notify ordering without a live
    /// ghostty surface (which needs Metal and can't init headless).
    var writeBufferOverride: ((Data) -> Void)?

    /// Feed data into the terminal from the host backend. While no surface
    /// is attached the bytes are buffered (bounded) and flushed on attach.
    public func receive(_ data: Data) {
        lock.lock()
        guard let surface else {
            TerminalDebugLog.log(
                .output,
                "terminal <- host buffered \(TerminalDebugLog.describe(data))"
            )
            pendingReceiveBuffer.append(data)
            if pendingReceiveBuffer.count > Self.maximumPendingBytes {
                pendingReceiveBuffer.removeFirst(
                    pendingReceiveBuffer.count - Self.maximumPendingBytes
                )
            }
            lock.unlock()
            return
        }

        TerminalDebugLog.log(
            .output,
            "terminal <- host \(TerminalDebugLog.describe(data))"
        )

        if let writeBufferOverride {
            writeBufferOverride(data)
        } else {
            Self.writeBuffer(data, to: surface)
        }
        let notify = hostBytesHandler
        lock.unlock()
        notify?()
    }

    /// Caller must hold `lock`.
    private func flushPendingReceiveBufferLocked() {
        guard let surface, !pendingReceiveBuffer.isEmpty else { return }
        let buffered = pendingReceiveBuffer
        pendingReceiveBuffer = Data()
        TerminalDebugLog.log(
            .output,
            "terminal <- host flushed \(buffered.count) buffered bytes"
        )
        if let writeBufferOverride {
            writeBufferOverride(buffered)
        } else {
            Self.writeBuffer(buffered, to: surface)
        }
    }

    private static func writeBuffer(_ data: Data, to surface: ghostty_surface_t) {
        data.withUnsafeBytes { buffer in
            guard let ptr = buffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return
            }
            ghostty_surface_write_buffer(surface, ptr, UInt(buffer.count))
        }
    }

    /// Feed a UTF-8 string into the terminal from the host backend.
    public func receive(_ string: String) {
        guard let data = string.data(using: .utf8) else { return }
        receive(data)
    }

    /// Inject input bytes directly into the host-side consumer.
    ///
    /// This bypasses `ghostty_surface_key` translation and is intended for
    /// control sequences that the in-memory backend must interpret itself.
    public func sendInput(_ data: Data) {
        TerminalDebugLog.log(
            .input,
            "host <- direct input \(TerminalDebugLog.describe(data))"
        )
        writeHandler(data)
    }

    // MARK: - Process Exit

    /// Signal that the host-managed process has exited.
    public func finish(exitCode: UInt32, runtimeMilliseconds: UInt64) {
        lock.lock()
        defer { lock.unlock() }
        guard let surface else {
            TerminalDebugLog.log(
                .lifecycle,
                "process exit ignored: missing surface exitCode=\(exitCode) runtimeMs=\(runtimeMilliseconds)"
            )
            return
        }

        TerminalDebugLog.log(
            .lifecycle,
            "process exit exitCode=\(exitCode) runtimeMs=\(runtimeMilliseconds)"
        )
        ghostty_surface_process_exit(surface, exitCode, runtimeMilliseconds)
    }

    // MARK: - C Callbacks

    static let receiveBufferCallback: ghostty_surface_receive_buffer_cb = { userdata, ptr, len in
        guard let userdata, let ptr else { return }
        let session = Unmanaged<InMemoryTerminalSession>
            .fromOpaque(userdata)
            .takeUnretainedValue()
        let data = Data(bytes: ptr, count: len)
        TerminalDebugLog.log(
            .input,
            "host <- terminal \(TerminalDebugLog.describe(data))"
        )
        session.writeHandler(data)
    }

    static let receiveResizeCallback: ghostty_surface_receive_resize_cb = { userdata, cols, rows, widthPx, heightPx in
        guard let userdata else { return }
        let session = Unmanaged<InMemoryTerminalSession>
            .fromOpaque(userdata)
            .takeUnretainedValue()
        TerminalDebugLog.log(
            .metrics,
            "receive resize cols=\(cols) rows=\(rows) pixels=\(widthPx)x\(heightPx)"
        )
        session.dispatchResize(InMemoryTerminalViewport(
            columns: cols,
            rows: rows,
            widthPixels: widthPx,
            heightPixels: heightPx
        ))
    }

    private func dispatchResize(_ resize: InMemoryTerminalViewport) {
        lock.lock()
        let mergedResize = mergedResize(resize)
        guard mergedResize != lastResize else {
            lock.unlock()
            TerminalDebugLog.log(
                .metrics,
                "resize unchanged cols=\(mergedResize.columns) rows=\(mergedResize.rows) pixels=\(mergedResize.widthPixels)x\(mergedResize.heightPixels) cell=\(mergedResize.cellWidthPixels)x\(mergedResize.cellHeightPixels)"
            )
            return
        }
        lastResize = mergedResize
        lock.unlock()

        TerminalDebugLog.log(
            .metrics,
            "resize dispatched cols=\(mergedResize.columns) rows=\(mergedResize.rows) pixels=\(mergedResize.widthPixels)x\(mergedResize.heightPixels) cell=\(mergedResize.cellWidthPixels)x\(mergedResize.cellHeightPixels)"
        )
        resizeHandler(mergedResize)
    }

    private func mergedResize(_ resize: InMemoryTerminalViewport) -> InMemoryTerminalViewport {
        guard let lastResize else { return resize }

        return InMemoryTerminalViewport(
            columns: resize.columns,
            rows: resize.rows,
            widthPixels: resize.widthPixels == 0 ? lastResize.widthPixels : resize.widthPixels,
            heightPixels: resize.heightPixels == 0 ? lastResize.heightPixels : resize.heightPixels,
            cellWidthPixels: resize.cellWidthPixels == 0 ? lastResize.cellWidthPixels : resize.cellWidthPixels,
            cellHeightPixels: resize.cellHeightPixels == 0 ? lastResize.cellHeightPixels : resize.cellHeightPixels
        )
    }
}
