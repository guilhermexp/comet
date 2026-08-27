//
//  RemoteHostWorkspaceView.swift
//  UnpeelNative
//
//  Remote-scope terminal hosting and connection presentation. The old
//  parallel remote sidebar/content hierarchy is gone (design decision
//  2026-08-13: remote scope is the SAME UI) — the normal SidebarView and
//  ContentArea render remote state through UnpeelStore's display projection.
//  What remains here is the transport-level seam those shared views mount:
//  the runtime-owned in-memory Ghostty pane host, the connection banners,
//  and the connecting/empty states.
//

import AppKit
import SwiftUI
import UnpeelShared

// MARK: - Remote terminal mount (used by ContentArea's workspace pane)

/// Hosts the runtime's in-memory VT pane for the selected remote Session
/// inside the normal content chrome. This is the ONLY remote/local branch in
/// the content area, and it is a byte-transport choice: the surrounding
/// titlebar, banners, and empty states are the shared ones.
struct RemoteScopeTerminalMount: View {
    @ObservedObject var store: UnpeelStore
    let session: SessionEntry
    let backgroundColor: NSColor

    @ObservedObject private var runtime: RemoteHostRuntime

    init(store: UnpeelStore, session: SessionEntry, backgroundColor: NSColor) {
        self.store = store
        self.session = session
        self.backgroundColor = backgroundColor
        _runtime = ObservedObject(wrappedValue: store.remoteHostRuntime)
    }

    var body: some View {
        if let pane = runtime.terminalPane(
            for: session.id,
            style: terminalPaneStyle
        ) {
            RemoteTerminalPaneHostView(
                pane: pane,
                backgroundColor: backgroundColor
            )
        } else {
            RemoteTerminalPreparingView(
                sessionTitle: session.label,
                state: runtime.connectionState
            )
        }
    }

    /// The Host resolves provider-specific terminal chrome; apply that value
    /// directly rather than reading provider configuration on the Controller.
    private var terminalPaneStyle: TerminalPaneStyle {
        var style = TerminalPaneStyle.resolved()
        guard let color = store.remoteTerminalBackgroundColor(for: session.id) else {
            return style
        }
        let rgb = color.usingColorSpace(.sRGB) ?? color
        let value = String(
            format: "#%02X%02X%02X",
            Int(rgb.redComponent * 255),
            Int(rgb.greenComponent * 255),
            Int(rgb.blueComponent * 255)
        )
        style.light.background = value
        style.dark.background = value
        return style
    }
}

/// Sidebar empty-state while a remote Host has no projects yet (connecting,
/// offline, or a genuinely empty Host). The local counterpart offers Add
/// Project — a Controller-local verb — so this presents connection state
/// instead.
struct RemoteScopeEmptySidebarView: View {
    let hostName: String
    let state: RemoteHostConnectionState

    private var presentation: RemoteConnectionPresentation {
        .init(state: state, hasSnapshot: false)
    }

    var body: some View {
        VStack(spacing: 11) {
            if presentation.showsProgress {
                ProgressView()
                    .controlSize(.small)
            } else {
                Image(systemName: emptyStateIcon)
                    .font(.system(size: 24, weight: .light))
                    .foregroundStyle(presentation.tint)
            }
            Text(presentation.shortLabel == "Connected" ? hostName : presentation.shortLabel)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Theme.foreground)
            Text(presentation.message ?? "Loading Sessions from \(hostName)…")
                .font(.system(size: 11))
                .foregroundStyle(Theme.mutedForeground)
                .multilineTextAlignment(.center)
                .lineLimit(4)
        }
        .padding(.horizontal, 20)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var emptyStateIcon: String {
        switch state {
        case .repairRequired: return "exclamationmark.shield"
        case .incompatible: return "exclamationmark.triangle"
        case .failed: return "wifi.slash"
        default: return "server.rack"
        }
    }
}

// MARK: - AppKit pane host

/// The only AppKit bridge in the remote workspace. It can attach only the
/// runtime-owned in-memory pane type; there is no command, working directory,
/// attach binary, Local SurfaceCache, or filesystem callback in this path.
struct RemoteTerminalPaneHostView: NSViewRepresentable {
    let pane: RemoteGhosttyTerminalPane
    let backgroundColor: NSColor

    @MainActor
    final class SwapContainer: NSView {
        private(set) weak var attachedPane: RemoteGhosttyTerminalPane?
        var backgroundColor = Theme.terminalBackgroundNSColor {
            didSet { needsDisplay = true }
        }

        override var isOpaque: Bool { true }
        override var wantsUpdateLayer: Bool { true }

        override func updateLayer() {
            layer?.backgroundColor = backgroundColor.cgColor
        }

        override func layout() {
            super.layout()
            attachedPane?.frame = bounds
        }

        @discardableResult
        func attach(_ pane: RemoteGhosttyTerminalPane) -> Bool {
            guard attachedPane !== pane else { return false }
            detachPane()
            pane.removeFromSuperview()
            pane.translatesAutoresizingMaskIntoConstraints = true
            pane.autoresizingMask = [.width, .height]
            pane.frame = bounds
            addSubview(pane)
            attachedPane = pane
            pane.setPresentationEnabled(true)
            pane.frame = bounds
            return true
        }

        func detachPane() {
            guard let pane = attachedPane else { return }
            attachedPane = nil
            pane.setPresentationEnabled(false)
            pane.removeFromSuperview()
        }
    }

    func makeNSView(context _: Context) -> SwapContainer {
        let container = SwapContainer()
        container.wantsLayer = true
        container.backgroundColor = backgroundColor
        return container
    }

    func updateNSView(_ container: SwapContainer, context _: Context) {
        container.backgroundColor = backgroundColor
        container.layer?.backgroundColor = backgroundColor.cgColor
        guard container.attach(pane) else { return }

        if container.window != nil {
            pane.focus()
            pane.renderNow()
        }

        DispatchQueue.main.async { [weak container, weak pane] in
            guard let container, let pane, container.attachedPane === pane else { return }
            pane.focus()
            pane.refitNow()
        }
    }

    static func dismantleNSView(_ container: SwapContainer, coordinator _: ()) {
        container.detachPane()
    }
}

// MARK: - Connection presentation (shared with ContentArea banners)

struct RemoteConnectionPresentation {
    struct Banner {
        let icon: String
        let message: String
        let tint: Color
        let showsProgress: Bool
    }

    let shortLabel: String
    let message: String?
    let tint: Color
    let showsProgress: Bool
    let isStale: Bool
    let contentBanner: Banner?

    init(
        state: RemoteHostConnectionState,
        hasSnapshot: Bool,
        route: RemoteHostConnectionRoute? = nil
    ) {
        switch state {
        case .idle:
            shortLabel = "Disconnected"
            message = "This Host is not connected."
            tint = Theme.mutedForeground
            showsProgress = false
            isStale = hasSnapshot
            contentBanner = hasSnapshot
                ? Banner(
                    icon: "wifi.slash",
                    message: "Disconnected — showing the last known Host state.",
                    tint: Theme.mutedForeground,
                    showsProgress: false
                )
                : nil
        case .connecting:
            shortLabel = "Connecting…"
            message = "Connecting to this Host."
            tint = Theme.accent
            showsProgress = true
            isStale = hasSnapshot
            contentBanner = hasSnapshot
                ? Banner(
                    icon: "arrow.clockwise",
                    message: "Connecting — showing the last known Host state.",
                    tint: Theme.accent,
                    showsProgress: true
                )
                : nil
        case .connected:
            shortLabel = route?.shortLabel ?? "Connected"
            message = nil
            tint = Theme.accent
            showsProgress = false
            isStale = false
            contentBanner = nil
        case let .reconnecting(message):
            shortLabel = "Reconnecting…"
            self.message = message
            tint = Theme.attention
            showsProgress = true
            isStale = hasSnapshot
            contentBanner = Banner(
                icon: "arrow.clockwise",
                message: "Connection interrupted — reconnecting while the last known state stays visible.",
                tint: Theme.attention,
                showsProgress: true
            )
        case let .repairRequired(message):
            shortLabel = "Pair again"
            self.message = message
            tint = Theme.danger
            showsProgress = false
            isStale = hasSnapshot
            contentBanner = Banner(
                icon: "exclamationmark.shield",
                message: "This pairing is no longer valid. Pair the Host again before sending input.",
                tint: Theme.danger,
                showsProgress: false
            )
        case let .incompatible(message):
            shortLabel = "Update required"
            self.message = message
            tint = Theme.danger
            showsProgress = false
            isStale = hasSnapshot
            contentBanner = Banner(
                icon: "exclamationmark.triangle",
                message: "This Host uses an incompatible protocol. Update Unpeel on both devices.",
                tint: Theme.danger,
                showsProgress: false
            )
        case let .failed(message):
            shortLabel = "Offline"
            self.message = message
            tint = Theme.danger
            showsProgress = false
            isStale = hasSnapshot
            contentBanner = hasSnapshot
                ? Banner(
                    icon: "wifi.slash",
                    message: "Host offline — showing the last known state.",
                    tint: Theme.danger,
                    showsProgress: false
                )
                : nil
        }
    }
}

struct RemoteHostConnectionBanner: View {
    let banner: RemoteConnectionPresentation.Banner

    var body: some View {
        HStack(spacing: 8) {
            if banner.showsProgress {
                ProgressView()
                    .controlSize(.small)
                    .tint(banner.tint)
            } else {
                Image(systemName: banner.icon)
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(banner.tint)
            }
            Text(banner.message)
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(Theme.mutedForeground)
                .lineLimit(2)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .frame(minHeight: 34)
        .background(banner.tint.opacity(0.07))
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(Theme.resizerLine)
                .frame(height: 1)
        }
    }
}

struct RemoteTerminalPreparingView: View {
    let sessionTitle: String
    let state: RemoteHostConnectionState

    var body: some View {
        VStack(spacing: 10) {
            if case .repairRequired = state {
                Image(systemName: "exclamationmark.shield")
                    .font(.system(size: 25, weight: .light))
                    .foregroundStyle(Theme.danger)
            } else if case .incompatible = state {
                Image(systemName: "exclamationmark.triangle")
                    .font(.system(size: 25, weight: .light))
                    .foregroundStyle(Theme.danger)
            } else {
                ProgressView()
                    .controlSize(.small)
            }
            Text(sessionTitle.isEmpty ? "Preparing terminal…" : sessionTitle)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(Theme.mutedForeground)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
