import Foundation
import UnpeelShared

/// Current git branch without spawning `git`: parse `.git/HEAD` directly
/// (worktree checkouts have a `.git` FILE pointing at the real gitdir —
/// follow it). Cheap enough to run per project on every bootstrap.
enum GitHeadReader {
    static func currentBranch(repoPath: String) -> String? {
        let gitEntry = repoPath + "/.git"
        var headPath = gitEntry + "/HEAD"
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: gitEntry, isDirectory: &isDirectory) else {
            return nil
        }
        if !isDirectory.boolValue {
            guard let contents = try? String(contentsOfFile: gitEntry, encoding: .utf8),
                  let gitdir = contents
                      .split(separator: "\n")
                      .first(where: { $0.hasPrefix("gitdir:") })?
                      .dropFirst("gitdir:".count)
                      .trimmingCharacters(in: .whitespaces)
            else { return nil }
            let resolved = gitdir.hasPrefix("/")
                ? gitdir
                : (repoPath as NSString).appendingPathComponent(gitdir)
            headPath = resolved + "/HEAD"
        }
        guard let head = try? String(contentsOfFile: headPath, encoding: .utf8) else {
            return nil
        }
        let trimmed = head.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.hasPrefix("ref: refs/heads/") else {
            // Detached HEAD: show the short commit instead of nothing.
            return trimmed.isEmpty ? nil : String(trimmed.prefix(7))
        }
        return String(trimmed.dropFirst("ref: refs/heads/".count))
    }
}

extension SessionStatus {
    var remoteStatus: RemoteSessionStatus {
        switch self {
        case .exited: return .exited
        case .starting, .busy, .idle, .attention: return .running
        }
    }
}

extension SessionActivityStatus {
    var remoteActivity: RemoteActivityState {
        switch self {
        case .starting: return .starting
        case .working: return .working
        case .blocked: return .blocked
        case .done: return .done
        case .idle, .exited: return .idle
        }
    }
}

extension Project {
    func remoteProjectSummary(
        folderID: String? = nil,
        parentProjectID: String? = nil,
        isGroup: Bool? = nil,
        colorID: String? = nil,
        mcpBlocked effectiveMcpBlocked: Bool? = nil,
        archivedSessionCount: Int? = nil,
        dateSorted: Bool? = nil,
        displaySortOrder: Int? = nil
    ) -> RemoteProjectSummary {
        RemoteProjectSummary(
            id: id,
            name: name,
            path: path,
            folderID: folderID,
            parentProjectID: parentProjectID,
            worktreeBranch: worktreeBranch,
            isGroup: isGroup,
            colorID: colorID,
            gitBranch: GitHeadReader.currentBranch(repoPath: path),
            mcpBlocked: effectiveMcpBlocked ?? mcpBlocked ?? false,
            // The bootstrap passes the DISPLAY rank so Controllers that sort
            // by the field (the TUI does) mirror the array order exactly;
            // the file's own sortOrder is a pre-overlay value that can
            // contradict a drag persisted in project-order.json.
            sortOrder: displaySortOrder ?? sortOrder,
            archivedSessionCount: archivedSessionCount,
            dateSorted: dateSorted
        )
    }

    func remoteFolderSummary(
        colorID: String? = nil,
        displaySortOrder: Int? = nil
    ) -> RemoteProjectFolderSummary {
        RemoteProjectFolderSummary(
            id: id,
            name: name,
            parentFolderID: parentProjectID,
            colorID: colorID,
            sortOrder: displaySortOrder ?? sortOrder
        )
    }
}

extension Preset {
    func remoteSummary(defaultPresetID: String? = nil) -> RemotePresetSummary {
        let cli = SetupTool.detect(in: command)
        return RemotePresetSummary(
            id: id,
            label: label,
            command: command,
            cliID: cli?.id,
            enabled: enabled,
            quickLaunch: quickLaunch,
            isDefault: defaultPresetID == id,
            tintColorHex: Theme.toolColorHex(forCommand: command)
        )
    }
}

extension SessionEntry {
    func remoteSummary(
        projectID effectiveProjectID: String? = nil,
        unread: Bool = false,
        pinned: Bool = false,
        lastOutputPreview: String? = nil,
        updatedAtUnixMs: Int64? = nil,
        notifyWhenDone: Bool = false,
        terminalBackgroundHex: Int? = nil,
        archived: Bool = false
    ) -> RemoteSessionSummary {
        RemoteSessionSummary(
            id: id,
            projectID: effectiveProjectID ?? projectID,
            activeRuntimeID: isLive ? activeRuntimeID : nil,
            runtimeLaunchPending: isLive && runtimeLaunchPending,
            providerID: SetupTool.detect(in: command)?.id,
            title: label,
            command: command,
            createdAtUnixMs: createdAt,
            // Every Controller gets the Host-computed lifecycle timestamp;
            // it is the sort/age key for Recently updated and never a
            // running manifest heartbeat. Explicit callers may override it.
            updatedAtUnixMs: updatedAtUnixMs
                ?? max(createdAt, lifecycleAtMs ?? 0),
            status: status.remoteStatus,
            activity: activityStatus(unread: unread).remoteActivity,
            unread: unread,
            pinned: pinned,
            worktreePath: worktreePath,
            worktreeBranch: worktreeBranch,
            // Kept nil on the wire for compatibility with older controllers;
            // current sessions are flat within their project/group.
            parentSessionID: nil,
            lastOutputPreview: lastOutputPreview,
            notifyWhenDone: notifyWhenDone,
            terminalBackgroundHex: terminalBackgroundHex,
            // Verb support, computed here on the Mac so the phone's session
            // sheet offers exactly what the desktop context menu offers.
            capabilities: ProviderCapabilities.remote(session: self),
            archived: archived,
            // Brand/spinner tint from the Mac's single color table, so a new
            // CLI's color reaches phones without a phone update.
            spinnerColorHex: Theme.toolSpinnerColorHex(forCommand: presentationCommand)
        )
    }
}

extension UnpeelStore {
    /// One project's archived sessions for GET /mobile/archive — the same
    /// Mac-resolved summary shape the bootstrap ships, newest first, so the
    /// phone's archive library renders rows without provider knowledge.
    func remoteArchivedSessionSummaries(projectID: String) -> [RemoteSessionSummary] {
        let pinnedIDs = Set(pinnedByProject.values.flatMap { pins in pins.compactMap(\.sessionID) })
        return localArchivedSessions(projectID: projectID)
            .sorted { $0.createdAt > $1.createdAt }
            .map { session in
                let effectiveProjectID = session.projectOverrideID.flatMap {
                    projectsByID[$0] == nil ? nil : $0
                } ?? session.projectID
                let workingDirectory = session.worktreePath
                    ?? projectsByID[session.projectID]?.path
                return session.remoteSummary(
                    projectID: effectiveProjectID,
                    unread: unreadSessionIDs.contains(session.id),
                    pinned: pinnedIDs.contains(session.id),
                    notifyWhenDone: notifyWhenDoneSessionIDs.contains(session.id),
                    terminalBackgroundHex: TerminalFrameStyle.darkBackgroundHex(
                        for: session, workingDirectory: workingDirectory
                    ),
                    archived: true
                )
            }
    }

    func remoteBootstrapSnapshot(
        capturedAtUnixMs: Int64 = Int64(Date().timeIntervalSince1970 * 1000),
        macID: String? = nil
    ) -> RemoteBootstrapSnapshot {
        // Legacy top-level folders stay in the old `folders` array. Plain
        // child groups are real inline sidebar nodes (like worktrees), so
        // they travel as projects with `isGroup` + `parentProjectID`.
        let folderIDs = Set(projectsByID.values.filter {
            $0.isFolder == true && $0.parentProjectID == nil
        }.map(\.id))

        // Emit projects/folders in the SAME order the desktop sidebar shows
        // them, so drag-reorders on the Mac (top-level projects and worktree
        // children — the `nodes` tree, which already has the
        // `unpeel.native.projectOrder[.<parent>]` overlay applied) mirror to
        // the phone. Ids not reachable through `nodes` (e.g. folder members)
        // fall back to their file `sortOrder` then name, unchanged.
        let displayRank = projectDisplayRanks()
        func orderedBefore(_ lhs: Project, _ rhs: Project) -> Bool {
            let lr = displayRank[lhs.id] ?? Int.max
            let rr = displayRank[rhs.id] ?? Int.max
            if lr != rr { return lr < rr }
            if (lhs.sortOrder ?? Int.max) != (rhs.sortOrder ?? Int.max) {
                return (lhs.sortOrder ?? Int.max) < (rhs.sortOrder ?? Int.max)
            }
            return lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
        }

        let folders = projectsByID.values
            .filter { $0.isFolder == true && $0.parentProjectID == nil }
            .sorted(by: orderedBefore)
            .enumerated()
            .map { index, project in
                project.remoteFolderSummary(
                    colorID: projectFolderColorIDs[project.id],
                    displaySortOrder: index
                )
            }

        let projects = projectsByID.values
            .filter { $0.isFolder != true || $0.acceptsSessionDrop }
            .sorted(by: orderedBefore)
            .enumerated()
            .map { index, project in
                let isGroup = project.acceptsSessionDrop
                let folderID = isGroup ? nil : project.parentProjectID.flatMap {
                    folderIDs.contains($0) ? $0 : nil
                }
                let parentProjectID = project.parentProjectID.flatMap {
                    folderIDs.contains($0) ? nil : $0
                }
                return project.remoteProjectSummary(
                    folderID: folderID,
                    parentProjectID: parentProjectID,
                    isGroup: isGroup ? true : nil,
                    // Colors are a main-project verb; stale group entries
                    // (set before the 2026-08-13 rule) never reach phones.
                    colorID: project.parentProjectID == nil
                        ? projectFolderColorIDs[project.id] : nil,
                    mcpBlocked: self.projectMcpBlocked(project.id),
                    archivedSessionCount: localArchivedSessions(projectID: project.id).count,
                    dateSorted: isDateSorted(projectID: project.id) ? true : nil,
                    displaySortOrder: index
                )
            }

        let defaultIDsByCLI = Dictionary(uniqueKeysWithValues: SetupTool.allCases.compactMap { cli in
            defaultPreset(for: cli).map { (cli.id, $0.id) }
        })
        // availablePresets, not enabledPresets: the phone's preset drawer must
        // mirror the desktop "+" menu, which also drops CLIs toggled off in
        // Settings (CLI availability).
        let presets = availablePresets.map { preset in
            let cliID = SetupTool.detect(in: preset.command)?.id
            return preset.remoteSummary(defaultPresetID: cliID.flatMap { defaultIDsByCLI[$0] })
        }

        let pinnedIDs = Set(pinnedByProject.values.flatMap { pins in pins.compactMap(\.sessionID) })
        let sessions = orderedRemoteSessionEntries()
            .map { session in
                let effectiveProjectID = session.projectOverrideID.flatMap {
                    projectsByID[$0] == nil ? nil : $0
                } ?? session.projectID
                // Resolve the provider TUI's dark background (opencode/grok read
                // their theme from Mac-only config files) so the phone chrome
                // can match. Working dir = the session's worktree or project cwd.
                let workingDirectory = session.worktreePath
                    ?? projectsByID[session.projectID]?.path
                return session.remoteSummary(
                    projectID: effectiveProjectID,
                    unread: unreadSessionIDs.contains(session.id),
                    pinned: pinnedIDs.contains(session.id),
                    notifyWhenDone: notifyWhenDoneSessionIDs.contains(session.id),
                    terminalBackgroundHex: TerminalFrameStyle.darkBackgroundHex(
                        for: session, workingDirectory: workingDirectory
                    ),
                    archived: archivedSessionIDs.contains(session.id)
                )
            }

        // Pending MCP approval prompts, with display copy resolved Mac-side
        // so controllers render them verbatim (and never need to understand
        // a new kind to show it correctly).
        let approvals = pendingMcpApprovals.map { approval in
            let message = mcpApprovalMessage(approval)
            return RemotePendingApproval(
                id: approval.id,
                kind: approval.kind.rawValue,
                title: message.title,
                body: message.body,
                callerSessionID: approval.callerSessionID,
                targetSessionID: approval.targetSessionID,
                requestedAtUnixMs: Int64(approval.requestedAt.timeIntervalSince1970 * 1000)
            )
        }

        return RemoteBootstrapSnapshot(
            hostProtocol: MobileRemoteServer.hostProtocol,
            macID: macID,
            macName: UnpeelWorkspaceContext.advertisedHostName,
            folders: folders,
            projects: projects,
            presets: presets,
            sessions: sessions,
            capturedAtUnixMs: capturedAtUnixMs,
            experimentalWorktreesEnabled: isExperimentalEnabled(.worktrees),
            proEntitled: LicenseManager.shared.isPro,
            pendingApprovals: approvals
        )
    }

    /// Flatten the desktop sidebar tree (`nodes`, DFS) into `id -> rank` so the
    /// bootstrap can emit projects/folders in the exact on-screen order. `nodes`
    /// already carries the native drag-reorder overlay (top-level projects and
    /// worktree children), so this is what makes a Mac reorder reach the phone.
    private func projectDisplayRanks() -> [String: Int] {
        var ranks: [String: Int] = [:]
        var next = 0
        func visit(_ node: ProjectNode) {
            if ranks[node.id] == nil {
                ranks[node.id] = next
                next += 1
            }
            for child in node.worktrees { visit(child) }
        }
        for node in nodes { visit(node) }
        return ranks
    }

    private func orderedRemoteSessionEntries() -> [SessionEntry] {
        var result: [SessionEntry] = []
        var seen = Set<String>()

        func append(_ session: SessionEntry) {
            guard !seen.contains(session.id) else { return }
            seen.insert(session.id)
            result.append(session)
        }

        func visit(_ node: ProjectNode) {
            for session in pinnedSessions(in: node) {
                append(session)
            }
            // The SAME rows the desktop sidebar shows, in the same order:
            // active subtree blocks first (never truncated), then the recent
            // stopped/archived window, in the exact desktop order.
            for session in displayedSessions(in: node) {
                append(session)
            }
            // Everything else in this node was deliberately windowed out
            // (old stopped/archived rows) — mark it seen so the orphan
            // catch-all below can't resurrect it at the end of the list.
            for session in node.sessions {
                seen.insert(session.id)
            }
            for child in node.worktrees {
                visit(child)
            }
        }

        for node in nodes {
            visit(node)
        }

        for session in sessionsByID.values.sorted(by: { $0.createdAt > $1.createdAt })
        where !archivedSessionIDs.contains(session.id) {
            append(session)
        }

        return result
    }
}
