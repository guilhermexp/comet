//
//  MCPApprovalPanel.swift
//  UnpeelNative
//
//  The desktop surface for pending MCP approvals: a floating, NON-MODAL
//  panel. Deliberately not NSAlert — `runModal()` inside a main-actor job
//  spins a nested modal run loop that stalls every queued main-actor job
//  (including the mobile server's bootstrap hop, which severed paired-phone
//  connections whenever a prompt appeared with no key window), and a modal
//  session can't be answered-and-dismissed programmatically when a paired
//  controller answers first. The panel renders the head of
//  `UnpeelStore.pendingMcpApprovals` and hides itself when the queue drains,
//  from wherever the answer came.
//
//  `FloatingPromptPanelController` is shared with the computer-permissions
//  nudge (ComputerPermissions.swift), which had the same runModal hazard.
//

import AppKit
import SwiftUI

/// A reusable floating alert-style panel: titled (so it can become key for
/// keyboard shortcuts) but with the title bar hidden, floating level so it
/// stays visible over the app, never modal.
@MainActor
final class FloatingPromptPanelController {
    private var panel: NSPanel?

    /// Show (or update) the panel with the given content. Activates the app
    /// and centers the panel when it was not already visible; a visible
    /// panel keeps its position and just swaps content.
    func show<Content: View>(_ content: Content) {
        let root = AnyView(content)
        if let panel, panel.isVisible {
            (panel.contentViewController as? NSHostingController<AnyView>)?.rootView = root
            return
        }
        let panel = self.panel ?? makePanel()
        self.panel = panel
        panel.contentViewController = NSHostingController(rootView: root)
        // Get the user's eyes on the prompt: the asking agent's tool call is
        // blocked until someone answers (~2 minute timeout).
        NSApp.activate(ignoringOtherApps: true)
        panel.center()
        panel.makeKeyAndOrderFront(nil)
    }

    func dismiss() {
        panel?.orderOut(nil)
        panel?.contentViewController = nil
    }

    private func makePanel() -> NSPanel {
        let panel = NSPanel(
            contentRect: .zero,
            styleMask: [.titled, .fullSizeContentView],
            backing: .buffered,
            defer: true
        )
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        panel.isMovableByWindowBackground = true
        panel.level = .floating
        panel.hidesOnDeactivate = false
        panel.isReleasedWhenClosed = false
        panel.standardWindowButton(.closeButton)?.isHidden = true
        panel.standardWindowButton(.miniaturizeButton)?.isHidden = true
        panel.standardWindowButton(.zoomButton)?.isHidden = true
        return panel
    }
}

/// Alert-style content for the head of the pending-approval queue. Observes
/// the store, so answering (from the panel or a paired controller) advances
/// it to the next prompt automatically.
struct McpApprovalPromptView: View {
    @ObservedObject var store: UnpeelStore

    var body: some View {
        if let approval = store.pendingMcpApprovals.first {
            let message = store.mcpApprovalMessage(approval)
            VStack(spacing: 12) {
                Image(nsImage: NSApp.applicationIconImage)
                    .resizable()
                    .frame(width: 48, height: 48)
                Text(message.title)
                    .font(.system(size: 13, weight: .semibold))
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                Text(message.body)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                if store.pendingMcpApprovals.count > 1 {
                    Text("\(store.pendingMcpApprovals.count - 1) more waiting")
                        .font(.system(size: 10))
                        .foregroundStyle(.tertiary)
                }
                HStack(spacing: 10) {
                    Button("Don't Allow") {
                        store.answerMcpApproval(id: approval.id, approved: false)
                    }
                    .keyboardShortcut(.cancelAction)
                    Button("Allow") {
                        store.answerMcpApproval(id: approval.id, approved: true)
                    }
                    .keyboardShortcut(.defaultAction)
                }
                .padding(.top, 4)
            }
            .padding(20)
            .frame(width: 380)
        }
    }
}

/// Content for the computer-permissions nudge (missing TCC grants after a
/// computer-use approval or a failing action) — one Grant button per missing
/// permission, plus Not Now.
struct ComputerPermissionsNudgeView: View {
    let missing: [String]
    let subject: String
    let onGrant: (String) -> Void
    let onDismiss: () -> Void

    var body: some View {
        VStack(spacing: 12) {
            Image(nsImage: NSApp.applicationIconImage)
                .resizable()
                .frame(width: 48, height: 48)
            Text("Computer use needs macOS permissions")
                .font(.system(size: 13, weight: .semibold))
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
            Text("\(subject) tried to control this Mac, but Unpeel is missing: "
                + missing.joined(separator: ", ")
                + ". Grant them to Unpeel in System Settings ▸ Privacy & Security — "
                + "Settings ▸ Computer shows live status.")
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
            VStack(spacing: 6) {
                ForEach(missing, id: \.self) { permission in
                    Button("Grant \(permission)") { onGrant(permission) }
                }
                Button("Not Now") { onDismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(.top, 4)
        }
        .padding(20)
        .frame(width: 380)
    }
}
