//
//  MCPBrowserApproval.swift
//  UnpeelNative
//
//  The /mcp/approve-browser bridge route: the unified MCP server calls it on
//  a session's first `browser` action while Browser access is "Ask each
//  session". The request blocks while the user answers; "Allow" persists the
//  session id into `browser_approvals`. Prompting, coalescing, and answering
//  (desktop panel or paired controller) live in MCPApprovalCenter.swift.
//

import Foundation

extension UnpeelStore {
    /// Handle POST /mcp/approve-browser. Asynchronous: the reply fires when
    /// the user answers (or immediately on fast paths).
    func handleMcpApproveBrowser(body: Data, reply: @escaping @Sendable (Int, String) -> Void) {
        func respond(_ approved: Bool) {
            reply(200, approved ? #"{"approved":true}"# : #"{"approved":false}"#)
        }
        guard let json = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any],
              let sessionID = (json["session_id"] as? String)?
                  .trimmingCharacters(in: .whitespacesAndNewlines),
              !sessionID.isEmpty
        else {
            reply(400, #"{"error":"session_id is required"}"#)
            return
        }

        // Fast paths: the mode may have changed since the host read it, and a
        // racing call may already have won the approval.
        switch browserDefaultAccess {
        case .on:
            respond(true)
            return
        case .off:
            respond(false)
            return
        case .ask:
            break
        }
        if browserApprovals.contains(sessionID) {
            respond(true)
            return
        }

        enqueueMcpApproval(kind: .browser, caller: sessionID, respond: respond)
    }
}
