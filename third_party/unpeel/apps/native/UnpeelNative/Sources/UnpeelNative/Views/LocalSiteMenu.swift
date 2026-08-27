//
//  LocalSiteMenu.swift
//  UnpeelNative
//
//  Titlebar chip for the local site(s) the shown session's project serves
//  (host-probed live loopback URLs from `detected_local_urls`). Icon-only
//  by default: a single URL opens directly from the globe (tooltip carries
//  the URL); multiple add a chevron with a right-anchored AppKit dropdown
//  listing the full URLs (same hand-positioned NSMenu approach as
//  WorkspaceOpenMenu — SwiftUI's Menu popup cannot be right-aligned to a
//  trailing button).
//

import AppKit
import SwiftUI

struct LocalSiteMenu: View {
    let urls: [String]

    @State private var controller = LocalSiteMenuController()

    var body: some View {
        LocalSiteButton(
            help: urls.count == 1
                ? "Open \(urls.first ?? "") in browser"
                : "Local sites (\(urls.count))",
            showsChevron: true,
            onOpenFirst: {
                if urls.count == 1 {
                    Self.open(urls.first ?? "")
                } else {
                    controller.urls = urls
                    controller.present()
                }
            },
            onShowMenu: {
                controller.urls = urls
                controller.present()
            },
            controller: controller
        )
        .fixedSize()
    }

    static func open(_ url: String) {
        guard let parsed = URL(string: url),
              parsed.scheme == "http" || parsed.scheme == "https"
        else { return }
        NSWorkspace.shared.open(parsed)
    }
}

@MainActor
final class LocalSiteMenuController: NSObject {
    var urls: [String] = []
    weak var anchorView: NSView?

    /// Resolve each URL's serving process off-main (lsof + ancestry walk,
    /// ~100ms, only on dropdown open — never on a timer), then present:
    /// open rows for every URL, and Stop rows for the session-owned servers.
    func present() {
        let urls = self.urls
        DispatchQueue.global(qos: .userInitiated).async {
            let servers: [(url: String, label: String)] = urls.compactMap { url in
                resolveServer(url).map { (url, $0) }
            }
            DispatchQueue.main.async { [weak self] in
                self?.presentMenu(urls: urls, stoppable: servers)
            }
        }
    }

    private func presentMenu(urls: [String], stoppable: [(url: String, label: String)]) {
        guard let anchorView else { return }

        let menu = NSMenu()
        menu.autoenablesItems = false
        for url in urls {
            let item = NSMenuItem(
                title: url,
                action: #selector(handleSelection(_:)),
                keyEquivalent: ""
            )
            item.target = self
            item.representedObject = url
            item.image = NSImage(
                systemSymbolName: "globe",
                accessibilityDescription: nil
            )
            menu.addItem(item)
        }
        if !stoppable.isEmpty {
            menu.addItem(.separator())
            for server in stoppable {
                let item = NSMenuItem(
                    title: "Stop \(server.label)",
                    action: #selector(handleStop(_:)),
                    keyEquivalent: ""
                )
                item.target = self
                item.representedObject = server.url
                item.image = NSImage(
                    systemSymbolName: "stop.circle",
                    accessibilityDescription: nil
                )
                menu.addItem(item)
            }
        }

        let origin = NSPoint(x: anchorView.bounds.maxX - menu.size.width, y: -4)
        menu.popUp(positioning: nil, at: origin, in: anchorView)
    }

    @objc private func handleSelection(_ sender: NSMenuItem) {
        guard let url = sender.representedObject as? String else { return }
        LocalSiteMenu.open(url)
    }

    @objc private func handleStop(_ sender: NSMenuItem) {
        guard let url = sender.representedObject as? String else { return }
        DispatchQueue.global(qos: .userInitiated).async {
            let stopped = runHostVerb(["__stop_local_site_server__", url]) != nil
            DispatchQueue.main.async {
                ToastCenter.shared.show(
                    stopped ? "Server stopped" : "Couldn't stop server",
                    systemImage: stopped ? "stop.circle" : "exclamationmark.triangle"
                )
            }
        }
    }
}

/// "vite-style" menu label for a session-owned server: `localhost:5173
/// (node)`. Nil when nothing listens or the server is not session-owned —
/// Unpeel never offers to kill infrastructure it didn't start.
private func resolveServer(_ url: String) -> String? {
    guard let line = runHostVerb(["__local_site_server__", url]) else { return nil }
    let parts = line.split(separator: "\t").map(String.init)
    guard parts.count == 3, parts[2] != "-" else { return nil }
    let compact = url
        .replacingOccurrences(of: "https://", with: "")
        .replacingOccurrences(of: "http://", with: "")
        .prefix(while: { $0 != "/" })
    return "\(compact) (\(parts[1]))"
}

/// Run an `unpeel-host` verb and return trimmed stdout on success.
private func runHostVerb(_ arguments: [String]) -> String? {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: LaunchConfig.hostBinary)
    process.arguments = arguments
    let pipe = Pipe()
    process.standardOutput = pipe
    process.standardError = FileHandle.nullDevice
    guard (try? process.run()) != nil else { return nil }
    let data = pipe.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    guard process.terminationStatus == 0 else { return nil }
    return String(data: data, encoding: .utf8)?
        .trimmingCharacters(in: .whitespacesAndNewlines)
}

private struct LocalSiteMenuAnchor: NSViewRepresentable {
    let controller: LocalSiteMenuController

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        controller.anchorView = view
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        controller.anchorView = nsView
    }
}

private struct LocalSiteButton: View {
    let help: String
    let showsChevron: Bool
    let onOpenFirst: () -> Void
    let onShowMenu: () -> Void
    let controller: LocalSiteMenuController

    @State private var leftHovering = false
    @State private var rightHovering = false

    var body: some View {
        styledControl
            .animation(.easeInOut(duration: 0.12), value: leftHovering)
            .animation(.easeInOut(duration: 0.12), value: rightHovering)
    }

    @ViewBuilder
    private var styledControl: some View {
        if #available(macOS 26.0, *) {
            control
                .glassEffect(.regular, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        } else {
            control
                .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .strokeBorder(Theme.foreground.opacity(0.08))
                )
        }
    }

    private var control: some View {
        HStack(spacing: 0) {
            Button(action: onOpenFirst) {
                Image(systemName: "globe")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(leftHovering ? Theme.foreground : Theme.mutedForeground)
                    .frame(width: 32, height: 26)
                    .background(leftHovering ? Theme.hoverRow.opacity(0.45) : .clear)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .onHover { leftHovering = $0 }
            .help(help)
            .accessibilityLabel(help)

            if showsChevron {
                divider

                Button(action: onShowMenu) {
                    Image(systemName: "chevron.down")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(rightHovering ? Theme.foreground : Theme.mutedForeground)
                        .frame(width: 25, height: 26)
                        .background(rightHovering ? Theme.hoverRow.opacity(0.45) : .clear)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .background(LocalSiteMenuAnchor(controller: controller))
                .onHover { rightHovering = $0 }
                .help("Choose local site")
                .accessibilityLabel("Choose local site")
            }
        }
        .frame(height: 26)
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        .contentShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private var divider: some View {
        Rectangle()
            .fill(Theme.foreground.opacity(0.10))
            .frame(width: 1, height: 14)
            .allowsHitTesting(false)
    }
}
