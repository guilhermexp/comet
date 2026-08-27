import Foundation
import SwiftUI
import UnpeelShared

@MainActor
@Observable
public final class RemotePreviewStore {
    public private(set) var snapshot: RemoteBootstrapSnapshot

    /// Whether the connected Mac has the experimental **Git worktrees** feature
    /// enabled (Settings ▸ Experimental). A missing value (older Mac) is treated
    /// as off, so the phone hides all worktree surfaces unless the Mac opts in —
    /// keeping the phone in lockstep with the desktop's gating.
    public var worktreesEnabled: Bool { snapshot.experimentalWorktreesEnabled == true }

    /// Pro entitlement of the connected Mac, from the bootstrap. Three-state on
    /// purpose: nil = older Mac that doesn't report it (treat as unknown, show
    /// nothing). Enforcement is Mac/server-side — this only informs phone UI
    /// (e.g. upsell copy), never gates functionality locally.
    public var macProEntitled: Bool? { snapshot.proEntitled }

    /// Capability-gated resumable image upload. Missing descriptors are
    /// legacy Hosts and must keep using the shipped one-shot upload route.
    public var supportsResumableArtifactUpload: Bool {
        guard let hostProtocol = snapshot.hostProtocol else { return false }
        return hostProtocol.isCompatible()
            && hostProtocol.supports(RemoteControlProtocol.resumableArtifactUploadCapability)
    }

    /// Whether this Host can launch sessions through the shared Controller
    /// contract. A missing descriptor is a shipped legacy Mac, whose launch
    /// route remains available; once a Host advertises a descriptor it must
    /// be compatible and explicitly list the operation.
    public var supportsSessionCreation: Bool {
        guard let hostProtocol = snapshot.hostProtocol else { return true }
        return hostProtocol.isCompatible() && hostProtocol.supports("session.create")
    }

    /// Capability-gated hold-to-reorder for sidebar sessions. Missing
    /// descriptors are legacy Hosts with no session-order route — the rows
    /// keep their long-press-for-organize behavior without drag tracking.
    public var supportsSessionReorder: Bool {
        guard let hostProtocol = snapshot.hostProtocol else { return false }
        return hostProtocol.isCompatible()
            && hostProtocol.supports(RemoteControlProtocol.sessionOrderCapability)
    }

    /// MCP approval prompts waiting on the Mac (Allow / Don't Allow), minus
    /// ones already answered from this phone — the answer POST wins the race
    /// against the next bootstrap poll, so answered prompts hide immediately.
    public var pendingApprovals: [RemotePendingApproval] {
        (snapshot.pendingApprovals ?? []).filter { !answeredApprovalIDs.contains($0.id) }
    }
    /// Approval ids answered from this phone, pruned once the Mac's bootstrap
    /// stops reporting them.
    private var answeredApprovalIDs: Set<String> = []

    /// Answer a pending approval on the Mac. 409 means it was answered on the
    /// desktop or another device first — the prompt is gone either way, so
    /// that is not an error. Any other failure re-arms the prompt.
    public func answerApproval(id: String, approved: Bool) async {
        answeredApprovalIDs.insert(id)
        do {
            try await client.answerApproval(id: id, approved: approved)
        } catch let error as RemoteMacClientError where error.statusCode == 409 {
            // Already handled elsewhere — nothing to surface.
        } catch {
            answeredApprovalIDs.remove(id)
            let reason = (error as? RemoteMacClientError)?.description ?? error.localizedDescription
            lastError = reason.isEmpty ? "Could not send the approval" : reason
        }
    }
    public var selectedSessionID: String? {
        didSet {
            // Remember the last opened session so the same one reopens after an
            // app restart/rebuild. Only persist a real selection (never clear
            // it on a transient nil, so a brief empty state doesn't lose it).
            if let selectedSessionID {
                UserDefaults.standard.set(selectedSessionID, forKey: Self.lastSessionKey)
            }
        }
    }
    /// UserDefaults key for the last-opened session id (restored on first
    /// bootstrap if it still exists on the Mac).
    private static let lastSessionKey = "unpeel.ios.lastSelectedSession"
    public var sessionsDrawerPresented = false
    /// Live rightward distance (points) of an in-progress open-drag from the
    /// terminal, so the drawer can follow the finger and peek in before the
    /// gesture commits. `nil` when no open-drag is active.
    public var sidebarDragReveal: CGFloat?
    /// Which top-bar sheet is up (bell/activity or organize). Presented at the
    /// ROOT (like pairing), because a `.sheet` nested over the Metal terminal
    /// surface doesn't present reliably — the app's own drawers are overlays
    /// for the same reason.
    public var topBarSheet: TopBarSheet? {
        didSet {
            // The organize sheet's target only lives as long as the sheet.
            if topBarSheet == nil {
                organizeTargetSessionID = nil
                organizeTargetProjectID = nil
                terminalTextSelection = nil
                archiveLibraryProjectID = nil
            }
        }
    }
    /// Payload for the text-selection sheet (terminal long-press): the
    /// viewport text snapshot plus the pre-selection range of the pressed
    /// word. Lives only as long as the sheet.
    public private(set) var terminalTextSelection: TerminalTextSelectionPayload?
    /// When the organize sheet is opened for a specific session (e.g. a
    /// long-press on a sidebar row) rather than the currently-open one, this
    /// holds that target. `nil` ⇒ organize the selected session (title press).
    public var organizeTargetSessionID: String?
    /// Which project's archive library the `.archive` top-bar sheet shows
    /// (set by the project's organize sheet before presenting).
    public var archiveLibraryProjectID: String?
    /// The project/group the `.organizeProject` sheet acts on (sidebar
    /// folder-row long-press). Lives only as long as the sheet.
    public var organizeTargetProjectID: String?

    /// Restore an archived session on the Mac (and optionally restart it so
    /// the conversation resumes). The combined path latches its correlation
    /// before Restore, because that first effect can publish the old id before
    /// Restart replaces it and neither receipt carries the replacement id.
    public func restoreArchivedSession(
        _ session: RemoteSessionSummary,
        resume: Bool
    ) async -> Bool {
        if resume, !beginReplacementSelection(for: session) {
            lastError = "Another session is still being resumed"
            return false
        }
        do {
            try await client.updateSessionOrganization(
                RemoteSessionOrganizationPatch(sessionID: session.id, archived: false)
            )
        } catch {
            // Restart was never submitted, so no replacement can appear even
            // if the Restore transport result itself was ambiguous.
            if resume { clearReplacementSelection(resetDefaultSuppression: true) }
            let reason = (error as? RemoteMacClientError)?.description
                ?? error.localizedDescription
            lastError = reason.isEmpty ? "Could not restore the session" : reason
            return false
        }
        if resume {
            do {
                try await requestSessionRestart(sessionID: session.id)
            } catch {
                handleReplacementEffectFailure(error)
                let reason = (error as? RemoteMacClientError)?.description
                    ?? error.localizedDescription
                lastError = reason.isEmpty ? "Could not resume the session" : reason
                return false
            }
        }
        await loadFromBridge()
        return true
    }
    public var expandedProjectIDs: Set<String> = []
    /// Folder accordion: at most one folder group is open at a time. Loose
    /// (unfoldered) projects are always visible.
    public var expandedFolderID: String?
    public var presetDrawerProjectID: String?
    public private(set) var launchingPresetID: String?
    public private(set) var connectionStatus = "Connecting…"
    public private(set) var lastError: String?
    /// The Mac stopped answering bootstrap polls. Set only by the connection
    /// path (never by action errors like "Could not start session") — the UI
    /// gates the session list on it, so it must mean "no live connection",
    /// not "something failed".
    public private(set) var isDisconnected = false
    /// Recovery path after the persisted Direct endpoint fails: connect
    /// through Unpeel Remote (the E2E relay). Automatic Bonjour endpoint
    /// adoption is deliberately absent: its TXT identity is unauthenticated,
    /// and probing a plaintext candidate would disclose the saved bearer.
    /// Returns true when the relay client took over.
    @ObservationIgnored public var attemptRelayFallback: (() async -> Bool)?
    /// While on the relay, probe the LAN and switch back when home again.
    @ObservationIgnored public var attemptDirectRestore: (() async -> Bool)?
    /// After a healthy direct poll: opportunistically fetch relay
    /// credentials for phones paired before Unpeel Remote (cheap no-op once
    /// they exist). The proof was captured before the bootstrap await; callers
    /// must not substitute the store's possibly-new current client.
    @ObservationIgnored public var onDirectPollSucceeded: ((RemoteConnectionPollProof) async -> Void)?
    /// Attach image bytes to the selected session's composer (upload → paste
    /// its dropped-images path, exactly like the accessory-bar photo attach).
    /// Set by the visible terminal surface, which owns the renderer; called by
    /// the browser gallery when the user applies a screenshot to the message.
    @ObservationIgnored public var attachImageToComposer: ((Data, String) -> Void)?
    /// Successful relay polls between LAN probes (~15s at the 2s cadence).
    @ObservationIgnored private var relayPollsSinceDirectProbe = 0
    /// Local-dev only: forces the client onto the relay once credentials
    /// exist, even on the same wifi (where direct would otherwise win), so
    /// Unpeel Remote can be exercised without leaving the LAN. Set with the
    /// Xcode scheme launch arg `-unpeel.ios.forceRelay YES`.
    static let forceRelayForDev = UserDefaults.standard.bool(forKey: "unpeel.ios.forceRelay")
    /// Sidebar-shaped index of the snapshot, rebuilt once per snapshot change.
    /// Rows read it every render pass, so it must not be recomputed lazily.
    private(set) var sidebarTree: IOSSidebarProjectTree
    public private(set) var projectsByID: [String: RemoteProjectSummary]
    /// One-shot error grace after returning to the foreground: the request
    /// iOS suspended mid-flight fails immediately on unlock even though the
    /// Mac is fine, and that failure used to flash the disconnected state.
    @ObservationIgnored private var suppressNextConnectionError = false
    /// The default sidebar expansion is applied on the first bootstrap (and
    /// when the selection moves), never on later polls — re-applying it every
    /// poll popped collapsed projects back open within 2s.
    @ObservationIgnored private var hasLoadedBootstrapOnce = false

    /// A replacement Resume mints a new Session id, while the legacy effect
    /// receipt contains no replacement id. Latch the source identity and the
    /// complete pre-effect Session set so a later bootstrap can select only a
    /// unique, newly-published replacement instead of the first timestamp
    /// collision in Host order.
    @ObservationIgnored private var pendingReplacementSelection: ReplacementSelectionIntent?
    /// Ambiguity or bounded expiry permanently disables automatic fallback
    /// for this intent. A later bootstrap cannot turn a failed correlation
    /// into a jump to an unrelated row; only a new explicit selection/create
    /// intent or a Host/client switch clears this latch.
    @ObservationIgnored private var replacementDefaultSelectionSuppressed = false

    struct ReplacementSelectionIntent: Equatable {
        static let maximumBootstrapObservations = 30

        let hostMacID: String?
        let sourceSessionID: String
        let projectID: String
        let createdAtUnixMs: Int64
        let runtimeID: String?
        let worktreePath: String?
        let worktreeBranch: String?
        let baselineSessionIDs: Set<String>
        var bootstrapObservationsRemaining: Int

        init(
            source: RemoteSessionSummary,
            hostMacID: String?,
            knownSessionIDs: Set<String>,
            bootstrapObservationsRemaining: Int = maximumBootstrapObservations
        ) {
            self.hostMacID = hostMacID
            sourceSessionID = source.id
            projectID = source.projectID
            createdAtUnixMs = source.createdAtUnixMs
            runtimeID = RemotePreviewStore.replacementRuntimeID(for: source)
            worktreePath = source.worktreePath
            worktreeBranch = source.worktreeBranch
            baselineSessionIDs = knownSessionIDs.union([source.id])
            self.bootstrapObservationsRemaining = bootstrapObservationsRemaining
        }
    }

    enum ReplacementSelectionResolution: Equatable {
        case wait(ReplacementSelectionIntent)
        case select(String)
        case cancel
    }

    /// Provider ids are preferred because restart commands deliberately gain
    /// resume flags. The shared runtime catalog normalizes aliases; unknown
    /// runtimes fall back to their executable family instead of comparing the
    /// full, mutation-sensitive command string.
    nonisolated static func replacementRuntimeID(
        for session: RemoteSessionSummary
    ) -> String? {
        if let providerID = session.providerID?
            .trimmingCharacters(in: .whitespacesAndNewlines),
           !providerID.isEmpty {
            return UnpeelRuntimeCatalog.runtime(id: providerID)?.stableID
                ?? providerID.lowercased()
        }
        if let runtime = UnpeelRuntimeCatalog.runtime(command: session.command) {
            return runtime.stableID
        }
        let executable = session.command
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .split(whereSeparator: { $0.isWhitespace })
            .first
            .map(String.init)
        guard let executable else { return nil }
        let family = (executable as NSString).lastPathComponent.lowercased()
        return family.isEmpty ? nil : family
    }

    /// Correlate a replacement only when it is new relative to the exact
    /// pre-effect snapshot and uniquely preserves every stable source field.
    /// The source must disappear first; while Restore briefly republishes the
    /// old id, a concurrent matching launch cannot be mistaken for it.
    nonisolated static func replacementSelectionResolution(
        _ intent: ReplacementSelectionIntent,
        sessions: [RemoteSessionSummary]
    ) -> ReplacementSelectionResolution {
        let sourceStillExists = sessions.contains { $0.id == intent.sourceSessionID }
        let candidates = sessions.filter { session in
            guard session.supportsIOSSessionAPI,
                  session.id != intent.sourceSessionID,
                  !intent.baselineSessionIDs.contains(session.id),
                  session.projectID == intent.projectID,
                  session.createdAtUnixMs == intent.createdAtUnixMs,
                  session.status == .running,
                  !session.archived,
                  session.worktreePath == intent.worktreePath,
                  session.worktreeBranch == intent.worktreeBranch
            else { return false }
            guard let expectedRuntimeID = intent.runtimeID else { return true }
            return replacementRuntimeID(for: session) == expectedRuntimeID
        }

        if candidates.count > 1 { return .cancel }
        if !sourceStillExists, let candidate = candidates.first {
            return .select(candidate.id)
        }
        guard intent.bootstrapObservationsRemaining > 1 else { return .cancel }
        var waiting = intent
        waiting.bootstrapObservationsRemaining -= 1
        return .wait(waiting)
    }

    @discardableResult
    private func beginReplacementSelection(for source: RemoteSessionSummary) -> Bool {
        guard pendingReplacementSelection == nil else { return false }
        pendingDeepLinkID = nil
        pendingReplacementSelection = ReplacementSelectionIntent(
            source: source,
            hostMacID: snapshot.macID,
            knownSessionIDs: Set(snapshot.sessions.map(\.id))
        )
        replacementDefaultSelectionSuppressed = true
        return true
    }

    private func clearReplacementSelection(resetDefaultSuppression: Bool) {
        pendingReplacementSelection = nil
        if resetDefaultSuppression {
            replacementDefaultSelectionSuppressed = false
        }
        isRestartingSelectedSession = false
    }

    private func handleReplacementEffectFailure(_ error: Error) {
        // A concrete Host response proves the replacement was rejected. A
        // transport failure can occur after submission, so retain the bounded
        // correlation and let later bootstraps settle it without replaying.
        if error is RemoteMacClientError {
            clearReplacementSelection(resetDefaultSuppression: true)
        } else {
            isRestartingSelectedSession = false
        }
    }
    /// Drives the "Restarting…" affordance on the exited-session bar.
    public private(set) var isRestartingSelectedSession = false
    /// Why the last restart attempt failed, shown inline on the restart bar so
    /// a rejected restart is never a silent "nothing happens". Cleared when a
    /// new attempt starts or the revived session lands.
    public private(set) var restartError: String?

    /// A session id from a tapped push notification that isn't in the snapshot
    /// yet (cold launch). Selected once it appears in a poll, instead of the
    /// default-selection fallback clobbering it.
    @ObservationIgnored private var pendingDeepLinkID: String?

    /// Select a session by id (push-notification tap). If it isn't loaded yet,
    /// remember it and select on the next poll.
    public func selectSessionByID(_ id: String) {
        clearReplacementSelection(resetDefaultSuppression: true)
        if let session = snapshot.sessions.first(where: {
            $0.id == id && $0.supportsIOSSessionAPI
        }) {
            select(session)
        } else {
            pendingDeepLinkID = id
            selectedSessionID = id
        }
    }

    /// Always sourced from RemoteConnectionStore — the store never builds
    /// its own endpoint. `adoptClient` swaps it after (un)pairing.
    @ObservationIgnored public private(set) var client: RemoteMacClient
    /// RemoteConnectionStore's generation for `client`. Captured before each
    /// bootstrap so a reply from an older Mac/re-pair cannot be attributed to
    /// whichever client happens to be current after the await.
    @ObservationIgnored private var clientConnectionEpoch = 0
    /// Narrow transport seams for deterministic effect/bootstrap convergence
    /// tests. Production leaves them nil and always uses the current client,
    /// including after `adoptClient` swaps Macs.
    @ObservationIgnored private let createSessionOverride: ((RemoteCreateSessionRequest) async throws -> RemoteCreateSessionResponse)?
    @ObservationIgnored private let bootstrapOverride: (() async throws -> RemoteBootstrapSnapshot)?
    @ObservationIgnored private let restartSessionOverride: ((String) async throws -> Void)?

    /// The real app starts EMPTY and fills in from the Mac — dummy data must
    /// never masquerade as sessions. `.mock` is for SwiftUI previews only
    /// (use `RemotePreviewStore.preview`).
    public convenience init() {
        self.init(snapshot: .empty, client: RemoteMacClient())
    }

    /// SwiftUI-preview store seeded with fake content. Never used at runtime.
    public static var preview: RemotePreviewStore {
        RemotePreviewStore(snapshot: .mock, client: RemoteMacClient())
    }

    public convenience init(
        snapshot: RemoteBootstrapSnapshot,
        client: RemoteMacClient = RemoteMacClient()
    ) {
        self.init(
            snapshot: snapshot,
            client: client,
            createSessionOverride: nil,
            bootstrapOverride: nil,
            restartSessionOverride: nil
        )
    }

    init(
        snapshot: RemoteBootstrapSnapshot,
        client: RemoteMacClient,
        createSessionOverride: ((RemoteCreateSessionRequest) async throws -> RemoteCreateSessionResponse)?,
        bootstrapOverride: (() async throws -> RemoteBootstrapSnapshot)?,
        restartSessionOverride: ((String) async throws -> Void)? = nil
    ) {
        self.snapshot = snapshot
        self.client = client
        self.createSessionOverride = createSessionOverride
        self.bootstrapOverride = bootstrapOverride
        self.restartSessionOverride = restartSessionOverride
        sidebarTree = IOSSidebarProjectTree(snapshot: snapshot)
        projectsByID = Dictionary(uniqueKeysWithValues: snapshot.projects.map { ($0.id, $0) })
        selectedSessionID = Self.defaultSelectedSessionID(in: snapshot)
        expandedProjectIDs = Self.defaultExpandedProjectIDs(
            in: snapshot,
            selectedSessionID: selectedSessionID
        )
        expandedFolderID = Self.defaultExpandedFolderID(
            in: snapshot,
            selectedSessionID: selectedSessionID
        )
    }

    /// Switch which Mac this store talks to (pair/unpair). The caller
    /// follows up with a refresh; stale snapshot data stays visible until
    /// the first bootstrap from the new endpoint lands.
    public func adoptClient(_ client: RemoteMacClient, connectionEpoch: Int) {
        if connectionEpoch != clientConnectionEpoch
            || client.baseURL != self.client.baseURL
            || client.authToken != self.client.authToken
            || client.isRelay != self.client.isRelay {
            clearReplacementSelection(resetDefaultSuppression: true)
        }
        self.client = client
        clientConnectionEpoch = connectionEpoch
    }

    public var selectedSession: RemoteSessionSummary? {
        guard let selectedSessionID else { return nil }
        return snapshot.sessions.first { $0.id == selectedSessionID && $0.supportsIOSSessionAPI }
    }

    /// Which session the organize (rename/pin) sheet acts on: an explicit
    /// long-press target if set, otherwise the currently-open session.
    public var organizeSheetSession: RemoteSessionSummary? {
        if let organizeTargetSessionID,
           let target = snapshot.sessions.first(where: {
               $0.id == organizeTargetSessionID && $0.supportsIOSSessionAPI
           }) {
            return target
        }
        return selectedSession
    }

    /// Open the organize (rename/pin) sheet for a specific session — used by
    /// the sidebar row long-press, so any session can be renamed/pinned without
    /// switching to it. Passing `nil` targets the currently-open session, which
    /// is what the terminal title long-press does.
    public func presentSessionOrganize(for session: RemoteSessionSummary?) {
        guard session?.supportsIOSSessionAPI ?? true else { return }
        organizeTargetSessionID = session?.id
        topBarSheet = .organize
    }

    /// The project the folder organize sheet acts on, resolved fresh from the
    /// snapshot so a background poll never leaves the sheet on stale data.
    public var organizeSheetProject: RemoteProjectSummary? {
        guard let organizeTargetProjectID else { return nil }
        return projectsByID[organizeTargetProjectID]
    }

    /// Whether the connected Host accepts folder organize patches (rename,
    /// color, session sort). Older Macs don't serve the route — the sheet
    /// hides its editing controls and offers only the archive library.
    public var supportsProjectOrganization: Bool {
        guard let hostProtocol = snapshot.hostProtocol else { return false }
        return hostProtocol.isCompatible()
            && hostProtocol.supports("project.organization.set")
    }

    /// Open the folder organize sheet for a sidebar project/group row.
    public func presentProjectOrganize(for project: RemoteProjectSummary) {
        organizeTargetProjectID = project.id
        topBarSheet = .organizeProject
    }

    /// Apply a folder organize patch on the Host and refresh the snapshot.
    public func applyProjectOrganization(_ patch: RemoteProjectOrganizationPatch) async -> Bool {
        do {
            try await client.updateProjectOrganization(patch)
            await loadFromBridge()
            return true
        } catch {
            let reason = (error as? RemoteMacClientError)?.description ?? error.localizedDescription
            lastError = reason.isEmpty ? "Could not update the folder" : reason
            return false
        }
    }

    /// Open the text-selection sheet for a terminal long-press. The sheet is
    /// presented at the ROOT (like every top-bar sheet) because a `.sheet`
    /// nested over the Metal terminal surface doesn't present reliably.
    public func presentTerminalTextSelection(text: String, anchorRange: NSRange?) {
        terminalTextSelection = TerminalTextSelectionPayload(
            text: text,
            anchorRange: anchorRange
        )
        topBarSheet = .textSelection
    }

    /// Bell "Blocked": every live attention session, including the one
    /// currently open. Blockers win over every other sheet bucket; the Host
    /// normally sends unique ids, but explicit uniquing keeps a malformed or
    /// transitional snapshot from rendering the same blocker twice.
    public var bellBlockedSessions: [RemoteSessionSummary] {
        Self.uniquedActivitySessions(
            snapshot.sessions.filter {
                $0.supportsIOSSessionAPI
                    && $0.status == .running
                    && $0.activity == .blocked
            }
        )
        .sorted(by: Self.lifecycleThenIDComesBefore)
    }

    /// Bell "Active": sessions actively working (the spinner list). Blocked
    /// ids are excluded explicitly so attention always owns the leading sheet
    /// section, even if a transitional snapshot repeats an id with two states.
    public var bellActiveSessions: [RemoteSessionSummary] {
        let blockerIDs = Set(bellBlockedSessions.map(\.id))
        return Self.uniquedActivitySessions(
            snapshot.sessions.filter {
                $0.supportsIOSSessionAPI
                    && $0.status == .running
                    && $0.activity == .working
                    && !blockerIDs.contains($0.id)
            }
        )
            .prefix(8)
            .map { $0 }
    }

    /// Bell "Recent": recently-touched non-active sessions (excludes the one
    /// you're already in), unread "blue dot" sessions floated to the top — so
    /// the bell is always a useful switcher, even with nothing running.
    public var bellRecentSessions: [RemoteSessionSummary] {
        // Use every qualifying working id, not only the eight rendered Active
        // rows; otherwise a ninth worker leaks into Recent. Blockers likewise
        // never become ordinary blue-dot/age rows.
        let reservedIDs: Set<String> = Set(snapshot.sessions.compactMap { session in
            guard session.supportsIOSSessionAPI,
                  session.status == .running,
                  session.activity == .blocked || session.activity == .working
            else { return nil }
            return session.id
        })
        return Self.uniquedActivitySessions(
            snapshot.sessions.filter {
                $0.supportsIOSSessionAPI
                    && $0.id != selectedSessionID
                    && !reservedIDs.contains($0.id)
            }
        )
            .sorted { lhs, rhs in
                if lhs.unread != rhs.unread { return lhs.unread }
                return Self.lifecycleThenIDComesBefore(lhs, rhs)
            }
            .prefix(8)
            .map { $0 }
    }

    private static func uniquedActivitySessions(
        _ sessions: [RemoteSessionSummary]
    ) -> [RemoteSessionSummary] {
        var seen = Set<String>()
        return sessions.filter { seen.insert($0.id).inserted }
    }

    /// Same non-working Recent rank used by the other clients: canonical Host
    /// lifecycle newest first, then id for deterministic equal timestamps.
    private static func lifecycleThenIDComesBefore(
        _ lhs: RemoteSessionSummary,
        _ rhs: RemoteSessionSummary
    ) -> Bool {
        let lhsStamp = max(lhs.createdAtUnixMs, lhs.updatedAtUnixMs ?? 0)
        let rhsStamp = max(rhs.createdAtUnixMs, rhs.updatedAtUnixMs ?? 0)
        if lhsStamp != rhsStamp { return lhsStamp > rhsStamp }
        return lhs.id < rhs.id
    }

    public var presetDrawerProject: RemoteProjectSummary? {
        guard supportsSessionCreation else { return nil }
        return presetDrawerProjectID.flatMap { projectsByID[$0] }
    }

    public var blockedSessions: [RemoteSessionSummary] {
        snapshot.sessions.filter { $0.supportsIOSSessionAPI && $0.activity == .blocked && $0.status == .running }
    }

    public func select(_ session: RemoteSessionSummary) {
        guard session.supportsIOSSessionAPI else { return }
        clearReplacementSelection(resetDefaultSuppression: true)
        if selectedSessionID != session.id {
            selectedSessionID = session.id
        }
        revealProject(session.projectID)
        hideSessions()
        markSessionRead(session.id, wasUnread: session.unread)
    }

    /// Opening a session on the phone clears its unread "blue dot" on the Mac
    /// (the desktop only clears unread for its own frontmost selection, which
    /// never fires while a phone drives it). Skips the round-trip when the
    /// session wasn't unread.
    private func markSessionRead(_ sessionID: String, wasUnread: Bool) {
        guard wasUnread else { return }
        Task { try? await client.markRead(sessionID: sessionID) }
    }

    /// Expand whatever contains the project (its folder, plus the project and
    /// its worktree parent) — mutating only what actually changes, so a
    /// no-op reveal never invalidates observers mid-animation.
    public func revealProject(_ projectID: String) {
        let revealIDs = Self.projectRevealIDs(for: projectID, in: snapshot)
        if !revealIDs.isSubset(of: expandedProjectIDs) {
            expandedProjectIDs.formUnion(revealIDs)
        }
        if let folderID = Self.folderID(forProjectID: projectID, in: snapshot),
           expandedFolderID != folderID {
            expandedFolderID = folderID
        }
    }

    public func toggleProject(_ projectID: String) {
        if expandedProjectIDs.contains(projectID) {
            expandedProjectIDs.remove(projectID)
        } else {
            expandedProjectIDs.insert(projectID)
        }
    }

    /// Accordion semantics: opening a folder closes the previous one; tapping
    /// the open folder closes it.
    public func toggleFolder(_ folderID: String) {
        expandedFolderID = expandedFolderID == folderID ? nil : folderID
    }

    public func showSessions() {
        withAnimation(.timingCurve(0.16, 1, 0.3, 1, duration: 0.28)) {
            sessionsDrawerPresented = true
        }
    }

    public func hideSessions() {
        withAnimation(.timingCurve(0.16, 1, 0.3, 1, duration: 0.22)) {
            sessionsDrawerPresented = false
        }
    }

    public func showPresetDrawer(for projectID: String) {
        guard supportsSessionCreation, projectsByID[projectID] != nil else { return }
        withAnimation(.timingCurve(0.16, 1, 0.3, 1, duration: 0.28)) {
            presetDrawerProjectID = projectID
        }
    }

    public func hidePresetDrawer() {
        withAnimation(.timingCurve(0.16, 1, 0.3, 1, duration: 0.22)) {
            presetDrawerProjectID = nil
        }
    }

    @discardableResult
    public func startSession(
        projectID: String,
        preset: RemotePresetSummary
    ) -> Task<Void, Never>? {
        guard supportsSessionCreation,
              launchingPresetID == nil,
              preset.supportsIOSSessionAPI
        else { return nil }
        // A new Session is an explicit focus intent. Its receipt carries the
        // exact id, so it supersedes any unresolved replacement correlation.
        clearReplacementSelection(resetDefaultSuppression: true)
        pendingDeepLinkID = nil
        launchingPresetID = preset.id
        // Slide the drawer (and sidebar) away immediately on tap — the launch
        // continues in the background and selects the new session when ready.
        hidePresetDrawer()
        hideSessions()
        return Task {
            do {
                let response = try await requestSessionCreation(
                    RemoteCreateSessionRequest(
                        projectID: projectID,
                        presetID: preset.id
                    )
                )
                if let created = response.session, created.supportsIOSSessionAPI {
                    publishCreatedSession(created, capturedAtUnixMs: response.capturedAtUnixMs)
                }

                // Newer Hosts return the summary and select immediately. A
                // headless/older adapter may only return the id, so briefly
                // converge through bootstrap. Only the read is repeated: the
                // create mutation above is never retried after an uncertain
                // or successful response.
                // Eight reads at 250ms spacing span ~1.75s, safely beyond a
                // headless Host's 1s manifest polling fallback. Its state-bus
                // announcement intentionally skips the writer's own port, so
                // the first few reads can legitimately predate local pickup.
                let bootstrapAttempts = response.session == nil ? 8 : 1
                for attempt in 0 ..< bootstrapAttempts {
                    await loadFromBridge()
                    if snapshot.sessions.contains(where: {
                        $0.id == response.sessionID && $0.supportsIOSSessionAPI
                    }) {
                        selectedSessionID = response.sessionID
                        revealProject(projectID)
                        break
                    }
                    guard attempt + 1 < bootstrapAttempts else { break }
                    do {
                        try await Task.sleep(nanoseconds: 250_000_000)
                    } catch {
                        break
                    }
                }
            } catch {
                lastError = "Could not start session"
            }
            launchingPresetID = nil
        }
    }

    private func requestSessionCreation(
        _ request: RemoteCreateSessionRequest
    ) async throws -> RemoteCreateSessionResponse {
        if let createSessionOverride {
            return try await createSessionOverride(request)
        }
        return try await client.createSession(request)
    }

    private func requestSessionRestart(sessionID: String) async throws {
        if let restartSessionOverride {
            try await restartSessionOverride(sessionID)
            return
        }
        try await client.restartSession(sessionID: sessionID)
    }

    private func requestBootstrap(
        using polledClient: RemoteMacClient
    ) async throws -> RemoteBootstrapSnapshot {
        if let bootstrapOverride {
            return try await bootstrapOverride()
        }
        return try await polledClient.bootstrap()
    }

    private func publishCreatedSession(
        _ session: RemoteSessionSummary,
        capturedAtUnixMs: Int64?
    ) {
        clearReplacementSelection(resetDefaultSuppression: true)
        var sessions = snapshot.sessions.filter { $0.id != session.id }
        sessions.insert(session, at: 0)

        let next = RemoteBootstrapSnapshot(
            protocolVersion: snapshot.protocolVersion,
            hostProtocol: snapshot.hostProtocol,
            macID: snapshot.macID,
            macName: snapshot.macName,
            folders: snapshot.folders,
            projects: snapshot.projects,
            presets: snapshot.presets,
            sessions: sessions,
            capturedAtUnixMs: capturedAtUnixMs ?? snapshot.capturedAtUnixMs,
            remoteServerPort: snapshot.remoteServerPort,
            remoteServerCertificateFingerprint: snapshot.remoteServerCertificateFingerprint,
            experimentalWorktreesEnabled: snapshot.experimentalWorktreesEnabled,
            proEntitled: snapshot.proEntitled,
            pendingApprovals: snapshot.pendingApprovals
        )
        if !Self.snapshotContentEqual(snapshot, next) {
            snapshot = next
            sidebarTree = IOSSidebarProjectTree(snapshot: next)
            projectsByID = Dictionary(uniqueKeysWithValues: next.projects.map { ($0.id, $0) })
        }
        selectedSessionID = session.id
        revealProject(session.projectID)
    }

    /// Rename, (un)pin, and/or archive a session on the Mac, then refresh the
    /// snapshot so the change shows immediately instead of on the next poll.
    /// Archiving stops the session on the Mac and files it into the project's
    /// Archived section there; archived sessions drop out of the phone
    /// snapshot entirely. Returns false (and sets lastError) when the Mac
    /// rejected it.
    public func updateSessionOrganization(
        sessionID: String,
        title: String?,
        pinned: Bool?,
        archived: Bool? = nil,
        notifyWhenDone: Bool? = nil
    ) async -> Bool {
        do {
            try await client.updateSessionOrganization(
                RemoteSessionOrganizationPatch(
                    sessionID: sessionID,
                    title: title,
                    pinned: pinned,
                    archived: archived,
                    notifyWhenDone: notifyWhenDone
                )
            )
            _ = await loadFromBridge()
            return true
        } catch {
            lastError = "Could not update session"
            return false
        }
    }

    /// Persist a hold-to-reorder drop: the combined pinned + regular order
    /// for one project, exactly as a desktop drag commits it. The local
    /// snapshot is reordered immediately so the dropped arrangement holds
    /// while the Host write and next bootstrap are in flight.
    public func reorderSessions(projectID: String, orderedIDs: [String]) {
        guard !orderedIDs.isEmpty else { return }
        applyLocalSessionOrder(projectID: projectID, orderedIDs: orderedIDs)
        Task {
            do {
                try await client.updateSessionOrder(
                    RemoteSessionOrderRequest(
                        projectID: projectID,
                        orderedSessionIDs: orderedIDs
                    )
                )
            } catch {
                lastError = "Could not reorder sessions"
            }
            // Success or failure, converge on the Host's truth.
            await loadFromBridge()
        }
    }

    private func applyLocalSessionOrder(projectID: String, orderedIDs: [String]) {
        var rank: [String: Int] = [:]
        for (index, id) in orderedIDs.enumerated() { rank[id] = index }
        // Reorder only this project's sessions, in place: ranked ones take
        // the dropped order; unranked ones (hidden by the stopped-session
        // window) sink below in their existing order, matching how the Host
        // files sessions absent from the rank list.
        var slots: [Int] = []
        var bucket: [RemoteSessionSummary] = []
        for (index, session) in snapshot.sessions.enumerated()
        where session.projectID == projectID {
            slots.append(index)
            bucket.append(session)
        }
        let reordered = bucket
            .enumerated()
            .sorted { a, b in
                switch (rank[a.element.id], rank[b.element.id]) {
                case let (left?, right?): return left < right
                case (_?, nil): return true
                case (nil, _?): return false
                case (nil, nil): return a.offset < b.offset
                }
            }
            .map(\.element)
        guard reordered.map(\.id) != bucket.map(\.id) else { return }
        var sessions = snapshot.sessions
        for (slot, session) in zip(slots, reordered) { sessions[slot] = session }
        let next = RemoteBootstrapSnapshot(
            protocolVersion: snapshot.protocolVersion,
            hostProtocol: snapshot.hostProtocol,
            macID: snapshot.macID,
            macName: snapshot.macName,
            folders: snapshot.folders,
            projects: snapshot.projects,
            presets: snapshot.presets,
            sessions: sessions,
            capturedAtUnixMs: snapshot.capturedAtUnixMs,
            remoteServerPort: snapshot.remoteServerPort,
            remoteServerCertificateFingerprint: snapshot.remoteServerCertificateFingerprint,
            experimentalWorktreesEnabled: snapshot.experimentalWorktreesEnabled,
            proEntitled: snapshot.proEntitled,
            pendingApprovals: snapshot.pendingApprovals
        )
        snapshot = next
        sidebarTree = IOSSidebarProjectTree(snapshot: next)
    }

    /// Resume the currently selected stopped Session on the Host. The legacy
    /// replacement operation re-runs its command with resume flags and mints
    /// a new id; bootstrap correlation selects it only when every stable
    /// source field yields one unique, post-effect candidate.
    @discardableResult
    public func restartSelectedSession() -> Task<Void, Never>? {
        guard let session = selectedSession,
              session.status == .exited,
              session.capabilities?.restart ?? true,
              !isRestartingSelectedSession,
              beginReplacementSelection(for: session)
        else { return nil }
        isRestartingSelectedSession = true
        restartError = nil
        return Task {
            do {
                try await requestSessionRestart(sessionID: session.id)
                await loadFromBridge()
            } catch {
                handleReplacementEffectFailure(error)
                // Surface the Mac's actual reason (unknown session, license
                // gate, etc.) instead of failing silently.
                let reason = (error as? RemoteMacClientError)?.description
                    ?? error.localizedDescription
                restartError = reason.isEmpty ? "Could not resume session" : reason
                lastError = restartError
            }
        }
    }

    /// Desktop-parity actions from the iOS edit sheet. Restart/remove reuse
    /// the Mac's own behavior; stop leaves the session history restartable.
    public func performSessionAction(
        sessionID: String,
        action: RemoteSessionAction
    ) async -> Bool {
        let target = snapshot.sessions.first { $0.id == sessionID && $0.supportsIOSSessionAPI }
        switch action {
        case .restart:
            guard target?.status == .exited,
                  target?.capabilities?.restart ?? true
            else { return false }
        case .resumeAgent:
            guard target?.status == .running,
                  target?.activeRuntimeID == nil,
                  target?.runtimeLaunchPending == false,
                  target?.capabilities?.resumeAgent == true,
                  snapshot.hostProtocol?.isCompatible() == true,
                  snapshot.hostProtocol?.supports(
                    RemoteControlProtocol.sessionRuntimeResumeCapability
                  ) == true
            else { return false }
        case .restartAgent:
            // Decode-only compatibility with protocol-minor-5 Hosts. Current
            // Controllers never emit the old active-runtime restart action.
            return false
        case .stop, .remove:
            break
        }
        let tracksReplacement = action == .restart && sessionID == selectedSessionID
        if tracksReplacement, let target {
            guard beginReplacementSelection(for: target) else { return false }
            isRestartingSelectedSession = true
            restartError = nil
        }
        do {
            try await client.performSessionAction(
                RemoteSessionActionRequest(sessionID: sessionID, action: action)
            )
            if action == .remove, selectedSessionID == sessionID {
                selectedSessionID = nil
            }
            await loadFromBridge()
            return true
        } catch {
            if tracksReplacement {
                handleReplacementEffectFailure(error)
                restartError = nil
            }
            let reason = (error as? RemoteMacClientError)?.description
                ?? error.localizedDescription
            lastError = reason.isEmpty ? "Could not update session" : reason
            return false
        }
    }

    /// Fetch the session's transcript as Markdown from the Mac (rendered with
    /// the Mac's Settings ▸ Transcripts options). Returns nil — with
    /// `lastError` set — when the fetch fails or the session has no readable
    /// transcript yet; the caller owns the clipboard write.
    public func transcriptMarkdown(sessionID: String, entries: Int? = nil) async -> String? {
        do {
            let response = try await client.transcriptMarkdown(sessionID: sessionID, entries: entries)
            let markdown = response.markdown
                .trimmingCharacters(in: .whitespacesAndNewlines)
            if markdown.isEmpty {
                lastError = "This session has no readable transcript yet"
                return nil
            }
            return markdown
        } catch {
            let reason = (error as? RemoteMacClientError)?.description
                ?? error.localizedDescription
            lastError = reason.isEmpty ? "Could not copy transcript" : reason
            return nil
        }
    }

    public func sendKey(_ key: RemoteKeyName) {
        sendTerminalInput(Self.sequence(for: key))
    }

    public func sendTerminalInput(_ data: String) {
        guard !data.isEmpty, let selectedSessionID else { return }
        let sessionID = selectedSessionID
        Task {
            do {
                try await client.write(sessionID: sessionID, data: data)
            } catch {
                lastError = "Input unavailable"
            }
        }
    }

    public func runBridgeRefreshLoop() async {
        RefreshDiagnostics.log("refresh loop STARTED")
        defer { RefreshDiagnostics.log("refresh loop EXITED (cancelled=\(Task.isCancelled))") }
        while !Task.isCancelled {
            let pollResult = await loadFromBridge()
            switch pollResult {
            case .currentFailure:
                // A Bonjour TXT record cannot authenticate a Host. Never send
                // the saved command bearer to a discovered plaintext URL;
                // recover only through the existing E2E Relay path. Without
                // Relay, the persisted endpoint keeps retrying and the user
                // can explicitly re-pair if the Host address changed.
                let recovered = await recoverDisconnectedConnection(after: pollResult)
                if recovered, !Task.isCancelled {
                    _ = await loadFromBridge()
                }
            case .success(let successfulPoll):
                if successfulPoll.client.isRelay {
                    // Dev force-relay pins the client to the relay; skip the
                    // probe that would switch back to the LAN.
                    guard !Self.forceRelayForDev else { break }
                    relayPollsSinceDirectProbe += 1
                    if relayPollsSinceDirectProbe >= 8,
                       let restore = attemptDirectRestore {
                        relayPollsSinceDirectProbe = 0
                        _ = await restore()
                    }
                } else {
                    relayPollsSinceDirectProbe = 0
                    // Fetch relay creds over the healthy LAN first (upgrade path).
                    await onDirectPollSucceeded?(successfulPoll)
                    // Dev force-relay: once creds exist, switch to the relay even
                    // though the LAN is fine, so the relay path is exercised on
                    // the same wifi (local dev — set via the Xcode scheme arg
                    // `-unpeel.ios.forceRelay YES`).
                    if Self.forceRelayForDev,
                       let relayFallback = attemptRelayFallback {
                        _ = await relayFallback()
                    }
                }
            case .superseded:
                // The selected client changed while this request was in
                // flight. Its own immediate/loop poll owns all recovery.
                break
            }
            guard !Task.isCancelled else { return }
            // Disconnected polls stay frequent: with the fail-fast bootstrap
            // timeout each attempt is cheap, and a returning Mac should be
            // picked up within seconds, not after a long backoff.
            let interval: UInt64 = isDisconnected ? 3_000_000_000 : 2_000_000_000
            do {
                try await Task.sleep(nanoseconds: interval)
            } catch {
                return
            }
        }
    }

    /// Attempt the only automatic recovery allowed after the persisted Direct
    /// endpoint fails: the authenticated, E2E Relay connection. Kept as one
    /// narrow decision point so a future discovery transport cannot silently
    /// reintroduce bearer-authenticated probing of Bonjour-derived HTTP URLs.
    /// Internal for the security regression test.
    func recoverDisconnectedConnection() async -> Bool {
        guard isDisconnected, !Task.isCancelled,
              let relayFallback = attemptRelayFallback
        else { return false }
        return await relayFallback()
    }

    /// A superseded completion is never evidence about the selected client,
    /// even if `isDisconnected` is still latched from the previous Mac.
    func recoverDisconnectedConnection(
        after pollResult: RemoteConnectionPollResult
    ) async -> Bool {
        guard case .currentFailure = pollResult else { return false }
        return await recoverDisconnectedConnection()
    }

    /// Called just before the refresh loop restarts on foreground resume:
    /// grants the next poll failure a one-shot pass so a stale suspended
    /// request can't flash "Can't reach your Mac" over a healthy connection.
    public func prepareForForegroundResume() {
        suppressNextConnectionError = true
    }

    /// When the last `loadFromBridge` call finished (success OR failure).
    /// The refresh loop is strictly sequential, so a completion timestamp
    /// that stops advancing while the app is foregrounded means a poll hung
    /// mid-await — the wedge the root view's watchdog restarts the loop for.
    @ObservationIgnored public private(set) var lastPollCompletedAt = Date()

    @discardableResult
    public func loadFromBridge() async -> RemoteConnectionPollResult {
        defer { lastPollCompletedAt = Date() }
        let polledClient = client
        let polledConnectionEpoch = clientConnectionEpoch
        do {
            RefreshDiagnostics.log("poll start (relay=\(polledClient.isRelay))")
            let remote = try await requestBootstrap(using: polledClient)
            RefreshDiagnostics.log(
                "poll ok captured=\(remote.capturedAtUnixMs) sessions=\(remote.sessions.count)"
            )
            guard polledConnectionEpoch == clientConnectionEpoch,
                  polledClient.baseURL == client.baseURL,
                  polledClient.authToken == client.authToken,
                  polledClient.isRelay == client.isRelay
            else {
                RefreshDiagnostics.log("poll response DISCARDED (client generation changed)")
                return .superseded
            }
            // Every bootstrap carries the current `unpeel-host __remote__`
            // port + TLS pin (nil while that server is down). Renderers
            // re-read this before every WS (re)connect — the port is
            // OS-assigned per server run. No-op when unchanged.
            RemoteServerDiscovery.shared.update(
                port: remote.remoteServerPort,
                certificateFingerprint: remote.remoteServerCertificateFingerprint
            )
            // The poll answers every 2s with a fresh capturedAtUnixMs even
            // when nothing changed; publishing an identical fleet would
            // rebuild the whole sidebar (and hitch its open animation), so
            // only content changes are adopted.
            if !Self.snapshotContentEqual(snapshot, remote) {
                RefreshDiagnostics.log("snapshot APPLIED (\(remote.sessions.count) sessions)")
                snapshot = remote
                sidebarTree = IOSSidebarProjectTree(snapshot: remote)
                projectsByID = Dictionary(uniqueKeysWithValues: remote.projects.map { ($0.id, $0) })
            } else {
                RefreshDiagnostics.log("snapshot unchanged (skipped)")
            }
            if !supportsSessionCreation {
                presetDrawerProjectID = nil
            }
            // Forget locally-answered approvals once the Mac stops reporting
            // them, so the set can't grow unbounded.
            answeredApprovalIDs.formIntersection(
                Set((remote.pendingApprovals ?? []).map(\.id))
            )
            suppressNextConnectionError = false
            let directStatus = remote.macName.map { "Connected to \($0)" } ?? "Connected"
            setConnectionState(
                status: client.isRelay ? "Connected via Unpeel Remote" : directStatus,
                error: nil,
                disconnected: false
            )
            var selectionChanged = false
            if let pending = pendingReplacementSelection {
                // Never carry an in-memory effect intent across Host identity,
                // including a client swap whose first bootstrap is the only
                // place the stable Host id becomes known.
                if pending.hostMacID != remote.macID {
                    clearReplacementSelection(resetDefaultSuppression: true)
                } else {
                    switch Self.replacementSelectionResolution(
                        pending,
                        sessions: remote.sessions
                    ) {
                    case let .select(replacementID):
                        clearReplacementSelection(resetDefaultSuppression: true)
                        if selectedSessionID != replacementID {
                            selectedSessionID = replacementID
                            selectionChanged = true
                        }
                        restartError = nil
                        if let replacement = remote.sessions.first(where: {
                            $0.id == replacementID
                        }) {
                            revealProject(replacement.projectID)
                        }
                    case let .wait(updated):
                        pendingReplacementSelection = updated
                    case .cancel:
                        // Keep default fallback suppressed after collision or
                        // expiry; a later one-candidate snapshot cannot revive
                        // this canceled correlation and steal selection.
                        clearReplacementSelection(resetDefaultSuppression: false)
                    }
                }
            }
            // A push-tap deep link that wasn't loaded at tap time: select it
            // now that it's here, before the default-selection fallback runs.
            if let pending = pendingDeepLinkID {
                if remote.sessions.contains(where: {
                    $0.id == pending && $0.supportsIOSSessionAPI
                }) {
                    selectedSessionID = pending
                    pendingDeepLinkID = nil
                }
            }
            // First bootstrap after launch: reopen the last session the user
            // had open (if it still exists on the Mac), before the
            // default-selection fallback picks something else.
            if pendingReplacementSelection == nil,
               !replacementDefaultSelectionSuppressed,
               pendingDeepLinkID == nil,
               !hasLoadedBootstrapOnce,
               selectedSessionID.flatMap({ id in remote.sessions.first { $0.id == id && $0.supportsIOSSessionAPI } }) == nil,
               let saved = UserDefaults.standard.string(forKey: Self.lastSessionKey),
               remote.sessions.contains(where: { $0.id == saved && $0.supportsIOSSessionAPI }) {
                selectedSessionID = saved
            }
            let selectionIsValid = selectedSessionID.flatMap { id in
                remote.sessions.first { $0.id == id && $0.supportsIOSSessionAPI }
            } != nil
            if !selectionIsValid,
               (pendingReplacementSelection != nil || replacementDefaultSelectionSuppressed) {
                if selectedSessionID != nil {
                    selectedSessionID = nil
                    selectionChanged = true
                }
            } else if pendingReplacementSelection == nil,
                      !replacementDefaultSelectionSuppressed,
                      pendingDeepLinkID == nil,
                      !selectionIsValid {
                selectedSessionID = Self.defaultSelectedSessionID(in: remote)
                selectionChanged = true
            }
            let validProjectIDs = Set(remote.projects.map(\.id))
            let prunedProjectIDs = expandedProjectIDs.intersection(validProjectIDs)
            if prunedProjectIDs != expandedProjectIDs {
                expandedProjectIDs = prunedProjectIDs
            }
            if let expandedFolderID, !remote.folders.contains(where: { $0.id == expandedFolderID }) {
                self.expandedFolderID = Self.defaultExpandedFolderID(
                    in: remote,
                    selectedSessionID: selectedSessionID
                )
            }
            if selectionChanged {
                expandedFolderID = Self.defaultExpandedFolderID(
                    in: remote,
                    selectedSessionID: selectedSessionID
                )
            }
            if expandedProjectIDs.isEmpty, !hasLoadedBootstrapOnce || selectionChanged {
                expandedProjectIDs = Self.defaultExpandedProjectIDs(
                    in: remote,
                    selectedSessionID: selectedSessionID
                )
            }
            hasLoadedBootstrapOnce = true
            return .success(
                RemoteConnectionPollProof(
                    client: polledClient,
                    connectionEpoch: polledConnectionEpoch,
                    hostMacID: remote.macID
                )
            )
        } catch {
            RefreshDiagnostics.log("poll FAILED: \(error)")
            guard polledConnectionEpoch == clientConnectionEpoch,
                  polledClient.baseURL == client.baseURL,
                  polledClient.authToken == client.authToken,
                  polledClient.isRelay == client.isRelay
            else {
                RefreshDiagnostics.log("poll failure DISCARDED (client generation changed)")
                return .superseded
            }
            if suppressNextConnectionError {
                suppressNextConnectionError = false
                return .currentFailure
            }
            setConnectionState(
                status: "Connection lost",
                error: "Looking for your Mac…",
                disconnected: true
            )
            return .currentFailure
        }
    }

    /// Status/error flip on every poll answer; writing equal values would
    /// still notify observers, so they only land when they change.
    private func setConnectionState(status: String, error: String?, disconnected: Bool) {
        if connectionStatus != status { connectionStatus = status }
        if lastError != error { lastError = error }
        if isDisconnected != disconnected { isDisconnected = disconnected }
    }

    /// Equality of what the UI actually renders. Ignores `capturedAtUnixMs`
    /// (and the constant protocol version), which change on every poll, plus
    /// per-session churn the sidebar never shows: `lastOutputPreview` is
    /// rendered nowhere, and `updatedAtUnixMs` only at minute granularity —
    /// without this, any streaming agent forced a full snapshot publish (and
    /// sidebar tree rebuild) on every 2s tick. Internal for tests.
    static func snapshotContentEqual(
        _ a: RemoteBootstrapSnapshot,
        _ b: RemoteBootstrapSnapshot
    ) -> Bool {
        a.macID == b.macID
            && a.macName == b.macName
            && a.hostProtocol == b.hostProtocol
            && a.folders == b.folders
            && a.projects == b.projects
            && a.presets == b.presets
            && a.pendingApprovals == b.pendingApprovals
            && a.sessions.count == b.sessions.count
            && zip(a.sessions, b.sessions).allSatisfy(sessionRenderEqual)
    }

    private static func sessionRenderEqual(
        _ a: RemoteSessionSummary,
        _ b: RemoteSessionSummary
    ) -> Bool {
        a.id == b.id
            && a.projectID == b.projectID
            && a.activeRuntimeID == b.activeRuntimeID
            && a.runtimeLaunchPending == b.runtimeLaunchPending
            && a.providerID == b.providerID
            && a.title == b.title
            && a.command == b.command
            && a.createdAtUnixMs == b.createdAtUnixMs
            && minuteBucket(a.updatedAtUnixMs) == minuteBucket(b.updatedAtUnixMs)
            && a.status == b.status
            && a.activity == b.activity
            && a.unread == b.unread
            && a.pinned == b.pinned
            && a.notifyWhenDone == b.notifyWhenDone
            && a.worktreePath == b.worktreePath
            && a.worktreeBranch == b.worktreeBranch
            // OpenCode/Grok theme is resolved on the Mac and rides bootstrap;
            // must publish so chrome + ghostty default bg can hot-update.
            && a.terminalBackgroundHex == b.terminalBackgroundHex
    }

    private static func minuteBucket(_ unixMs: Int64?) -> Int64? {
        unixMs.map { $0 / 60_000 }
    }

    private static func defaultExpandedProjectIDs(
        in snapshot: RemoteBootstrapSnapshot,
        selectedSessionID: String?
    ) -> Set<String> {
        var ids = Set<String>()
        if let selected = selectedSessionID.flatMap({ id in snapshot.sessions.first { $0.id == id && $0.supportsIOSSessionAPI } }) {
            ids.formUnion(projectRevealIDs(for: selected.projectID, in: snapshot))
        }
        for session in snapshot.sessions where session.supportsIOSSessionAPI && session.status == .running && session.activity != .idle {
            ids.formUnion(projectRevealIDs(for: session.projectID, in: snapshot))
        }
        if ids.isEmpty, let firstSession = snapshot.sessions.first(where: \.supportsIOSSessionAPI) {
            ids.formUnion(projectRevealIDs(for: firstSession.projectID, in: snapshot))
        }
        return ids
    }

    private static func defaultSelectedSessionID(in snapshot: RemoteBootstrapSnapshot) -> String? {
        snapshot.sessions.first(where: { $0.supportsIOSSessionAPI && $0.activity == .blocked })?.id
            ?? snapshot.sessions.first(where: \.supportsIOSSessionAPI)?.id
    }

    private static func projectRevealIDs(
        for projectID: String,
        in snapshot: RemoteBootstrapSnapshot
    ) -> Set<String> {
        var ids: Set<String> = [projectID]
        if let parentID = inlineParentProjectID(forProjectID: projectID, in: snapshot) {
            ids.insert(parentID)
        }
        return ids
    }

    /// The folder a project surfaces under in the sidebar. Inline groups and
    /// worktrees live under their parent project's folder.
    private static func folderID(
        forProjectID projectID: String,
        in snapshot: RemoteBootstrapSnapshot
    ) -> String? {
        guard let project = snapshot.projects.first(where: { $0.id == projectID }) else {
            return nil
        }
        if let parentID = inlineParentProjectID(forProjectID: projectID, in: snapshot) {
            return snapshot.projects.first { $0.id == parentID }?.folderID
        }
        return project.folderID
    }

    private static func defaultExpandedFolderID(
        in snapshot: RemoteBootstrapSnapshot,
        selectedSessionID: String?
    ) -> String? {
        if let selected = selectedSessionID.flatMap({ id in
            snapshot.sessions.first { $0.id == id && $0.supportsIOSSessionAPI }
        }),
            let folderID = folderID(forProjectID: selected.projectID, in: snapshot) {
            return folderID
        }
        // No foldered selection: open the first folder so the drawer never
        // reads as an empty stack of collapsed headers.
        return snapshot.folders.first?.id
    }

    private static func inlineParentProjectID(
        forProjectID projectID: String,
        in snapshot: RemoteBootstrapSnapshot
    ) -> String? {
        guard let project = snapshot.projects.first(where: { $0.id == projectID }),
              project.isGroup == true || project.worktreeBranch != nil,
              let parentID = project.parentProjectID,
              snapshot.projects.contains(where: { $0.id == parentID })
        else { return nil }
        return parentID
    }

    private static func sequence(for key: RemoteKeyName) -> String {
        switch key {
        case .enter: return "\r"
        case .escape: return "\u{1B}"
        case .tab: return "\t"
        case .arrowUp: return "\u{1B}[A"
        case .arrowDown: return "\u{1B}[B"
        case .arrowRight: return "\u{1B}[C"
        case .arrowLeft: return "\u{1B}[D"
        case .controlC: return "\u{3}"
        case .controlD: return "\u{4}"
        case .controlZ: return "\u{1A}"
        }
    }

}

extension RemoteSessionSummary {
    /// Runtime identity for live presentation only. Lifecycle verbs continue
    /// to use the Host-advertised capabilities and stable launch command.
    var presentationProviderID: String? {
        guard status == .running,
              let activeRuntimeID,
              !activeRuntimeID.isEmpty
        else { return providerID }
        return UnpeelRuntimeCatalog.runtime(id: activeRuntimeID)?.legacySlug
            ?? activeRuntimeID
    }

    /// Every session the Mac ships is renderable on the phone: the mobile
    /// terminal renders any PTY, and verbs are gated by the Mac-computed
    /// `capabilities`, never by provider knowledge here. This used to be a
    /// hardcoded CLI allowlist, which silently dropped sessions of every CLI
    /// added after it was written (muse, cline, kiro) plus blank terminals
    /// and custom commands — the phone sidebar missing rows the desktop
    /// showed. Kept as a property because call sites read as intent
    /// ("listable session"), and a future gate would slot back in here.
    var supportsIOSSessionAPI: Bool { true }
}

extension RemotePresetSummary {
    /// Presets mirror the desktop "+" menu: the Mac's `availablePresets`
    /// already filters to installed/enabled CLIs (unknown-head custom
    /// commands deliberately included), so the phone adds no gate of its own
    /// — the old CLI allowlist hid presets for every newer CLI.
    var supportsIOSSessionAPI: Bool { true }

}

extension RemoteBootstrapSnapshot {
    static let empty = RemoteBootstrapSnapshot(
        macID: nil,
        macName: nil,
        folders: [],
        projects: [],
        presets: [],
        sessions: [],
        capturedAtUnixMs: 0
    )

    static let mock = RemoteBootstrapSnapshot(
        macID: "mac-studio",
        macName: "Tommy's Mac Studio",
        folders: [
            .init(id: "folder-product", name: "Product", colorID: "blue", sortOrder: 0),
            .init(id: "folder-client", name: "Client Work", colorID: "purple", sortOrder: 1),
        ],
        projects: [
            .init(id: "project-unpeel", name: "Unpeel", path: "~/Dev/unpeel", folderID: "folder-product", mcpBlocked: false, sortOrder: 0),
            .init(id: "project-site", name: "Website", path: "~/Dev/unpeel/apps/website", folderID: "folder-product", mcpBlocked: false, sortOrder: 1),
            .init(id: "project-shop", name: "Checkout Redesign", path: "~/Work/checkout", folderID: "folder-client", mcpBlocked: false, sortOrder: 2),
        ],
        presets: [
            .init(id: "claude", label: "Claude", command: "claude", cliID: "claude", quickLaunch: true, isDefault: true),
            .init(id: "codex", label: "Codex", command: "codex --dangerously-bypass-approvals-and-sandbox", cliID: "codex", quickLaunch: true, isDefault: true),
        ],
        sessions: [
            .init(
                id: "session-design-qa",
                projectID: "project-unpeel",
                providerID: "codex",
                title: "iOS remote shell",
                command: "codex",
                createdAtUnixMs: 1_789_996_800_000,
                updatedAtUnixMs: 1_789_996_960_000,
                status: .running,
                activity: .blocked,
                unread: true,
                pinned: true,
                worktreeBranch: "ios-remote",
                lastOutputPreview: "Permission needed for local-network bridge"
            ),
            .init(
                id: "session-terminal",
                projectID: "project-unpeel",
                providerID: nil,
                title: "Release notes",
                command: "",
                createdAtUnixMs: 1_789_996_300_000,
                updatedAtUnixMs: 1_789_996_820_000,
                status: .running,
                activity: .working,
                lastOutputPreview: "Rendering appcast diff..."
            ),
            .init(
                id: "session-copy",
                projectID: "project-site",
                providerID: "claude",
                title: "Home page polish",
                command: "claude",
                createdAtUnixMs: 1_789_995_900_000,
                updatedAtUnixMs: 1_789_996_200_000,
                status: .running,
                activity: .done,
                pinned: false,
                lastOutputPreview: "Done. Updated menu-bar section copy."
            ),
            .init(
                id: "session-checkout",
                projectID: "project-shop",
                providerID: "codex",
                title: "Checkout flow audit",
                command: "codex",
                createdAtUnixMs: 1_789_994_800_000,
                updatedAtUnixMs: 1_789_995_000_000,
                status: .exited,
                activity: .idle,
                lastOutputPreview: "Exited with summary"
            ),
        ],
        capturedAtUnixMs: 1_789_997_000_000
    )
}

/// Temporary sidebar-staleness diagnostics (2026-08-06): prints ride the
/// devicectl `--console` stream on debug installs; os_log survives for
/// `log collect`. Remove once the stale-sidebar wedge is root-caused.
enum RefreshDiagnostics {
    private static let fileURL = FileManager.default
        .urls(for: .documentDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("refresh-diag.log")

    static func log(_ message: String) {
        #if DEBUG
        let stamp = ISO8601DateFormatter().string(from: Date())
        let line = "[refresh-diag] \(stamp) \(message)\n"
        print(line, terminator: "")
        if let data = line.data(using: .utf8) {
            if let handle = try? FileHandle(forWritingTo: fileURL) {
                _ = try? handle.seekToEnd()
                try? handle.write(contentsOf: data)
                try? handle.close()
            } else {
                try? data.write(to: fileURL)
            }
        }
        #endif
    }
}
