//
//  MCPWriteApproval.swift
//  UnpeelNative
//
//  The /mcp/approve-write bridge route: the Sessions MCP host calls it when a
//  session tries to write into (send_text/send_keys) a session outside its
//  sidebar group and the app-wide policy is "Ask for approval". The
//  request blocks while the user answers the prompt; "Allow" persists the
//  caller→target pair into app-state.json (`mcp_write_approvals`), so the next
//  write to the same pair passes without asking. The host reads the pair map
//  per call, so approval/revocation applies live.
//
//  The hook server holds the connection open while the user decides (the MCP
//  host reads with a ~130s timeout; HookServer's per-request ceiling is raised
//  for this route). Prompting, coalescing, and answering (desktop panel or
//  paired controller) live in MCPApprovalCenter.swift.
//

import Foundation

extension UnpeelStore {
    /// Handle POST /mcp/approve-write. Unlike the other bridge routes this is
    /// asynchronous: the reply fires when the user answers the prompt (or
    /// immediately for the fast paths below).
    func handleMcpApproveWrite(body: Data, reply: @escaping @Sendable (Int, String) -> Void) {
        func respond(_ approved: Bool) {
            reply(200, approved ? #"{"approved":true}"# : #"{"approved":false}"#)
        }
        guard let json = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any],
              let caller = (json["caller_session_id"] as? String)?
                  .trimmingCharacters(in: .whitespacesAndNewlines),
              let target = (json["target_session_id"] as? String)?
                  .trimmingCharacters(in: .whitespacesAndNewlines),
              !caller.isEmpty, !target.isEmpty, caller != target
        else {
            reply(400, #"{"error":"caller_session_id and target_session_id are required"}"#)
            return
        }

        // Fast paths: the policy may have changed since the host read it, and
        // a racing caller may already have won an approval for this pair.
        switch mcpNonChildWriteAccess {
        case .allow:
            respond(true)
            return
        case .deny:
            respond(false)
            return
        case .ask:
            break
        }
        if mcpWriteApprovals[caller]?.contains(target) == true {
            respond(true)
            return
        }

        enqueueMcpApproval(kind: .write, caller: caller, target: target, respond: respond)
    }

    /// Human-readable session name for the approval prompt and the Settings
    /// approved-pairs list. Falls back to a short id for sessions this
    /// instance doesn't know (e.g. already removed).
    func sessionDisplayName(_ sessionID: String) -> String {
        if let session = sessionsByID[sessionID] {
            let label = session.label.trimmingCharacters(in: .whitespaces)
            if !label.isEmpty { return label }
        }
        return String(sessionID.prefix(8))
    }
}
