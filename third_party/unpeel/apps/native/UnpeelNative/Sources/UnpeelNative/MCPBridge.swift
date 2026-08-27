//
//  MCPBridge.swift
//  UnpeelNative
//
//  Native port of the app-side MCP bridge
//  (apps/desktop/src-tauri/src/mcp_bridge.rs + unpeel-core/src/mcp_auth.rs).
//
//  Unpeel Sessions MCP (`unpeel-host __mcp__`) talks to session hosts
//  directly for reads/writes, but lifecycle maintenance, controller-driven
//  session launches, and preset listing must run inside the app. Requests use
//  the hook server port and the x-unpeel-auth shared secret. start-session is
//  an internal user/controller route, never a Sessions MCP action.
//
//  Request/response JSON mirrors mcp_bridge.rs exactly so the same MCP host
//  binary works against either app.
//

import Foundation
import UnpeelShared

// MARK: - Auth token (mcp_auth.rs)

enum MCPAuth {
    static let headerName = "x-unpeel-auth"

    static var tokenURL: URL {
        LaunchConfig.unpeelDir
            .appendingPathComponent("mcp")
            .appendingPathComponent("auth-token")
    }

    /// Read the shared MCP auth token, creating it on first use
    /// (ensure_auth_token, mcp_auth.rs): two UUIDs of hex, trailing newline,
    /// user-only file mode.
    @discardableResult
    static func ensureToken() -> String? {
        if let existing = try? String(contentsOf: tokenURL, encoding: .utf8) {
            let trimmed = existing.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty { return trimmed }
        }

        let token = (UUID().uuidString + UUID().uuidString)
            .replacingOccurrences(of: "-", with: "")
            .lowercased()
        do {
            try FileManager.default.createDirectory(
                at: tokenURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try (token + "\n").write(to: tokenURL, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600], ofItemAtPath: tokenURL.path
            )
        } catch {
            NSLog("[UnpeelNative] failed to write MCP auth token: \(error)")
            return nil
        }
        return token
    }

    static func verify(_ provided: String?) -> Bool {
        guard let provided, let expected = ensureToken() else { return false }
        return provided.trimmingCharacters(in: .whitespacesAndNewlines) == expected
    }
}

/// Bridge failures surface as 400 {"error": message}, like mcp_bridge.rs.
/// String-expressible so failure sites can stay plain interpolated strings.
struct MCPBridgeError: Error, ExpressibleByStringInterpolation {
    let message: String
    init(stringLiteral value: String) { self.message = value }
}

/// Resolved launch destination for a new session: the (possibly worktree
/// child) project it lands in plus the spawn cwd/worktree fields. Shared by
/// controller-driven starts and the native "in a new worktree" session flow.
struct SessionLaunchTarget {
    let projectID: String
    let cwd: String
    let worktreePath: String?
    let worktreeBranch: String?
}

private func mcpTrimmedString(_ value: Any?) -> String? {
    guard let value = value as? String else { return nil }
    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
}

// MARK: - /mcp/* request handling (mcp_bridge.rs handle_request)

extension UnpeelStore {
    /// Dispatch a `/mcp/*` request. Returns (http_status, json_body), the
    /// same shapes as mcp_bridge.rs so the MCP host can't tell the apps
    /// apart.
    func handleMcpRequest(path: String, body: Data) -> (Int, String) {
        guard let json = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any] else {
            return (400, #"{"error":"invalid json"}"#)
        }
        let result: Result<[String: Any], MCPBridgeError>
        switch path {
        case "/mcp/list-presets":
            result = mcpListPresets(json)
        case "/mcp/start-session":
            result = mcpStartSession(json)
        case "/mcp/restart-session":
            result = mcpRestartSession(json)
        case "/mcp/sidebar":
            result = mcpSidebar(json)
        case "/mcp/rename-group":
            result = mcpRenameGroup(json)
        case "/mcp/remove-group":
            result = mcpRemoveGroup(json)
        case "/mcp/archive-session":
            result = mcpArchiveSession(json)
        case "/mcp/restore-session":
            result = mcpRestoreSession(json)
        case "/mcp/close-session":
            result = mcpCloseSession(json)
        case "/mcp/phone-resize":
            result = mcpPhoneResize(json)
        case "/mcp/mark-read":
            result = mcpMarkRead(json)
        case "/mcp/organize-session":
            result = mcpOrganizeSession(json)
        case "/mcp/computer-permissions-needed":
            result = mcpComputerPermissionsNeeded(json)
        case "/mcp/create-worktree":
            result = mcpCreateWorktree(json)
        case "/mcp/list-worktrees":
            result = mcpListWorktrees(json)
        default:
            return (404, #"{"error":"not found"}"#)
        }

        switch result {
        case .success(let value):
            guard let data = try? JSONSerialization.data(withJSONObject: value),
                  let body = String(data: data, encoding: .utf8)
            else {
                return (400, #"{"error":"response encoding failed"}"#)
            }
            return (200, body)
        case .failure(let failure):
            let error = (try? JSONSerialization.data(withJSONObject: ["error": failure.message]))
                .flatMap { String(data: $0, encoding: .utf8) }
            return (400, error ?? #"{"error":"request failed"}"#)
        }
    }

    /// list_presets (mcp_bridge.rs): enabled presets only. The native app is
    /// global-presets-only, so every preset reports scope "global" and the
    /// project_id filter has nothing extra to add.
    private func mcpListPresets(_ json: [String: Any]) -> Result<[String: Any], MCPBridgeError> {
        let presets: [[String: Any]] = enabledPresets.map { preset in
            var entry: [String: Any] = [
                "id": preset.id,
                "label": preset.label,
                "command": preset.command,
                "scope": "global",
            ]
            // Flag the preset the MCP launches when given a bare CLI id.
            if let cli = SetupTool.detect(in: preset.command), isDefaultPreset(preset, for: cli) {
                entry["default"] = true
            }
            return entry
        }
        return .success(["presets": presets])
    }

    /// Internal user/controller launch route. Lineage fields are deliberately
    /// ignored: the selected project or group is the complete collaboration
    /// boundary for the new session.
    private func mcpStartSession(_ json: [String: Any]) -> Result<[String: Any], MCPBridgeError> {
        guard let projectID = (json["project_id"] as? String)?
            .trimmingCharacters(in: .whitespaces),
            !projectID.isEmpty
        else {
            return .failure("start-session requires project_id")
        }
        let presetID = json["preset_id"] as? String
        let explicitCommand = json["command"] as? String
        let explicitLabel = mcpTrimmedString(json["label"])
        let spawnedBy = mcpTrimmedString(json["spawned_by"])
        let role = mcpTrimmedString(json["role"])
        let task = mcpTrimmedString(json["task"])
        let requestedWorktreeBranch = mcpTrimmedString(json["worktree_branch"])
        let requestedWorktreeName = mcpTrimmedString(json["worktree_name"])
        let worktreeBaseRef = mcpTrimmedString(json["worktree_base_ref"])

        if requestedWorktreeBranch == nil && worktreeBaseRef != nil {
            return .failure("worktree_base_ref requires worktree_branch")
        }
        if requestedWorktreeBranch == nil && requestedWorktreeName != nil {
            return .failure("worktree_name requires worktree_branch")
        }
        if requestedWorktreeBranch != nil && !isExperimentalEnabled(.worktrees) {
            return .failure("Git worktrees are an experimental feature and are currently disabled. Enable them in Settings ▸ Experimental to launch worktree sessions.")
        }
        if presetID == nil && explicitCommand == nil {
            return .failure("start-session requires either preset_id or command")
        }
        guard let project = projectsByID[projectID], project.isFolder != true else {
            return .failure("Unknown project id: \(projectID)")
        }

        let command: String
        if let presetID {
            if let cli = SetupTool(rawValue: presetID), let preset = defaultPreset(for: cli) {
                command = preset.command
            } else if let preset = mergedPresets.first(where: { $0.id == presetID }) {
                command = preset.command
            } else {
                return .failure("Unknown preset id: \(presetID)")
            }
        } else {
            command = (explicitCommand ?? "").trimmingCharacters(in: .whitespaces)
        }

        let target: SessionLaunchTarget
        switch sessionLaunchTarget(
            project: project,
            worktreeBranch: requestedWorktreeBranch,
            worktreeName: requestedWorktreeName,
            baseRef: worktreeBaseRef
        ) {
        case .success(let resolved):
            target = resolved
        case .failure(let failure):
            return .failure(failure)
        }

        let customTitle = explicitLabel != nil || role != nil
        let label = explicitLabel ?? role ?? (command.isEmpty ? "Terminal" : command)
        let createdAt = Int64(Date().timeIntervalSince1970 * 1000)
        guard let sessionID = spawnSession(
            projectID: target.projectID,
            command: command,
            label: label,
            customTitle: customTitle,
            createdAt: createdAt,
            cwd: target.cwd,
            worktreePath: target.worktreePath,
            worktreeBranch: target.worktreeBranch,
            spawnedBy: spawnedBy,
            role: role,
            task: task,
            activateUI: false
        ) else {
            return .failure("Failed to spawn session host")
        }

        let session: [String: Any] = [
            "id": sessionID,
            "project_id": target.projectID,
            "label": label,
            "custom_title": customTitle,
            "command": command,
            "created_at": createdAt,
            "tag_id": NSNull(),
            "worktree_path": target.worktreePath ?? NSNull() as Any,
            "worktree_branch": target.worktreeBranch ?? NSNull() as Any,
            "spawned_by": spawnedBy ?? NSNull() as Any,
            "role": role ?? NSNull() as Any,
            "task": task ?? NSNull() as Any,
        ]
        return .success(["session": session])
    }

    /// Resolve where a session with an optional worktree request launches.
    /// Reuses an existing child project for the branch, adopts an existing
    /// git worktree, or creates branch + worktree; refuses a branch that is
    /// checked out in the project root. Blocking git work runs on the
    /// calling thread. Shared by controller-driven starts and the native "in
    /// a new worktree" session flow (`startWorktreeSession`).
    func sessionLaunchTarget(
        project: Project,
        worktreeBranch requestedBranch: String?,
        worktreeName requestedName: String?,
        baseRef: String?
    ) -> Result<SessionLaunchTarget, MCPBridgeError> {
        guard let requestedBranch else {
            return .success(SessionLaunchTarget(
                projectID: project.id,
                cwd: project.path,
                worktreePath: project.worktreeBranch == nil ? nil : project.path,
                worktreeBranch: project.worktreeBranch
            ))
        }

        if project.worktreeBranch == requestedBranch {
            return .success(SessionLaunchTarget(
                projectID: project.id,
                cwd: project.path,
                worktreePath: project.path,
                worktreeBranch: requestedBranch
            ))
        }

        let parentProject: Project
        if project.worktreeBranch != nil {
            guard let parentID = project.parentProjectID,
                  let parent = projectsByID[parentID],
                  parent.isFolder != true
            else {
                return .failure("Worktree project '\(project.id)' has no usable parent project")
            }
            parentProject = parent
        } else {
            parentProject = project
        }

        if let existing = projectsByID.values.first(where: {
            $0.parentProjectID == parentProject.id && $0.worktreeBranch == requestedBranch
        }) {
            return .success(SessionLaunchTarget(
                projectID: existing.id,
                cwd: existing.path,
                worktreePath: existing.path,
                worktreeBranch: requestedBranch
            ))
        }

        if let existingPath = WorktreeGit.worktreePath(
            repoPath: parentProject.path,
            branch: requestedBranch
        ) {
            let canonicalParent = URL(fileURLWithPath: parentProject.path)
                .resolvingSymlinksInPath().path
            let canonicalExisting = URL(fileURLWithPath: existingPath)
                .resolvingSymlinksInPath().path
            if canonicalExisting == canonicalParent {
                return .failure(
                    "Branch '\(requestedBranch)' is already checked out in the project root; choose a different branch for a worktree session."
                )
            }
            guard let childID = registerWorktreeProject(
                parentID: parentProject.id,
                path: existingPath,
                branch: requestedBranch,
                name: requestedName
            ), let child = projectsByID[childID] else {
                return .failure("Failed to register existing worktree for branch '\(requestedBranch)'")
            }
            return .success(SessionLaunchTarget(
                projectID: child.id,
                cwd: child.path,
                worktreePath: child.path,
                worktreeBranch: requestedBranch
            ))
        }

        switch WorktreeGit.createWorktree(
            repoPath: parentProject.path,
            branch: requestedBranch,
            baseRef: baseRef,
            folderName: requestedName
        ) {
        case .created(let path):
            guard let childID = registerWorktreeProject(
                parentID: parentProject.id,
                path: path,
                branch: requestedBranch,
                name: requestedName
            ), let child = projectsByID[childID] else {
                return .failure("Created worktree but failed to register project for branch '\(requestedBranch)'")
            }
            return .success(SessionLaunchTarget(
                projectID: child.id,
                cwd: child.path,
                worktreePath: child.path,
                worktreeBranch: requestedBranch
            ))
        case .failed(let message):
            return .failure("Couldn't create worktree for branch '\(requestedBranch)': \(message)")
        }
    }

    /// create-worktree: create (or adopt) an Unpeel-managed worktree and
    /// register the child project — the sessions tool's `create_worktree`
    /// action. Reuses the exact resolution of the UI's "In a new worktree"
    /// flow (`sessionLaunchTarget`) WITHOUT launching a session: agents
    /// prepare checkouts, users spawn agents. Double-gated: the host checks
    /// `mcp_worktree_access` per call, and this handler re-checks it plus
    /// the worktrees experimental flag (the setting may have changed while
    /// the request was in flight).
    private func mcpCreateWorktree(_ json: [String: Any]) -> Result<[String: Any], MCPBridgeError> {
        guard isExperimentalEnabled(.worktrees) else {
            return .failure("Git worktrees are an experimental feature and are currently disabled. Enable them in Settings ▸ Experimental first.")
        }
        guard mcpWorktreeAccess else {
            return .failure("Creating worktrees from sessions is disabled in Settings ▸ Sessions use.")
        }
        guard let projectID = mcpTrimmedString(json["project_id"]),
              let project = projectsByID[projectID], project.isFolder != true
        else {
            return .failure("create-worktree requires a known project_id")
        }
        guard let branch = mcpTrimmedString(json["branch"]) else {
            return .failure("create-worktree requires branch")
        }
        // branch is git-validated downstream (check-ref-format) and the
        // folder name is slugified; base_ref is passed to git as a
        // positional revision, so keep flag-shaped values out of it.
        if let baseRef = mcpTrimmedString(json["base_ref"]), baseRef.hasPrefix("-") {
            return .failure("base_ref must be a ref name or commit, not an option")
        }
        let rootID = project.parentProjectID ?? project.id
        let existedBefore = projectsByID.values.contains {
            $0.parentProjectID == rootID && $0.worktreeBranch == branch
        }

        let target: SessionLaunchTarget
        switch sessionLaunchTarget(
            project: project,
            worktreeBranch: branch,
            worktreeName: mcpTrimmedString(json["name"]),
            baseRef: mcpTrimmedString(json["base_ref"])
        ) {
        case .success(let resolved):
            target = resolved
        case .failure(let failure):
            return .failure(failure)
        }

        let response: [String: Any] = [
            "project_id": target.projectID,
            "path": target.cwd,
            "branch": branch,
            "adopted": existedBefore,
        ]

        return .success(response)
    }

    /// list-worktrees: a project's Unpeel-managed worktree child projects.
    private func mcpListWorktrees(_ json: [String: Any]) -> Result<[String: Any], MCPBridgeError> {
        guard mcpWorktreeAccess else {
            return .failure("Creating worktrees from sessions is disabled in Settings ▸ Sessions use.")
        }
        guard let projectID = mcpTrimmedString(json["project_id"]),
              let project = projectsByID[projectID]
        else {
            return .failure("list-worktrees requires a known project_id")
        }
        let rootID = project.parentProjectID ?? project.id
        let root = projectsByID[rootID]
        let worktrees: [[String: Any]] = projectsByID.values
            .filter { $0.parentProjectID == rootID && $0.worktreeBranch != nil }
            .sorted { $0.path < $1.path }
            .map { child in
                [
                    "project_id": child.id,
                    "branch": child.worktreeBranch ?? "",
                    "path": child.path,
                ]
            }
        return .success([
            "project_id": rootID,
            "project_path": root?.path ?? "",
            "worktrees": worktrees,
        ])
    }

    /// restart_session: internal maintenance endpoint that reuses the regular
    /// app restart path so resume flags, titles, pins, worktrees, and grants
    /// are preserved.
    private func mcpRestartSession(_ json: [String: Any]) -> Result<[String: Any], MCPBridgeError> {
        guard let sessionID = (json["session_id"] as? String)?
            .trimmingCharacters(in: .whitespaces),
            !sessionID.isEmpty,
            !sessionID.contains("/"), !sessionID.contains("..")
        else {
            return .failure("restart-session requires session_id")
        }

        guard let session = sessionsByID[sessionID] else {
            return .failure("Unknown session id: \(sessionID)")
        }
        // This compatibility endpoint is the stopped-Session Resume verb.
        // A stale TUI row must never turn it into a live terminal replacement;
        // intentional Host maintenance goes through Reload Terminal instead.
        guard !session.isLive, sessionCanRestart(sessionID) else {
            return .failure("Session is still running or cannot be resumed: \(sessionID)")
        }

        guard restartSession(sessionID) else {
            return .failure("Could not restart session: \(sessionID)")
        }
        return .success(["ok": true])
    }

    /// sidebar: the app-computed sidebar model for controller clients (TUI) —
    /// the SAME rows the desktop renders via `sidebarLists`: pinned first,
    /// then active subtree blocks, then the recent stopped/archived window,
    /// with title overlays and per-project archive-library counts. Read-only.
    private func mcpSidebar(_ json: [String: Any]) -> Result<[String: Any], MCPBridgeError> {
        func statusString(_ status: SessionStatus) -> String {
            switch status {
            case .starting: return "starting"
            case .busy: return "busy"
            case .idle: return "idle"
            case .attention: return "attention"
            case .exited: return "exited"
            }
        }
        func sessionDict(_ session: SessionEntry, pinned: Bool) -> [String: Any] {
            var result: [String: Any] = [
                "id": session.id,
                "label": session.label,
                "command": session.command,
                "status": statusString(session.status),
                "pinned": pinned,
                "archived": archivedSessionIDs.contains(session.id),
                "unread": unreadSessionIDs.contains(session.id),
                "created_at": session.createdAt,
            ]
            if session.isLive, let activeRuntimeID = session.activeRuntimeID {
                result["active_runtime_id"] = activeRuntimeID
            }
            if let hostProtocolVersion = session.hostProtocolVersion {
                result["host_protocol_version"] = hostProtocolVersion
            }
            return result
        }
        func nodeDict(_ node: ProjectNode) -> [String: Any] {
            var rows: [[String: Any]] = []
            for session in renderedPinnedSessions(in: node) {
                rows.append(sessionDict(session, pinned: true))
            }
            for session in displayedSessions(in: node) {
                rows.append(sessionDict(session, pinned: false))
            }
            return [
                "id": node.project.id,
                "name": node.project.name,
                // The TUI renders worktrees and organizational groups through
                // the same nested array. Preserve the distinction on the wire
                // so a plain group does not inherit the worktree glyph.
                "is_group": node.project.acceptsSessionDrop,
                "sessions": rows,
                "archived_count": archivedSessions(in: node).count,
                "worktrees": node.worktrees.map(nodeDict),
            ]
        }
        var response: [String: Any] = ["projects": nodes.map(nodeDict)]
        // Additive mixed-version handshake: advertise only while this store
        // has a live listener or persistent retry intent. A new TUI may then
        // release Direct knowing native will claim that exact endpoint.
        // Released apps omit this field and can fall back to a random port,
        // so the TUI keeps Direct alive.
        if mobileEndpointHandoffIntent {
            response["mobile_endpoint_handoff"] = 1
        }
        return .success(response)
    }

    /// TUI group maintenance. These routes update the native UserDefaults
    /// source when the group originated in the app and the shared file when
    /// it originated in the TUI, so neither frontend can overwrite the other.
    private func mcpRenameGroup(
        _ json: [String: Any]
    ) -> Result<[String: Any], MCPBridgeError> {
        guard let projectID = mcpTrimmedString(json["project_id"]),
              let name = mcpTrimmedString(json["name"])
        else { return .failure("rename-group requires project_id and name") }
        guard renameGroupProject(projectID, to: name) else {
            return .failure("Unknown or non-group project: \(projectID)")
        }
        return .success(["ok": true])
    }

    private func mcpRemoveGroup(
        _ json: [String: Any]
    ) -> Result<[String: Any], MCPBridgeError> {
        guard let projectID = mcpTrimmedString(json["project_id"])
        else { return .failure("remove-group requires project_id") }
        guard let archived = removeGroupProject(projectID, confirm: false) else {
            return .failure("Unknown or non-group project: \(projectID)")
        }
        return .success(["ok": true, "archived_count": archived])
    }

    /// mark-read: a controller (TUI, phone) showed this session to the user,
    /// so drop its unread dot here too. Unread is in-memory app state; the
    /// shared `read.json` receipt covers frontends running app-lessly.
    private func mcpMarkRead(_ json: [String: Any]) -> Result<[String: Any], MCPBridgeError> {
        guard let sessionID = (json["session_id"] as? String)?
            .trimmingCharacters(in: .whitespaces),
            !sessionID.isEmpty,
            !sessionID.contains("/"), !sessionID.contains("..")
        else {
            return .failure("mark-read requires session_id")
        }
        clearUnreadFromRemoteViewer(sessionID)
        return .success(["ok": true])
    }

    /// archive-session: internal maintenance endpoint for controller clients
    /// (TUI). Reuses the regular archive path — non-destructive stop, native
    /// overlay bookkeeping, recency stamp — with the same restartability
    /// gate the sidebar applies (non-resumable sessions would strand in the
    /// archive library).
    private func mcpArchiveSession(_ json: [String: Any]) -> Result<[String: Any], MCPBridgeError> {
        guard let sessionID = (json["session_id"] as? String)?
            .trimmingCharacters(in: .whitespaces),
            !sessionID.isEmpty,
            !sessionID.contains("/"), !sessionID.contains("..")
        else {
            return .failure("archive-session requires session_id")
        }

        guard sessionsByID[sessionID] != nil else {
            return .failure("Unknown session id: \(sessionID)")
        }
        guard sessionCanArchive(sessionID) else {
            return .failure("Session is not resumable; use close-session instead")
        }

        archiveSession(sessionID, stampRecency: true)
        return .success(["ok": true])
    }

    /// restore-session: bring an archived session back to the sidebar
    /// (no restart — the row returns as a restartable stopped session).
    private func mcpRestoreSession(_ json: [String: Any]) -> Result<[String: Any], MCPBridgeError> {
        guard let sessionID = (json["session_id"] as? String)?
            .trimmingCharacters(in: .whitespaces),
            !sessionID.isEmpty,
            !sessionID.contains("/"), !sessionID.contains("..")
        else {
            return .failure("restore-session requires session_id")
        }

        guard sessionsByID[sessionID] != nil else {
            return .failure("Unknown session id: \(sessionID)")
        }

        restoreArchivedSessionToSidebar(sessionID)
        return .success(["ok": true])
    }

    /// phone-resize: temporarily letterbox a session's desktop terminal to a
    /// phone's grid, or clear it. Native maintenance endpoint used by the
    /// mobile dev bridge; the public MCP tool surface does not expose it.
    private func mcpPhoneResize(_ json: [String: Any]) -> Result<[String: Any], MCPBridgeError> {
        guard let sessionID = (json["session_id"] as? String)?
            .trimmingCharacters(in: .whitespaces),
            !sessionID.isEmpty,
            !sessionID.contains("/"), !sessionID.contains("..")
        else {
            return .failure("phone-resize requires session_id")
        }

        if (json["clear"] as? Bool) == true {
            clearPhoneResizeOverride(for: sessionID)
            return .success(["ok": true])
        }

        guard let cols = json["cols"] as? Int, let rows = json["rows"] as? Int,
              cols > 0, rows > 0
        else {
            return .failure("phone-resize requires cols and rows (or clear)")
        }
        guard setPhoneResizeOverride(sessionID: sessionID, cols: cols, rows: rows) else {
            return .failure("Unknown session id: \(sessionID)")
        }
        return .success(["ok": true])
    }

    /// organize-session: rename and/or (un)pin a session. Native maintenance
    /// endpoint used by the mobile dev bridge (the production path is
    /// /mobile/session-organization on MobileRemoteServer); the public MCP
    /// tool surface does not expose it.
    private func mcpOrganizeSession(_ json: [String: Any]) -> Result<[String: Any], MCPBridgeError> {
        guard let sessionID = (json["session_id"] as? String)?
            .trimmingCharacters(in: .whitespaces),
            !sessionID.isEmpty,
            !sessionID.contains("/"), !sessionID.contains("..")
        else {
            return .failure("organize-session requires session_id")
        }
        let title = mcpTrimmedString(json["title"])
        let pinned = json["pinned"] as? Bool
        guard title != nil || pinned != nil else {
            return .failure("organize-session requires title or pinned")
        }
        do {
            try applyRemoteSessionOrganization(RemoteSessionOrganizationPatch(
                sessionID: sessionID,
                title: title,
                pinned: pinned
            ))
        } catch {
            return .failure("Unknown session id: \(sessionID)")
        }
        return .success(["ok": true])
    }

    /// close_session (mcp_bridge.rs): session resolved through its manifest,
    /// then the regular kill/cleanup path.
    private func mcpCloseSession(_ json: [String: Any]) -> Result<[String: Any], MCPBridgeError> {
        guard let sessionID = (json["session_id"] as? String)?
            .trimmingCharacters(in: .whitespaces),
            !sessionID.isEmpty,
            !sessionID.contains("/"), !sessionID.contains("..")
        else {
            return .failure("close-session requires session_id")
        }

        let manifestURL = LaunchConfig.appSessionsDir
            .appendingPathComponent(sessionID)
            .appendingPathComponent("manifest.json")
        guard let data = try? Data(contentsOf: manifestURL),
              (try? JSONDecoder().decode(HostedSessionManifest.self, from: data)) != nil
        else {
            return .failure("Unknown session id: \(sessionID)")
        }

        confirmRemoveSession(sessionID)
        return .success(["ok": true])
    }
}
