//
//  MCPComputerApproval.swift
//  UnpeelNative
//
//  The /mcp/approve-computer bridge route: the unified MCP server calls it on
//  a session's first `computer` action while the app-wide Computer access is
//  "Ask each session". The request blocks while the user answers the prompt;
//  "Allow" persists the session id into app-state.json (`computer_approvals`),
//  so later actions from the same session pass without asking. The host reads
//  the list per call, so approval/revocation applies live.
//
//  Same connection discipline as MCPWriteApproval: the hook server holds the
//  request open (150s ceiling; the host client reads with ~130s). Prompting,
//  coalescing, and answering (desktop panel or paired controller) live in
//  MCPApprovalCenter.swift.
//

import Foundation

extension UnpeelStore {
    /// Handle POST /mcp/approve-computer. Asynchronous like approve-write:
    /// the reply fires when the user answers (or immediately on fast paths).
    func handleMcpApproveComputer(body: Data, reply: @escaping @Sendable (Int, String) -> Void) {
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
        switch computerDefaultAccess {
        case .allow:
            respond(true)
            return
        case .off:
            respond(false)
            return
        case .ask:
            break
        }
        if computerApprovals.contains(sessionID) {
            respond(true)
            return
        }

        enqueueMcpApproval(kind: .computer, caller: sessionID, respond: respond)
    }
}
