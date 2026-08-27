//
//  MCPApprovalCenter.swift
//  UnpeelNative
//
//  Unified pending-approval queue behind the three ask-mode bridge routes
//  (/mcp/approve-write, /mcp/approve-browser, /mcp/approve-computer). Each
//  route's handler keeps its own fast paths and remembered-answer store; what
//  they share is everything after that: a FIFO queue of `PendingMcpApproval`s
//  with stable ids, coalescing of identical requests, and an `answer` API that
//  any surface can call — the desktop prompt panel, or a paired controller via
//  POST /mobile/approvals/answer. First answer wins; the rest see the request
//  disappear.
//
//  The desktop prompt is a floating non-modal panel (MCPApprovalPanel.swift),
//  never `NSAlert.runModal()`. These requests arrive inside main-actor jobs
//  (Task { @MainActor } in attachHookServer), and a nested modal run loop
//  there stalls every queued main-actor job — including the mobile server's
//  bootstrap hop — so a pending prompt used to sever the phone connection
//  whenever the app had no key window. Nothing on this path may block the
//  main actor.
//

import Foundation

/// One pending ask-mode approval request, unified across the three kinds.
struct PendingMcpApproval: Identifiable, Equatable {
    enum Kind: String {
        /// Cross-group `send_text`/`send_keys` (caller → target pair).
        case write
        /// First browser action of a session under Ask.
        case browser
        /// First computer action of a session under Ask.
        case computer
    }

    let id: String
    let kind: Kind
    let callerSessionID: String
    /// Write approvals only: the session being written into.
    let targetSessionID: String?
    let requestedAt: Date
}

extension UnpeelStore {
    /// Enqueue a pending approval (or coalesce onto an identical one already
    /// waiting) and surface the prompt panel. `respond` fires once, when any
    /// surface answers.
    func enqueueMcpApproval(
        kind: PendingMcpApproval.Kind,
        caller: String,
        target: String? = nil,
        respond: @escaping (Bool) -> Void
    ) {
        if let existing = pendingMcpApprovals.first(where: {
            $0.kind == kind && $0.callerSessionID == caller && $0.targetSessionID == target
        }) {
            mcpApprovalCompletions[existing.id, default: []].append(respond)
        } else {
            let approval = PendingMcpApproval(
                id: UUID().uuidString,
                kind: kind,
                callerSessionID: caller,
                targetSessionID: target,
                requestedAt: Date()
            )
            pendingMcpApprovals.append(approval)
            mcpApprovalCompletions[approval.id] = [respond]
            notifyMcpApprovalRequested(approval)
        }
        syncMcpApprovalPanel()
    }

    /// Answer a pending approval by id. Returns false when the id is no
    /// longer pending (already answered elsewhere) — remote callers surface
    /// that as "handled on another device" instead of an error.
    @discardableResult
    func answerMcpApproval(id: String, approved: Bool) -> Bool {
        guard let index = pendingMcpApprovals.firstIndex(where: { $0.id == id }) else {
            return false
        }
        let approval = pendingMcpApprovals.remove(at: index)
        if approved {
            switch approval.kind {
            case .write:
                if let target = approval.targetSessionID {
                    approveMcpWrite(caller: approval.callerSessionID, target: target)
                }
            case .browser:
                approveBrowserAccess(sessionID: approval.callerSessionID)
            case .computer:
                approveComputerAccess(sessionID: approval.callerSessionID)
                // The user is engaged right now — if required TCC grants are
                // missing, the approval they just gave leads straight into a
                // failing first action, so chain into the grant prompt (shown
                // on the Mac, where the grants live, even for remote answers).
                checkComputerPermissionsAfterApproval()
            }
        }
        let completions = mcpApprovalCompletions.removeValue(forKey: id) ?? []
        for completion in completions {
            completion(approved)
        }
        syncMcpApprovalPanel()
        return true
    }

    /// Show the floating prompt panel while approvals are pending, hide it
    /// when the queue drains (regardless of which surface answered).
    func syncMcpApprovalPanel() {
        if pendingMcpApprovals.isEmpty {
            mcpApprovalPanel.dismiss()
        } else {
            mcpApprovalPanel.show(McpApprovalPromptView(store: self))
        }
    }

    /// Prompt copy shared by the desktop panel and remote controllers.
    /// Resolved at render time so titles follow session renames.
    func mcpApprovalMessage(_ approval: PendingMcpApproval) -> (title: String, body: String) {
        switch approval.kind {
        case .write:
            let target = approval.targetSessionID.map(sessionDisplayName) ?? "another session"
            return (
                "Allow “\(sessionDisplayName(approval.callerSessionID))” to type into “\(target)”?",
                "An agent session is asking to send input to a session outside its "
                    + "group. Allowing remembers this pair until either session "
                    + "is removed — manage approvals in Settings ▸ Sessions MCP."
            )
        case .browser:
            return (
                "Allow “\(sessionDisplayName(approval.callerSessionID))” to use a browser?",
                "The agent gets its own isolated browser window — separate profile, no "
                    + "access to your logins or tabs. Allowing remembers this session "
                    + "until it is removed — manage approvals in Settings ▸ Browser."
            )
        case .computer:
            return (
                "Allow “\(sessionDisplayName(approval.callerSessionID))” to control this Mac?",
                "The agent will be able to read app windows and click and type into them "
                    + "in the background — your real apps, including anything sensitive "
                    + "they show. It won't move your cursor or steal focus. Allowing "
                    + "remembers this session until it is removed — manage approvals in "
                    + "Settings ▸ Computer."
            )
        }
    }
}
