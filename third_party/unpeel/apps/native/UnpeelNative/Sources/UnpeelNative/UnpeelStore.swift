//
//  UnpeelStore.swift
//  UnpeelNative
//
//  @MainActor store layer: reads projects from app-state.json, scans hosted
//  session manifests on a timer, derives busy state from output.bin growth,
//  and launches new sessions through unpeel-host.
//
//  Mostly read-only with respect to app-state.json; direct writes are limited
//  to shared settings the host must observe (`mcp_*`, `setup_completed`) plus
//  launch files (which the host deletes after reading).
//

import AppKit
import Combine
import CryptoKit
import Foundation
import SwiftUI
import UniformTypeIdentifiers
import UnpeelShared

/// UserDefaults keys that must be readable from nonisolated contexts too
/// (AdvancedDiagnostics.collect runs off the main actor).
enum NativeOverlay {
    /// [session id: custom title] — native renames. The Tauri app owns the
    /// manifest `label`/`custom_title`, so native renames live here and are
    /// merged over the manifest label at read time; entries are GC'd when
    /// the session dir disappears.
    static let sessionTitlesKey = "unpeel.native.sessionTitles"

    /// [Unpeel session id: provider conversation id] — captured from hook
    /// payloads (Claude forwards its `session_id`). Lets Restart resume the
    /// exact conversation via `--resume <id>` (see ResumeCommand). Entries are
    /// pruned with the session (pruneNativeState) and survive app restarts.
    static let providerSessionIDsKey = "unpeel.native.providerSessionIDs"

    /// [session id: pending context] — system context the user wants appended
    /// the next time the agent resumes. The live provider process cannot
    /// receive this after launch.
    static let appendedSystemContextsKey = "unpeel.native.appendedSystemContexts"

    /// [session id: restart recommendation token] — native dismissals for the
    /// compact restart bar. Tokens are stable capability/reason identifiers,
    /// so ordinary app rebuilds do not re-show dismissed recommendations.
    static let restartRecommendationDismissalsKey =
        "unpeel.native.restartRecommendationDismissals"

    /// [session id] — sessions the user opted into a "finished" push
    /// notification for (stored as an array of ids). Mac-side per-session flag,
    /// toggled from the desktop context menu or the phone's organize sheet;
    /// surfaced on `RemoteSessionSummary.notifyWhenDone` and read by the push
    /// dispatcher when a session settles. Pruned with the session.
    static let notifyWhenDoneKey = "unpeel.native.notifyWhenDone"

    /// [session id] — archived sessions (stored as an array of ids). Archive
    /// is the non-destructive "clear it out" verb: the hosted PTY is stopped
    /// but the session dir (manifest, output.bin, artifacts) stays on disk,
    /// so Resume brings the conversation back via ResumeCommand. Archived
    /// sessions stay out of the sidebar and are opened from the project's
    /// context menu in a dedicated main-pane list. GC'd when the session dir
    /// disappears; pruned on true removal.
    static let archivedSessionsKey = "unpeel.native.archivedSessions"

    /// [session id: ms epoch] — when the user explicitly archived the
    /// session. Drives the stopped-group ordering ("file it away" moves the
    /// row to the top of the stopped sessions). Auto-archived overflow is
    /// deliberately unstamped so it never resurfaces above genuinely recent
    /// rows. Pruned with the archived flag.
    static let archivedAtKey = "unpeel.native.archivedAt"
}

struct SessionRestartRecommendation: Equatable {
    enum Action: Equatable {
        case resumeAgent
        case reloadTerminal

        var label: String {
            switch self {
            case .resumeAgent: return "Resume Agent"
            case .reloadTerminal: return "Reload Terminal"
            }
        }
    }

    let token: String
    let message: String
    /// Nil is an informational recommendation: the intent is queued, but no
    /// safe immediate action exists while the managed runtime is active.
    let action: Action?
}

/// Failure returned by the synchronous Host CLI receipt used for an in-place
/// agent restart. Eligibility/concurrency conflicts are 409; helper launch,
/// transport, signal, PTY, and ambiguous post-submit failures are 500.
struct ResumeAgentHostCommandFailure: Equatable, Sendable {
    let status: Int
    let message: String
}

/// Archive listings are fetched independently from the live Host bootstrap.
/// Keep their summaries in a separate page-scoped cache so a normal live
/// bootstrap refresh cannot erase the source metadata needed by Restore &
/// Resume. Session ids are Host-global, but tracking ownership by requested
/// project lets a refreshed archive page replace its rows without retaining
/// stale summaries.
struct RemoteArchivedSessionSummaryCache {
    private(set) var summariesByID: [String: RemoteSessionSummary] = [:]
    private var sessionIDsByProject: [String: Set<String>] = [:]

    subscript(sessionID: String) -> RemoteSessionSummary? {
        summariesByID[sessionID]
    }

    var sessionIDs: Set<String> {
        Set(summariesByID.keys)
    }

    mutating func replaceProject(
        _ projectID: String,
        summaries: [RemoteSessionSummary]
    ) {
        for sessionID in sessionIDsByProject[projectID] ?? [] {
            summariesByID.removeValue(forKey: sessionID)
        }
        let sessionIDs = Set(summaries.map(\.id))
        sessionIDsByProject[projectID] = sessionIDs
        for summary in summaries {
            summariesByID[summary.id] = summary
        }
    }

    mutating func retainProjects(_ projectIDs: Set<String>) {
        let removedProjectIDs = sessionIDsByProject.keys.filter {
            !projectIDs.contains($0)
        }
        for projectID in removedProjectIDs {
            for sessionID in sessionIDsByProject.removeValue(forKey: projectID) ?? [] {
                summariesByID.removeValue(forKey: sessionID)
            }
        }
    }

    mutating func removeAll() {
        summariesByID.removeAll()
        sessionIDsByProject.removeAll()
    }
}

/// RAII lease for one cross-process lifecycle/marker flock. Replacement
/// restart work crosses suspension points and actors, so the descriptor must
/// stay owned until teardown and replacement launch have both completed.
final class NativeSessionFileLockLease: @unchecked Sendable {
    private let lock = NSLock()
    private var descriptor: Int32

    init(descriptor: Int32) {
        self.descriptor = descriptor
    }

    func release() {
        lock.withLock {
            guard descriptor >= 0 else { return }
            _ = flock(descriptor, LOCK_UN)
            close(descriptor)
            descriptor = -1
        }
    }

    deinit {
        release()
    }
}

struct NativeReplacementContextSnapshot: Equatable, Sendable {
    /// Nil means the exact derived state was "no valid marker". If corrupt
    /// bytes exist, the final exact comparison therefore rejects teardown.
    let raw: Data?
    let context: String?
}

/// Generation binding decided before a hook may mutate provider metadata,
/// activity, unread state, history, or push state. The accepted generation is
/// exact provenance when the hook carried it, or a conservative binding for a
/// legacy event after a proven current-generation turn opener / bounded
/// compatibility guard.
enum HookRuntimeDecision: Equatable {
    case reject
    case accept(effectiveGeneration: UInt64?)
}

/// Temporary phone-driven terminal size for a session: the desktop pane is
/// letterboxed to this grid so the Mac and the phone render the same cells.
/// In-memory only — closing the app (or the banner's X) reverts to full size.
struct PhoneResizeOverride: Equatable {
    let cols: Int
    let rows: Int
}

struct SidebarSessionRevealRequest: Equatable {
    let sessionID: String
    let serial: Int
    /// Center the row (jump-to-session) vs minimal scroll that just brings
    /// it into view (follow a row that moved, e.g. after pinning).
    let centered: Bool
}

/// Host-side presentation for one active pairing grant. A successful pair
/// consumes the credential, so the UI must stop advertising that code at the
/// same moment and show completion instead. Matching by token prevents a late
/// completion callback from clearing a newer code the user already generated.
struct HostPairingPresentation: Equatable {
    let payload: RemotePairingPayload?
    let code: String?
    let completed: Bool

    static let idle = HostPairingPresentation(payload: nil, code: nil, completed: false)

    static func active(_ payload: RemotePairingPayload) -> HostPairingPresentation {
        HostPairingPresentation(
            payload: payload,
            code: RemotePairingCode.encode(payload),
            completed: false
        )
    }

    func completing(token: String) -> HostPairingPresentation {
        guard payload?.token == token else { return self }
        return HostPairingPresentation(payload: nil, code: nil, completed: true)
    }
}

@MainActor
final class UnpeelStore: ObservableObject {
    @Published private(set) var nodes: [ProjectNode] = []
    @Published var selectedSessionID: String? {
        didSet {
            // Remote scope: the same property drives selection, but the
            // backend owns it — forward to the runtime and skip every local
            // bookkeeping path (MRU, prewarm, local unread observation). A
            // stale local id can never smuggle a local Session into a
            // remote-scoped workspace.
            if selectedHostScope != .local {
                if let id = selectedSessionID {
                    guard remoteSessionsByID[id] != nil else {
                        selectedSessionID = nil
                        return
                    }
                    launcherProjectID = nil
                    archivedProjectID = nil
                    recentActivityVisible = false
                    if remoteHostRuntime.selectedSessionID != id {
                        remoteHostRuntime.selectSession(id)
                    }
                }
                if selectedSessionID != oldValue { invalidateSidebarLists() }
                return
            }
            // Selecting a real session dismisses the main-screen launcher
            // (launching a launcher tile selects the new session, which is
            // exactly how the picker gives way to the terminal). Do this even
            // when the id was already selected: clicking the highlighted row
            // is also a natural way to close the archive library.
            if selectedSessionID != nil {
                launcherProjectID = nil
                archivedProjectID = nil
                recentActivityVisible = false
            }
            guard selectedSessionID != oldValue else { return }
            if let id = selectedSessionID { noteSessionMRU(id) }
            // The stopped-group window keeps the selected session's block
            // visible, so selection is a sidebar-list input.
            invalidateSidebarLists()
            handleObservationChanged()
            // Keep the session we just left warm: switching back to it is
            // the most common next switch (A↔B ping-pong), and a prewarmed
            // pane stays mounted + ticking in WarmPaneHostView instead of
            // pausing detached. Deferred one runloop turn so the swap
            // container detaches the pane first — WarmPaneHostView refuses
            // to adopt a pane another superview still owns.
            if let previous = oldValue {
                DispatchQueue.main.async { [weak self] in
                    self?.prewarmSession(previous)
                }
            }
        }
    }
    @Published private(set) var projectsByID: [String: Project] = [:]

    /// Sessions with an unread badge (7px #60a5fa dot after the title).
    /// Mirrors `unreadSessionIds` in sessionState.ts plus the mark/clear
    /// rules from App.svelte's hook-event listener and sessionUnread.ts.
    @Published private(set) var unreadSessionIDs: Set<String> = []

    /// Persisted history feed (newest-last) behind the Recent panel and the
    /// titlebar bell. Backed by `<UNPEEL_HOME>/activity-log.jsonl`.
    @Published private(set) var activityLogEntries: [ActivityLogEntry] = []
    private let activityLog = ActivityLogStore()
    /// Last session status this run logged from, so rebuildTree can log the
    /// live → exited edge exactly once — and never for sessions that were
    /// already exited when the app started.
    private var activityLoggedStatuses: [String: SessionStatus] = [:]

    /// Whether the "All recent" main-pane page (RecentActivityView) is
    /// showing — a content-pane swap like `archivedProjectID`, opened from
    /// the activity dropdowns' footer link. Deliberately not persisted.
    @Published var recentActivityVisible = false {
        didSet {
            if recentActivityVisible != oldValue { handleObservationChanged() }
        }
    }

    /// Sessions the user opted into a "finished" push notification for
    /// (`NativeOverlay.notifyWhenDoneKey`). Surfaced on
    /// `RemoteSessionSummary.notifyWhenDone` and read by the push dispatcher
    /// when a session settles (Stop). Persisted; pruned with the session.
    @Published private(set) var notifyWhenDoneSessionIDs: Set<String> =
        Set(AppDefaults.shared.stringArray(forKey: NativeOverlay.notifyWhenDoneKey) ?? [])

    /// Archived sessions (`NativeOverlay.archivedSessionsKey`): hidden from
    /// the regular sidebar lists (and the phone snapshot) and opened from the
    /// owning project's context menu in the main pane. Persisted; GC'd on
    /// rescan when the session dir is gone, pruned on removal.
    @Published private(set) var archivedSessionIDs: Set<String> =
        Set(AppDefaults.shared.stringArray(forKey: NativeOverlay.archivedSessionsKey) ?? []) {
        didSet { if archivedSessionIDs != oldValue { invalidateSidebarLists() } }
    }
    /// Deduplicates asynchronous stop/reap work, including recovery after an
    /// app interruption between persisting the archive flag and stopping the
    /// host.
    private var stoppingArchivedSessionIDs: Set<String> = []

    /// Archives whose live host is still shutting down: the sidebar keeps
    /// these rows visible — muted, with a spinner — until the stop finishes,
    /// instead of vanishing them mid-click. In-memory only (a relaunch's
    /// recovery stop needs no visible row).
    @Published private(set) var archivingSessionIDs: Set<String> = [] {
        didSet { if archivingSessionIDs != oldValue { invalidateSidebarLists() } }
    }

    /// When the user explicitly archived each session (ms epoch,
    /// `NativeOverlay.archivedAtKey`): a stamped row sorts to the top of the
    /// stopped group. Auto-archive paths never stamp. Pruned alongside the
    /// archived flag.
    private var archivedAtBySession: [String: Int64] =
        (AppDefaults.shared.dictionary(forKey: NativeOverlay.archivedAtKey) ?? [:])
            .compactMapValues { ($0 as? NSNumber)?.int64Value }

    /// Project ids whose session lists are expanded in the sidebar.
    /// Persisted (unlike the Svelte `expandedProjectIds` store, which reset
    /// to all-collapsed on every launch) so reopening the app restores which
    /// folders were open. Entries are pruned on project removal.
    @Published var expandedProjectIDs: Set<String> =
        Set(AppDefaults.shared.stringArray(forKey: UnpeelStore.expandedProjectsKey) ?? []) {
        didSet {
            guard expandedProjectIDs != oldValue else { return }
            if expandedProjectIDs.isEmpty, expandedProjectsStorageKey == Self.expandedProjectsKey {
                AppDefaults.shared.removeObject(forKey: expandedProjectsStorageKey)
            } else {
                // Per-Host keys persist an explicit empty array: "user collapsed
                // everything" must stay distinguishable from "never visited"
                // (which triggers the open-all-roots first-visit default).
                AppDefaults.shared.set(
                    Array(expandedProjectIDs), forKey: expandedProjectsStorageKey
                )
            }
        }
    }

    /// Where `expandedProjectIDs` persists: the shared local key, or a
    /// per-Host key while a remote Host is selected — expansion is
    /// Controller-local view state, remembered separately per Host.
    private var expandedProjectsStorageKey = UnpeelStore.expandedProjectsKey

    /// Native-only project id -> `ProjectFolderColor.rawValue`.
    @Published private(set) var projectFolderColorIDs: [String: String] = [:]

    /// Sidebar stopped-group truncation: active (live) sessions always all
    /// render; below them at most this many stopped/archived rows show,
    /// newest first. Older stopped rows auto-archive into the
    /// project's archive library (the context menu's "Archived (N)").
    /// Pinned rows don't count against the window.
    static let sidebarVisibleSessionLimit = 5

    /// Session ids pinned visible past the stopped-group window (and exempt
    /// from the overflow auto-archive) — set when the app must bring a
    /// hidden row back into the sidebar (restore from archive, reveal on
    /// select). In-memory only; pruned with the session on rescan.
    @Published private(set) var sidebarKeepVisibleSessionIDs: Set<String> = [] {
        didSet { if sidebarKeepVisibleSessionIDs != oldValue { invalidateSidebarLists() } }
    }

    /// Collapse-all's reset of the keep-visible pins (the per-project
    /// collapse clears its own inside `toggleProjectExpanded`).
    func clearSidebarKeepVisiblePins() {
        guard !sidebarKeepVisibleSessionIDs.isEmpty else { return }
        sidebarKeepVisibleSessionIDs = []
    }

    /// Explicit request for the sidebar to bring a session row into view.
    /// Selection alone is not enough because the target can be behind a
    /// collapsed project, a worktrees pane switch, or the "Show N more" cap.
    @Published private(set) var sidebarSessionRevealRequest: SidebarSessionRevealRequest?
    private var sidebarSessionRevealSerial = 0

    /// Sidebar collapsed (hidden) state. The Svelte app does not persist
    /// this; the native app does (requested), alongside unpeel.sidebar.width.
    @Published var sidebarCollapsed: Bool {
        didSet {
            AppDefaults.shared.set(sidebarCollapsed, forKey: Self.sidebarCollapsedKey)
        }
    }

    /// Whether to surface a session's attention badge when the host detects an
    /// agent-drawn select menu waiting for a choice (Claude/Codex numbered
    /// prompts, which fire no lifecycle hook). On by default; a rescan re-derives
    /// status when this flips, so toggling it applies live. See the host's
    /// `menu_prompt_active` manifest flag.
    @Published var menuAttentionDetectionEnabled: Bool {
        didSet {
            guard oldValue != menuAttentionDetectionEnabled else { return }
            AppDefaults.shared.set(
                menuAttentionDetectionEnabled, forKey: Self.menuAttentionDetectionKey
            )
            rescan()
        }
    }

    /// Whether sidebar session rows show their agent CLI's logo to the right
    /// of the date (Appearance ▸ "Show agent logos"). On by default. Pure
    /// display — flipping it re-renders via @Published, no rescan needed.
    @Published var showSessionToolIcons: Bool {
        didSet {
            guard oldValue != showSessionToolIcons else { return }
            AppDefaults.shared.set(showSessionToolIcons, forKey: Self.showSessionToolIconsKey)
        }
    }

    /// Whether the terminal title bar shows the session gallery chip and the
    /// Session ▸ Take Screenshot… flow that feeds it (Appearance ▸ "Session
    /// gallery"). Off by default — users with their own screenshot tooling
    /// keep a plain title bar. Desktop-only: the phone gallery and the
    /// artifact dirs it reads are unaffected.
    @Published var showSessionGallery: Bool {
        didSet {
            guard oldValue != showSessionGallery else { return }
            AppDefaults.shared.set(showSessionGallery, forKey: Self.showSessionGalleryKey)
        }
    }

    /// Whether the window is in macOS fullscreen. In fullscreen the traffic
    /// lights are hidden, so the titlebar toggle slides flush to the left
    /// edge instead of clearing the (now-absent) traffic lights. Driven by
    /// the window-delegate fullscreen notifications in `AppDelegate`.
    @Published var windowIsFullScreen = false

    /// When set, the main content area shows the session launcher (a
    /// pick-a-tool screen) for this project instead of a terminal/empty
    /// state. Driven by the Finder "New Unpeel Session Here" service and
    /// the empty state. Pure native UI state; never persisted. Cleared the
    /// moment a real session is selected (see `selectedSessionID`).
    @Published var launcherProjectID: String?

    /// Project whose archived-session library fills the main content area.
    /// Opened from the project row's context menu; selecting a session,
    /// opening Settings, or opening the launcher dismisses it.
    @Published var archivedProjectID: String? {
        didSet {
            guard archivedProjectID != oldValue else { return }
            // The Host archive endpoint is a page-scoped data source. Drop
            // the old page immediately so a late response cannot make an
            // archived row (or its resume metadata) actionable after close
            // or after switching projects.
            remoteArchivePageGeneration &+= 1
            remoteArchivedByProject = [:]
            remoteArchivedSummaryCache.removeAll()
            handleObservationChanged()
        }
    }

    /// Display pins per project id, newest-first, pruned to sessions that
    /// still exist. Merged from app-state.json (Tauri-owned, read-only) and
    /// native-side overrides in UserDefaults; native overrides win.
    @Published private(set) var pinnedByProject: [String: [PinnedSidebarSession]] = [:]

    /// Quick-preset strip contents (same for every project — the native app
    /// is global-presets-only): starred presets grouped by CLI in flat-list
    /// order. A single-preset group is a plain launch chip; 2+ starred
    /// presets of one CLI render as a dropdown chip.
    /// Refreshed on rescan, so edits to app-state.json show up promptly.
    @Published private(set) var quickPresetGroups: [QuickPresetGroup] = []

    /// Flattened quick-strip launch targets (each group's topmost preset),
    /// plus the blank-terminal preset — for single-click surfaces like ⌘N.
    @Published private(set) var quickPresets: [Preset] = [.newTerminal]

    /// All enabled global presets, in flat list order. Backs MCP
    /// `list_presets` (installed-filtering is a native UI preference; MCP
    /// callers see the real enabled set).
    @Published private(set) var enabledPresets: [Preset] = []

    /// Enabled presets whose CLI is installed (unknown-head custom commands
    /// always count), in flat list order. Backs the sidebar "+" new-session
    /// menu and the project context menu. Strict subset of `enabledPresets`.
    @Published private(set) var availablePresets: [Preset] = []

    /// Flat preset display order (preset ids, native UserDefaults overlay).
    /// Empty → app-state.json order. Ids missing from the saved order (new
    /// presets) append at the end; the order also decides each CLI's default
    /// preset (topmost enabled preset wins — see `defaultPreset(for:)`).
    /// Legacy: unread once `presetsInSharedFile` is true.
    @Published private(set) var presetOrder: [String] = []

    /// True once app-state.json carries the migrated preset truth (the
    /// `native_preset_overlay_migrated` marker): every preset mutator then
    /// writes the shared file — which the TUI edits too — instead of the
    /// legacy UserDefaults overlay. Set during rescan.
    private var presetsInSharedFile = false

    /// `code_editor` from app-state.json (state.rs default "code"), with a
    /// native UserDefaults overlay. Backs "Open in <Editor>" and the
    /// titlebar open button.
    @Published private(set) var codeEditor = "code"

    /// Effective appearance mode (Settings → Appearance). Merge rule like
    /// pins/presets: the native UserDefaults overlay wins; until the user
    /// picks a mode natively, this follows the Tauri app-state.json `theme`
    /// (refreshed on rescan). Drives NSApp.appearance — every dynamic
    /// Theme color, the vibrancy materials, and the Ghostty surfaces'
    /// light/dark configs follow from that.
    @Published private(set) var themePreference: ThemePreference = .system

    /// Per-session Unpeel Sessions MCP access overrides, keyed by session id.
    /// Read from app-state.json on rescan; written back by `setSessionAccess`.
    /// The host reads the same `mcp_orchestrators` field, so this is a direct
    /// (not overlay) setting. A session absent from the map uses the default
    /// grant (`McpGrant.default` — Member at project reach).
    @Published private(set) var mcpOrchestrators: [String: McpGrant] = [:]

    /// App-wide default Sessions MCP access for sessions without an explicit
    /// override (the `mcp_default_access` field). Read from / written back to
    /// app-state.json so the host honors it too.
    @Published private(set) var mcpDefaultAccess: SessionAccessLevel = .read

    /// App-wide policy for Sessions MCP writes outside the caller's group
    /// (`mcp_nonchild_write_access`). Read from / written back to
    /// app-state.json; the host re-reads it per tool call, so changes apply
    /// live. Same-group writes never consult it.
    @Published private(set) var mcpNonChildWriteAccess: McpNonChildWriteAccess = .ask

    /// User-approved cross-group write pairs (`mcp_write_approvals`), caller
    /// session id → approved target session ids. Written when the user answers
    /// the approval dialog (and revoked from Settings ▸ Sessions MCP); the host
    /// reads it per write. Pruned with sessions, re-pointed across restarts.
    @Published private(set) var mcpWriteApprovals: [String: [String]] = [:]

    /// Unified FIFO of pending ask-mode approvals (MCPApprovalCenter.swift):
    /// bridge requests blocked on an answer. Published because two surfaces
    /// render it — the floating desktop prompt panel and the paired-phone
    /// bootstrap snapshot; either may answer, first one wins. Mutated only by
    /// enqueueMcpApproval/answerMcpApproval (internal set for the same-module
    /// extension in MCPApprovalCenter.swift).
    @Published var pendingMcpApprovals: [PendingMcpApproval] = []
    /// Blocked bridge `reply` callbacks per pending approval id (coalesced
    /// duplicates append). Fired exactly once by answerMcpApproval.
    var mcpApprovalCompletions: [String: [(Bool) -> Void]] = [:]
    /// The floating non-modal prompt panel for the head of the queue.
    let mcpApprovalPanel = FloatingPromptPanelController()
    /// The floating panel for the computer-permissions (TCC) nudge.
    let computerNudgePanel = FloatingPromptPanelController()
    /// Missing-permission sets (sorted, "|"-joined) already alerted about this
    /// app run — the grant-prompt nudge fires once per distinct set.
    var shownComputerPermissionNudges: Set<String> = []

    /// App-wide Browser Access (the `browser_default_access` field). Defaults
    /// to on — the browser is an isolated per-session profile (no access to the
    /// user's logins) and agents already have full shell, so it adds
    /// visibility, not privilege. Settings ▸ Browser ▸ Off is the master
    /// disable. There is no per-session override; this is the single switch.
    @Published private(set) var browserDefaultAccess: BrowserAccess = .on

    /// App-wide Computer MCP access (`computer_default_access`) and the
    /// remembered per-session approvals (`computer_approvals`). The unified
    /// MCP server reads both from app-state.json per call, so changes apply
    /// live; the app owns the writes.
    @Published private(set) var computerDefaultAccess: ComputerAccess = .ask
    @Published private(set) var computerApprovals: [String] = []
    /// Remembered per-session browser approvals (`browser_approvals`), used
    /// only while `browserDefaultAccess == .ask`.
    @Published private(set) var browserApprovals: [String] = []
    /// Whether sessions may create Unpeel-managed worktrees
    /// (`mcp_worktree_access`, Settings ▸ Sessions use). Default off.
    @Published private(set) var mcpWorktreeAccess = false
    /// Whether Browser MCP screenshots are added to the Session gallery by
    /// default (`mcp_auto_add_browser_screenshots`). Default on to preserve
    /// the original gallery behavior.
    @Published private(set) var mcpAutoAddBrowserScreenshots = true

    /// Unpeel Link profile (`profile_display_name` / `profile_avatar`): the
    /// nickname and emoji avatar presence surfaces show for this person when
    /// several controllers share a Host. The TUI edits the same
    /// app-state.json keys, so writes go through the locked shared-file
    /// editor and edits from either frontend land in both.
    @Published private(set) var profileDisplayName = ""
    @Published private(set) var profileAvatar = ""

    /// App-wide Browser MCP engine options (`browser_settings`): window
    /// visibility, site rules, browsing-data mode, custom executable. The
    /// host reads them per tool call, so changes apply to the agent's next
    /// browser action without a restart.
    @Published private(set) var browserSettings = BrowserSettings()

    /// App-wide transcript rendering options (`transcript_settings`): which
    /// content types the Markdown transcript includes and how many entries.
    /// The host reads them from app-state.json when building the transcript for
    /// "Copy Transcript" and the Sessions MCP `read_transcript` tool, so changes
    /// apply on the next copy / read without a restart.
    @Published private(set) var transcriptSettings = TranscriptSettings()

    /// Live sessions that should be restarted to pick up a required
    /// `unpeel-host` protocol. Populated from manifest host_protocol_version
    /// during rescans; native dismissals hide individual recommendations.
    @Published private(set) var restartRecommendations: [String: SessionRestartRecommendation] =
        [:]

    /// Loopback URLs each live session currently serves as a browsable page,
    /// from the host's `detected_local_urls` manifest field (printed on the
    /// session's screen, then HTTP-probed live by the host; dead servers are
    /// removed host-side). Keyed by session id; the titlebar chip aggregates
    /// per project family via `localSiteURLs(forProjectFamilyOf:)`.
    @Published private(set) var detectedLocalURLs: [String: [String]] = [:]

    /// Display-layer verdicts for detected local-site URLs, keyed by URL.
    /// Session hosts are long-lived processes running whatever detection
    /// code they started with, so the chip re-verifies every manifest URL
    /// against the CURRENT probe rules (via `unpeel-host
    /// __check_local_url__`, keeping the "is this an openable site" logic
    /// single-sourced in Rust) before showing it.
    private var localURLVerdicts: [String: (ok: Bool, at: Date)] = [:]
    private var localURLChecksInFlight: Set<String> = []
    /// Bumped when a verdict lands so SwiftUI re-renders the chip.
    @Published private var localURLVerdictRevision = 0
    /// URLs already announced with a toast; cleared when the site goes down
    /// so a dev server restart announces again.
    private var announcedLocalURLs: Set<String> = []

    /// Current-rules verdict for one manifest URL, from the async cache. An
    /// unknown URL kicks a background probe and stays hidden until it
    /// passes; verdicts refresh every few seconds so a server that dies —
    /// or starts working — converges quickly.
    private func localURLVerdict(_ url: String) -> Bool {
        let cached = localURLVerdicts[url]
        let fresh = cached.map { Date().timeIntervalSince($0.at) < 5 } ?? false
        if !fresh, !localURLChecksInFlight.contains(url) {
            localURLChecksInFlight.insert(url)
            DispatchQueue.global(qos: .utility).async { [weak self] in
                let process = Process()
                process.executableURL = URL(fileURLWithPath: LaunchConfig.hostBinary)
                process.arguments = ["__check_local_url__", url]
                let pipe = Pipe()
                process.standardOutput = pipe
                process.standardError = FileHandle.nullDevice
                var ok = false
                if (try? process.run()) != nil {
                    let data = pipe.fileHandleForReading.readDataToEndOfFile()
                    process.waitUntilExit()
                    ok = String(data: data, encoding: .utf8)?
                        .contains("\ttrue") ?? false
                }
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.localURLVerdicts[url] = (ok, Date())
                    self.localURLChecksInFlight.remove(url)
                    self.localURLVerdictRevision &+= 1
                    self.announceLocalURLIfNew(url, ok: ok)
                }
            }
        }
        return cached?.ok ?? false
    }

    /// Toast the first time a local site verifies live (mirrors the phone
    /// "connected" toast; tap opens the site). Scoped to the currently shown
    /// session's project family so background projects don't spam, and
    /// re-armed when the site goes down so a dev-server restart announces
    /// again.
    private func announceLocalURLIfNew(_ url: String, ok: Bool) {
        guard ok else {
            announcedLocalURLs.remove(url)
            return
        }
        guard !announcedLocalURLs.contains(url),
              let session = selectedSession,
              localSiteURLs(forProjectFamilyOf: session.projectID).contains(url)
        else { return }
        announcedLocalURLs.insert(url)
        let compact = url
            .replacingOccurrences(of: "https://", with: "")
            .replacingOccurrences(of: "http://", with: "")
            .prefix(while: { $0 != "/" })
        ToastCenter.shared.show(
            "\(compact) is running — tap to open",
            systemImage: "globe"
        ) {
            LocalSiteMenu.open(url)
        }
    }

    /// Union of detected local-site URLs across every live session in the
    /// same project family (the top-level project plus its groups and
    /// worktrees) — the dev server usually runs in one session while the
    /// user watches another, so the chip is project-scoped, not
    /// session-scoped. Stable order: session creation time, then URL.
    func localSiteURLs(forProjectFamilyOf projectID: String) -> [String] {
        func rootID(_ id: String) -> String {
            var current = id
            var hops = 0
            while let parent = projectsByID[current]?.parentProjectID, hops < 8 {
                current = parent
                hops += 1
            }
            return current
        }
        let familyRoot = rootID(projectID)
        let members = detectedLocalURLs.compactMap { sessionID, urls -> (Int64, [String])? in
            guard let entry = sessionsByID[sessionID],
                  rootID(entry.projectID) == familyRoot
            else { return nil }
            return (entry.createdAt, urls)
        }
        // One row per server: group by origin and keep the URL closest to
        // the parent — a deep link survives only while no parent URL exists
        // for the same origin. Then each survivor must pass the
        // current-rules probe.
        var byOrigin: [(origin: String, url: String)] = []
        for (_, urls) in members.sorted(by: { $0.0 < $1.0 }) {
            for url in urls {
                guard let origin = Self.urlOrigin(url) else { continue }
                if let index = byOrigin.firstIndex(where: { $0.origin == origin }) {
                    if Self.urlPathLength(url) < Self.urlPathLength(byOrigin[index].url) {
                        byOrigin[index].url = url
                    }
                } else {
                    byOrigin.append((origin, url))
                }
            }
        }
        return byOrigin.map(\.url).filter { localURLVerdict($0) }
    }

    /// "http://localhost:5173/whatever" → "http://localhost:5173/".
    private static func urlOrigin(_ url: String) -> String? {
        guard let schemeEnd = url.range(of: "://") else { return nil }
        let rest = url[schemeEnd.upperBound...]
        let authority = rest.prefix(while: { $0 != "/" })
        return url[..<schemeEnd.upperBound] + authority + "/"
    }

    /// Path length after the authority; bare origin and "/" both count 0.
    private static func urlPathLength(_ url: String) -> Int {
        guard let schemeEnd = url.range(of: "://") else { return .max }
        let rest = url[schemeEnd.upperBound...]
        guard let slash = rest.firstIndex(of: "/") else { return 0 }
        return rest.distance(from: slash, to: rest.endIndex) - 1
    }

    /// Sessions whose restart-with-resume relaunch failed because the
    /// provider's conversation no longer exists on disk (e.g. Claude Code's
    /// auto-cleanup deleted the transcript) — the CLI printed its
    /// "conversation not found" error and exited to a bare shell. Keyed by the
    /// REPLACEMENT session id; drives ResumeFailedBar's one-click fresh
    /// relaunch. In-memory only: detection runs in the seconds after a
    /// restart, so it has nothing to survive an app relaunch.
    @Published private(set) var resumeFailures: Set<String> = []

    /// Post-restart output watchers behind `resumeFailures`, keyed by the
    /// replacement session id so removal/restart can cancel them.
    private var resumeFailureWatchers: [String: Task<Void, Never>] = [:]
    /// Same-Session Resume Agent can replace a watcher while its cancelled
    /// predecessor is still unwinding. Tokens keep the predecessor from
    /// removing or publishing results into the newer generation's watcher.
    private var resumeFailureWatcherTokens: [String: UUID] = [:]

    /// Sessions whose desktop terminal is temporarily letterboxed to a
    /// phone's grid (set over the mobile/dev bridge). Cleared by the
    /// terminal banner's X, by the phone, or when the session goes away.
    @Published private(set) var phoneResizeOverrides: [String: PhoneResizeOverride] = [:]

    /// Project ids explicitly blocked from MCP (app-state.json
    /// `mcp_blocked_projects`). Keyed by id so overlay-only/worktree projects
    /// are blockable too. `projectMcpBlocked` adds parent-chain inheritance.
    @Published private(set) var mcpBlockedProjectIDs: Set<String> = []

    /// Groups (project/group/worktree id) whose sessions sort by date
    /// (recently updated first) instead of the manual drag order —
    /// app-state.json `session_sort_modes`, shared with the TUI. Date sort
    /// disables drag re-ordering for the group; the stored manual order
    /// survives a switch back to custom.
    @Published private(set) var dateSortedProjectIDs: Set<String> = []

    /// Settings ▸ Advanced auto-stop-and-archive: sessions CONTINUOUSLY idle
    /// for this many minutes (0 = off) are stopped and archived in one motion
    /// — the same `archiveSession` verb as "Stop and archive" in the sidebar,
    /// so nothing is deleted and Restore + Restart resumes the conversation.
    /// Shared with the TUI via `auto_stop_archive_minutes` in app-state.json.
    @Published private(set) var autoStopArchiveMinutes = 0

    /// Settings open (App.svelte shellView kind 'settings'). Natively the
    /// app layout stays mounted: the sidebar list area slides to the
    /// settings nav (SettingsSidebarPanel) and the content pane swaps to
    /// the active settings panel (SettingsContentHost) — the retained
    /// Ghostty surfaces are never animated or hidden behind a fade.
    /// Opened by the footer gear and ⌘,. Toggling re-runs the unread
    /// reconciliation because the selected session stops being "observed"
    /// while settings covers the content pane.
    @Published var settingsVisible = false {
        didSet {
            if settingsVisible != oldValue { handleObservationChanged() }
        }
    }

    /// Active settings tab (App.svelte shellView.tab); defaults to the
    /// first tab in the nav.
    @Published var settingsTab: SettingsTab = .presets

    /// Keys of the experimental features (Settings ▸ Experimental) that are
    /// currently enabled. Seeded from the registry so an env override or a
    /// stored preference is reflected at launch; publishing it lets the
    /// sidebar's worktree gates re-evaluate live when a toggle flips.
    @Published private(set) var enabledExperimentalKeys: Set<String> =
        Set(ExperimentalFeature.all.filter { UnpeelFeatureFlags.isEnabled($0) }.map(\.key))

    /// This app's Host-side remote-control server. Mobile was its first
    /// Controller, so the shipped implementation and routes retain legacy
    /// mobile names; the app-facing model is generic Host/Controller pairing.
    /// It binds on the LAN and requires a per-device bearer token for every
    /// terminal endpoint. Pairing codes are short-lived and one-time.
    @Published private(set) var hostServerEndpoint: URL?
    @Published private(set) var hostServerError: String?
    @Published private(set) var hostPairingPresentation = HostPairingPresentation.idle
    @Published private(set) var pairedControllers: [RemotePairedDeviceSummary] = []
    private var hostRemoteServer: MobileRemoteServer?
    /// A TUI may still own the persisted phone endpoint when this app first
    /// appears. Retry asynchronously while its sidebar poll notices us and
    /// releases the exact port; never replace the Controller's saved URL.
    private var hostRemoteServerRetryTask: Task<Void, Never>?

    var hostPairingPayload: RemotePairingPayload? { hostPairingPresentation.payload }
    var hostPairingCode: String? { hostPairingPresentation.code }
    var hostPairingCompleted: Bool { hostPairingPresentation.completed }

    /// Merged (file + native overlay) presets for the editor — including
    /// disabled ones, unlike `enabledPresets`.
    @Published private(set) var mergedPresets: [Preset] = []

    /// Legacy `setup_completed` from app-state.json. The onboarding wizard is
    /// gone (first run boots straight into the main UI with builtin presets
    /// seeded and every superpower on by default); the flag survives only to
    /// keep the legacy-preference migration and usage seeding one-shot for
    /// users who completed the old wizard.
    @Published private(set) var setupCompleted = false
    @Published private(set) var setupToolReport: ToolScanReport?
    /// True while a PATH scan is running — drives the Presets panel's
    /// Rescan button spinner.
    @Published private(set) var toolScanInProgress = false

    /// CLIs the Agent CLI Tools window's Install button is currently
    /// installing, and the last failure message per CLI (cleared on retry).
    @Published private(set) var toolInstallsInProgress: Set<SetupTool> = []
    @Published private(set) var toolInstallErrors: [SetupTool: String] = [:]

    /// Background AI-tool PATH scan (aiTools.ts isPresetAvailable parity).
    private let toolAvailability = ToolAvailability()

    /// Sessions indexed by id (flattened from nodes) for cheap lookup.
    private(set) var sessionsByID: [String: SessionEntry] = [:]

    /// Always-on safety-net rescan (5s). File events drive normal updates;
    /// this catches killed hosts (whose manifests never get a final write)
    /// and any missed/coalesced FSEvents.
    private var safetyTimer: Timer?
    /// 1s sweep that runs ONLY while some session is busy: it expires the
    /// 2.5s output-growth busy window and the 5-minute hook-busy deadline
    /// with the same timing the old always-on 1s timer had. No file event
    /// fires when output STOPS growing, so this cannot be event-driven.
    private var busySweepTimer: Timer?
    /// FSEvents stream over app-sessions + app-state.json
    /// (file-level events, 0.5s coalescing).
    private var fsEventStream: FSEventStreamRef?
    private var watchedPaths: [String] = []
    private var pendingRescanWork: DispatchWorkItem?
    private var pendingRescanDeadline: Date?
    /// Semantic content of the last activity-state.json write, so unchanged
    /// snapshots skip the disk write entirely.
    private var lastActivitySnapshotSignature: [String: String] = [:]
    /// Inputs of the last completed rescan, kept so overlay-only changes
    /// (drag-reorder) can rebuild the tree without re-hitting disk.
    private var lastScanProjects: [Project] = []
    private var lastScanSessions: [SessionEntry] = []
    private var lastScanTauriPins: [String: [PinnedSidebarSession]] = [:]
    private var hasCompletedScan = false
    /// Final combined pin + regular order being previewed by an in-flight
    /// desktop session drag. This deliberately never reaches UserDefaults or
    /// session-order.json; a successful drop commits it once, while a
    /// cancelled drag removes it and rebuilds from durable state.
    private var sessionOrderPreviews: [String: [String]] = [:]
    /// Sibling order being previewed by an in-flight desktop project/worktree
    /// drag (one at a time), keyed by parent (nil = top-level). Same contract
    /// as `sessionOrderPreviews`: never persisted; a successful drop commits
    /// it once, a cancelled drag removes it and rebuilds from durable state.
    /// `draggedID` identifies the moved project so a remote-scope commit can
    /// send the one-project `project.organization.set` patch.
    private var projectOrderPreview: (parentID: String?, ids: [String], draggedID: String)?
    /// While any NSMenu is tracking (context menus, SwiftUI Menu dropdowns,
    /// the menu bar), rescans park here: a store publish mid-track makes
    /// SwiftUI rebuild the open menu's items, which visibly blinks flyout
    /// submenus. The UI shows a frozen snapshot for the few seconds a menu
    /// is open; the deferred rescan runs the moment tracking ends.
    private var menuTrackingDepth = 0
    private var rescanDeferredForMenuTracking = false
    /// Per-project memo of the sidebar's rendered row lists; see
    /// `sidebarLists(in:)`. Cleared whenever an input changes.
    private var sidebarListsCache: [String: (pinned: [SessionEntry], displayed: [SessionEntry])] = [:]

    /// output.bin size + last time it grew, per session (busy heuristic).
    private var outputSizes: [String: (size: UInt64, grewAt: Date)] = [:]
    /// Last runtime launch generation observed per live Session. A generation
    /// edge invalidates hook activity from the preceding agent process while
    /// keeping the same Session and terminal identity.
    private var runtimeLaunchGenerations: [String: UInt64] = [:]
    /// Launch boundary for the latest observed in-place runtime generation.
    /// Hook receipt happens off-main, so this also rejects an old queued Stop
    /// after the transient restart-in-flight flag has cleared.
    private var runtimeLaunchCutoffs: [String: Date] = [:]
    /// Compatibility bound for hook assets installed by an older build. A
    /// legacy Stop is quarantined immediately after an in-place edge, but may
    /// settle after this window if the old provider never emits a recognized
    /// Start/UserPromptSubmit. Exact generation tags remain authoritative.
    private nonisolated static let legacyGenerationStopGuard: TimeInterval = 30
    /// Sessions launched by this app before their host writes manifest.json.
    /// These are UI-only rows; the manifest-backed entry replaces them as
    /// soon as rescan sees the real session on disk.
    private var pendingSessions: [String: SessionEntry] = [:]

    // MARK: Decode caches (mtime+size gated; skip JSON work when unchanged)

    /// (mtime, size) fingerprint of a file as of the last decode.
    private struct FileStamp: Equatable {
        var mtimeSec: Int
        var mtimeNsec: Int
        var size: Int64
    }

    /// manifest.json decode cache keyed by session dir name. `manifest` is
    /// nil when the last read failed to decode (torn write); a finished
    /// write changes mtime/size and re-triggers the decode.
    private var manifestCache: [String: (stamp: FileStamp, manifest: HostedSessionManifest?)] = [:]
    private var appStateCache: (stamp: FileStamp, file: AppStateFile?)?

    /// Single `stat(2)` call: much cheaper than FileManager.attributesOfItem
    /// (which builds a full NSDictionary incl. xattrs — it dominated the
    /// idle CPU profile at one rescan per second over ~57 session dirs).
    private static func statFile(_ path: String) -> FileStamp? {
        var st = stat()
        guard stat(path, &st) == 0 else { return nil }
        return FileStamp(
            mtimeSec: Int(st.st_mtimespec.tv_sec),
            mtimeNsec: Int(st.st_mtimespec.tv_nsec),
            size: Int64(st.st_size)
        )
    }

    private static func unixMilliseconds(for stamp: FileStamp) -> UInt64? {
        guard stamp.mtimeSec >= 0, stamp.mtimeNsec >= 0 else { return nil }
        return UInt64(stamp.mtimeSec) * 1_000
            + UInt64(stamp.mtimeNsec) / 1_000_000
    }

    private static func jsonUInt64(_ value: Any?) -> UInt64? {
        guard let number = value as? NSNumber else { return nil }
        let signed = number.int64Value
        return signed >= 0 ? UInt64(signed) : nil
    }

    struct SharedTitleMarker: Equatable {
        let title: String?
        /// The writer's durable ordering timestamp. Markers from old builds
        /// fall back to their file mtime when decoded below.
        let updatedAt: UInt64?
    }

    /// title.json marker decode cache, same (mtime, size) gating — rescan
    /// consults the marker for every session, so the unchanged case must cost
    /// a stat, not a read + parse.
    private var titleMarkerCache: [
        String: (stamp: FileStamp, marker: SharedTitleMarker?)
    ] = [:]

    private func titleMarkerValue(
        sessionID: String,
        dirPath: String
    ) -> SharedTitleMarker? {
        let path = dirPath + "/" + SharedMarker.title.rawValue
        guard let stamp = Self.statFile(path) else {
            titleMarkerCache[sessionID] = nil
            return nil
        }
        if let cached = titleMarkerCache[sessionID], cached.stamp == stamp {
            return cached.marker
        }
        let object = FileManager.default.contents(atPath: path)
            .flatMap { (try? JSONSerialization.jsonObject(with: $0)) as? [String: Any] }
        let marker = object.map {
            SharedTitleMarker(
                title: $0["title"] as? String,
                updatedAt: Self.jsonUInt64($0["updated_at"])
                    ?? Self.unixMilliseconds(for: stamp)
            )
        }
        titleMarkerCache[sessionID] = (stamp, marker)
        return marker
    }

    /// project-override.json marker decode cache, same (mtime, size) gating
    /// as the title marker — rescan consults it for every session.
    private var projectOverrideCache: [String: (stamp: FileStamp, id: String?)] = [:]

    private func projectOverrideValue(sessionID: String, dirPath: String) -> String? {
        let path = dirPath + "/" + SharedMarker.projectOverride.rawValue
        guard let stamp = Self.statFile(path) else {
            projectOverrideCache[sessionID] = nil
            return nil
        }
        if let cached = projectOverrideCache[sessionID], cached.stamp == stamp {
            return cached.id
        }
        let id = FileManager.default.contents(atPath: path)
            .flatMap { (try? JSONSerialization.jsonObject(with: $0)) as? [String: Any] }
            .flatMap { $0["project_id"] as? String }
        projectOverrideCache[sessionID] = (stamp, id)
        return id
    }

    /// Re-seed the in-memory hook latch from the last lifecycle event the
    /// session's hook scripts persisted to disk (last-hook-event.json). Hook
    /// scripts keep firing while no app instance is listening, so after an app
    /// restart this restores busy/attention for sessions that were mid-turn —
    /// and correctly stays idle when the turn finished while the app was
    /// closed.
    private func seedHookActivity(
        sessionID: String,
        dirPath: String,
        runtimeGeneration: UInt64,
        runtimeLaunchedAt: Date?,
        anchorStartEventToOutput: Bool = true
    ) {
        let path = dirPath + "/last-hook-event.json"
        guard let stamp = Self.statFile(path),
              let data = FileManager.default.contents(atPath: path),
              let event = LastHookEvent.parse(data)
        else { return }
        let receivedAt = Self.stampDate(stamp)
        let decision = Self.hookRuntimeDecision(
            eventGeneration: event.runtimeGeneration,
            hookEventName: event.hookEventName,
            receivedAt: receivedAt,
            currentGeneration: runtimeGeneration,
            runtimeLaunchedAt: runtimeLaunchedAt,
            currentGenerationOwned: activity.hasRuntimeOwnership(
                sessionID, generation: runtimeGeneration
            )
        )
        guard case let .accept(effectiveGeneration) = decision else { return }
        var seedAt = receivedAt
        // Turns routinely run longer than the 5-minute hook idle timeout, so
        // a busy seed anchored at the event's own mtime would expire on the
        // first sweep for any long turn. While the agent works, the hosted
        // PTY keeps appending streamed output to output.bin — for an open
        // turn (Start/UserPromptSubmit with no Stop recorded after it), a
        // fresh output.bin means "still working right now", so anchor the
        // deadline at whichever timestamp is fresher. A recorded Stop always
        // wins: idle sessions stay idle no matter how the TUI repaints, and
        // a stale open turn (agent died mid-turn) still expires on the first
        // sweep because both timestamps are old.
        if event.shouldAnchorSeedToOutput(anchorStartEventToOutput: anchorStartEventToOutput),
           let outputStamp = Self.statFile(dirPath + "/output.bin") {
            seedAt = max(seedAt, Self.stampDate(outputStamp))
        }
        activity.applyHookEvent(
            sessionID: sessionID,
            hookEventName: event.hookEventName,
            latchOnly: event.latchOnly,
            runtimeGeneration: effectiveGeneration,
            now: seedAt
        )
    }

    private static func stampDate(_ stamp: FileStamp) -> Date {
        Date(timeIntervalSince1970:
            TimeInterval(stamp.mtimeSec) + TimeInterval(stamp.mtimeNsec) / 1_000_000_000)
    }

    /// Maintenance compatibility floor. Bounded journaling is adopted when a
    /// Host is naturally replaced; a healthy v2/v3 Host must not recommend a
    /// disruptive reload merely to reclaim its existing terminal journal.
    private static let requiredSessionHostProtocolVersion = 2

    /// How recently output.bin must have grown for "busy".
    private let busyWindow: TimeInterval = 2.5

    /// Wall-clock of the last ordinary keystroke the user typed into the
    /// observed session. Output that closely trails typing is keystroke echo,
    /// not agent work, so the output heuristic must not read it as busy
    /// (input-aware suppression). Only the observed session can be typed into,
    /// so background sessions are never affected.
    private var lastUserInputAt: [String: Date] = [:]
    /// How long after a keystroke output growth is treated as echo, not work.
    private let inputEchoWindow: TimeInterval = 2.5

    // MARK: Hook-driven activity (session_activity.rs / sessionState.ts)

    /// Hook latch + busy/idle/attention per session. Once a session has an
    /// entry here, the output-growth heuristic above stops driving its state.
    private let activity = SessionActivityEngine()

    /// Hook server owned by the app delegate; attached after init so the
    /// preset self-test's throwaway `UnpeelStore()` never starts one.
    private(set) var hookServer: HookServer?

    /// Sessions whose last hook event was Stop (completedSessionIds in
    /// sessionState.ts) — feeds the pending-unread reconciliation.
    private var completedSessionIDs: Set<String> = []

    /// Irreversible Stop effects wait briefly for a possible cross-process
    /// Resume Agent generation edge. Activity/completion updates stay
    /// immediate; history, unread, and pushes publish only if the manifest
    /// generation is unchanged.
    private struct DeferredStopEffects {
        let token: UUID
        let runtimeGeneration: UInt64?
        let task: Task<Void, Never>
    }
    private var deferredStopEffects: [String: DeferredStopEffects] = [:]
    private static let deferredStopEffectDelay: UInt64 = 3_000_000_000

    /// Busy/attention sessions the user switched away from; they become
    /// unread when they settle (sessionUnread.ts pendingUnreadSessions).
    private var pendingUnreadSessions: Set<String> = []

    /// Sessions whose current `menu_prompt_active` flag the user dismissed
    /// ("Clear attention" in the sidebar context menu). The host's flag is
    /// level-held while the detected menu stays on screen, so a plain clear
    /// would re-badge on the next rescan; this set suppresses the override
    /// until the host lowers the flag, which re-arms detection for the next
    /// real menu. In-memory only — a stale false positive shouldn't survive
    /// an app relaunch as a dismissal.
    private var menuAttentionDismissals: Set<String> = []
    /// Raw host menu state, kept separately from the derived attention badge so
    /// only a false -> true edge emits a notification. The runtime generation
    /// is part of the identity: an in-place agent restart resets the host flag,
    /// and a fast new prompt must still re-arm even if this app missed the
    /// intermediate false manifest write.
    private var menuPromptNotificationStates: [String: MenuPromptNotificationState] = [:]
    /// `scanSessions` discovers menu edges before `rebuildTree` publishes the
    /// matching SessionEntry. Stage them by generation and deliver immediately
    /// after the rebuilt index is available.
    private var pendingMenuPromptNotifications: [String: UInt64] = [:]
    private var previousObservedSessionID: String?
    private var appActivationObservers: [NSObjectProtocol] = []

    /// The picker scopes the whole workspace. Local state remains loaded so
    /// switching back is instant, but every remote surface/verb must use the
    /// remote backend and the spawn boundary below refuses local execution.
    @Published private(set) var selectedHostScope: SelectedHostScope = .local
    let localHostID: String
    let remoteHostStore: RemoteHostStore
    let remoteHostRuntime = RemoteHostRuntime()
    private var localSelectedSessionIDBeforeRemote: String?

    // MARK: Remote-scope display projection
    //
    // Local truth (`nodes`, `sessionsByID`, `projectsByID`, presets, pins,
    // unread/archived sets) always tracks THIS Mac — it also feeds the
    // /mobile Host serving path, which keeps serving paired phones while a
    // remote Host is selected. Remote scope projects the selected Host's
    // bootstrap into these parallel structures, and the views read the
    // `display*` accessors so the SAME sidebar/content hierarchy renders
    // either source. Nothing here is ever persisted locally.
    @Published private(set) var remoteNodes: [ProjectNode] = []
    @Published private(set) var remoteSessionsByID: [String: SessionEntry] = [:]
    @Published private(set) var remoteProjectsByID: [String: Project] = [:]
    @Published private(set) var remoteArchivedByProject: [String: [SessionEntry]] = [:]
    private(set) var remoteSummariesByID: [String: RemoteSessionSummary] = [:]
    private var remoteArchivedSummaryCache = RemoteArchivedSessionSummaryCache()
    private var remoteArchivePageGeneration: UInt64 = 0
    private var remoteProjectSummariesByID: [String: RemoteProjectSummary] = [:]
    private var remoteSessionOrderByProject: [String: [String]] = [:]
    private var remotePresetSummaries: [RemotePresetSummary] = []
    private var remotePresets: [Preset] = []
    private var remoteQuickPresetGroups: [QuickPresetGroup] = []
    /// Host key whose root projects were auto-expanded once per selection, so
    /// entering a remote Host never lands on an all-collapsed tree.
    private var remoteAutoExpandedHostKey: String?
    /// Session id whose project chain was last auto-revealed by the remote
    /// projection. Reveal must fire only when the selection changes — the
    /// projection also re-runs on every drag-preview hover, and re-expanding
    /// a deliberately collapsed project mid-drag pops it open under the cursor.
    private var remoteRevealedSelectionID: String?
    /// Optimistic sibling order held after a remote reorder commit until a
    /// bootstrap confirms it, the verb fails, or the hold expires. Without
    /// it, a periodic bootstrap captured BEFORE the Host applied the write
    /// can land right after the drop and visibly snap the drag back.
    private var remoteCommittedOrderHold:
        (parentID: String?, ids: [String], heldAt: Date)?
    private var remoteScopeCancellables: Set<AnyCancellable> = []

    static let sidebarCollapsedKey = "unpeel.sidebar.collapsed"
    static let expandedProjectsKey = "unpeel.native.expandedProjects"
    static let menuAttentionDetectionKey = "unpeel.native.menuAttentionDetection"
    static let showSessionToolIconsKey = "unpeel.native.showSessionToolIcons"
    static let showSessionGalleryKey = "unpeel.native.showSessionGallery"
    private static let nativePinsKey = "unpeel.sidebar.pins"
    private static let nativePendingTitleWritesKey = "unpeel.native.pendingTitleWrites"
    static let nativePresetsKey = "unpeel.native.presets"
    private static let nativeThemeKey = "unpeel.native.theme"
    private nonisolated static let nativeCodeEditorKey = "unpeel.native.codeEditor"
    // Legacy cleanup keys (pre auto-stop-and-archive merge). The stop-minutes
    // value is folded once into app-state.json's `auto_stop_archive_minutes`
    // (the shared truth the TUI also reads); the cleanup-days setting was
    // removed outright.
    private static let legacyAutoSessionCleanupDaysKey = "unpeel.native.autoSessionCleanupDays"
    private static let legacyAutoSessionStopMinutesKey = "unpeel.native.autoSessionStopMinutes"
    static let nativePresetOrderKey = "unpeel.native.presetOrder"
    // Legacy per-CLI preference keys (pre flat-preset-list). Read once by
    // `migrateCLIPreferencesIfNeeded`; left in place afterwards so older
    // builds sharing the defaults suite keep working.
    private static let legacyCLIAvailabilityKey = "unpeel.native.cliAvailability"
    private static let legacyCLIDefaultsKey = "unpeel.native.cliDefaults"
    private static let legacyCLIOrderKey = "unpeel.native.cliOrder"
    private static let nativeProjectFolderColorsKey = "unpeel.native.projectFolderColors"

    static let autoStopArchiveMinuteOptions = [0, 30, 60, 120, 240, 480, 1440]

    /// Opt-out default: a day of unbroken idleness before the terminal is
    /// stopped and archived. Safe because archive is non-destructive
    /// (Restore + Restart resumes the conversation) and plain shells are
    /// exempt entirely.
    static let defaultAutoStopArchiveMinutes = 1440

    static func autoStopArchiveLabel(for minutes: Int) -> String {
        switch minutes {
        case 0: return "Never"
        case ..<60: return "After \(minutes) minutes"
        case 60: return "After 1 hour"
        case 1440: return "After 1 day"
        default: return "After \(minutes / 60) hours"
        }
    }

    init() {
        localHostID = MobilePairingStore.defaultMacID()
        remoteHostStore = RemoteHostStore(localHostID: localHostID)
        sidebarCollapsed = AppDefaults.shared.bool(forKey: Self.sidebarCollapsedKey)
        // Absent = on (opt-out feature); an explicit stored value wins.
        menuAttentionDetectionEnabled = AppDefaults.shared
            .object(forKey: Self.menuAttentionDetectionKey) == nil
            ? true
            : AppDefaults.shared.bool(forKey: Self.menuAttentionDetectionKey)
        // Absent = on (opt-out); explicit stored value wins so an existing
        // user who turned it off stays off after the default flip.
        showSessionToolIcons = AppDefaults.shared.object(forKey: Self.showSessionToolIconsKey) == nil
            ? true
            : AppDefaults.shared.bool(forKey: Self.showSessionToolIconsKey)
        showSessionGallery = AppDefaults.shared.bool(forKey: Self.showSessionGalleryKey)
        projectFolderColorIDs = Self.loadProjectFolderColorIDs()
        Self.migrateAutoStopArchiveSetting()
        activityLogEntries = activityLog.entries
        presetOrder = Self.loadPresetOrder()
        refreshToolAvailability()
        migrateAwayFromPerSessionMediaAccess()
        rescan()
        Self.compactStoppedOutputJournals()
        // Remote-scope projection inputs. The sinks no-op while Local is
        // selected; scope entry re-projects explicitly.
        // Delivered on the next main-loop tick (never at willSet time), so
        // the runtime's own state is settled when the projection reads it.
        remoteHostRuntime.$snapshot
            .receive(on: DispatchQueue.main)
            .sink { [weak self] snapshot in
                guard let self, self.selectedHostScope != .local else { return }
                self.projectRemoteScope(snapshot: snapshot)
            }
            .store(in: &remoteScopeCancellables)
        remoteHostRuntime.$selectedSessionID
            .receive(on: DispatchQueue.main)
            .sink { [weak self] sessionID in
                guard let self, self.selectedHostScope != .local else { return }
                if self.selectedSessionID != sessionID {
                    self.selectedSessionID = sessionID
                }
            }
            .store(in: &remoteScopeCancellables)
        // Connection-state changes repaint the host button and banners.
        remoteHostRuntime.$connectionState
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in
                guard let self, self.selectedHostScope != .local else { return }
                self.objectWillChange.send()
            }
            .store(in: &remoteScopeCancellables)
        if RemoteHostFeature.pickerEnabled,
           let hostID = remoteHostStore.selectedHostID,
           let host = remoteHostStore.records.first(where: { $0.hostID == hostID }),
           let credentials = remoteHostStore.credentials(for: hostID) {
            localSelectedSessionIDBeforeRemote = selectedSessionID
            selectedSessionID = nil
            selectedHostScope = .remote(hostID: hostID)
            connectRemoteHost(host, credentials: credentials)
            projectRemoteScope(snapshot: remoteHostRuntime.snapshot)
        }
        // Rescans are normally event-driven (FSEvents on app-sessions and
        // the preset files, set up by rescan() above). The 5s timer is a
        // safety net for killed hosts and missed events; a separate 1s sweep
        // runs only while some session is busy (see updateBusySweepTimer).
        let timer = Timer(timeInterval: 5.0, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.rescan() }
        }
        RunLoop.main.add(timer, forMode: .common)
        safetyTimer = timer

        // Window focus gates the "observed" session (sessionUnread.ts
        // getObservedWorkspaceSessionId: documentVisible && windowFocused).
        for name in [NSApplication.didBecomeActiveNotification,
                     NSApplication.didResignActiveNotification] {
            appActivationObservers.append(
                NotificationCenter.default.addObserver(
                    forName: name, object: nil, queue: .main
                ) { [weak self] _ in
                    Task { @MainActor in self?.handleObservationChanged() }
                }
            )
        }

        // A phone streaming a session's output clears its unread badge — the
        // remote counterpart of the desktop "observed session" gate.
        appActivationObservers.append(
            NotificationCenter.default.addObserver(
                forName: .unpeelMobileViewerObservedSession, object: nil, queue: .main
            ) { [weak self] note in
                guard let sessionID = note.userInfo?["sessionID"] as? String else { return }
                Task { @MainActor in self?.clearUnreadFromRemoteViewer(sessionID) }
            }
        )

        // Menu tracking gates rescans (see menuTrackingDepth). Depth-counted:
        // AppKit can post begin/end per menu in a tracking session, and the
        // sweep/safety timers run in .common mode so they DO fire mid-track.
        for (name, delta) in [(NSMenu.didBeginTrackingNotification, 1),
                              (NSMenu.didEndTrackingNotification, -1)] {
            appActivationObservers.append(
                NotificationCenter.default.addObserver(
                    forName: name, object: nil, queue: .main
                ) { [weak self] _ in
                    MainActor.assumeIsolated { self?.applyMenuTracking(delta) }
                }
            )
        }
    }

    /// Upgrade maintenance: old builds retained every terminal repaint
    /// forever. Reclaim only exited journals in the background; live pre-v4
    /// Hosts remain untouched and surface the normal Reload Terminal action.
    private nonisolated static func compactStoppedOutputJournals() {
        DispatchQueue.global(qos: .utility).async {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: LaunchConfig.hostBinary)
            process.arguments = ["__compact_output_journals__"]
            process.standardInput = FileHandle.nullDevice
            process.standardOutput = FileHandle.nullDevice
            process.standardError = FileHandle.nullDevice
            guard (try? process.run()) != nil else { return }
            process.waitUntilExit()
        }
    }

    /// The only Host-scope mutation path. Remote connection failures never
    /// call this, so an offline Host cannot silently fall back to Local and
    /// turn the next user action into a local effect.
    func selectHost(_ hostID: String?, forceReconnect: Bool = false) {
        if let hostID {
            guard RemoteHostFeature.pickerEnabled else { return }
            if selectedHostScope.remoteHostID == hostID,
               remoteHostRuntime.selectionConnectionIsActive,
               !forceReconnect {
                // A checked Host is already the current scope. Reopening its
                // backend could duplicate an in-flight semantic effect from a
                // candidate bootstrap captured before the old tail settles.
                return
            }
            guard let host = remoteHostStore.records.first(where: { $0.hostID == hostID }),
                  let credentials = remoteHostStore.credentials(for: hostID)
            else {
                return
            }
            if selectedHostScope == .local {
                localSelectedSessionIDBeforeRemote = selectedSessionID
            }
            selectedSessionID = nil
            settingsVisible = false
            commandPaletteVisible = false
            cancelSessionSwitcher()
            commandHintsVisible = false
            setControlHintsVisible(false)
            launcherProjectID = nil
            archivedProjectID = nil
            recentActivityVisible = false
            remoteHostStore.selectHost(hostID)
            selectedHostScope = .remote(hostID: hostID)
            connectRemoteHost(host, credentials: credentials)
            projectRemoteScope(snapshot: remoteHostRuntime.snapshot)
            return
        }

        remoteHostRuntime.disconnect()
        remoteHostStore.selectHost(nil)
        selectedHostScope = .local
        clearRemoteScopeProjection()
        if let prior = localSelectedSessionIDBeforeRemote,
           sessionsByID[prior] != nil {
            selectedSessionID = prior
        }
        localSelectedSessionIDBeforeRemote = nil
        refreshTitlebarBranch()
    }

    private func connectRemoteHost(
        _ host: PairedHostRecord,
        credentials: RemoteHostCredentials
    ) {
        // Pairing seals the Link key to this Controller id. A stale record
        // from another Controller identity must never be repurposed as a new
        // Link principal; repairing it means pairing again.
        guard host.controllerDeviceID == remoteHostStore.controllerIdentity.id else {
            remoteHostRuntime.requirePairingRepair()
            return
        }
        remoteHostRuntime.connectPairedHost(
            record: host,
            credentials: credentials
        )
    }

    func forgetHost(_ hostID: String) {
        if selectedHostScope.remoteHostID == hostID {
            selectHost(nil)
        }
        remoteHostStore.forget(hostID: hostID)
    }

    private func applyMenuTracking(_ delta: Int) {
        menuTrackingDepth = max(0, menuTrackingDepth + delta)
        if menuTrackingDepth == 0, rescanDeferredForMenuTracking {
            rescanDeferredForMenuTracking = false
            scheduleRescan(after: 0)
        }
    }

    /// Clear a session's unread badge because a paired phone is viewing it.
    /// No-op unless it was actually unread, so the hot output-poll path that
    /// drives this stays cheap.
    func clearUnreadFromRemoteViewer(_ sessionID: String) {
        guard unreadSessionIDs.contains(sessionID) else { return }
        removeUnread(sessionID)
        persistActivitySnapshot()
    }

    /// Phone explicitly opened a session (transport-independent — the WS
    /// stream path never hits `/mobile/output`, so presence alone can't be
    /// relied on). Clears its unread "blue dot".
    func applyRemoteMarkRead(_ request: RemoteMarkReadRequest) throws {
        clearUnreadFromRemoteViewer(request.sessionID)
    }

    deinit {
        // The preset self-test creates throwaway stores; the FSEvents
        // context holds an unretained pointer to self, so the stream (and
        // the timers) must be torn down with the instance. Stores live and
        // die on the main actor.
        MainActor.assumeIsolated {
            teardownFileWatcher()
            safetyTimer?.invalidate()
            busySweepTimer?.invalidate()
            if let monitor = shortcutKeyMonitor { NSEvent.removeMonitor(monitor) }
            if let monitor = shortcutFlagsMonitor { NSEvent.removeMonitor(monitor) }
            // Block-based NotificationCenter observers are not auto-removed on
            // dealloc; drop them so throwaway self-test stores don't leave live
            // observers behind.
            for observer in appActivationObservers {
                NotificationCenter.default.removeObserver(observer)
            }
            appActivationObservers.removeAll()
        }
    }

    // MARK: - Hook events → activity + unread (App.svelte:638-680)

    /// Attach the app-owned hook server so launches export its port.
    /// Shared-core Phase 1: the app's own projects are mirrored into
    /// `app-state.json` so no project exists only in this UI's UserDefaults
    /// — the file is what the TUI (and a Linux host) read. UserDefaults
    /// stays the app-local working copy; the pre-existing merge already
    /// dedupes file-vs-record by path, so mirroring cannot double a project
    /// even under an older app build. Change-guarded: rescans call the
    /// write sites repeatedly and an unchanged mirror must cost a compare,
    /// not a file write that pings every peer.
    private func mirrorProjectsToSharedState() {
        let records = loadNativeProjects()
        let tombstoned = Set(
            AppDefaults.shared.stringArray(forKey: Self.removedProjectsKey) ?? []
        )
        guard let raw = try? Data(contentsOf: LaunchConfig.appStateFile),
              let object = (try? JSONSerialization.jsonObject(with: raw)) as? [String: Any]
        else { return } // never invent or clobber the file from here
        var projects = (object["projects"] as? [[String: Any]]) ?? []
        var changed = false
        let recordByID = Dictionary(uniqueKeysWithValues: records.map { ($0.id, $0) })
        // Mirrored entries whose record is gone or tombstoned leave the file.
        projects.removeAll { entry in
            guard let id = entry["id"] as? String, id.hasPrefix("native-") else { return false }
            let dead = recordByID[id] == nil || tombstoned.contains(id)
            if dead { changed = true }
            return dead
        }
        for record in records where !tombstoned.contains(record.id) {
            var desired: [String: Any] = [
                "id": record.id, "name": record.name, "path": record.path,
            ]
            if let parent = record.parentProjectID { desired["parent_project_id"] = parent }
            if let branch = record.worktreeBranch { desired["worktree_branch"] = branch }
            if record.isFolder == true { desired["is_folder"] = true }
            if let index = projects.firstIndex(where: { ($0["id"] as? String) == record.id }) {
                // Update in place, preserving fields we don't model.
                var mergedEntry = projects[index]
                var rowChanged = false
                for (key, value) in desired
                where (mergedEntry[key] as? NSObject) != (value as? NSObject) {
                    mergedEntry[key] = value
                    rowChanged = true
                }
                if rowChanged {
                    projects[index] = mergedEntry
                    changed = true
                }
            } else if record.parentProjectID != nil || !projects.contains(where: { entry in
                guard let path = entry["path"] as? String else { return false }
                return Self.normalizedProjectPath(path) == Self.normalizedProjectPath(record.path)
            }) {
                // Another frontend already covers this folder → leave its
                // entry as the truth (ids then agree across UIs). Child
                // records (groups share the parent's path by design) skip
                // the path guard — only id identity matters for them.
                projects.append(desired)
                changed = true
            }
        }
        guard changed else { return }
        let snapshot = projects
        editPresetStateAnnouncing { object in object["projects"] = snapshot }
    }

    /// Apply only this app's pending pin intents to the latest shared object.
    /// `PresetStateFile.edit` invokes the mutation while holding the same
    /// app-state lock as Rust, so a TUI pin committed after our last scan is
    /// preserved instead of being erased by an outside-the-lock snapshot.
    @discardableResult
    private func mirrorPinsToSharedState(
        _ overrides: NativePinOverrides
    ) -> Bool {
        guard !overrides.added.isEmpty || !overrides.removedKeys.isEmpty else {
            return true
        }
        var applied = false
        let wrote = editPresetStateAnnouncing { object in
            applied = Self.applyPinOverrides(overrides, to: &object)
        }
        return applied && wrote
    }

    /// Raw-JSON mutation used inside the app-state lock. Unknown fields on
    /// unrelated pins (and on a moved existing pin) survive the rewrite. The
    /// legacy flat-array shape remains readable and is normalized to the
    /// current project-grouped shape on the first successful native intent.
    @discardableResult
    static func applyPinOverrides(
        _ overrides: NativePinOverrides,
        to object: inout [String: Any]
    ) -> Bool {
        let additions = Dictionary(
            overrides.added.map { ($0.key, $0) },
            uniquingKeysWith: { _, newest in newest }
        )
        let targetKeys = Set(overrides.removedKeys).union(additions.keys)
        guard !targetKeys.isEmpty else { return true }

        let rawPins = object["pinned_sessions"]
        var grouped: [String: [[String: Any]]] = [:]
        if rawPins == nil || rawPins is NSNull {
            grouped = [:]
        } else if let rawGroups = rawPins as? [String: Any] {
            for (projectID, rawRows) in rawGroups {
                guard let rows = rawRows as? [Any],
                      rows.allSatisfy({ $0 is [String: Any] })
                else { return false }
                grouped[projectID] = rows.compactMap { $0 as? [String: Any] }
            }
        } else if let rawRows = rawPins as? [Any] {
            // Legacy app-state.json stored one flat array.
            for rawRow in rawRows {
                guard let row = rawRow as? [String: Any],
                      let projectID = row["project_id"] as? String
                else { return false }
                grouped[projectID, default: []].append(row)
            }
        } else {
            // A corrupt/unknown shape must not be replaced with an empty map;
            // returning false keeps the UserDefaults intent for a later retry.
            return false
        }

        var priorRows: [String: [String: Any]] = [:]
        for projectID in grouped.keys {
            grouped[projectID] = grouped[projectID]?.filter { row in
                let key = (row["key"] as? String)
                    ?? (row["session_id"] as? String).map(
                        PinnedSidebarSession.key(forSessionID:)
                    )
                guard let key, targetKeys.contains(key) else { return true }
                if priorRows[key] == nil {
                    priorRows[key] = row
                }
                return false
            }
        }

        for pin in additions.values.sorted(by: { $0.key < $1.key }) {
            var row = priorRows[pin.key] ?? [:]
            row["key"] = pin.key
            row["project_id"] = pin.projectID
            if let sessionID = pin.sessionID {
                row["session_id"] = sessionID
            } else {
                row.removeValue(forKey: "session_id")
            }
            row["pinned_at"] = pin.pinnedAt
            grouped[pin.projectID, default: []].append(row)
        }
        object["pinned_sessions"] = grouped
        return true
    }

    /// Every preset write goes through here so the other frontends hear it
    /// — same rule as unpeel-core's app_state::save choke point.
    @discardableResult
    func editPresetStateAnnouncing(_ mutate: (inout [String: Any]) -> Void) -> Bool {
        let wrote = PresetStateFile.edit(mutate)
        if wrote {
            announceStateChange("app-state")
        }
        return wrote
    }

    func attachHookServer(_ server: HookServer) {
        hookServer = server
        // Another frontend changed shared state: refresh now instead of
        // waiting for FSEvents coalescing or the safety-net rescan.
        server.stateChangeHandler = { [weak self] change in
            Task { @MainActor in
                guard let self else { return }
                Self.sharedOrderCache = nil
                Self.sharedProjectOrderCache = nil
                _ = change
                self.scheduleRescan(after: 0)
            }
        }
        // /mcp/* lifecycle requests (start/close/list-presets) arrive on the
        // hook server's connection threads; answer them on the main actor
        // where all session/project state lives.
        server.mcpHandler = { [weak self] path, body, reply in
            Task { @MainActor in
                guard let self else {
                    reply(400, #"{"error":"app is shutting down"}"#)
                    return
                }
                // approve-write and approve-computer reply when the user
                // answers the approval alert, not synchronously — they get
                // the reply callback.
                if path == "/mcp/approve-write" {
                    self.handleMcpApproveWrite(body: body, reply: reply)
                    return
                }
                if path == "/mcp/approve-computer" {
                    self.handleMcpApproveComputer(body: body, reply: reply)
                    return
                }
                if path == "/mcp/approve-browser" {
                    self.handleMcpApproveBrowser(body: body, reply: reply)
                    return
                }
                let (status, responseBody) = self.handleMcpRequest(path: path, body: body)
                reply(status, responseBody)
            }
        }
    }

    /// The session whose terminal the user is actually looking at:
    /// the selected session while the app is frontmost and the workspace is
    /// showing (getObservedWorkspaceSessionId, sessionUnread.ts:25-38 —
    /// shellView must be the terminal workspace, so Settings and the archive
    /// library both un-observe it).
    private var observedSessionID: String? {
        guard NSApp.isActive,
              !settingsVisible,
              archivedProjectID == nil,
              !recentActivityVisible
        else { return nil }
        return selectedSessionID
    }

    // MARK: - Host remote control

    /// Additive bridge capability is also a live takeover intent. A TUI must
    /// not release Direct merely because this build knows the protocol when
    /// the feature is disabled or shutdown cancelled the listener/retry.
    var mobileEndpointHandoffIntent: Bool {
        UnpeelFeatureFlags.mobileRemoteControlEnabled
            && (hostRemoteServer != nil || hostRemoteServerRetryTask != nil)
    }

    func startHostRemoteServer(scheduleRetry: Bool = true) {
        guard UnpeelFeatureFlags.mobileRemoteControlEnabled else { return }
        guard hostRemoteServer == nil else { return }
        let pairingStore = MobilePairingStore(macID: localHostID)
        guard let server = MobileRemoteServer(
            pairingStore: pairingStore,
            bootstrapProvider: { [weak self] in
                guard let self else {
                    return RemoteBootstrapSnapshot(
                        macID: nil,
                        macName: UnpeelWorkspaceContext.advertisedHostName,
                        folders: [],
                        projects: [],
                        presets: [],
                        sessions: [],
                        capturedAtUnixMs: Int64(Date().timeIntervalSince1970 * 1000)
                    )
                }
                return self.remoteBootstrapSnapshot(macID: pairingStore.macID)
            },
            createSessionProvider: { [weak self] request in
                guard let self else {
                    throw MobileRemoteError(400, "app is shutting down")
                }
                return try self.createMobileRemoteSession(request)
            },
            resizeDesktopProvider: { [weak self] request in
                guard let self else {
                    throw MobileRemoteError(400, "app is shutting down")
                }
                try self.applyRemoteDesktopResize(request)
            },
            sessionOrganizationProvider: { [weak self] patch in
                guard let self else {
                    throw MobileRemoteError(400, "app is shutting down")
                }
                try self.applyRemoteSessionOrganization(patch)
            },
            projectOrganizationProvider: { [weak self] patch in
                guard let self else {
                    throw MobileRemoteError(400, "app is shutting down")
                }
                try self.applyRemoteProjectOrganization(patch)
            },
            sessionOrderProvider: { [weak self] request in
                guard let self else {
                    throw MobileRemoteError(400, "app is shutting down")
                }
                try self.applyRemoteSessionOrder(request)
            },
            restartSessionProvider: { [weak self] request in
                guard let self else {
                    throw MobileRemoteError(400, "app is shutting down")
                }
                try self.applyRemoteRestartSession(request)
            },
            sessionActionProvider: { [weak self] request in
                guard let self else {
                    throw MobileRemoteError(400, "app is shutting down")
                }
                try await self.applyRemoteSessionAction(request)
            },
            markReadProvider: { [weak self] request in
                guard let self else {
                    throw MobileRemoteError(400, "app is shutting down")
                }
                try self.applyRemoteMarkRead(request)
            },
            approvalAnswerProvider: { [weak self] request in
                guard let self else {
                    throw MobileRemoteError(400, "app is shutting down")
                }
                try self.applyRemoteApprovalAnswer(request)
            },
            desktopViewingProvider: { [weak self] sessionID in
                guard let self else { return false }
                return self.observedSessionID == sessionID
            },
            archivedSessionsProvider: { [weak self] projectID in
                guard let self, self.projectsByID[projectID] != nil else { return nil }
                return RemoteArchivedSessionsResponse(
                    projectID: projectID,
                    sessions: self.remoteArchivedSessionSummaries(projectID: projectID)
                )
            },
            onDevicesChanged: { [weak self] in
                Task { @MainActor in self?.refreshPairedControllers() }
            },
            onPairingCompleted: { [weak self] token in
                Task { @MainActor in self?.completeHostPairing(token: token) }
            }
        ) else {
            hostServerError = "Waiting for the current Host listener to hand off."
            if scheduleRetry && hostRemoteServerRetryTask == nil {
                hostRemoteServerRetryTask = Task { @MainActor [weak self] in
                    // Retry quickly through the TUI's bridge poll/yield, then
                    // keep a cancellable low-frequency claim alive for as
                    // long as the app runs. Sidebar/MainActor may remain busy
                    // beyond any fixed ceiling; once capability-driven yield
                    // happens, native must eventually take the exact port.
                    var attempts = 0
                    while !Task.isCancelled {
                        let delay: UInt64 = attempts < 240
                            ? 250_000_000
                            : 5_000_000_000
                        do {
                            try await Task.sleep(nanoseconds: delay)
                        } catch {
                            return
                        }
                        guard !Task.isCancelled, let self else { return }
                        self.startHostRemoteServer(scheduleRetry: false)
                        if self.hostRemoteServer != nil {
                            self.hostRemoteServerRetryTask = nil
                            return
                        }
                        attempts += 1
                    }
                }
            }
            return
        }
        hostRemoteServerRetryTask?.cancel()
        hostRemoteServerRetryTask = nil
        hostRemoteServer = server
        hostServerEndpoint = server.endpoint
        hostServerError = nil
        pairedControllers = server.pairedDevices
        RelayUplinkManager.shared.attach(server: server)
    }

    func stopHostRemoteServer() {
        hostRemoteServerRetryTask?.cancel()
        hostRemoteServerRetryTask = nil
        RelayUplinkManager.shared.detach()
        hostRemoteServer?.stop()
        hostRemoteServer = nil
        hostServerEndpoint = nil
        hostPairingPresentation = .idle
    }

    func beginHostPairing() {
        guard UnpeelFeatureFlags.mobileRemoteControlEnabled else { return }
        if hostRemoteServer == nil {
            startHostRemoteServer()
        }
        guard let server = hostRemoteServer else { return }
        let payload = server.beginPairing()
        hostPairingPresentation = .active(payload)
    }

    func cancelHostPairing() {
        hostRemoteServer?.cancelPairing()
        hostPairingPresentation = .idle
    }

    func completeHostPairing(token: String) {
        hostPairingPresentation = hostPairingPresentation.completing(token: token)
    }

    func refreshPairedControllers() {
        pairedControllers = hostRemoteServer?.pairedDevices ?? []
    }

    var mobilePushTargetCount: Int {
        hostRemoteServer?.pairingStore.pushTargets().count ?? 0
    }

    func revokeMobileDevice(_ deviceID: String) {
        hostRemoteServer?.revokeDevice(id: deviceID)
        refreshPairedControllers()
    }

    /// Scope a paired Controller to Direct-only (allowed = false) or enroll
    /// it on Unpeel Link. Enforcement is Host-side: the uplink re-registers
    /// its device token set on the change notification.
    func setDeviceRelayAllowed(_ deviceID: String, _ allowed: Bool) {
        hostRemoteServer?.setDeviceRelayAllowed(id: deviceID, allowed: allowed)
        if allowed {
            // Downgrade compatibility: older builds still gate the uplink on
            // the retired global toggle's key, so enrolling a device here
            // must also flip that stored preference back on — otherwise a
            // downgrade would strand an enrolled phone with the relay off.
            AppDefaults.shared.set(true, forKey: RelayConfig.enabledDefaultsKey)
            RelayUplinkManager.shared.refresh()
        }
        refreshPairedControllers()
    }

    /// Scope a paired Host (outbound) to Direct-only or restore its Unpeel
    /// Link fallback. If that Host is the current scope, reconnect so the
    /// active connection plan matches the new enrollment immediately —
    /// otherwise a live Direct connection could still fall back to Link
    /// later (or a narrowed Host could keep an open Link route).
    func setHostLinkEnabled(_ hostID: String, _ enabled: Bool) {
        remoteHostStore.setLinkEnabled(enabled, forHost: hostID)
        if selectedHostScope.remoteHostID == hostID {
            selectHost(hostID, forceReconnect: true)
        }
    }

    private func createMobileRemoteSession(
        _ request: RemoteCreateSessionRequest
    ) throws -> RemoteCreateSessionResponse {
        guard let project = projectsByID[request.projectID], project.isFolder != true else {
            throw MobileRemoteError(400, "Unknown project id: \(request.projectID)")
        }

        let command: String
        if let presetID = request.presetID {
            if let cli = SetupTool(rawValue: presetID), let preset = defaultPreset(for: cli) {
                command = preset.command
            } else if let preset = mergedPresets.first(where: { $0.id == presetID }) {
                command = preset.command
            } else {
                throw MobileRemoteError(400, "Unknown preset id: \(presetID)")
            }
        } else if let explicit = request.command {
            command = explicit.trimmingCharacters(in: .whitespacesAndNewlines)
        } else {
            throw MobileRemoteError(400, "Missing presetID or command")
        }

        let worktreePath = project.worktreeBranch == nil ? request.worktreePath : project.path
        let worktreeBranch = project.worktreeBranch ?? request.worktreeBranch
        let createdAt = Int64(Date().timeIntervalSince1970 * 1000)
        let label = command.isEmpty ? "Terminal" : command
        guard let sessionID = spawnSession(
            projectID: project.id,
            command: command,
            label: label,
            customTitle: false,
            createdAt: createdAt,
            cwd: worktreePath ?? project.path,
            worktreePath: worktreePath,
            worktreeBranch: worktreeBranch,
            activateUI: false
        ) else {
            throw MobileRemoteError(400, "Failed to spawn session host")
        }

        if let initialText = request.initialText, !initialText.isEmpty {
            let data: String
            switch request.initialTextSubmitMode {
            case .pasteOnly:
                data = initialText
            case .pasteAndSubmit:
                data = initialText + "\r"
            case .raw:
                data = initialText
            }
            Task.detached {
                try? await Task.sleep(nanoseconds: 350_000_000)
                try? MobileSessionControl.write(sessionID: sessionID, data: data)
            }
        }

        let createdSession = sessionsByID[sessionID] ?? pendingSessions[sessionID]

        return RemoteCreateSessionResponse(
            sessionID: sessionID,
            capturedAtUnixMs: Int64(Date().timeIntervalSince1970 * 1000),
            session: createdSession?.remoteSummary()
        )
    }

    /// Phone-driven rename/pin/archive. Applies through the same overlay
    /// paths as the desktop sidebar, so the next /mobile/bootstrap poll
    /// returns the updated state. Internal (not private): the
    /// /mcp/organize-session maintenance endpoint in MCPBridge.swift reuses
    /// it for the mobile dev bridge.
    func applyRemoteSessionOrganization(
        _ patch: RemoteSessionOrganizationPatch
    ) throws {
        guard let session = sessionsByID[patch.sessionID] else {
            throw MobileRemoteError(404, "Unknown session id: \(patch.sessionID)")
        }
        if let title = patch.title {
            renameSession(session.id, to: title)
        }
        if let pinned = patch.pinned {
            if pinned {
                pinSession(
                    projectID: effectiveProjectID(for: session),
                    sessionID: session.id
                )
            } else {
                unpinSession(
                    projectID: effectiveProjectID(for: session),
                    sessionID: session.id
                )
            }
        }
        if let notifyWhenDone = patch.notifyWhenDone {
            setNotifyWhenDone(session.id, enabled: notifyWhenDone)
        }
        // The phone confirms before sending, so this goes straight to
        // archiveSession (stop + file away) — never the desktop's inline
        // confirm, which is a sidebar row state the phone can't see.
        // Archived sessions are filtered from the phone snapshot, so the
        // session disappears from the phone on its next refresh; restoring
        // is currently a desktop verb (the phone never lists archived rows).
        if let archived = patch.archived {
            if archived {
                archiveSession(session.id)
            } else {
                unarchiveSession(session.id)
            }
        }
    }

    /// A Controller's project organize patch (capability
    /// `project.organization.set`) lands here: rename (groups only — same as
    /// the desktop context menu), folder color ("" clears to default), the
    /// per-group session sort, and `sortOrder` — move the project to that
    /// index among its same-parent siblings in the CURRENT desktop display
    /// order. Everything goes through the same store verbs the desktop menu
    /// and drag paths use, so shared-state announces and both frontends
    /// update. Unsupported fields are rejected before anything applies.
    func applyRemoteProjectOrganization(
        _ patch: RemoteProjectOrganizationPatch
    ) throws {
        guard let project = projectsByID[patch.projectID] else {
            throw MobileRemoteError(404, "Unknown project id: \(patch.projectID)")
        }
        // No Host implements legacy-folder moves; reject rather than
        // silently ignore, and before any other field half-applies.
        if patch.folderID != nil {
            throw MobileRemoteError(
                501, "Moving a project between folders is not supported"
            )
        }
        if let sortOrder = patch.sortOrder, sortOrder < 0 {
            throw MobileRemoteError(400, "sortOrder must be a non-negative integer")
        }
        if let displayName = patch.displayName {
            guard renameGroupProject(patch.projectID, to: displayName) else {
                throw MobileRemoteError(400, "Only groups can be renamed remotely")
            }
        }
        if let colorID = patch.colorID {
            // Folder color is a MAIN-project verb — groups and worktrees
            // stay neutral (same rule as the desktop and TUI menus).
            guard project.parentProjectID == nil else {
                throw MobileRemoteError(400, "Only main projects can be colored")
            }
            if colorID.isEmpty {
                setProjectFolderColor(nil, for: patch.projectID)
            } else if let color = ProjectFolderColor(rawValue: colorID) {
                setProjectFolderColor(color, for: patch.projectID)
            } else {
                throw MobileRemoteError(400, "Unknown folder color: \(colorID)")
            }
        }
        if let dateSorted = patch.dateSorted {
            setSessionDateSorted(dateSorted, for: patch.projectID)
        }
        if let sortOrder = patch.sortOrder {
            // Always against the LOCAL tree: this Mac is the Host here, even
            // if its own picker currently scopes the UI to another Host.
            let parentID = project.parentProjectID
            let current = localProjectOrderIDs(parentID: parentID)
            guard let from = current.firstIndex(of: project.id) else {
                throw MobileRemoteError(400, "Project is not reorderable")
            }
            var ids = current
            ids.remove(at: from)
            ids.insert(project.id, at: min(sortOrder, ids.count))
            // A no-op move skips the write (and its state-bus announce).
            if ids != current {
                setProjectOrder(ids, parentID: parentID)
            }
        }
    }

    /// This Host's own displayed sibling order — the local counterpart of
    /// `projectOrderIDs`, which reads the scope-selected display tree.
    private func localProjectOrderIDs(parentID: String?) -> [String] {
        guard let parentID else { return nodes.map(\.id) }
        return findNode(parentID)?.worktrees.map(\.id) ?? []
    }

    /// A phone reorder commits the same combined pinned + regular rank list a
    /// desktop drag does: split it into the two buckets so both legacy
    /// UserDefaults fallbacks stay in sync, then publish the shared file once.
    func applyRemoteSessionOrder(_ request: RemoteSessionOrderRequest) throws {
        let projectID = request.projectID.trimmingCharacters(in: .whitespaces)
        guard !projectID.isEmpty, projectsByID[projectID] != nil else {
            throw MobileRemoteError(404, "Unknown project id: \(request.projectID)")
        }
        var seen = Set<String>()
        let ids = request.orderedSessionIDs
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty && seen.insert($0).inserted }
        guard !ids.isEmpty else {
            throw MobileRemoteError(400, "orderedSessionIDs must not be empty")
        }
        let pinnedSet = Set((pinnedByProject[projectID] ?? []).compactMap(\.sessionID))
        let pinnedIDs = ids.filter { pinnedSet.contains($0) }
        let regularIDs = ids.filter { !pinnedSet.contains($0) }
        if !pinnedIDs.isEmpty {
            AppDefaults.shared.set(pinnedIDs, forKey: Self.pinnedOrderKey(projectID))
        }
        if !regularIDs.isEmpty {
            AppDefaults.shared.set(regularIDs, forKey: Self.sessionOrderKey(projectID))
        }
        if Self.writeSharedSessionOrder(projectID: projectID, ids: ids) {
            announceStateChange("order")
        }
        withAnimation(.easeInOut(duration: 0.18)) { rebuildTreeFromLastScan() }
    }

    /// Opt a session in/out of the "finished" push notification. Persisted to
    /// the native overlay; the next remote snapshot reflects it, and the push
    /// dispatcher reads it when the session settles.
    func setNotifyWhenDone(_ sessionID: String, enabled: Bool) {
        guard remoteSummariesByID[sessionID] == nil else { return }
        let has = notifyWhenDoneSessionIDs.contains(sessionID)
        guard has != enabled else { return }
        if enabled {
            notifyWhenDoneSessionIDs.insert(sessionID)
        } else {
            notifyWhenDoneSessionIDs.remove(sessionID)
        }
        AppDefaults.shared.set(
            Array(notifyWhenDoneSessionIDs), forKey: NativeOverlay.notifyWhenDoneKey
        )
    }

    /// Force-clear a session's attention badge ("Clear attention" in the
    /// sidebar context menu) — the escape hatch for a stuck or false badge.
    /// Covers both sources: a hook-owned PermissionRequest state drops to
    /// idle (later hook events re-drive it as usual), and the host's
    /// menu-prompt flag is dismissed until it lowers and re-raises.
    func clearAttention(_ sessionID: String) {
        guard remoteSummariesByID[sessionID] == nil else { return }
        activity.clearAttention(sessionID)
        menuAttentionDismissals.insert(sessionID)
        rescan()
    }

    // MARK: - Archive (non-destructive "clear it out")

    /// Session ids showing the inline "Stop & archive?" confirmation row —
    /// only sessions that are actively working (busy/starting/attention)
    /// confirm; idle and exited sessions archive directly.
    @Published private(set) var confirmingArchiveSessionID: String?

    /// Archive entry point for the UI: working sessions get an inline
    /// confirm (archiving kills the turn mid-flight); settled ones archive
    /// straight away. Non-resumable commands route to Remove instead —
    /// archiving a session whose CLI can't resume just strands it in the
    /// library (the auto-archive sweeps still file such sessions away, but
    /// the user-facing verb is honest about what's possible).
    func requestArchiveSession(_ sessionID: String) {
        guard let session = displaySessionsByID[sessionID] else { return }
        guard sessionCanArchive(sessionID) else {
            requestRemoveSession(sessionID)
            return
        }
        confirmingRemoveSessionID = nil
        switch session.status {
        case .starting, .busy, .attention:
            confirmingArchiveSessionID = sessionID
        case .idle, .exited:
            archiveSession(sessionID)
        }
    }

    func cancelArchiveConfirm() {
        confirmingArchiveSessionID = nil
    }

    /// Stop-and-file-away: kill the hosted PTY (same identity-guarded path as
    /// Remove) and reap the session's browser daemon, but keep the session
    /// dir — manifest, output.bin, artifacts, provider-id overlay — intact,
    /// then hide the row from the sidebar. The project's archived-session
    /// view can resume the exact conversation via ResumeCommand.
    /// `stampRecency` is true for user-initiated archives (the stamp floats
    /// the row to the top of the stopped group); auto-archive sweeps pass
    /// false so old sessions file away without resurfacing.
    // MARK: - Shared sidebar order (cross-frontend contract)

    /// `~/.unpeel/session-order.json` — `{ project_id: [session ids] }`, the
    /// same hand-ordering this app keeps per project in UserDefaults. A drag
    /// in the TUI lands here; a drag here lands there.
    static var sharedSessionOrderURL: URL {
        LaunchConfig.unpeelDir.appendingPathComponent("session-order.json")
    }

    /// Parsed `session-order.json`, cached against the file's modification
    /// date. This is consulted for every project on every sidebar rebuild,
    /// so the common "nothing changed" case must cost a stat, not a parse.
    private nonisolated(unsafe) static var sharedOrderCache: (stamp: Date, root: [String: [String]])?

    static func sharedSessionOrder(projectID: String) -> [String]? {
        let url = sharedSessionOrderURL
        let stamp = (try? FileManager.default.attributesOfItem(atPath: url.path))?[.modificationDate] as? Date
        guard let stamp else {
            sharedOrderCache = nil
            return nil
        }
        if sharedOrderCache?.stamp != stamp {
            var parsed: [String: [String]] = [:]
            if let data = try? Data(contentsOf: url),
               let root = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] {
                for (key, value) in root {
                    if let ids = value as? [String] { parsed[key] = ids }
                }
            }
            sharedOrderCache = (stamp, parsed)
        }
        let ids = sharedOrderCache?.root[projectID] ?? []
        return ids.isEmpty ? nil : ids
    }

    @discardableResult
    private static func editSharedSessionOrders(
        _ edit: (inout [String: Any]) -> Bool
    ) -> Bool {
        // Read-modify-write on a cross-frontend file: take the same lock
        // the Rust writer does, or a concurrent TUI drag loses this edit.
        let wrote = PresetStateFile.withExclusiveLock(on: sharedSessionOrderURL) {
            var root: [String: Any]
            if let data = try? Data(contentsOf: sharedSessionOrderURL) {
                guard let parsed = (try? JSONSerialization.jsonObject(with: data))
                        as? [String: Any]
                else { return false }
                root = parsed
            } else {
                root = [:]
            }
            guard edit(&root), JSONSerialization.isValidJSONObject(root),
                  let data = try? JSONSerialization.data(withJSONObject: root)
            else { return false }
            do {
                try FileManager.default.createDirectory(
                    at: sharedSessionOrderURL.deletingLastPathComponent(),
                    withIntermediateDirectories: true
                )
                try data.write(to: sharedSessionOrderURL, options: .atomic)
                return true
            } catch {
                NSLog("[UnpeelNative] shared session order write failed: \(error)")
                return false
            }
        } ?? false
        // Never let this process's cached pre-write value win the rebuild that
        // immediately follows a local commit.
        sharedOrderCache = nil
        return wrote
    }

    @discardableResult
    static func writeSharedSessionOrder(projectID: String, ids: [String]) -> Bool {
        editSharedSessionOrders { root in
            if ids.isEmpty {
                root.removeValue(forKey: projectID)
            } else {
                root[projectID] = ids
            }
            return true
        }
    }

    @discardableResult
    static func removeSessionFromSharedOrders(_ sessionID: String) -> Bool {
        editSharedSessionOrders { root in
            var changed = false
            for key in Array(root.keys) {
                guard let ids = root[key] as? [String], ids.contains(sessionID) else {
                    continue
                }
                let kept = ids.filter { $0 != sessionID }
                if kept.isEmpty {
                    root.removeValue(forKey: key)
                } else {
                    root[key] = kept
                }
                changed = true
            }
            return changed
        }
    }

    // MARK: - Shared session markers (cross-frontend contract)

    /// Session-dir markers any frontend can write: `archived.json`,
    /// `title.json`, `read.json`. The TUI and CLI have no access to this
    /// app's UserDefaults overlays, so these files are how the desktop, a
    /// headless host, and the phone agree on organization state. The
    /// Shared markers are authoritative once present. UserDefaults overlays
    /// remain only as migration/write-failure fallbacks for older builds.
    enum SharedMarker: String {
        case archived = "archived.json"
        case title = "title.json"
        case read = "read.json"
        /// Hook-captured provider conversation metadata — see
        /// unpeel-core session_ops::set_provider_session for the merge and
        /// no-announce semantics both sides follow.
        case providerSession = "provider-session.json"
        /// Pending appended system context, consumed by the next restart.
        case appendedContext = "appended-context.json"
        /// The user filed the session under another project (group or
        /// worktree folder): `{"project_id": "<target>", "moved_at": ms}`.
        /// Any frontend may write or delete it; a missing/stale target
        /// falls back to the manifest project.
        case projectOverride = "project-override.json"
    }

    /// Pending context for a session: the cross-frontend marker first, then
    /// this app's legacy/write-failure overlay.
    private func pendingAppendedContext(_ sessionID: String) -> String? {
        // The marker is the cross-frontend truth. UserDefaults is only the
        // older native write-failure fallback and must never hide a newer TUI
        // or second-app marker.
        (Self.readSharedMarker(sessionID, .appendedContext)?["context"] as? String)
            ?? loadPendingAppendSystemContexts()[sessionID]
    }

    static func sharedMarkerURL(_ sessionID: String, _ marker: SharedMarker) -> URL {
        LaunchConfig.appSessionsDir
            .appendingPathComponent(sessionID)
            .appendingPathComponent(marker.rawValue)
    }

    /// Cross-process lock path shared with
    /// `session_ops::appended_context_lock_target_at`. Keep this separate
    /// from the manifest lock: Resume Agent holds it only across its final
    /// marker comparison and PTY submission, while manifest updates can
    /// independently continue on the Host's observer threads.
    nonisolated static func appendedContextLockURL(
        unpeelDir: URL, sessionID: String
    ) -> URL {
        let digest = SHA256.hash(data: Data(sessionID.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        return unpeelDir
            .appendingPathComponent("session-appended-context-locks", isDirectory: true)
            .appendingPathComponent("\(digest).lock")
    }

    /// Exact counterpart of Rust `session_ops::lifecycle_lock_target_at`
    /// followed by `app_state::lock_exclusive`'s `.lock` extension.
    nonisolated static func sessionLifecycleLockURL(
        unpeelDir: URL, sessionID: String
    ) -> URL {
        let digest = SHA256.hash(data: Data(sessionID.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        return unpeelDir
            .appendingPathComponent("session-lifecycle-locks", isDirectory: true)
            .appendingPathComponent("\(digest).lock")
    }

    /// Acquire without waiting. Lifecycle actions run from MainActor entry
    /// points, so contention is a retryable rejection, never a UI stall.
    nonisolated static func acquireSessionLifecycleLease(
        unpeelDir: URL, sessionID: String
    ) -> NativeSessionFileLockLease? {
        acquireSessionFileLock(
            at: sessionLifecycleLockURL(unpeelDir: unpeelDir, sessionID: sessionID)
        )
    }

    nonisolated static func replacementRestartAllowsState(
        _ manifestState: String?,
        stoppedOnly: Bool,
        childProcessExists: Bool?,
        pidIdentity: ManifestPidIdentity
    ) -> Bool {
        guard stoppedOnly else { return true }
        if manifestState == "exited" { return true }
        guard manifestState == "running" else { return false }
        // A crashed Host can leave its final manifest at `running`. Resume is
        // safe only when the recorded child is definitely absent, or its pid
        // has definitely been recycled onto an unrelated process. Unknown
        // identity plus a live/unknown pid must fail closed.
        return childProcessExists == false || pidIdentity == .notOurs
    }

    /// `kill(pid, 0)` existence probe with EPERM treated as alive. Nil means
    /// the manifest did not provide a valid pid, which is unknown rather than
    /// proof that the child died.
    nonisolated static func hostedChildProcessExists(_ pid: Int32?) -> Bool? {
        guard let pid, pid > 1 else { return nil }
        if kill(pid, 0) == 0 { return true }
        switch errno {
        case EPERM: return true
        case ESRCH: return false
        default: return nil
        }
    }

    private nonisolated static func acquireSessionFileLock(
        at lockURL: URL
    ) -> NativeSessionFileLockLease? {
        do {
            try FileManager.default.createDirectory(
                at: lockURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
        } catch {
            NSLog("[UnpeelNative] failed to create Session lock directory: \(error)")
            return nil
        }
        let descriptor = open(
            lockURL.path,
            O_CREAT | O_RDWR | O_CLOEXEC,
            mode_t(0o600)
        )
        guard descriptor >= 0 else { return nil }
        guard fchmod(descriptor, mode_t(0o600)) == 0,
              flock(descriptor, LOCK_EX | LOCK_NB) == 0
        else {
            close(descriptor)
            return nil
        }
        return NativeSessionFileLockLease(descriptor: descriptor)
    }

    /// Snapshot (and, for a legacy overlay, first publish) the exact marker
    /// bytes used for replacement command derivation. Invalid bytes deliberately
    /// become an expected-missing snapshot: the final compare then rejects a
    /// destructive Resume exactly like Rust.
    private static func replacementContextSnapshot(
        sessionID: String,
        fallbackContext: String?
    ) -> NativeReplacementContextSnapshot? {
        guard let lease = acquireSessionFileLock(at: appendedContextLockURL(
            unpeelDir: LaunchConfig.unpeelDir,
            sessionID: sessionID
        )) else { return nil }
        defer { lease.release() }

        let markerURL = sharedMarkerURL(sessionID, .appendedContext)
        var raw: Data?
        do {
            raw = try Data(contentsOf: markerURL)
        } catch let error as CocoaError where error.code == .fileReadNoSuchFile {
            raw = nil
        } catch {
            NSLog("[UnpeelNative] failed to snapshot appended context: \(error)")
            return nil
        }

        if raw == nil,
           let fallbackContext,
           !fallbackContext.isEmpty {
            let body: [String: Any] = [
                "context": fallbackContext,
                "updated_at": Int64(Date().timeIntervalSince1970 * 1000),
                "revision": UUID().uuidString.lowercased(),
            ]
            guard let encoded = try? JSONSerialization.data(withJSONObject: body) else {
                return nil
            }
            do {
                try encoded.write(to: markerURL, options: .atomic)
                raw = encoded
            } catch {
                NSLog("[UnpeelNative] failed to migrate appended context: \(error)")
                return nil
            }
        }

        guard let raw else {
            return NativeReplacementContextSnapshot(raw: nil, context: nil)
        }
        guard let object = (try? JSONSerialization.jsonObject(with: raw)) as? [String: Any],
              let context = object["context"] as? String,
              !context.isEmpty
        else {
            return NativeReplacementContextSnapshot(raw: nil, context: nil)
        }
        return NativeReplacementContextSnapshot(raw: raw, context: context)
    }

    /// Re-acquire after command derivation, compare exact bytes, and move the
    /// consumed marker aside while holding the lock. The returned lease stays
    /// live across teardown + spawn; `killAndCleanup` removes the staged file
    /// with the old Session directory.
    private static func stageReplacementContext(
        sessionID: String,
        snapshot: NativeReplacementContextSnapshot
    ) -> NativeSessionFileLockLease? {
        guard let lease = acquireSessionFileLock(at: appendedContextLockURL(
            unpeelDir: LaunchConfig.unpeelDir,
            sessionID: sessionID
        )) else { return nil }
        let markerURL = sharedMarkerURL(sessionID, .appendedContext)
        let current: Data?
        do {
            current = try Data(contentsOf: markerURL)
        } catch let error as CocoaError where error.code == .fileReadNoSuchFile {
            current = nil
        } catch {
            lease.release()
            return nil
        }
        guard current == snapshot.raw else {
            lease.release()
            return nil
        }
        guard current != nil else { return lease }

        let stagedURL = markerURL.deletingLastPathComponent().appendingPathComponent(
            ".appended-context.json.replace-\(getpid())-\(UUID().uuidString.lowercased())"
        )
        do {
            try FileManager.default.moveItem(at: markerURL, to: stagedURL)
            guard try Data(contentsOf: stagedURL) == snapshot.raw else {
                if !FileManager.default.fileExists(atPath: markerURL.path) {
                    try? FileManager.default.moveItem(at: stagedURL, to: markerURL)
                }
                lease.release()
                return nil
            }
        } catch {
            if !FileManager.default.fileExists(atPath: markerURL.path),
               FileManager.default.fileExists(atPath: stagedURL.path) {
                try? FileManager.default.moveItem(at: stagedURL, to: markerURL)
            }
            lease.release()
            return nil
        }
        return lease
    }

    private static func withAppendedContextLock<Result>(
        sessionID: String,
        _ operation: () -> Result
    ) -> Result? {
        let lockURL = appendedContextLockURL(
            unpeelDir: LaunchConfig.unpeelDir,
            sessionID: sessionID
        )
        do {
            try FileManager.default.createDirectory(
                at: lockURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
        } catch {
            NSLog("[UnpeelNative] failed to create appended-context lock directory: \(error)")
            return nil
        }
        let descriptor = open(
            lockURL.path,
            O_CREAT | O_RDWR | O_CLOEXEC,
            mode_t(0o600)
        )
        guard descriptor >= 0 else { return nil }
        defer { close(descriptor) }
        guard fchmod(descriptor, mode_t(0o600)) == 0,
              flock(descriptor, LOCK_EX) == 0
        else { return nil }
        defer { _ = flock(descriptor, LOCK_UN) }
        return operation()
    }

    private static func sharedMarkerExistsUnlocked(
        _ sessionID: String, _ marker: SharedMarker
    ) -> Bool {
        FileManager.default.fileExists(atPath: sharedMarkerURL(sessionID, marker).path)
    }

    /// Existence check only — a stat instead of an open+parse. Rescan asks
    /// this for every live session, so the common "no marker" case must not
    /// cost a file read.
    static func sharedMarkerExists(_ sessionID: String, _ marker: SharedMarker) -> Bool {
        let read = { sharedMarkerExistsUnlocked(sessionID, marker) }
        if marker == .appendedContext {
            return withAppendedContextLock(sessionID: sessionID, read) ?? false
        }
        return read()
    }

    static func readSharedMarker(_ sessionID: String, _ marker: SharedMarker) -> [String: Any]? {
        let read: () -> [String: Any]? = {
            guard sharedMarkerExistsUnlocked(sessionID, marker),
                  let data = try? Data(contentsOf: sharedMarkerURL(sessionID, marker))
            else { return nil }
            return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        }
        if marker == .appendedContext {
            return withAppendedContextLock(sessionID: sessionID, read) ?? nil
        }
        return read()
    }

    @discardableResult
    static func writeSharedMarker(
        _ sessionID: String, _ marker: SharedMarker, _ body: [String: Any]
    ) -> Bool {
        var persistedBody = body
        if marker == .appendedContext {
            // A re-publish of identical text is still a distinct NEXT intent.
            // Rust compares the exact serialized snapshot, including this
            // revision, before consuming it.
            persistedBody["revision"] = UUID().uuidString.lowercased()
        }
        let write = {
            let url = sharedMarkerURL(sessionID, marker)
            guard FileManager.default.fileExists(atPath: url.deletingLastPathComponent().path),
                  let data = try? JSONSerialization.data(withJSONObject: persistedBody)
            else { return false }
            do {
                try data.write(to: url, options: .atomic)
                return true
            } catch {
                return false
            }
        }
        if marker == .appendedContext {
            return withAppendedContextLock(sessionID: sessionID, write) ?? false
        }
        return write()
    }

    static func removeSharedMarker(_ sessionID: String, _ marker: SharedMarker) {
        let remove = {
            try? FileManager.default.removeItem(at: sharedMarkerURL(sessionID, marker))
        }
        if marker == .appendedContext {
            _ = withAppendedContextLock(sessionID: sessionID, remove)
        } else {
            remove()
        }
    }

    /// Resolve the latest real activity signal with the same provider-aware
    /// rule as unpeel-core. Hook-capable agents have a truthful durable hook
    /// seed; when that seed is absent they have not produced a lifecycle
    /// event yet, so a TUI repaint in output.bin must NOT make them recent.
    /// Hookless tools use the host's parsed-screen change stamp, falling back
    /// to output.bin only for manifests from hosts that predate that field.
    static func resolvedLastRealActivityAtMs(
        command: String,
        hookEventAtMs: Int64?,
        screenChangedAtMs: Int64?,
        outputAtMs: Int64?
    ) -> Int64? {
        if SetupTool.detect(in: command)?.usesLifecycleHooks == true {
            return hookEventAtMs
        }
        return screenChangedAtMs ?? outputAtMs
    }

    /// Canonical timestamp used by Recent/date ordering. Creation is the
    /// start event and therefore the floor. The host's `updated_at` joins the
    /// rank only after it writes an exited manifest; while running that field
    /// is a heartbeat and would otherwise float every live session to now.
    static func resolvedLifecycleAtMs(
        createdAtMs: Int64,
        command: String,
        hookEventAtMs: Int64?,
        screenChangedAtMs: Int64?,
        outputAtMs: Int64?,
        finalExitedAtMs: Int64?
    ) -> Int64 {
        max(
            max(
                createdAtMs,
                resolvedLastRealActivityAtMs(
                    command: command,
                    hookEventAtMs: hookEventAtMs,
                    screenChangedAtMs: screenChangedAtMs,
                    outputAtMs: outputAtMs
                ) ?? 0
            ),
            finalExitedAtMs ?? 0
        )
    }

    private static func fileModificationAtMs(_ path: String) -> Int64? {
        guard let stamp = statFile(path) else { return nil }
        return Int64(stamp.mtimeSec) * 1_000 + Int64(stamp.mtimeNsec) / 1_000_000
    }

    /// Filesystem-backed last-real-activity read for unread-marker
    /// reconciliation. Keep this command-aware: a missing hook seed on
    /// Claude/Codex/etc intentionally returns nil rather than consulting
    /// screen/output repaint signals.
    static func sessionLastRealActivityAtMs(
        _ sessionID: String, command: String
    ) -> Int64? {
        let dir = LaunchConfig.appSessionsDir.appendingPathComponent(sessionID).path
        if SetupTool.detect(in: command)?.usesLifecycleHooks == true {
            return fileModificationAtMs(dir + "/last-hook-event.json")
        }
        var screenChangedAt: Int64?
        if let raw = try? Data(contentsOf: URL(fileURLWithPath: dir + "/manifest.json")),
           let json = try? JSONSerialization.jsonObject(with: raw) as? [String: Any],
           let stamp = (json["screen_changed_at"] as? NSNumber)?.int64Value,
           stamp > 0 {
            screenChangedAt = stamp
        }
        return screenChangedAt ?? fileModificationAtMs(dir + "/output.bin")
    }

    /// Unified Recent recency for ⌘K and sidebar date mode: the canonical
    /// lifecycle timestamp with creation as its floor. Read receipts are not
    /// activity — selecting/reading a row must never reshuffle any Recent
    /// surface. Ctrl-Tab's explicit MRU remains a separate interaction model.
    func sessionRecencyMs(_ sessionID: String) -> Int64 {
        guard let session = sessionsByID[sessionID] else { return 0 }
        return max(session.createdAt, session.lifecycleAtMs ?? 0)
    }

    /// Recency for every listable (non-archived) session, snapshotted once
    /// per ⌘K open so every keystroke filters one stable ordering.
    func paletteRecencySnapshot() -> [String: Int64] {
        var snapshot: [String: Int64] = [:]
        func collect(_ nodes: [ProjectNode]) {
            for node in nodes {
                for session in node.sessions
                where !archivedSessionIDs.contains(session.id) {
                    snapshot[session.id] = sessionRecencyMs(session.id)
                }
                collect(node.worktrees)
            }
        }
        collect(nodes)
        return snapshot
    }

    func archiveSession(_ sessionID: String, stampRecency: Bool = true) {
        if remoteSummariesByID[sessionID] != nil {
            if confirmingArchiveSessionID == sessionID {
                confirmingArchiveSessionID = nil
            }
            performRemoteVerb("Couldn't archive the session") { runtime in
                try await runtime.archiveSession(sessionID)
            }
            return
        }
        guard let session = sessionsByID[sessionID],
              !archivedSessionIDs.contains(sessionID),
              !removingSessionIDs.contains(sessionID),
              !restartingSessionIDs.contains(sessionID)
        else { return }
        // Same title freeze as Stop: an old-generation host's final exit
        // write must not revert the archived row to its preset label.
        preserveSettledTitleBeforeStop(session)
        if confirmingArchiveSessionID == sessionID {
            confirmingArchiveSessionID = nil
        }
        // Deselect first so the content pane swaps off the surface before
        // the host dies under it (confirmRemoveSession parity).
        if selectedSessionID == sessionID {
            selectedSessionID = nil
        }
        // Archiving is an explicit "I'm done with this": the unread badge
        // must not roll up from a hidden row.
        removeUnread(sessionID)
        // A live host takes a beat to stop: keep the row visible (muted,
        // spinner) for that beat so the click reads as "stopping…" rather
        // than an instant vanish. Settled rows have nothing to stop and
        // hide immediately.
        if session.isLive {
            archivingSessionIDs.insert(sessionID)
        }
        if stampRecency {
            archivedAtBySession[sessionID] = Int64(Date().timeIntervalSince1970 * 1000)
        }
        archivedSessionIDs.insert(sessionID)
        persistArchivedSessionIDs()
        // `stamped` distinguishes "the user just filed this" (floats to the
        // top of the stopped group and lingers) from the auto-archive sweep
        // (files away silently). Without it the terminal floated rows this
        // app had swept. Missing field reads as stamped, matching every
        // marker shipped before the field existed.
        Self.writeSharedMarker(
            sessionID, .archived,
            [
                "archived_at": Int64(Date().timeIntervalSince1970 * 1000),
                "stamped": stampRecency,
            ]
        )
        announceStateChange("session-markers")
        stopAndReapArchivedSession(sessionID, cleanupIfAlreadyStopped: true)
    }

    /// Archive work is intentionally recoverable. The archived flag is
    /// persisted before the asynchronous host shutdown starts so the row
    /// disappears immediately; if the app exits in that gap, the next rescan
    /// calls this again for any archived session that is still live.
    private func stopAndReapArchivedSession(
        _ sessionID: String,
        cleanupIfAlreadyStopped: Bool
    ) {
        guard archivedSessionIDs.contains(sessionID),
              let session = sessionsByID[sessionID],
              session.isLive || cleanupIfAlreadyStopped,
              stoppingArchivedSessionIDs.insert(sessionID).inserted
        else { return }

        let dirURL = LaunchConfig.appSessionsDir.appendingPathComponent(sessionID)
        let manifest = (try? Data(
            contentsOf: dirURL.appendingPathComponent("manifest.json")
        )).flatMap { try? JSONDecoder().decode(HostedSessionManifest.self, from: $0) }
        let pid = manifest?.pid
        let live = manifest?.state == "running"
            && pid.map { kill($0, 0) == 0 } ?? false
            && Self.manifestPidIdentity(manifest) != .notOurs

        Task { [weak self] in
            if live {
                await Self.terminateHost(dirURL: dirURL, manifest: manifest)
            }
            // Archived must not keep burning resources invisibly: the browser
            // engine daemon (and its Chrome) deliberately outlives the CLI,
            // so reap it like Remove does. No-op for browserless sessions.
            Self.cleanupBrowserDaemon(sessionID: sessionID)
            await MainActor.run {
                self?.stoppingArchivedSessionIDs.remove(sessionID)
                // Stop finished: the lingering "archiving…" row (if any)
                // can now disappear into the archive.
                self?.archivingSessionIDs.remove(sessionID)
                // The initial archive wants an immediate UI refresh. Recovery
                // retries rely on the host's manifest event (or the normal
                // safety rescan) so an unreachable legacy host cannot create
                // a tight rescan/retry loop.
                if cleanupIfAlreadyStopped {
                    self?.rescan()
                }
            }
        }
    }

    /// Put an archived session back in the regular list (as a restartable
    /// exited row — archive stopped its host).
    func unarchiveSession(_ sessionID: String) {
        if remoteSummariesByID[sessionID] != nil
            || remoteArchivedByProject.values.contains(where: { sessions in
                sessions.contains(where: { $0.id == sessionID })
            }) {
            performRemoteVerb("Couldn't restore the session") { runtime in
                try await runtime.restoreSession(sessionID)
            }
            return
        }
        Self.removeSharedMarker(sessionID, .archived)
        announceStateChange("session-markers")
        guard archivedSessionIDs.remove(sessionID) != nil else { return }
        archivingSessionIDs.remove(sessionID)
        archivedAtBySession[sessionID] = nil
        persistArchivedSessionIDs()
    }

    /// Restore without starting: return the row to its project, close the
    /// archive library, and make sure the sidebar can actually show it.
    func restoreArchivedSessionToSidebar(_ sessionID: String) {
        // Remote archive page: restore on the Host; the row returns to the
        // sidebar on the next bootstrap.
        if remoteProjectSummariesByID.isEmpty == false,
           remoteSummariesByID[sessionID] != nil
            || remoteArchivedByProject.values.contains(where: { sessions in
                sessions.contains(where: { $0.id == sessionID })
            }) {
            unarchiveSession(sessionID)
            archivedProjectID = nil
            return
        }
        guard let session = sessionsByID[sessionID],
              archivedSessionIDs.contains(sessionID)
        else { return }
        unarchiveSession(sessionID)
        archivedProjectID = nil
        prepareSidebarToRenderSession(session)
        requestSidebarScroll(to: sessionID, centered: false)
    }

    /// Resume an archived provider conversation and take the user straight
    /// back to its terminal. `restartSession` mints the replacement id and
    /// prunes the archived flag from the old id during teardown.
    @discardableResult
    func resumeArchivedSession(_ sessionID: String) -> Bool {
        // Remote archive page: restore + restart on the Host as one flow.
        if remoteSummary(for: sessionID) != nil
            || remoteArchivedByProject.values.contains(where: { sessions in
                sessions.contains(where: { $0.id == sessionID })
            }) {
            guard let source = remoteSummary(for: sessionID) else { return false }
            let knownSessionIDs = Set(remoteSummariesByID.keys)
                .union(remoteArchivedSummaryCache.sessionIDs)
            archivedProjectID = nil
            performRemoteVerb("Couldn't resume the session") { runtime in
                try await runtime.restoreAndRestartSession(
                    source,
                    knownSessionIDs: knownSessionIDs
                )
            }
            return true
        }
        guard archivedSessionIDs.contains(sessionID),
              let session = sessionsByID[sessionID],
              session.status == .exited,
              sessionCanRestart(sessionID)
        else { return false }

        let projectID = session.projectID
        selectedSessionID = sessionID
        guard restartSession(sessionID) else {
            if selectedSessionID == sessionID {
                selectedSessionID = nil
            }
            archivedProjectID = projectID
            return false
        }
        return true
    }

    private func persistArchivedSessionIDs() {
        if archivedSessionIDs.isEmpty {
            AppDefaults.shared.removeObject(forKey: NativeOverlay.archivedSessionsKey)
        } else {
            AppDefaults.shared.set(
                Array(archivedSessionIDs), forKey: NativeOverlay.archivedSessionsKey
            )
        }
        // The recency stamps live and die with the archived flag.
        archivedAtBySession = archivedAtBySession.filter {
            archivedSessionIDs.contains($0.key)
        }
        if archivedAtBySession.isEmpty {
            AppDefaults.shared.removeObject(forKey: NativeOverlay.archivedAtKey)
        } else {
            AppDefaults.shared.set(
                archivedAtBySession.mapValues { NSNumber(value: $0) },
                forKey: NativeOverlay.archivedAtKey
            )
        }
    }

    /// Restart a session from the phone — the exact desktop path (resume
    /// flag, title, pin, worktree, grants preserved). Restart mints a new
    /// session id, so the phone re-selects by the same command/project on the
    /// next bootstrap poll rather than the dead id.
    func applyRemoteRestartSession(
        _ request: RemoteRestartSessionRequest
    ) throws {
        guard sessionsByID[request.sessionID] != nil else {
            throw MobileRemoteError(404, "Unknown session id: \(request.sessionID)")
        }
        guard sessionCanRestart(request.sessionID) else {
            throw MobileRemoteError(409, "Session cannot be resumed: \(request.sessionID)")
        }
        guard restartSession(request.sessionID) else {
            throw MobileRemoteError(500, "Could not restart session: \(request.sessionID)")
        }
    }

    /// Stop/restart/remove from the iPhone session sheet. Restart and remove
    /// deliberately reuse the desktop paths, so resume behavior, overlay
    /// pruning, and artifact cleanup stay identical.
    func applyRemoteSessionAction(
        _ request: RemoteSessionActionRequest
    ) async throws {
        guard let session = sessionsByID[request.sessionID] else {
            throw MobileRemoteError(404, "Unknown session id: \(request.sessionID)")
        }
        switch request.action {
        case .stop:
            guard session.isLive else {
                throw MobileRemoteError(409, "Session is not running: \(request.sessionID)")
            }
            guard stopSession(request.sessionID) else {
                throw MobileRemoteError(500, "Could not stop session: \(request.sessionID)")
            }
        case .restart:
            try applyRemoteRestartSession(RemoteRestartSessionRequest(sessionID: request.sessionID))
        case .restartAgent, .resumeAgent:
            guard session.isLive else {
                throw MobileRemoteError(409, "Session is not running: \(request.sessionID)")
            }
            guard sessionCanResumeAgent(request.sessionID),
                  !resumingAgentSessionIDs.contains(request.sessionID),
                  !restartingSessionIDs.contains(request.sessionID),
                  !removingSessionIDs.contains(request.sessionID)
            else {
                throw MobileRemoteError(
                    409, "Session does not have a resumable ended agent: \(request.sessionID)"
                )
            }
            // Unlike the desktop button, the Host protocol action is an
            // effect receipt: do not acknowledge until the live Host accepted
            // the generation-bound command. Process launch, wait, stderr
            // drain, and the Host lifecycle lock all stay off MainActor.
            resumingAgentSessionIDs.insert(request.sessionID)
            if let failure = await Self.runResumeAgentHostCommandOffMainActor(
                sessionID: request.sessionID
            ) {
                resumingAgentSessionIDs.remove(request.sessionID)
                throw MobileRemoteError(failure.status, failure.message)
            }
            // Keep the transition flag through the generation rescan so an
            // old agent's shutdown Stop cannot publish finished/unread/push
            // side effects. The generation edge clears it below.
            consumePendingAppendedContext(afterResumingAgent: request.sessionID)
            rescan()
        case .remove:
            confirmRemoveSession(request.sessionID)
        }
    }

    // MARK: - Settings screen (App.svelte openSettings/closeSettings:442-456)

    /// `tab: nil` keeps the current tab, so the gear/⌘, on an already-open
    /// settings view doesn't yank the user back to Presets.
    /// NOTE: never wrap settingsVisible changes in withAnimation — the
    /// sidebar slide is driven by a scoped `.animation(value:)` modifier in
    /// SidebarView, and the content-pane swap (ContentArea) must stay
    /// non-animated so the Metal-backed terminal surface is never part of
    /// an opacity/frame animation (that's what caused the settings blink).
    func openSettings(tab: SettingsTab? = nil) {
        if let tab {
            settingsTab = (tab == .mobile && !UnpeelFeatureFlags.mobileRemoteControlEnabled)
                ? .presets
                : tab
        } else if settingsTab == .mobile && !UnpeelFeatureFlags.mobileRemoteControlEnabled {
            settingsTab = .presets
        }
        // The settings nav takes over the sidebar list area; drop the
        // main-pane library so Back always returns to the project tree.
        archivedProjectID = nil
        recentActivityVisible = false
        settingsVisible = true
    }

    func closeSettings() {
        settingsVisible = false
    }


    /// Open a folder picker and reuse-or-add it as a project. Reports the
    /// chosen path back to the caller; reports `nil` if the user cancels.
    func pickProjectFolder(completion: @escaping @MainActor (String?) -> Void) {
        guard selectedHostScope == .local else {
            completion(nil)
            return
        }
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.title = "Select project folder"
        panel.prompt = "Add Project"
        panel.begin { [weak self] response in
            Task { @MainActor in
                guard response == .OK, let url = panel.url else {
                    completion(nil)
                    return
                }
                // Open the freshly added project (its launcher) by default.
                self?.openLauncher(forFolder: url.path)
                completion(url.path)
            }
        }
    }

    /// `showScanning` blanks the current report so the UI shows the scanning
    /// state; pass false for background refreshes (e.g. after an install
    /// finishes) so the Agent CLI Tools list doesn't flash empty.
    func refreshToolAvailability(
        showScanning: Bool = true,
        completion: (@MainActor (ToolScanReport) -> Void)? = nil
    ) {
        if showScanning { setupToolReport = nil }
        toolScanInProgress = true
        let started = Date()
        toolAvailability.scan { [weak self] report in
            Task { @MainActor in
                guard let self else { return }
                self.setupToolReport = report
                self.seedPresetPreferencesFromUsage(report)
                self.rescan()
                completion?(report)
                // Hold the scanning state briefly on fast scans so the
                // Rescan button's feedback is actually visible.
                let remaining = 0.5 - Date().timeIntervalSince(started)
                if remaining > 0 {
                    try? await Task.sleep(for: .seconds(remaining))
                }
                self.toolScanInProgress = false
            }
        }
    }

    /// Run a missing CLI's official install one-liner in the user's login
    /// shell (so npm/brew/curl resolve the way they do in their terminal),
    /// then rescan the PATH so the row moves into the installed list.
    func installTool(_ tool: SetupTool) {
        guard let command = tool.installCommand,
              !toolInstallsInProgress.contains(tool) else { return }
        toolInstallErrors[tool] = nil
        toolInstallsInProgress.insert(tool)
        DispatchQueue.global(qos: .userInitiated).async {
            let shell = ProcessInfo.processInfo.environment["SHELL"] ?? "/bin/zsh"
            let process = Process()
            process.executableURL = URL(fileURLWithPath: shell)
            process.arguments = ["-l", "-c", command]
            let pipe = Pipe()
            process.standardOutput = pipe
            process.standardError = pipe
            process.standardInput = FileHandle.nullDevice
            var failure: String?
            do {
                try process.run()
                let data = pipe.fileHandleForReading.readDataToEndOfFile()
                process.waitUntilExit()
                if process.terminationStatus != 0 {
                    let output = String(data: data, encoding: .utf8) ?? ""
                    NSLog("Unpeel install %@ failed (exit %d): %@",
                          tool.commandName, process.terminationStatus,
                          String(output.suffix(2000)))
                    // Keep the tail of the real output for the row's hover
                    // tooltip; npm's final "A complete log of this run can be
                    // found in …" pointer is noise, and the actual cause sits
                    // just above it.
                    let lines = output
                        .split(whereSeparator: \.isNewline)
                        .map { $0.trimmingCharacters(in: .whitespaces) }
                        .filter { !$0.isEmpty && !$0.contains("A complete log of this run") }
                    let tail = lines.suffix(5).joined(separator: "\n")
                    failure = tail.isEmpty
                        ? "Install failed (exit \(process.terminationStatus))"
                        : tail
                }
            } catch {
                failure = error.localizedDescription
            }
            Task { @MainActor [weak self] in
                guard let self else { return }
                self.toolInstallsInProgress.remove(tool)
                if let failure {
                    self.toolInstallErrors[tool] = failure
                } else {
                    self.refreshToolAvailability(showScanning: false) { report in
                        // An installer can exit 0 without putting the binary
                        // on the PATH (e.g. the package renamed its bin);
                        // silently restoring the Install button reads as
                        // "nothing happened".
                        if report.status(for: tool)?.installed != true {
                            self.toolInstallErrors[tool] =
                                "Installed, but \(tool.commandName) was not found on your PATH"
                        }
                    }
                }
            }
        }
    }

    /// First-run only (runs off the startup PATH scan): seed the flat preset
    /// order and favorites from detected session-store usage, so an existing
    /// user's most-used CLIs lead the quick strip and the Presets panel.
    /// Never touches an explicit saved order (the absent-`presetOrder`-key
    /// guard also makes it one-shot, so stars and drags stick), and leaves
    /// fresh machines (no usage anywhere) on the app-state order with the
    /// claude/codex default favorites.
    private func seedPresetPreferencesFromUsage(_ report: ToolScanReport) {
        guard !setupCompleted,
              AppDefaults.shared.object(forKey: Self.nativePresetOrderKey) == nil
        else { return }
        guard report.installedStatuses.contains(where: { $0.usage.hasAny }) else { return }
        let orderedCLIs = report.usageOrderedInstalledTools
        let cliRank = Dictionary(
            uniqueKeysWithValues: orderedCLIs.enumerated().map { ($1, $0) }
        )
        // Presets sorted by their CLI's usage rank (unranked CLIs, then custom
        // commands, keep their app-state order at the end).
        let ordered = mergedPresets.enumerated()
            .sorted { lhs, rhs in
                let lhsRank = SetupTool.detect(in: lhs.element.command).flatMap { cliRank[$0] } ?? Int.max
                let rhsRank = SetupTool.detect(in: rhs.element.command).flatMap { cliRank[$0] } ?? Int.max
                if lhsRank != rhsRank { return lhsRank < rhsRank }
                return lhs.offset < rhs.offset
            }
            .map(\.element)
        let seededOrder = ordered.map(\.id)
        applyPresetOrder(seededOrder)
        if presetsInSharedFile {
            // Stamp the legacy key too: it is this seeding's one-shot guard,
            // and an older build sharing the defaults starts from the same
            // order the file now has.
            presetOrder = seededOrder
            savePresetOrder()
        }

        // Favorites = the top 3 actually-used CLIs' leading presets. Unstar
        // the builtin claude/codex defaults when they didn't make the cut, so
        // the quick strip opens with what this user really reaches for.
        let top = Set(orderedCLIs.filter { report.status(for: $0)?.usage.hasAny == true }.prefix(3))
        guard !top.isEmpty else { return }
        var seenCLIs = Set<SetupTool>()
        for preset in ordered {
            guard let cli = SetupTool.detect(in: preset.command) else { continue }
            let want = top.contains(cli) && seenCLIs.insert(cli).inserted
            if preset.quickLaunch != want {
                updatePreset(id: preset.id, quickLaunch: want)
            }
        }
    }

    // MARK: - Appearance (Settings → Appearance)

    /// User picked a mode in the native Appearance panel: persist the
    /// overlay (it wins over app-state.json from now on) and re-apply.
    func setThemePreference(_ preference: ThemePreference) {
        AppDefaults.shared.set(preference.rawValue, forKey: Self.nativeThemeKey)
        guard preference != themePreference else { return }
        themePreference = preference
        applyAppAppearance()
    }

    private func nativeThemeOverride() -> ThemePreference? {
        AppDefaults.shared.string(forKey: Self.nativeThemeKey)
            .flatMap(ThemePreference.init(rawValue:))
    }

    /// User picked a default editor natively: persist the overlay (it wins
    /// over app-state.json's `code_editor`) and apply immediately so the
    /// "Open in editor" button and project menu pick it up.
    func setCodeEditor(_ editor: String) {
        AppDefaults.shared.set(editor, forKey: Self.nativeCodeEditorKey)
        guard editor != codeEditor else { return }
        codeEditor = editor
    }

    private func nativeCodeEditorOverride() -> String? {
        AppDefaults.shared.string(forKey: Self.nativeCodeEditorKey)
    }

    /// The selected editor id, readable without a store instance (cmd-click
    /// file opening runs from the terminal pane). Mirrors the overlay used by
    /// `nativeCodeEditorOverride`; defaults to VS Code like `codeEditor`.
    nonisolated static func preferredCodeEditor() -> String {
        AppDefaults.shared.string(forKey: nativeCodeEditorKey) ?? "code"
    }

    // MARK: - Advanced session cleanup

    private static func normalizedAutoStopArchiveMinutes(_ minutes: Int) -> Int {
        autoStopArchiveMinuteOptions.contains(minutes) ? minutes : 0
    }

    /// Resolve the shared file value: absent key = on at the default cutoff
    /// (opt-out feature); an explicit value — including 0 = Never — wins.
    static func resolvedAutoStopArchiveMinutes(_ stateFile: AppStateFile?) -> Int {
        guard let raw = stateFile?.autoStopArchiveMinutes else {
            return defaultAutoStopArchiveMinutes
        }
        return normalizedAutoStopArchiveMinutes(raw)
    }

    /// One-time fold of the legacy UserDefaults auto-stop minutes into
    /// `auto_stop_archive_minutes` in app-state.json, so the app and the TUI
    /// share a single knob. A key already present in the file wins (already
    /// folded, or written by a peer); an explicit legacy value — including
    /// 0 = Never — must survive the move. The legacy keys stay in the
    /// defaults suite (older builds sharing it keep working) but are never
    /// read again.
    private static func migrateAutoStopArchiveSetting() {
        guard let legacy = AppDefaults.shared
            .object(forKey: legacyAutoSessionStopMinutesKey) as? Int
        else { return }
        _ = PresetStateFile.edit { object in
            if object["auto_stop_archive_minutes"] == nil {
                object["auto_stop_archive_minutes"] = normalizedAutoStopArchiveMinutes(legacy)
            }
        }
    }

    func setAutoStopArchiveMinutes(_ minutes: Int) {
        let normalized = Self.normalizedAutoStopArchiveMinutes(minutes)
        // Always store explicitly — absent means "default on", so Never (0)
        // must be a written value, not a removed key.
        _ = editPresetStateAnnouncing { object in
            object["auto_stop_archive_minutes"] = normalized
        }
        guard normalized != autoStopArchiveMinutes else { return }
        autoStopArchiveMinutes = normalized
        runAutoStopArchiveIfNeeded()
    }

    /// When each session was last seen ENTERING idle, i.e. the start of its
    /// current unbroken idle stretch. Any other status (busy, attention,
    /// starting, exited) clears the entry, so a looping session — hook
    /// Start/Stop cycles, scheduled wake-ups, long compiles — resets its
    /// clock on every iteration and never accumulates "inactivity".
    /// Maintained on every rescan regardless of the auto-stop-and-archive
    /// setting, so enabling the feature acts on idleness that already
    /// accumulated.
    private var idleSinceBySession: [String: Date] = [:]

    /// Same canonical lifecycle floor used by Recently updated. Kept pure so
    /// tests can prove that a fresh hook-capable output repaint never moves
    /// the cleanup clock: the scanner has already excluded that signal from
    /// `lifecycleAtMs`.
    static func inactivityAnchorMs(_ session: SessionEntry) -> Int64 {
        max(session.createdAt, session.lifecycleAtMs ?? 0)
    }

    /// Auto-stop and archive inactive terminals (Settings ▸ Advanced ▸
    /// Cleanup): sessions continuously idle for the selected time get the
    /// same treatment as the sidebar's "Stop and archive" — the host stops,
    /// the row files away into the project's archive library, and everything
    /// stays on disk (Restore + Restart resumes the conversation). Nothing
    /// is ever deleted automatically.
    ///
    /// "Inactive" is deliberately NOT "old": only an unbroken idle stretch
    /// qualifies. Its clock follows provider-aware real activity — durable
    /// hooks for hook-owned agents; parsed-screen/output fallback only for
    /// hookless tools. That makes real remote typing count without allowing
    /// a resize or an idle full-screen repaint to keep Claude/Codex alive.
    private func runAutoStopArchiveIfNeeded() {
        let now = Date()

        // Maintain the continuously-idle map first. A session first observed
        // idle seeds from its canonical lifecycle event, clamped to now — an
        // app restart must not reset hours of already-accumulated idleness.
        // If real activity advances without leaving idle (possible for a
        // hookless terminal), move the anchor forward; never move it back.
        for (id, session) in sessionsByID {
            if session.status == .idle {
                let stamp = Self.inactivityAnchorMs(session)
                // A malformed/legacy manifest with no usable creation or
                // activity timestamp fails safe toward keeping the session.
                let activityAt = stamp > 0
                    ? min(
                        Date(timeIntervalSince1970: TimeInterval(stamp) / 1_000),
                        now
                    )
                    : now
                idleSinceBySession[id] = max(
                    idleSinceBySession[id] ?? activityAt,
                    activityAt
                )
            } else {
                idleSinceBySession[id] = nil
            }
        }
        idleSinceBySession = idleSinceBySession.filter { sessionsByID[$0.key] != nil }

        let minutes = autoStopArchiveMinutes
        guard minutes > 0 else { return }
        let threshold = TimeInterval(minutes) * 60

        let pinnedSessionIDs = Set(
            pinnedByProject.values.flatMap { pins in pins.compactMap { $0.sessionID } }
        )
        for (id, session) in sessionsByID {
            guard session.status == .idle else { continue }
            // Plain shells are exempt: a quiet long-lived process (ssh, a
            // silent daemon) looks idle for days, and a shell has no
            // conversation to resume — Restart can't bring back what was
            // running inside. Agent CLIs are the reclaim target.
            guard !session.command.isEmpty else { continue }
            guard let idleSince = idleSinceBySession[id],
                  now.timeIntervalSince(idleSince) >= threshold
            else { continue }
            guard id != selectedSessionID else { continue }
            guard !pinnedSessionIDs.contains(id) else { continue }
            // Settled while unobserved: the user hasn't seen the result yet,
            // and an archived session's row isn't visible to notice it.
            guard !unreadSessionIDs.contains(id) else { continue }
            guard !archivedSessionIDs.contains(id) else { continue }
            guard !removingSessionIDs.contains(id),
                  !restartingSessionIDs.contains(id)
            else { continue }
            // Unstamped: an aged-out session must not resurface at the top
            // of the archive library. (archiveSession marks the id
            // synchronously, so the rescan this runs inside cannot re-enter
            // on the same session.)
            archiveSession(id, stampRecency: false)
        }
    }

    /// The sidebar shows at most `sidebarVisibleSessionLimit` stopped
    /// rows per project; older ones file away into the archive
    /// library automatically (non-destructive — everything stays on disk and
    /// is restorable from the archive page). Bounded per sweep: the completion
    /// rescan of each batch triggers the next until converged, so a first
    /// run over a long backlog never spawns dozens of cleanup tasks at once.
    private func runArchiveOverflowSweep() {
        var budget = 10
        func sweep(_ nodes: [ProjectNode]) {
            for node in nodes {
                guard budget > 0 else { return }
                for block in stoppedOverflowBlocks(in: node) {
                    guard budget > 0 else { return }
                    var archivedAny = false
                    for session in block
                    where !archivedSessionIDs.contains(session.id) && !session.isLive {
                        archiveSession(session.id, stampRecency: false)
                        archivedAny = true
                    }
                    if archivedAny { budget -= 1 }
                }
                sweep(node.worktrees)
            }
        }
        sweep(nodes)
    }

    /// NSApp-level override (nil = follow macOS) so the window chrome,
    /// SwiftUI dynamic colors, vibrancy and the Ghostty surfaces all
    /// resolve from one appearance.
    private func applyAppAppearance() {
        NSApp.appearance = themePreference.nsAppearance
        writeAppAppearanceFile()
    }

    private func currentAppDarkMode() -> Bool {
        switch themePreference {
        case .dark:
            return true
        case .light:
            return false
        case .system:
            return NSApp.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) != .aqua
        }
    }

    private func writeAppAppearanceFile() {
        let value: String
        switch themePreference {
        case .system:
            value = "system\n"
        case .dark:
            value = "dark\n"
        case .light:
            value = "light\n"
        }
        let url = LaunchConfig.unpeelDir.appendingPathComponent("app-appearance")
        do {
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try value.write(to: url, atomically: true, encoding: .utf8)
        } catch {
            NSLog("[UnpeelNative] failed to write app appearance file: \(error)")
        }
    }

    // MARK: - MCP security (Settings → Sessions MCP)
    //
    // Unlike every other native preference, this is NOT a UserDefaults overlay:
    // Unpeel Sessions MCP reads `mcp_orchestrators` only from app-state.json, so
    // a grant that lived anywhere else would have no effect. It is the one field
    // the native app writes back to the shared file — via a field-preserving
    // read-modify-write (mutateAppStateJSON) that never drops the many keys the
    // native decoder doesn't model.

    private static func commandCanHostSessionsMCP(_ command: String?) -> Bool {
        SetupTool.detect(in: command ?? "")?
            .metadata?.capabilities.contains(.mcpSessions) == true
    }

    /// Write the legacy access-override map back to app-state.json as
    /// `{ role, reach }` objects. Access is no longer configured per session
    /// (reads are open; same-group control is implicit), so nothing
    /// adds new entries — but restart/prune still round-trip any legacy grants
    /// left in `app-state.json` so an old file keeps decoding cleanly.
    private func persistMcpGrants() {
        let snapshot = mcpOrchestrators.mapValues { grant in
            ["role": grant.role.rawValue, "reach": grant.reach.rawValue]
        }
        mutateAppStateJSON { root in
            root["mcp_orchestrators"] = snapshot
        }
    }

    /// Write the user-approved cross-group pairs to `mcp_write_approvals`
    /// in app-state.json so the Sessions MCP host sees them per write.
    private func persistMcpWriteApprovals() {
        let snapshot = mcpWriteApprovals
        mutateAppStateJSON { root in
            root["mcp_write_approvals"] = snapshot
        }
    }

    /// Set the app-wide cross-group write policy (Settings ▸ Sessions MCP).
    /// Applied live: the host re-reads it per tool call.
    func setMcpNonChildWriteAccess(_ policy: McpNonChildWriteAccess) {
        guard policy != mcpNonChildWriteAccess else { return }
        mcpNonChildWriteAccess = policy
        mutateAppStateJSON { root in
            root["mcp_nonchild_write_access"] = policy.rawValue
        }
    }

    /// Remember the user's "Allow" answer for a caller→target write pair. The
    /// approval is directional and per pair; it lives until either session is
    /// removed (pruneNativeState) and follows a restart's new session id.
    func approveMcpWrite(caller: String, target: String) {
        var targets = mcpWriteApprovals[caller] ?? []
        guard !targets.contains(target) else { return }
        targets.append(target)
        mcpWriteApprovals[caller] = targets
        persistMcpWriteApprovals()
    }

    /// Revoke one remembered pair (Settings ▸ Sessions MCP ▸ approved list).
    /// The next write from `caller` to `target` asks again.
    func revokeMcpWriteApproval(caller: String, target: String) {
        guard var targets = mcpWriteApprovals[caller],
              targets.contains(target) else { return }
        targets.removeAll { $0 == target }
        if targets.isEmpty {
            mcpWriteApprovals.removeValue(forKey: caller)
        } else {
            mcpWriteApprovals[caller] = targets
        }
        persistMcpWriteApprovals()
    }

    /// Drop every approval involving a removed session (as writer or target).
    private func pruneMcpWriteApprovals(forRemovedSession sessionID: String) {
        var pruned = mcpWriteApprovals
        pruned.removeValue(forKey: sessionID)
        pruned = pruned.compactMapValues { targets in
            let kept = targets.filter { $0 != sessionID }
            return kept.isEmpty ? nil : kept
        }
        guard pruned != mcpWriteApprovals else { return }
        mcpWriteApprovals = pruned
        persistMcpWriteApprovals()
    }

    /// A restart mints a new session id; keep remembered approvals alive by
    /// re-adding, under the new id, every pair the old id appeared in (as
    /// writer or target). The snapshot is captured BEFORE pruneNativeState
    /// drops the old id's entries — same read-before-prune discipline as the
    /// carried access grant and provider conversation id.
    private func carryMcpWriteApprovals(
        snapshot: [String: [String]], from oldID: String, to newID: String
    ) {
        var changed = false
        // Pairs where the restarted session was the approved writer.
        for target in snapshot[oldID] ?? [] where target != newID {
            var targets = mcpWriteApprovals[newID] ?? []
            if !targets.contains(target) {
                targets.append(target)
                mcpWriteApprovals[newID] = targets
                changed = true
            }
        }
        // Pairs where it was the approved target.
        for (caller, targets) in snapshot
        where caller != oldID && caller != newID && targets.contains(oldID) {
            var kept = mcpWriteApprovals[caller] ?? []
            if !kept.contains(newID) {
                kept.append(newID)
                mcpWriteApprovals[caller] = kept
                changed = true
            }
        }
        if changed {
            persistMcpWriteApprovals()
        }
    }

    // MARK: - Computer MCP access (Settings → Computer)
    //
    // Same persistence contract as browser access: the unified MCP server
    // reads `computer_default_access` and `computer_approvals` from
    // app-state.json per call, so every change here applies live.

    /// Set the app-wide Computer access mode. Off applies live through the
    /// per-call gate; enabling reaches existing sessions at their next
    /// natural restart (domain advertising is launch-time). The engine
    /// daemon follows the mode: it runs exactly while access isn't Off.
    func setDefaultComputerAccess(_ access: ComputerAccess) {
        guard access != computerDefaultAccess else { return }
        computerDefaultAccess = access
        let value = access.rawValue
        mutateAppStateJSON { root in
            root["computer_default_access"] = value
        }
        ComputerEngineManager.shared.sync()
        rescan()
    }

    private func persistComputerApprovals() {
        let snapshot = computerApprovals
        mutateAppStateJSON { root in
            root["computer_approvals"] = snapshot
        }
    }

    /// Remember the user's "Allow" answer for one session (Ask mode). Lives
    /// until the session is removed; follows a restart's new session id.
    func approveComputerAccess(sessionID: String) {
        guard !computerApprovals.contains(sessionID) else { return }
        computerApprovals.append(sessionID)
        persistComputerApprovals()
    }

    /// Revoke one remembered approval (Settings ▸ Computer). The session's
    /// next computer action asks again.
    func revokeComputerApproval(sessionID: String) {
        guard computerApprovals.contains(sessionID) else { return }
        computerApprovals.removeAll { $0 == sessionID }
        persistComputerApprovals()
    }

    private func pruneComputerApprovals(forRemovedSession sessionID: String) {
        guard computerApprovals.contains(sessionID) else { return }
        computerApprovals.removeAll { $0 == sessionID }
        persistComputerApprovals()
    }

    /// A restart mints a new session id; keep the remembered approval alive.
    /// `approved` is captured BEFORE pruneNativeState drops the old id — the
    /// read-before-prune discipline every carried per-session fact uses.
    private func carryComputerApproval(approved: Bool, to newID: String) {
        guard approved, !computerApprovals.contains(newID) else { return }
        computerApprovals.append(newID)
        persistComputerApprovals()
    }

    // MARK: - Unpeel Link profile (Settings ▸ Remote ▸ Unpeel Link)
    //
    // The TUI edits the same keys, so these go through the flocked shared-file
    // editor (a concurrent TUI edit is never lost) and announce, so the other
    // frontend repaints at once.

    /// Save the presence nickname (`profile_display_name`).
    func setProfileDisplayName(_ name: String) {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed != profileDisplayName else { return }
        profileDisplayName = trimmed
        _ = editPresetStateAnnouncing { root in
            root["profile_display_name"] = trimmed
        }
    }

    /// Save the presence avatar (`profile_avatar`, an emoji from the shared
    /// picker set — `LinkLicenseSections.avatarChoices`, the TUI's
    /// `LINK_AVATARS`).
    func setProfileAvatar(_ avatar: String) {
        guard avatar != profileAvatar else { return }
        profileAvatar = avatar
        _ = editPresetStateAnnouncing { root in
            root["profile_avatar"] = avatar
        }
    }

    /// Toggle whether sessions may create worktrees (Settings ▸ Sessions
    /// use). Applied live: the host re-reads the flag per tool call.
    func setMcpWorktreeAccess(_ enabled: Bool) {
        guard enabled != mcpWorktreeAccess else { return }
        mcpWorktreeAccess = enabled
        mutateAppStateJSON { root in
            root["mcp_worktree_access"] = enabled
        }
    }

    /// Toggle Browser MCP's default screenshot destination. Applied live: the
    /// browser host re-reads the shared app-state key for every screenshot.
    func setMcpAutoAddBrowserScreenshots(_ enabled: Bool) {
        guard enabled != mcpAutoAddBrowserScreenshots else { return }
        mcpAutoAddBrowserScreenshots = enabled
        mutateAppStateJSON { root in
            root["mcp_auto_add_browser_screenshots"] = enabled
        }
    }

    private func persistBrowserApprovals() {
        let snapshot = browserApprovals
        mutateAppStateJSON { root in
            root["browser_approvals"] = snapshot
        }
    }

    /// Remember the user's "Allow" answer for one session (browser Ask mode).
    func approveBrowserAccess(sessionID: String) {
        guard !browserApprovals.contains(sessionID) else { return }
        browserApprovals.append(sessionID)
        persistBrowserApprovals()
    }

    /// Revoke one remembered browser approval (Settings ▸ Browser).
    func revokeBrowserApproval(sessionID: String) {
        guard browserApprovals.contains(sessionID) else { return }
        browserApprovals.removeAll { $0 == sessionID }
        persistBrowserApprovals()
    }

    private func pruneBrowserApprovals(forRemovedSession sessionID: String) {
        guard browserApprovals.contains(sessionID) else { return }
        browserApprovals.removeAll { $0 == sessionID }
        persistBrowserApprovals()
    }

    private func carryBrowserApproval(approved: Bool, to newID: String) {
        guard approved, !browserApprovals.contains(newID) else { return }
        browserApprovals.append(newID)
        persistBrowserApprovals()
    }

    // MARK: - Browser MCP access (Settings → Browser)
    //
    // Same persistence contract as the Sessions MCP default grant above: the
    // host's `browser` MCP domain reads `browser_default_access` only from
    // app-state.json (re-read per tool call), so the native app writes it
    // through the same field-preserving mutateAppStateJSON. Browser access is a
    // single app-wide on/off — there is no per-session override.

    /// Set the app-wide Browser Access and persist it. Browser access is a
    /// single global on/off — every capable session uses this. Applied live:
    /// the host re-reads `browser_default_access` per tool call.
    func setDefaultBrowserAccess(_ access: BrowserAccess) {
        guard access != browserDefaultAccess else { return }
        browserDefaultAccess = access
        persistBrowserDefaultAccess()
        rescan()
    }

    private func persistBrowserDefaultAccess() {
        let value = browserDefaultAccess.rawValue
        mutateAppStateJSON { root in
            root["browser_default_access"] = value
        }
    }

    /// Update the app-wide browser engine options and persist them. Applied
    /// live: the host re-reads `browser_settings` on every engine invocation.
    func updateBrowserSettings(_ mutate: (inout BrowserSettings) -> Void) {
        var updated = browserSettings
        mutate(&updated)
        guard updated != browserSettings else { return }
        browserSettings = updated
        let snapshot: [String: Any] = [
            "headed": updated.headed,
            "allowed_domains": updated.allowedDomains,
            "profile_mode": updated.profileMode,
            "executable_path": updated.executablePath,
            "show_cursor": updated.showCursor,
        ]
        mutateAppStateJSON { root in
            root["browser_settings"] = snapshot
        }
    }

    /// Update the app-wide transcript rendering options and persist them.
    /// Applied live: the host re-reads `transcript_settings` from app-state.json
    /// each time it builds a Markdown transcript (Copy Transcript / MCP
    /// `read_transcript`), so changes take effect on the next copy or read.
    func updateTranscriptSettings(_ mutate: (inout TranscriptSettings) -> Void) {
        var updated = transcriptSettings
        mutate(&updated)
        guard updated != transcriptSettings else { return }
        transcriptSettings = updated
        let snapshot: [String: Any] = [
            "include_user": updated.includeUser,
            "include_assistant": updated.includeAssistant,
            "include_reasoning": updated.includeReasoning,
            "include_tools": updated.includeTools,
            "include_file_changes": updated.includeFileChanges,
            "include_plan_updates": updated.includePlanUpdates,
            "include_session_info": updated.includeSessionInfo,
            "max_entries": updated.maxEntries,
        ]
        mutateAppStateJSON { root in
            root["transcript_settings"] = snapshot
        }
    }

    /// Delete the "kept per project" browsing data: the Unpeel-managed
    /// profiles (localStorage/cache) AND the engine's saved login state.
    /// A live engine daemon keeps its already-open browser state until that
    /// session's browser closes; new browsers start clean.
    func clearBrowserProfiles() {
        let dir = LaunchConfig.unpeelDir.appendingPathComponent("browser/profiles")
        try? FileManager.default.removeItem(at: dir)
        // Login *state* (cookies) lives in the engine's own store, not the
        // profile dir — the host saves it per project as
        // ~/.agent-browser/sessions/unpeel-proj-*.json[.enc]. Clearing must
        // cover both or "Clear" silently keeps every login. Always the real
        // home: the engine ignores UNPEEL_HOME.
        let engineSessions = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".agent-browser/sessions")
        if let files = try? FileManager.default.contentsOfDirectory(
            at: engineSessions, includingPropertiesForKeys: nil
        ) {
            for file in files where file.lastPathComponent.hasPrefix("unpeel-proj-") {
                try? FileManager.default.removeItem(at: file)
            }
        }
    }

    /// Effective MCP block state for a project, including explicit id-keyed
    /// blocks, legacy project flags, and inherited parent/worktree blocks.
    func projectMcpBlocked(_ projectID: String) -> Bool {
        var current = projectID
        for _ in 0..<16 {
            if mcpBlockedProjectIDs.contains(current) { return true }
            guard let project = projectsByID[current] else { return false }
            if project.mcpBlocked == true { return true }
            guard let parent = project.parentProjectID else { return false }
            current = parent
        }
        return false
    }

    /// Block or unblock a project from MCP by id (works for overlay-only and
    /// worktree projects, which never appear in app-state.json's `projects`).
    func setProjectMcpBlocked(_ projectID: String, blocked: Bool) {
        let affectedIDs = projectIDAndWorktreeDescendants(projectID)
        if blocked { mcpBlockedProjectIDs.formUnion(affectedIDs) }
        else { mcpBlockedProjectIDs.subtract(affectedIDs) }
        mutateAppStateJSON { root in
            var ids = (root["mcp_blocked_projects"] as? [String]) ?? []
            ids.removeAll { affectedIDs.contains($0) }
            if blocked {
                ids.append(contentsOf: affectedIDs.sorted())
            }
            root["mcp_blocked_projects"] = ids
        }
    }

    private func projectIDAndWorktreeDescendants(_ projectID: String) -> Set<String> {
        var result: Set<String> = [projectID]
        var stack = [projectID]
        while let current = stack.popLast() {
            let children = projectsByID.values.filter {
                $0.parentProjectID == current && $0.worktreeBranch != nil
            }
            for child in children where !result.contains(child.id) {
                result.insert(child.id)
                stack.append(child.id)
            }
        }
        return result
    }

    /// Read-modify-write app-state.json at the JSON-object level so unmodeled
    /// keys (presets, tags, saved_sessions, …) survive untouched. Writes
    /// atomically; seeds a minimal object if the file is absent or unreadable.
    /// Invalidates the decode cache so the next rescan re-reads the new value.
    private func mutateAppStateJSON(_ mutate: (inout [String: Any]) -> Void) {
        let url = LaunchConfig.appStateFile
        var root: [String: Any] = {
            guard let data = try? Data(contentsOf: url),
                  let object = try? JSONSerialization.jsonObject(with: data),
                  let dict = object as? [String: Any]
            else { return [:] }
            return dict
        }()
        Self.seedAppStateDefaults(&root)
        mutate(&root)
        guard let data = try? JSONSerialization.data(
            withJSONObject: root, options: [.prettyPrinted, .sortedKeys]
        ) else {
            NSLog("[UnpeelNative] failed to serialize app-state.json for MCP security write")
            return
        }
        do {
            // Ensure the directory exists, then write atomically.
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(), withIntermediateDirectories: true
            )
            try data.write(to: url, options: .atomic)
            appStateCache = nil
        } catch {
            NSLog("[UnpeelNative] failed to write app-state.json: \(error)")
        }
    }

    private static func seedAppStateDefaults(_ root: inout [String: Any]) {
        func assign(_ key: String, _ value: @autoclosure () -> Any) {
            if root[key] == nil {
                root[key] = value()
            }
        }

        assign("projects", [] as [Any])
        assign("active_project_id", NSNull())
        assign("workspaces", [["id": "personal", "name": "Personal"]])
        assign("active_workspace_id", "personal")
        assign("presets", builtinPresetJSON())
        assign("tags", [] as [Any])
        assign("active_tabs", [:] as [String: Any])
        assign("saved_sessions", [] as [Any])
        assign("pinned_sessions", [:] as [String: Any])
        assign("theme", "system")
        assign("color_scheme", "default")
        assign("code_editor", "code")
        assign("last_sessions", [:] as [String: Any])
        assign("setup_completed", false)
        assign("mcp_orchestrators", [:] as [String: Any])
        assign("mcp_default_access", ["role": "read", "reach": "project"])
        assign("mcp_nonchild_write_access", "ask")
        assign("mcp_write_approvals", [:] as [String: Any])
        assign("mcp_blocked_projects", [] as [Any])
        assign("browser_default_access", "on")
    }

    /// Browser access is a single app-wide switch (Settings ▸ Browser), and the
    /// Device MCP was removed entirely. Strip any legacy per-session
    /// `browser_access` / `device_access` maps (and the removed
    /// `device_default_access`) so a stale entry can't silently override the
    /// global default on the host's per-call gate. No-op once cleared.
    private func migrateAwayFromPerSessionMediaAccess() {
        guard let data = try? Data(contentsOf: LaunchConfig.appStateFile),
              let object = try? JSONSerialization.jsonObject(with: data),
              let dict = object as? [String: Any]
        else { return }
        let hasBrowser = (dict["browser_access"] as? [String: Any])?.isEmpty == false
        let hasDevice = (dict["device_access"] as? [String: Any])?.isEmpty == false
        let hasDeviceDefault = dict["device_default_access"] != nil
        guard hasBrowser || hasDevice || hasDeviceDefault else { return }
        mutateAppStateJSON { root in
            root.removeValue(forKey: "browser_access")
            root.removeValue(forKey: "device_access")
            root.removeValue(forKey: "device_default_access")
        }
    }

    private static func builtinPresetJSON() -> [[String: Any]] {
        Preset.builtinGlobalPresets.map { preset in
            [
                "id": preset.id,
                "label": preset.label,
                "command": preset.command,
                "project_id": NSNull(),
                "enabled": preset.enabled,
                "quick_launch": preset.quickLaunch,
            ]
        }
    }

    func handleHookEvent(_ event: HookEvent) {
        let sessionID = event.sessionID
        let wasHookOwned = activity.hookOwnedState(sessionID) != nil
        let diskLaunch = Self.runtimeLaunchSnapshotOnDisk(sessionID)
        let manifestGeneration = diskLaunch?.generation
            ?? runtimeLaunchGenerations[sessionID]
        let currentGeneration = [manifestGeneration, activity.latestRuntimeGeneration(sessionID)]
            .compactMap { $0 }
            .max()
        let runtimeLaunchedAt = diskLaunch?.launchedAt
            ?? runtimeLaunchCutoffs[sessionID]
        let currentGenerationOwned = currentGeneration.map {
            activity.hasRuntimeOwnership(sessionID, generation: $0)
        } ?? false
        let runtimeDecision = Self.hookRuntimeDecision(
            eventGeneration: event.runtimeGeneration,
            hookEventName: event.hookEventName,
            receivedAt: event.receivedAt,
            currentGeneration: currentGeneration,
            runtimeLaunchedAt: runtimeLaunchedAt,
            currentGenerationOwned: currentGenerationOwned
        )
        guard case let .accept(effectiveGeneration) = runtimeDecision else {
            if ProcessInfo.processInfo.environment["UNPEEL_DEBUG"] == "1" {
                NSLog(
                    "[activity] discarded stale hook session=%@ event=%@ event-generation=%@ current-generation=%@",
                    sessionID,
                    event.hookEventName,
                    event.runtimeGeneration.map(String.init) ?? "legacy",
                    currentGeneration.map(String.init) ?? "unknown"
                )
            }
            return
        }

        // Capture provider metadata so Restart can resume the exact
        // conversation and MCP can read the provider JSONL transcript.
        if event.providerSessionID != nil || event.providerTranscriptPath != nil {
            recordProviderMetadata(
                providerSessionID: event.providerSessionID,
                providerTranscriptPath: event.providerTranscriptPath,
                for: sessionID
            )
        }

        switch event.hookEventName {
        case "Start", "UserPromptSubmit":
            deferredStopEffects.removeValue(forKey: sessionID)?.task.cancel()
            activity.applyHookEvent(
                sessionID: sessionID,
                hookEventName: event.hookEventName,
                runtimeGeneration: effectiveGeneration,
                now: event.receivedAt
            )
            completedSessionIDs.remove(sessionID)
            removeUnread(sessionID)
        case "Stop", "StopFailure":
            activity.applyHookEvent(
                sessionID: sessionID,
                hookEventName: event.hookEventName,
                runtimeGeneration: effectiveGeneration,
                now: event.receivedAt
            )
            // TERM during an in-place restart makes the departing provider
            // emit its ordinary Stop hook. Preserve the hook long enough for
            // the generation reset to distinguish a fast new Stop from the
            // old one, but never publish history/unread/push side effects for
            // the restart-induced shutdown itself.
            deferStopEffectsUntilRuntimeGenerationSettles(
                sessionID: sessionID,
                eventGeneration: effectiveGeneration
            )
        case "PermissionRequest":
            // AskUserQuestion is the agent asking in-band, not a permission
            // gate; it latches the session but never shows attention
            // (App.svelte:666-678).
            let latchOnly = event.toolName == "AskUserQuestion"
            activity.applyHookEvent(
                sessionID: sessionID,
                hookEventName: event.hookEventName,
                latchOnly: latchOnly,
                runtimeGeneration: effectiveGeneration,
                now: event.receivedAt
            )
            if !latchOnly {
                completedSessionIDs.remove(sessionID)
                logActivity(.needsInput, sessionID: sessionID)
                let menuDecision = Self.permissionRequestNotificationDecision(
                    previous: menuPromptNotificationStates[sessionID],
                    runtimeGeneration: effectiveGeneration
                )
                if let state = menuDecision.state {
                    menuPromptNotificationStates[sessionID] = state
                }
                let policy = Self.notificationDeliveryPolicy(
                    macIsObserving: observedSessionID == sessionID,
                    anyControllerIsViewing: ViewerPresenceStore.shared
                        .hasLiveMobileViewer(sessionID: sessionID)
                )
                if policy.markUnread {
                    markUnread(sessionID)
                }
                // Looking at the same session on this Mac suppresses only the
                // redundant Mac banner. Background phones still receive the
                // time-sensitive Link alert; each phone is filtered below if
                // that exact device is already viewing the session.
                if menuDecision.sendNotification {
                    dispatchSessionPush(
                        sessionID: sessionID,
                        kind: .needsInput,
                        sendDesktop: policy.sendDesktop
                    )
                }
            }
        default:
            // Unknown events still latch hook ownership
            // (transition_state latches before the state match).
            activity.applyHookEvent(
                sessionID: sessionID,
                hookEventName: event.hookEventName,
                latchOnly: true,
                runtimeGeneration: effectiveGeneration,
                now: event.receivedAt
            )
        }

        if ProcessInfo.processInfo.environment["UNPEEL_DEBUG"] == "1" {
            let state = activity.hookOwnedState(sessionID)
            NSLog(
                "[activity] session=%@ event=%@ state=%@ hook-latched=%@%@",
                sessionID,
                event.hookEventName,
                state.map(String.init(describing:)) ?? "none",
                activity.hookOwnedState(sessionID) != nil ? "yes" : "no",
                wasHookOwned ? "" : " (first hook event — output heuristic disabled)"
            )
        }

        // Reflect the transition immediately instead of waiting for the
        // next 1s tick (the Svelte app updates on the hook-event push too).
        rescan()
    }

    private func deferStopEffectsUntilRuntimeGenerationSettles(
        sessionID: String,
        eventGeneration: UInt64?
    ) {
        let observedGeneration = eventGeneration
            ?? Self.runtimeLaunchSnapshotOnDisk(sessionID)?.generation
            ?? runtimeLaunchGenerations[sessionID]
        deferredStopEffects[sessionID]?.task.cancel()
        let token = UUID()
        let task = Task { [weak self] in
            try? await Task.sleep(nanoseconds: Self.deferredStopEffectDelay)
            guard !Task.isCancelled, let self else { return }
            let currentGeneration = Self.runtimeLaunchSnapshotOnDisk(sessionID)?.generation
            guard Self.shouldPublishDeferredStopEffects(
                observedGeneration: observedGeneration,
                currentGeneration: currentGeneration
            ), self.deferredStopEffects[sessionID]?.token == token
            else {
                if self.deferredStopEffects[sessionID]?.token == token {
                    self.deferredStopEffects.removeValue(forKey: sessionID)
                }
                return
            }
            self.deferredStopEffects.removeValue(forKey: sessionID)
            self.publishStopEffects(sessionID: sessionID)
        }
        deferredStopEffects[sessionID] = DeferredStopEffects(
            token: token,
            runtimeGeneration: observedGeneration,
            task: task
        )
    }

    private func publishStopEffects(sessionID: String) {
        completedSessionIDs.insert(sessionID)
        defer { persistActivitySnapshot() }
        // History feed records every proven settle, observed or not.
        logActivity(.finished, sessionID: sessionID)
        let policy = Self.notificationDeliveryPolicy(
            macIsObserving: observedSessionID == sessionID,
            anyControllerIsViewing: ViewerPresenceStore.shared
                .hasLiveMobileViewer(sessionID: sessionID)
        )
        if policy.markUnread {
            markUnread(sessionID)
        }
        // Opted-in "notify when done" → phone push (once the session
        // actually settles). Mac observation suppresses only its own banner;
        // a background phone still receives the Link alert.
        if notifyWhenDoneSessionIDs.contains(sessionID) {
            dispatchSessionPush(
                sessionID: sessionID,
                kind: .done,
                sendDesktop: policy.sendDesktop
            )
        }
    }

    nonisolated static func shouldPublishDeferredStopEffects(
        observedGeneration: UInt64?,
        currentGeneration: UInt64?
    ) -> Bool {
        observedGeneration == currentGeneration
    }

    private struct RuntimeLaunchSnapshot {
        let generation: UInt64
        let launchedAt: Date?
    }

    private nonisolated static func runtimeLaunchSnapshotOnDisk(
        _ sessionID: String
    ) -> RuntimeLaunchSnapshot? {
        let url = LaunchConfig.appSessionsDir
            .appendingPathComponent(sessionID)
            .appendingPathComponent("manifest.json")
        guard let data = try? Data(contentsOf: url),
              let manifest = try? JSONDecoder().decode(HostedSessionManifest.self, from: data)
        else { return nil }
        return RuntimeLaunchSnapshot(
            generation: manifest.runtimeLaunchGeneration,
            launchedAt: manifest.runtimeLaunchedAt.map {
                Date(timeIntervalSince1970: TimeInterval($0) / 1_000)
            }
        )
    }

    /// Selection / focus changed: clear the now-observed session's unread
    /// and run the pending-unread reconciliation (App.svelte
    /// handleSelectSession:1264-1267 + the reconcile $effect:806-828).
    private func handleObservationChanged() {
        // ⌘ hints can't outlive app focus: the flagsChanged monitor never
        // sees the release once another app is frontmost.
        if !NSApp.isActive, commandHintsVisible {
            commandHintsVisible = false
        }
        reconcileUnread()
        refreshTitlebarBranch()
    }

    private func reconcileUnread() {
        var states: [String: SessionStatus] = [:]
        for (id, session) in sessionsByID { states[id] = session.status }

        let result = UnreadReconciliation.reconcile(
            pendingUnreadSessions: pendingUnreadSessions,
            sessionStates: states,
            completedSessionIDs: completedSessionIDs,
            previousObservedSessionID: previousObservedSessionID,
            currentObservedSessionID: observedSessionID
        )
        pendingUnreadSessions = result.pendingUnreadSessions
        for sessionID in result.unreadToClear { removeUnread(sessionID) }
        for sessionID in result.unreadToMark { markUnread(sessionID) }
        previousObservedSessionID = observedSessionID
        persistActivitySnapshot()
    }

    private func markUnread(_ sessionID: String) {
        guard !unreadSessionIDs.contains(sessionID) else { return }
        unreadSessionIDs.insert(sessionID)
        // Unread blocks stay visible past the stopped-group window.
        invalidateSidebarLists()
    }

    /// Append one event to the persisted history feed (the Recent panel).
    /// Same-kind repeats per session collapse in ActivityLogStore, so a
    /// permission loop or back-to-back turn finishes bump one row.
    private func logActivity(_ kind: ActivityLogEntry.Kind, sessionID: String) {
        guard let session = sessionsByID[sessionID]
            ?? pendingSessions[sessionID]
            ?? restartPlaceholders[sessionID]
        else { return }
        let title = session.label.trimmingCharacters(in: .whitespacesAndNewlines)
        activityLog.append(ActivityLogEntry(
            id: UUID().uuidString,
            sessionID: sessionID,
            kind: kind,
            at: UInt64(Date().timeIntervalSince1970 * 1000),
            title: title.isEmpty ? "Untitled session" : title,
            command: session.command,
            projectID: session.projectID,
            projectName: projectsByID[session.projectID] != nil
                ? activityProjectName(session.projectID) : ""
        ))
        activityLogEntries = activityLog.entries
    }

    /// `scanSessions` runs before the rebuilt Session index is installed. Drain
    /// its rising menu edges here, once labels/liveness are authoritative, and
    /// reject a stale edge if an in-place restart advanced again meanwhile.
    private func publishPendingMenuPromptNotifications() {
        let pending = pendingMenuPromptNotifications
        pendingMenuPromptNotifications.removeAll(keepingCapacity: true)
        for (sessionID, runtimeGeneration) in pending {
            guard runtimeLaunchGenerations[sessionID] == runtimeGeneration,
                  sessionsByID[sessionID]?.isLive == true
            else { continue }
            completedSessionIDs.remove(sessionID)
            logActivity(.needsInput, sessionID: sessionID)
            let policy = Self.notificationDeliveryPolicy(
                macIsObserving: observedSessionID == sessionID,
                anyControllerIsViewing: ViewerPresenceStore.shared
                    .hasLiveMobileViewer(sessionID: sessionID)
            )
            if policy.markUnread {
                markUnread(sessionID)
            }
            dispatchSessionPush(
                sessionID: sessionID,
                kind: .needsInput,
                sendDesktop: policy.sendDesktop
            )
        }
    }

    /// The push reasons, matched to the phone's notification copy + tap
    /// handling (`kind` in the APNs payload).
    enum SessionPushKind: String {
        case needsInput = "needs_input"
        case done
        case approval
        case test
    }

    struct NotificationDeliveryPolicy: Equatable {
        let markUnread: Bool
        let sendDesktop: Bool
    }

    struct MenuPromptNotificationState: Equatable {
        let runtimeGeneration: UInt64
        let active: Bool
        /// True once either the menu edge or its matching PermissionRequest
        /// hook has emitted the needs-input notification.
        let notificationSent: Bool
    }

    struct MenuPromptNotificationDecision: Equatable {
        let state: MenuPromptNotificationState
        let sendNotification: Bool
    }

    /// Reduce one host-observed `menu_prompt_active` sample. The first app scan
    /// seeds state without alerting, while a session first discovered after
    /// startup may alert immediately if it is already active. Every later
    /// false -> true edge alerts once. A runtime generation change is also a
    /// re-arm because the Host resets this flag when it launches the new agent,
    /// even if native missed that short-lived false write.
    nonisolated static func menuPromptNotificationDecision(
        previous: MenuPromptNotificationState?,
        runtimeGeneration: UInt64,
        active: Bool,
        initialAppScan: Bool,
        detectionEnabled: Bool,
        dismissed: Bool,
        hookAlreadyNeedsInput: Bool
    ) -> MenuPromptNotificationDecision {
        guard let previous else {
            let send = active
                && !initialAppScan
                && detectionEnabled
                && !dismissed
                && !hookAlreadyNeedsInput
            return MenuPromptNotificationDecision(
                state: MenuPromptNotificationState(
                    runtimeGeneration: runtimeGeneration,
                    active: active,
                    // A PermissionRequest can arrive before native's first
                    // manifest sample. Remember that delivery so the menu and
                    // a repeated hook cannot emit the same semantic alert.
                    notificationSent: active && (send || hookAlreadyNeedsInput)
                ),
                sendNotification: send
            )
        }
        guard active else {
            return MenuPromptNotificationDecision(
                state: MenuPromptNotificationState(
                    runtimeGeneration: runtimeGeneration,
                    active: false,
                    notificationSent: false
                ),
                sendNotification: false
            )
        }

        let rose = previous.runtimeGeneration != runtimeGeneration || !previous.active
        guard rose else {
            return MenuPromptNotificationDecision(
                state: previous,
                sendNotification: false
            )
        }
        let send = detectionEnabled && !dismissed && !hookAlreadyNeedsInput
        return MenuPromptNotificationDecision(
            state: MenuPromptNotificationState(
                runtimeGeneration: runtimeGeneration,
                active: true,
                // A hook-owned attention state means the hook already emitted
                // this semantic alert. Otherwise record only an actual menu
                // delivery; a disabled menu detector must never suppress a
                // later authoritative PermissionRequest hook.
                notificationSent: send || hookAlreadyNeedsInput
            ),
            sendNotification: send
        )
    }

    /// Claim an authoritative PermissionRequest against the currently visible
    /// menu. If the menu edge already alerted, suppress only that duplicate.
    /// An initially-active menu has not alerted, so a later hook still sends.
    nonisolated static func permissionRequestNotificationDecision(
        previous: MenuPromptNotificationState?,
        runtimeGeneration: UInt64?
    ) -> (state: MenuPromptNotificationState?, sendNotification: Bool) {
        guard let previous,
              let runtimeGeneration,
              previous.runtimeGeneration == runtimeGeneration,
              previous.active
        else { return (previous, true) }
        guard !previous.notificationSent else { return (previous, false) }
        return (
            MenuPromptNotificationState(
                runtimeGeneration: previous.runtimeGeneration,
                active: true,
                notificationSent: true
            ),
            true
        )
    }

    /// Desktop observation and phone delivery are independent channels. A
    /// user looking at a session on this Mac does not imply that every paired
    /// phone is also being watched; Link still delivers to background phones.
    /// Any live Controller viewer suppresses the local unread/banner, while
    /// phone fan-out applies the precise per-device check below.
    nonisolated static func notificationDeliveryPolicy(
        macIsObserving: Bool,
        anyControllerIsViewing: Bool
    ) -> NotificationDeliveryPolicy {
        let observedAnywhere = macIsObserving || anyControllerIsViewing
        return NotificationDeliveryPolicy(
            markUnread: !observedAnywhere,
            sendDesktop: !observedAnywhere
        )
    }

    /// Notify about a session transition on whichever device the user is at.
    /// The caller decides whether the local Mac banner is redundant; phone
    /// delivery remains independent and is filtered per paired device below.
    /// Approval pushes skip the macOS banner because the floating prompt panel
    /// is already on screen there.
    private func dispatchSessionPush(
        sessionID: String,
        kind: SessionPushKind,
        bodyOverride: String? = nil,
        sendDesktop: Bool = true
    ) {
        guard let session = sessionsByID[sessionID] else { return }
        let rawTitle = session.label.trimmingCharacters(in: .whitespacesAndNewlines)
        let title = rawTitle.isEmpty ? "Unpeel session" : rawTitle
        let body: String
        switch kind {
        case .needsInput: body = bodyOverride ?? "Needs your input"
        case .done: body = bodyOverride ?? "Finished"
        case .approval: body = bodyOverride ?? "Waiting for your approval"
        case .test: body = bodyOverride ?? "Link notifications are working"
        }
        // macOS banner (no-op if the user denied notification permission).
        if sendDesktop && kind != .approval && kind != .test {
            DesktopNotifier.shared.notify(
                title: title, body: body, sessionID: sessionID, kind: kind.rawValue
            )
        }
        dispatchPhonePush(
            sessionID: sessionID,
            title: title,
            body: body,
            kind: kind,
            suppressViewingTargets: true
        )
    }

    /// Deterministic Settings diagnostic. Unlike a lifecycle alert, an
    /// explicit test intentionally bypasses viewer suppression so it can prove
    /// the complete production TestFlight → Link → APNs path while the
    /// user has the settings screens open.
    func sendTestPhoneNotification() {
        let sessionID = selectedSessionID
            ?? sessionsByID.values.first?.id
            ?? "unpeel-test"
        dispatchPhonePush(
            sessionID: sessionID,
            title: "Unpeel",
            body: "Link notifications are working",
            kind: .test,
            suppressViewingTargets: false
        )
    }

    private func dispatchPhonePush(
        sessionID: String,
        title: String,
        body: String,
        kind: SessionPushKind,
        suppressViewingTargets: Bool
    ) {
        // Phone push uses Link/APNs even when terminal traffic is currently
        // Direct or SSH. The Relay WebSocket does not need to be connected.
        guard let server = hostRemoteServer else { return }
        let targets = server.pairingStore.pushTargets()
        guard !targets.isEmpty else { return }
        let eligibleTargets = targets.filter { target in
            !suppressViewingTargets
                || !ViewerPresenceStore.shared.isDeviceViewing(
                    sessionID: sessionID,
                    deviceID: target.deviceID
                )
        }
        guard !eligibleTargets.isEmpty else { return }
        let pairingStore = server.pairingStore
        Task {
            for target in eligibleTargets {
                let result = await RelayUplinkManager.shared.sendPush(
                    apnsToken: target.token,
                    environment: target.environment,
                    title: title,
                    body: body,
                    sessionID: sessionID,
                    kind: kind.rawValue
                )
                // APNs says this token is dead — stop pushing to it.
                if let reason = result.reason,
                   reason == "BadDeviceToken" || reason == "Unregistered" {
                    pairingStore.clearPushToken(deviceID: target.deviceID)
                    self.refreshPairedControllers()
                }
            }
        }
    }

    /// Push "wants your approval" to paired phones when a new approval prompt
    /// is enqueued (MCPApprovalCenter). Always pushed — like PermissionRequest,
    /// the asking agent is blocked and times out in ~2 minutes — except to a
    /// phone actively viewing the caller session, which sees the in-app prompt
    /// on its next bootstrap poll within seconds.
    func notifyMcpApprovalRequested(_ approval: PendingMcpApproval) {
        let body: String
        switch approval.kind {
        case .write:
            let target = approval.targetSessionID.map(sessionDisplayName) ?? "another session"
            body = "Wants to type into “\(target)”"
        case .browser:
            body = "Wants to use a browser"
        case .computer:
            body = "Wants to control this Mac"
        }
        dispatchSessionPush(
            sessionID: approval.callerSessionID, kind: .approval, bodyOverride: body
        )
    }

    /// POST /mobile/approvals/answer — a paired controller answered a pending
    /// approval prompt. 409 when the id is no longer pending (answered on the
    /// desktop or another device first); controllers dismiss silently.
    func applyRemoteApprovalAnswer(_ request: RemoteApprovalAnswerRequest) throws {
        guard answerMcpApproval(id: request.id, approved: request.approved) else {
            throw MobileRemoteError(409, "approval no longer pending")
        }
    }

    private func removeUnread(_ sessionID: String) {
        // Always leave a receipt, even when this app didn't think the row
        // was unread: another frontend may still be showing the dot.
        Self.writeSharedMarker(
            sessionID, .read, ["read_at": Int64(Date().timeIntervalSince1970 * 1000)]
        )
        announceStateChange("session-markers")
        guard unreadSessionIDs.contains(sessionID) else { return }
        unreadSessionIDs.remove(sessionID)
        invalidateSidebarLists()
    }

    private func persistActivitySnapshot() {
        let nowMs = Int64(Date().timeIntervalSince1970 * 1000)
        var snapshotSessions: [String: Any] = [:]
        var signature: [String: String] = [:]

        for (id, session) in sessionsByID {
            let unread = unreadSessionIDs.contains(id)
            let status = session.activityStatus(unread: unread).rawValue
            let raw = String(describing: session.status)
            let completed = completedSessionIDs.contains(id)
            snapshotSessions[id] = [
                "activity_status": status,
                "raw_status": raw,
                "unread": unread,
                "completed": completed,
                "updated_at": nowMs,
            ]
            signature[id] = "\(status)|\(raw)|\(unread)|\(completed)"
        }

        // No reader consumes `updated_at` (the host's ActivityStateEntry
        // doesn't even decode it), so identical content never needs a
        // rewrite — this ran a pretty-printed atomic write on EVERY rescan
        // (1-2×/s while any agent streams).
        guard signature != lastActivitySnapshotSignature else { return }

        let payload: [String: Any] = [
            "version": 1,
            "updated_at": nowMs,
            "sessions": snapshotSessions,
        ]

        do {
            try FileManager.default.createDirectory(
                at: LaunchConfig.activityStateFile.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let data = try JSONSerialization.data(
                withJSONObject: payload,
                options: [.prettyPrinted, .sortedKeys]
            )
            try data.write(to: LaunchConfig.activityStateFile, options: .atomic)
            lastActivitySnapshotSignature = signature
        } catch {
            NSLog("[UnpeelNative] failed to persist activity-state.json: \(error)")
        }
    }

    // MARK: - Scanning

    func rescan() {
        // Defer while a menu is open: publishing store changes mid-track
        // rebuilds the open NSMenu and blinks its flyout submenus. The
        // initial scan is exempt (no menu can be open before first render,
        // and callers rely on hasCompletedScan flipping).
        if menuTrackingDepth > 0, hasCompletedScan {
            rescanDeferredForMenuTracking = true
            return
        }
        // Folder colors are UserDefaults-backed but cross-frontend now: the
        // TUI's color menu writes the same key and pings the state bus, so
        // re-read here or a running app shows (and later saves) stale colors.
        let externalColors = Self.loadProjectFolderColorIDs()
        if externalColors != projectFolderColorIDs {
            projectFolderColorIDs = externalColors
        }
        let stateFile = loadAppState()
        // Before rebuildTree: node.sessions ordering consults this set.
        let dateSorted = Set(
            (stateFile?.sessionSortModes ?? [:])
                .filter { $0.value == "date" }.keys
        )
        if dateSorted != dateSortedProjectIDs {
            dateSortedProjectIDs = dateSorted
            invalidateSidebarLists()
        }
        let sessions = scanSessions()
        var projects = (stateFile?.projects ?? []) + ephemeralProjects + nativeProjects(
            excludingPaths: Set((stateFile?.projects ?? []).map(\.path)),
            excludingIDs: Set((stateFile?.projects ?? []).map(\.id))
        )
        // Native "Remove project" tombstones hide Tauri-owned projects (we
        // can't delete them from app-state.json). Worktree children of a
        // removed parent disappear with it — remove_project (project.rs:164)
        // orphans them, and orphaned worktrees never render in the Svelte
        // tree either (topLevelProjects filters worktree_branch).
        let removed = removedProjectIDs(
            prunedAgainst: Set(projects.map(\.id))
        )
        if !removed.isEmpty {
            projects.removeAll {
                removed.contains($0.id)
                    || ($0.parentProjectID.map(removed.contains) ?? false)
            }
        }
        lastScanProjects = projects
        lastScanSessions = sessions
        lastScanTauriPins = stateFile?.pinnedSessions ?? [:]
        hasCompletedScan = true
        rebuildTree(projects: projects, sessions: sessions)
        publishPendingMenuPromptNotifications()

        // Hook/unread bookkeeping for sessions that vanished, then settle
        // any pending unread transitions (sessionUnread.ts reconcile).
        let liveIDs = Set(sessions.map(\.id))
        activity.retainSessions(liveIDs)
        completedSessionIDs.formIntersection(liveIDs)
        pendingUnreadSessions.formIntersection(liveIDs)
        menuAttentionDismissals.formIntersection(liveIDs)
        menuPromptNotificationStates = menuPromptNotificationStates.filter {
            liveIDs.contains($0.key)
        }
        let keptPrewarm = prewarmSessionIDs.filter { id in
            liveIDs.contains(id) && sessionsByID[id]?.isLive == true
        }
        if keptPrewarm != prewarmSessionIDs {
            prewarmSessionIDs = keptPrewarm
        }
        let staleUnread = unreadSessionIDs.subtracting(liveIDs)
        if !staleUnread.isEmpty {
            // Not via removeUnread: that writes a read receipt, and these
            // sessions are gone. Invalidate once for the batch instead.
            unreadSessionIDs.subtract(staleUnread)
            invalidateSidebarLists()
        }
        // Another frontend showed the session to the user: a receipt newer
        // than the last settle clears our dot too.
        for sessionID in unreadSessionIDs.intersection(liveIDs) {
            guard let receipt = Self.readSharedMarker(sessionID, .read)?["read_at"] as? Int64
            else { continue }
            let command = sessionsByID[sessionID]?.command ?? ""
            let settled = Self.sessionLastRealActivityAtMs(
                sessionID, command: command
            ) ?? 0
            if settled <= receipt {
                unreadSessionIDs.remove(sessionID)
                invalidateSidebarLists()
            }
        }
        reconcileUnread()
        // A vanished session can't keep its inline remove-confirm row,
        // nor its inline rename editor.
        if let confirming = confirmingRemoveSessionID, !liveIDs.contains(confirming) {
            confirmingRemoveSessionID = nil
        }
        if let confirming = confirmingArchiveSessionID, !liveIDs.contains(confirming) {
            confirmingArchiveSessionID = nil
        }
        // Archived-ids overlay GC: the flag follows the session dir (like the
        // rename overlay), so a removed/expired session never leaves a stale
        // entry behind.
        // Adopt archives written by other frontends (TUI/CLI/phone running
        // app-lessly); our own archive path writes the same marker.
        let markerArchived = liveIDs.filter {
            Self.sharedMarkerExists($0, .archived)
        }
        let adopted = markerArchived.subtracting(archivedSessionIDs)
        if !adopted.isEmpty {
            archivedSessionIDs.formUnion(adopted)
            persistArchivedSessionIDs()
            // Adopt the recency stamp too, so an archive performed in the
            // terminal floats and lingers here exactly as our own would.
            for sessionID in adopted {
                guard archivedAtBySession[sessionID] == nil,
                      let marker = Self.readSharedMarker(sessionID, .archived),
                      (marker["stamped"] as? Bool ?? true),
                      let at = marker["archived_at"] as? Int64
                else { continue }
                archivedAtBySession[sessionID] = at
            }
        }
        let staleArchived = archivedSessionIDs.subtracting(liveIDs)
        if !staleArchived.isEmpty {
            archivedSessionIDs.subtract(staleArchived)
            persistArchivedSessionIDs()
        }
        // A visible "archiving…" row only exists while its stop/reap task is
        // in flight; anything else (task lost to a guard, session dir gone)
        // must not strand a phantom row.
        let keptArchiving = archivingSessionIDs.intersection(
            stoppingArchivedSessionIDs.intersection(archivedSessionIDs)
        )
        // Guarded: an in-place mutation of a @Published set fires
        // objectWillChange even when nothing changes, which would republish
        // the whole store on every rescan.
        if keptArchiving != archivingSessionIDs {
            archivingSessionIDs = keptArchiving
        }
        if !sidebarKeepVisibleSessionIDs.isSubset(of: liveIDs) {
            sidebarKeepVisibleSessionIDs.formIntersection(liveIDs)
        }
        // The archive flag is persisted before host shutdown begins. If the
        // app was interrupted between those two operations, finish stopping
        // the hidden terminal now instead of letting it burn resources.
        for sessionID in archivedSessionIDs
        where sessionsByID[sessionID]?.isLive == true {
            stopAndReapArchivedSession(sessionID, cleanupIfAlreadyStopped: false)
        }
        if let editing = editingSessionID, !liveIDs.contains(editing) {
            editingSessionID = nil
        }
        rebuildPins(tauriPins: stateFile?.pinnedSessions ?? [:])
        // Shared knob (app-state.json) — a TUI edit lands here on the next
        // rescan, which the state-bus ping schedules immediately.
        let stopArchiveMinutes = Self.resolvedAutoStopArchiveMinutes(stateFile)
        if stopArchiveMinutes != autoStopArchiveMinutes {
            autoStopArchiveMinutes = stopArchiveMinutes
        }
        runArchiveOverflowSweep()
        runAutoStopArchiveIfNeeded()
        // First run (no app-state.json yet): seed from the builtin global
        // presets so the app starts with Claude + Codex starred by default
        // (their builtins are quick_launch). A present-but-empty presets list
        // is a deliberate "user cleared everything" state and is left as-is.
        let globalPresets: [Preset]
        if let presets = stateFile?.presets {
            globalPresets = presets
        } else if stateFile == nil {
            globalPresets = Preset.builtinGlobalPresets
        } else {
            globalPresets = []
        }
        // `setupCompleted` (the published var) is only updated further down,
        // so pass the file's value directly — the legacy-preference migration
        // inside rebuildPresets needs it on the very first rescan.
        rebuildPresets(
            globalPresets: globalPresets,
            setupDone: stateFile?.setupCompleted ?? false,
            overlayMigrated: stateFile?.nativePresetOverlayMigrated ?? false,
            allowFold: stateFile != nil
                || !FileManager.default.fileExists(atPath: LaunchConfig.appStateFile.path)
        )
        let editor = nativeCodeEditorOverride() ?? stateFile?.codeEditor ?? "code"
        if editor != codeEditor {
            codeEditor = editor
        }
        let theme = nativeThemeOverride() ?? stateFile?.theme ?? .system
        if theme != themePreference {
            themePreference = theme
            applyAppAppearance()
        }

        // MCP security is read straight from app-state.json (no overlay): the
        // host reads the same fields, so the toggles have to live in the file.
        let orchestrators = stateFile?.mcpOrchestrators ?? [:]
        if orchestrators != mcpOrchestrators {
            mcpOrchestrators = orchestrators
        }
        let writePolicy = stateFile?.mcpNonChildWriteAccess ?? .ask
        if writePolicy != mcpNonChildWriteAccess {
            mcpNonChildWriteAccess = writePolicy
        }
        let writeApprovals = stateFile?.mcpWriteApprovals ?? [:]
        if writeApprovals != mcpWriteApprovals {
            mcpWriteApprovals = writeApprovals
        }
        let defaultAccess = (stateFile?.mcpDefaultAccess ?? .default).accessLevel
        if defaultAccess != mcpDefaultAccess {
            mcpDefaultAccess = defaultAccess
        }
        let blocked = Set(stateFile?.mcpBlockedProjects ?? [])
        if blocked != mcpBlockedProjectIDs {
            mcpBlockedProjectIDs = blocked
        }
        let browserDefault = stateFile?.browserDefaultAccess ?? .on
        if browserDefault != browserDefaultAccess {
            browserDefaultAccess = browserDefault
        }
        let computerDefault = stateFile?.computerDefaultAccess ?? .ask
        if computerDefault != computerDefaultAccess {
            computerDefaultAccess = computerDefault
        }
        let computerApproved = stateFile?.computerApprovals ?? []
        if computerApproved != computerApprovals {
            computerApprovals = computerApproved
        }
        let browserApproved = stateFile?.browserApprovals ?? []
        if browserApproved != browserApprovals {
            browserApprovals = browserApproved
        }
        let worktreeAccess = stateFile?.mcpWorktreeAccess ?? false
        if worktreeAccess != mcpWorktreeAccess {
            mcpWorktreeAccess = worktreeAccess
        }
        let autoAddBrowserScreenshots = stateFile?.mcpAutoAddBrowserScreenshots ?? true
        if autoAddBrowserScreenshots != mcpAutoAddBrowserScreenshots {
            mcpAutoAddBrowserScreenshots = autoAddBrowserScreenshots
        }
        let engineSettings = stateFile?.browserSettings ?? BrowserSettings()
        if engineSettings != browserSettings {
            browserSettings = engineSettings
        }
        let transcriptOptions = stateFile?.transcriptSettings ?? TranscriptSettings()
        if transcriptOptions != transcriptSettings {
            transcriptSettings = transcriptOptions
        }
        let setupDone = stateFile?.setupCompleted ?? false
        if setupDone != setupCompleted {
            setupCompleted = setupDone
        }
        let linkDisplayName = stateFile?.profileDisplayName ?? ""
        if linkDisplayName != profileDisplayName {
            profileDisplayName = linkDisplayName
        }
        let linkAvatar = stateFile?.profileAvatar ?? ""
        if linkAvatar != profileAvatar {
            profileAvatar = linkAvatar
        }

        if let id = archivedProjectID, projectsByID[id] == nil {
            archivedProjectID = nil
        }

        // A vanished project can't keep its inline remove-confirm row.
        if let confirming = confirmingRemoveProjectID,
           projectsByID[confirming] == nil {
            confirmingRemoveProjectID = nil
        }

        // Keep the file watcher alive, and run the 1s busy sweep only
        // while something is actually busy.
        rebuildFileWatcher()
        updateBusySweepTimer()
        persistActivitySnapshot()
    }

    /// app-state.json, decode gated on (mtime, size).
    private func loadAppState() -> AppStateFile? {
        guard let stamp = Self.statFile(LaunchConfig.appStateFile.path) else {
            appStateCache = nil
            return nil
        }
        if let cached = appStateCache, cached.stamp == stamp {
            return cached.file
        }
        let file = (try? Data(contentsOf: LaunchConfig.appStateFile))
            .flatMap { try? JSONDecoder().decode(AppStateFile.self, from: $0) }
        appStateCache = (stamp, file)
        return file
    }

    // MARK: - File watching (FSEvents) + busy sweep

    /// Coalesced rescan trigger for file events. Requests keep the earliest
    /// pending deadline, so a slow-lane request never delays a fast one.
    private func scheduleRescan(after delay: TimeInterval = 0.1) {
        let deadline = Date().addingTimeInterval(delay)
        if let pending = pendingRescanDeadline, pending <= deadline { return }
        pendingRescanWork?.cancel()
        pendingRescanDeadline = deadline
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.pendingRescanDeadline = nil
            self.pendingRescanWork = nil
            self.rescan()
        }
        pendingRescanWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
    }

    /// FSEvents fan-in. `output.bin` appends are by far the noisiest event
    /// source (a couple per second per streaming agent) and only feed the
    /// output-growth busy-onset heuristic for hookless sessions — while the
    /// 1s busy sweep runs they carry no extra information at all, and
    /// otherwise a ~1s delay before the spinner appears is fine. Everything
    /// else (manifests, app-state.json) keeps the fast lane.
    private func handleFileEvents(outputOnly: Bool) {
        if outputOnly {
            guard busySweepTimer == nil else { return }
            scheduleRescan(after: 1.0)
        } else {
            scheduleRescan()
        }
    }

    /// One FSEvents stream (file-level events, 0.5s latency) over:
    /// - ~/.unpeel/app-sessions  (manifests appear/change/vanish; output.bin
    ///   growth drives the busy heuristic onset),
    /// - ~/.unpeel/app-state.json (projects/pins/global presets),
    /// - ~/.unpeel/session-order.json + project-order.json (a drag in
    ///   another frontend). Without these two, a reorder in the terminal UI
    ///   waited on the 5s safety-net rescan instead of showing up at once.
    /// Rebuilt only when the watched path set changes.
    private func rebuildFileWatcher() {
        // FSEvents cannot watch a path that does not exist, and the order
        // files only appear on someone's first drag — so seed them empty.
        // Cheap, and it means a reorder in another frontend is seen at once
        // rather than whenever the 5s safety net next fires.
        for (url, empty) in [
            (Self.sharedSessionOrderURL, "{}"),
            (Self.sharedProjectOrderURL, "[]"),
        ] where !FileManager.default.fileExists(atPath: url.path) {
            try? Data(empty.utf8).write(to: url, options: .atomic)
        }
        let paths = [
            LaunchConfig.appSessionsDir.path,
            LaunchConfig.appStateFile.path,
            Self.sharedSessionOrderURL.path,
            Self.sharedProjectOrderURL.path,
        ]

        guard paths != watchedPaths || fsEventStream == nil else { return }
        watchedPaths = paths
        teardownFileWatcher()

        var context = FSEventStreamContext()
        context.info = Unmanaged.passUnretained(self).toOpaque()
        let callback: FSEventStreamCallback = { _, info, _, eventPaths, _, _ in
            guard let info else { return }
            // Delivered on the main queue (FSEventStreamSetDispatchQueue).
            let store = Unmanaged<UnpeelStore>.fromOpaque(info).takeUnretainedValue()
            // kFSEventStreamCreateFlagUseCFTypes: eventPaths is a CFArray of
            // CFString paths, one per event.
            let paths = unsafeBitCast(eventPaths, to: NSArray.self) as? [String] ?? []
            let outputOnly = !paths.isEmpty
                && paths.allSatisfy {
                    $0.hasSuffix("/output.bin")
                        || $0.hasSuffix("/output-retention.json")
                        || $0.contains("/output-retention.tmp.")
                }
            MainActor.assumeIsolated {
                store.handleFileEvents(outputOnly: outputOnly)
            }
        }
        guard let stream = FSEventStreamCreate(
            nil,
            callback,
            &context,
            paths as CFArray,
            FSEventStreamEventId(kFSEventStreamEventIdSinceNow),
            0.5,
            FSEventStreamCreateFlags(
                kFSEventStreamCreateFlagFileEvents | kFSEventStreamCreateFlagUseCFTypes
            )
        ) else {
            NSLog("[UnpeelNative] FSEvents stream creation failed; timer fallback only")
            return
        }
        FSEventStreamSetDispatchQueue(stream, .main)
        FSEventStreamStart(stream)
        fsEventStream = stream
    }

    private func teardownFileWatcher() {
        guard let stream = fsEventStream else { return }
        FSEventStreamStop(stream)
        FSEventStreamInvalidate(stream)
        FSEventStreamRelease(stream)
        fsEventStream = nil
    }

    /// Busy states are time-based on the way DOWN and nothing on disk changes
    /// when their windows expire, so keep the 1s rescan cadence while active.
    private func updateBusySweepTimer() {
        let anyBusy = sessionsByID.values.contains { $0.status == .busy }
        if anyBusy {
            guard busySweepTimer == nil else { return }
            let timer = Timer(timeInterval: 1.0, repeats: true) { [weak self] _ in
                Task { @MainActor in self?.rescan() }
            }
            RunLoop.main.add(timer, forMode: .common)
            busySweepTimer = timer
        } else {
            busySweepTimer?.invalidate()
            busySweepTimer = nil
        }
    }

    private func scanSessions() -> [SessionEntry] {
        // Path-based enumeration + string concatenation: URL building for
        // ~57 dirs × 2 files per rescan showed up in profiles once the JSON
        // decodes were cached away.
        let root = LaunchConfig.appSessionsDir.path
        let names = (try? FileManager.default.contentsOfDirectory(atPath: root)) ?? []

        let now = Date()
        let loadedTitleOverrides = loadSessionTitleOverrides()
        var titleOverrides = loadedTitleOverrides
        let loadedPendingTitleWrites = loadPendingTitleWrites()
        var pendingTitleWrites = loadedPendingTitleWrites
        var migratedTitleMarker = false
        let loadedPendingAppendSystemContexts = loadPendingAppendSystemContexts()
        var pendingAppendSystemContexts = loadedPendingAppendSystemContexts
        let loadedDismissedRestartRecommendations = loadRestartRecommendationDismissals()
        var dismissedRestartRecommendations = loadedDismissedRestartRecommendations
        var nextRestartRecommendations: [String: SessionRestartRecommendation] = [:]
        var nextDetectedLocalURLs: [String: [String]] = [:]
        var entries: [SessionEntry] = []
        var seen = Set<String>()

        for dirName in names where !dirName.hasPrefix(".") {
            let dirPath = root + "/" + dirName
            let manifestPath = dirPath + "/manifest.json"

            // A closed/restarted session's old host (pre-fix builds) rewrites
            // manifest.json up to ~60s after the dir was deleted; removal
            // wins — delete the resurrected dir instead of listing it.
            if let purgedAt = purgedSessionDirs[dirName] {
                if now.timeIntervalSince(purgedAt) < Self.purgedSessionDirTTL {
                    try? FileManager.default.removeItem(atPath: dirPath)
                    manifestCache.removeValue(forKey: dirName)
                    continue
                }
                purgedSessionDirs.removeValue(forKey: dirName)
            }

            // Decode the manifest only when (mtime, size) changed since the
            // last read. A nil cached manifest records a decode failure
            // (torn write); the finishing write re-stamps the file.
            guard let stamp = Self.statFile(manifestPath) else {
                manifestCache.removeValue(forKey: dirName)
                continue
            }
            let manifest: HostedSessionManifest
            if let cached = manifestCache[dirName], cached.stamp == stamp {
                guard let cachedManifest = cached.manifest else { continue }
                manifest = cachedManifest
            } else {
                let decoded = (try? Data(contentsOf: URL(fileURLWithPath: manifestPath)))
                    .flatMap { try? JSONDecoder().decode(HostedSessionManifest.self, from: $0) }
                manifestCache[dirName] = (stamp, decoded)
                guard let decoded else { continue }
                manifest = decoded
            }

            let info = manifest.session
            let live = manifest.state == "running"
                && (manifest.pid.map { kill($0, 0) == 0 } ?? false)
                && Self.manifestPidIdentity(manifest) != .notOurs
            let previousRuntimeGeneration = runtimeLaunchGenerations[info.id]
            let runtimeGenerationAdvanced = previousRuntimeGeneration.map {
                manifest.runtimeLaunchGeneration > $0
            } ?? false
            runtimeLaunchGenerations[info.id] = manifest.runtimeLaunchGeneration
            let runtimeLaunchedAt = manifest.runtimeLaunchedAt.map {
                Date(timeIntervalSince1970: TimeInterval($0) / 1_000)
            }
            if let runtimeLaunchedAt {
                runtimeLaunchCutoffs[info.id] = runtimeLaunchedAt
            } else {
                runtimeLaunchCutoffs.removeValue(forKey: info.id)
            }
            if runtimeGenerationAdvanced {
                if let deferred = deferredStopEffects[info.id],
                   deferred.runtimeGeneration != manifest.runtimeLaunchGeneration {
                    deferred.task.cancel()
                    deferredStopEffects.removeValue(forKey: info.id)
                }
                // Same Session, new managed agent process: old hook ownership,
                // completion, and menu-dismissal state belong to the prior
                // generation. Preserve a fast replacement hook received after
                // the Host's launch stamp; otherwise the current generation's
                // durable hook seed is re-read below after this reset.
                let preservedHookCompletedTurn = activity.resetForRuntimeLaunch(
                    info.id,
                    runtimeGeneration: manifest.runtimeLaunchGeneration,
                    launchedAt: runtimeLaunchedAt
                )
                let completionIsStillDeferred = deferredStopEffects[info.id]
                    .map { $0.runtimeGeneration == manifest.runtimeLaunchGeneration }
                    ?? false
                if preservedHookCompletedTurn == true, !completionIsStillDeferred {
                    completedSessionIDs.insert(info.id)
                } else {
                    completedSessionIDs.remove(info.id)
                }
                resumingAgentSessionIDs.remove(info.id)
                menuAttentionDismissals.remove(info.id)
                watchForResumeFailure(
                    sessionID: info.id,
                    markers: manifest.resumeFailureMarkers,
                    startOffset: manifest.runtimeLaunchOutputOffset
                )
            }
            // A cross-process Resume Agent transactionally consumes its old
            // shared context snapshot in core. Because the Session id is
            // retained, reconcile the native compatibility overlay here: no
            // marker means consumed; a surviving marker is a concurrently
            // published NEXT intent. Generation > 1 also reconciles an
            // in-place restart that happened while this app was closed.
            let shouldReconcileAppendedContext = runtimeGenerationAdvanced
                || (previousRuntimeGeneration == nil
                    && manifest.runtimeLaunchGeneration > 1)
            // `rescan()` runs on the main actor once per second while any
            // Session is busy. Taking the cross-process context lock for every
            // retained, exited Session made large histories stall the UI even
            // though those Sessions cannot have a live restart recommendation.
            // Read once for live rows, or for the one scan that must reconcile
            // a newly observed runtime generation, and reuse that snapshot
            // below instead of acquiring the same lock twice.
            let currentSharedContext: String? = (live || shouldReconcileAppendedContext)
                ? Self.readSharedMarker(info.id, .appendedContext)?["context"] as? String
                : nil
            if shouldReconcileAppendedContext {
                // Core consumed only its old snapshot. A marker that still
                // exists is a concurrently published NEXT intent.
                pendingAppendSystemContexts = Self.reconciledPendingAppendedContexts(
                    pendingAppendSystemContexts,
                    sessionID: info.id,
                    currentSharedContext: currentSharedContext
                )
                dismissedRestartRecommendations.removeValue(forKey: info.id)
            }
            let activeRuntimeID = live
                ? manifest.runtime?.currentObservation?.id
                : nil
            let launchTool = SetupTool.detect(in: info.command ?? "")
            let launchRuntime = launchTool?.metadata
            let launchUsesLifecycleHooks = launchTool?.usesLifecycleHooks == true
            if live,
               let recommendation = Self.restartRecommendation(
                   for: manifest,
                   pendingAppendSystemContext:
                       currentSharedContext ?? pendingAppendSystemContexts[info.id]
               ),
               dismissedRestartRecommendations[info.id] != recommendation.token {
                nextRestartRecommendations[info.id] = recommendation
            }

            var status: SessionStatus = live ? .idle : .exited
            if live {
                // output.bin is only consulted where its size can influence
                // state: sessions on the output heuristic (no hook latch),
                // hook-BUSY sessions (growth re-arms the 5-minute idle
                // deadline), and hook-ATTENTION sessions (growth means the
                // user answered and the agent resumed → clear attention to
                // busy; see SessionActivityEngine.noteOutputAndSweep). Hook-idle
                // sessions ignore output entirely, so they are not stat'ed —
                // except when the runtime descriptor marks Stops as
                // provisional while output keeps growing.
                //
                // For known hook-capable tools, also suppress the *pre-hook*
                // output heuristic. After an app restart the hook latch is
                // in-memory and may not have been re-established yet, but
                // full-screen TUIs still repaint during user scroll/resize;
                // those repaint bursts should not make Claude/Codex/etc spin.
                // Runtime observation enriches presentation only in this
                // first slice. Hook markers do not carry a runtime identity,
                // so a blank shell must stay on generic output activity: an
                // old Claude marker cannot become authority for a later Codex.
                // The hook latch is in-memory-only, so after an app restart a
                // mid-turn session would sit spinner-less until its next hook
                // event (often Stop — i.e. never busy). Hook scripts persist
                // their last lifecycle event to disk precisely for this gap;
                // re-seed the latch from it before reading the hook state.
                if launchUsesLifecycleHooks,
                   activity.hookOwnedState(info.id) == nil {
                    // Older Grok hook assets collapsed CLI SessionStart and
                    // prompt submission into "Start". Grok's idle TUI can keep
                    // repainting output.bin, so don't revive those legacy
                    // launch-only seeds from output recency.
                    seedHookActivity(
                        sessionID: info.id,
                        dirPath: dirPath,
                        runtimeGeneration: manifest.runtimeLaunchGeneration,
                        runtimeLaunchedAt: runtimeLaunchedAt,
                        anchorStartEventToOutput:
                            launchRuntime?.anchorStartEventToOutput ?? true
                    )
                }
                let hookStateBefore = launchUsesLifecycleHooks
                    ? activity.hookOwnedState(info.id)
                    : nil
                let suppressPreHookOutputBusy =
                    hookStateBefore == nil && launchUsesLifecycleHooks
                // Output/attention and provisional-Stop semantics live beside
                // each runtime's hook recipe. Unknown commands retain the
                // generic lifecycle defaults.
                let allowAttentionClearFromOutput =
                    launchRuntime?.attentionClearsOnOutput ?? true
                let distrustStops =
                    launchRuntime?.distrustStopsWhileOutputGrows ?? false
                if !suppressPreHookOutputBusy
                    && (hookStateBefore == nil || hookStateBefore == .busy
                        || hookStateBefore == .attention
                        || (hookStateBefore == .idle && distrustStops)) {
                    // The activity signal is consumed as "value changed since
                    // last observation": prefer the host's parsed-screen
                    // change stamp — idle repaint loops that redraw identical
                    // content (grok's idle animation) never advance it — and
                    // fall back to raw output.bin size under older hosts.
                    let outputPath = dirPath + "/output.bin"
                    let signal = manifest.screenChangedAt
                        ?? (Self.statFile(outputPath).map { UInt64(clamping: $0.size) } ?? 0)
                    if let prev = outputSizes[info.id] {
                        if signal > prev.size {
                            outputSizes[info.id] = (signal, now)
                        }
                    } else {
                        // First sighting: record the signal, don't claim busy yet.
                        outputSizes[info.id] = (signal, .distantPast)
                    }

                    // Signal growth re-arms the hook-busy 5-minute deadline
                    // and expires it when passed (session_activity.rs sweep).
                    activity.noteOutputAndSweep(
                        sessionID: info.id,
                        outputSize: signal,
                        allowAttentionClearFromOutput: allowAttentionClearFromOutput,
                        distrustStops: distrustStops,
                        now: now
                    )
                }

                if launchUsesLifecycleHooks,
                   let hookState = activity.hookOwnedState(info.id) {
                    // Hook latch: once a session has produced hook events,
                    // hooks (+ the timeout above) are the only trusted
                    // busy/idle signal — output volume must not flip it
                    // (session_activity.rs:446-457, sessionState.ts
                    // explicitLifecycle).
                    status = hookState
                } else if let entry = outputSizes[info.id],
                          now.timeIntervalSince(entry.grewAt) <= busyWindow,
                          !outputIsLikelyEcho(info.id, now: now) {
                    // Pre-hook / no-hook sessions only: growth that closely
                    // trails the user's own keystrokes is echo, not work, so it
                    // must not flip the spinner on (input-aware suppression).
                    status = .busy
                }

                // Agent-drawn select menus (Claude/Codex numbered prompts) fire
                // no lifecycle hook, so the host detects them from the rendered
                // screen and flags `menu_prompt_active`. Surface it as the
                // attention badge — replacing the busy spinner with the yellow
                // dot — so a waiting menu is glanceable in the sidebar. Skipped
                // when the user turns the detection off in Settings, or
                // dismissed this flag via "Clear attention" (re-armed once the
                // host lowers the flag).
                if !manifest.menuPromptActive {
                    menuAttentionDismissals.remove(info.id)
                }
                let menuNotification = Self.menuPromptNotificationDecision(
                    previous: menuPromptNotificationStates[info.id],
                    runtimeGeneration: manifest.runtimeLaunchGeneration,
                    active: manifest.menuPromptActive,
                    initialAppScan: !hasCompletedScan,
                    detectionEnabled: menuAttentionDetectionEnabled,
                    dismissed: menuAttentionDismissals.contains(info.id),
                    // PermissionRequest is authoritative and already travels
                    // through the same dispatcher. If it won the race, consume
                    // this visual edge without delivering a duplicate.
                    hookAlreadyNeedsInput: activity.hookOwnedState(info.id) == .attention
                )
                menuPromptNotificationStates[info.id] = menuNotification.state
                if menuNotification.sendNotification {
                    pendingMenuPromptNotifications[info.id] = manifest.runtimeLaunchGeneration
                }
                if menuAttentionDetectionEnabled,
                   manifest.menuPromptActive,
                   !menuAttentionDismissals.contains(info.id),
                   status == .busy || status == .idle {
                    status = .attention
                }

                // Live-probed loopback URLs the host published for this
                // session (titlebar "open local site" chip). Only live
                // sessions surface them — the host cannot retract the list
                // after its process exits.
                if !manifest.detectedLocalURLs.isEmpty {
                    nextDetectedLocalURLs[info.id] = manifest.detectedLocalURLs
                }
            } else {
                outputSizes.removeValue(forKey: info.id)
                activity.removeSession(info.id)
            }

            let command = info.command ?? ""
            let manifestLabel = (info.label?.isEmpty == false)
                ? info.label!
                : (command.isEmpty ? "Terminal" : command)
            // The resolved custom title wins over the manifest label (the
            // backend may keep auto-titling the manifest underneath; a
            // custom title stops that from showing — custom_title parity).
            // A rename from any frontend arrives as a title.json marker. It
            // is the shared truth; UserDefaults is only a pre-marker/write-
            // failure fallback and is retired once the marker is durable.
            let markedTitle = titleMarkerValue(sessionID: info.id, dirPath: dirPath)
            let nativeTitle = Self.normalizedSessionTitle(titleOverrides[info.id])
            let pendingWriteAt = nativeTitle.flatMap { _ in pendingTitleWrites[info.id] }
            let titleResolution = Self.resolvedSessionTitle(
                sharedMarker: markedTitle,
                nativeTitle: nativeTitle,
                pendingWriteAt: pendingWriteAt
            )
            let titleOverride = titleResolution.title
            if titleResolution.shouldPublishNative, let nativeTitle {
                // Retry a failed newer native intent with its original
                // timestamp. Advancing the timestamp on each rescan would let
                // a stale fallback leapfrog a later TUI/CLI marker forever.
                let publishAt = pendingWriteAt.flatMap { $0 > 0 ? $0 : nil }
                    ?? Self.nextTitleIntentTimestamp(after: markedTitle?.updatedAt)
                if publishTitleMarker(
                    sessionID: info.id,
                    title: nativeTitle,
                    updatedAt: publishAt
                ) {
                    titleOverrides.removeValue(forKey: info.id)
                    pendingTitleWrites.removeValue(forKey: info.id)
                    migratedTitleMarker = true
                }
            } else if Self.normalizedSessionTitle(markedTitle?.title) != nil {
                // `title.json` is the cross-frontend truth. Retire the old
                // UserDefaults fallback once any frontend has published a
                // valid marker so it cannot hide a later TUI/CLI rename.
                titleOverrides.removeValue(forKey: info.id)
                pendingTitleWrites.removeValue(forKey: info.id)
            } else if nativeTitle == nil {
                pendingTitleWrites.removeValue(forKey: info.id)
            }
            // Sync only when the decoded manifest disagrees (label or
            // custom_title): the no-op case must stay stat-free — an
            // unconditional call re-reads manifest.json for every titled
            // session on every rescan.
            if let titleOverride,
               info.label != titleOverride || info.customTitle != true {
                syncSessionTitleOverrideToManifest(sessionID: info.id, label: titleOverride)
            }
            let label = titleOverride ?? manifestLabel
            let usesLifecycleHooks = launchUsesLifecycleHooks
            let hookEventAt = usesLifecycleHooks
                ? Self.fileModificationAtMs(dirPath + "/last-hook-event.json")
                : nil
            // Hook-owned agents never consult screen/output here: resize and
            // idle TUI repaint traffic is not a lifecycle event. Hookless
            // tools prefer the host's semantic text-change timestamp, with
            // output mtime retained solely for older manifests.
            let screenChangedAt = usesLifecycleHooks
                ? nil
                : manifest.screenChangedAt.map { Int64(clamping: $0) }
            let outputAt = !usesLifecycleHooks && screenChangedAt == nil
                ? Self.fileModificationAtMs(dirPath + "/output.bin")
                : nil
            let finalExitedAt = manifest.state == "exited" && manifest.updatedAt > 0
                ? Int64(clamping: manifest.updatedAt)
                : nil
            let lifecycleAt = Self.resolvedLifecycleAtMs(
                createdAtMs: info.createdAt ?? 0,
                command: command,
                hookEventAtMs: hookEventAt,
                screenChangedAtMs: screenChangedAt,
                outputAtMs: outputAt,
                finalExitedAtMs: finalExitedAt
            )
            seen.insert(info.id)
            entries.append(SessionEntry(
                id: info.id,
                projectID: info.projectID,
                label: label,
                command: command,
                createdAt: info.createdAt ?? 0,
                status: status,
                activeRuntimeID: activeRuntimeID,
                runtimeLaunchPending: manifest.runtimeLaunchPending,
                hostProtocolVersion: manifest.hostProtocolVersion,
                customTitle: info.customTitle ?? false,
                worktreePath: info.worktreePath,
                worktreeBranch: info.worktreeBranch,
                spawnedBy: info.spawnedBy,
                role: info.role,
                task: info.task,
                providerTranscriptPath: manifest.providerTranscriptPath,
                projectOverrideID: projectOverrideValue(sessionID: info.id, dirPath: dirPath),
                lifecycleAtMs: lifecycleAt
            ))
        }

        for id in seen {
            pendingSessions.removeValue(forKey: id)
        }
        for pending in pendingSessions.values where !seen.contains(pending.id) {
            seen.insert(pending.id)
            entries.append(pending)
        }

        outputSizes = outputSizes.filter { seen.contains($0.key) }
        runtimeLaunchGenerations = runtimeLaunchGenerations.filter { seen.contains($0.key) }
        runtimeLaunchCutoffs = runtimeLaunchCutoffs.filter { seen.contains($0.key) }
        resumingAgentSessionIDs.formIntersection(seen)
        let staleDeferredStops = deferredStopEffects.keys.filter { !seen.contains($0) }
        for sessionID in staleDeferredStops {
            deferredStopEffects.removeValue(forKey: sessionID)?.task.cancel()
        }
        lastUserInputAt = lastUserInputAt.filter { seen.contains($0.key) }
        // Session dirs are named by session id; drop cache entries for
        // dirs that no longer exist on disk.
        let dirNames = Set(names)
        manifestCache = manifestCache.filter { dirNames.contains($0.key) }
        titleMarkerCache = titleMarkerCache.filter { dirNames.contains($0.key) }
        projectOverrideCache = projectOverrideCache.filter { dirNames.contains($0.key) }
        // GC rename-overlay entries whose session dir is gone for good
        // (dir existence, not manifest decode success, so a torn manifest
        // write can't drop a rename).
        let keptTitles = titleOverrides.filter { dirNames.contains($0.key) }
        if keptTitles != loadedTitleOverrides {
            saveSessionTitleOverrides(keptTitles)
        }
        let keptPendingTitleWrites = pendingTitleWrites.filter { dirNames.contains($0.key) }
        if keptPendingTitleWrites != loadedPendingTitleWrites {
            savePendingTitleWrites(keptPendingTitleWrites)
        }
        let keptPendingAppendContexts = pendingAppendSystemContexts.filter {
            dirNames.contains($0.key)
        }
        if keptPendingAppendContexts != loadedPendingAppendSystemContexts {
            savePendingAppendSystemContexts(keptPendingAppendContexts)
        }
        if migratedTitleMarker {
            announceStateChange("session-markers")
        }
        let keptDismissals = dismissedRestartRecommendations.filter { dirNames.contains($0.key) }
        if keptDismissals != loadedDismissedRestartRecommendations {
            saveRestartRecommendationDismissals(keptDismissals)
        }
        // Provider-session-id overlay is written for essentially every hook POST
        // that carries a session_id; GC it here (dir existence) so entries for
        // sessions whose dir vanished — externally, or via another instance's
        // cleanup — don't accumulate in UserDefaults forever. pruneNativeState
        // still handles the explicit remove/restart path.
        let providerIDs = loadProviderSessionIDs()
        let keptProviderIDs = providerIDs.filter { dirNames.contains($0.key) }
        if keptProviderIDs.count != providerIDs.count {
            saveProviderSessionIDs(keptProviderIDs)
        }
        if nextRestartRecommendations != restartRecommendations {
            restartRecommendations = nextRestartRecommendations
        }
        if nextDetectedLocalURLs != detectedLocalURLs {
            detectedLocalURLs = nextDetectedLocalURLs
        }
        if phoneResizeOverrides.keys.contains(where: { !seen.contains($0) }) {
            phoneResizeOverrides = phoneResizeOverrides.filter { seen.contains($0.key) }
        }
        if resumeFailures.contains(where: { !seen.contains($0) }) {
            resumeFailures = resumeFailures.filter { seen.contains($0) }
        }

        // Restart leaves the pre-restart session as a dead row until its host
        // finishes exiting. `killAndCleanup` deletes the old dir and re-sweeps
        // for ~3s to catch the host's final `state=exited` write, but a slow
        // provider (codex/claude) can flush that write after the sweep, so the
        // dir reappears and the dead session lingers as a greyed duplicate.
        // GC it here: restart copies `created_at` exactly onto the replacement,
        // so an exited session sharing (project, created_at) with a live one is
        // that stale leftover. Its host is already gone (it's exited), so
        // removing the dir is safe and can't race a live writer.
        let ghosts = Self.supersededRestartGhostIDs(
            entries.map {
                RestartGhostCandidate(
                    id: $0.id,
                    projectID: $0.projectID,
                    createdAt: $0.createdAt,
                    isLive: $0.isLive
                )
            }
        )
        if !ghosts.isEmpty {
            entries.removeAll { ghosts.contains($0.id) }
            for id in ghosts {
                try? FileManager.default.removeItem(
                    at: LaunchConfig.appSessionsDir.appendingPathComponent(id)
                )
                manifestCache.removeValue(forKey: id)
            }
        }
        return entries
    }

    /// Minimal view of a session for restart-ghost detection (kept tiny so the
    /// pure detection logic is unit-testable without building a `SessionEntry`).
    struct RestartGhostCandidate {
        let id: String
        let projectID: String
        let createdAt: Int64
        let isLive: Bool
    }

    /// Ids of exited sessions that are pre-restart leftovers. Restart copies a
    /// session's `created_at` exactly onto its replacement, so an **exited**
    /// session that shares `(projectID, created_at)` with a **live** session is
    /// the stale old instance a restart left behind. `created_at == 0` is
    /// ignored so timestamp-less manifests never group together, and fork is
    /// unaffected because it deliberately takes a fresh `created_at`.
    nonisolated static func supersededRestartGhostIDs(
        _ candidates: [RestartGhostCandidate]
    ) -> Set<String> {
        var liveKeys = Set<String>()
        for c in candidates where c.isLive && c.createdAt > 0 {
            liveKeys.insert("\(c.projectID)\u{1f}\(c.createdAt)")
        }
        guard !liveKeys.isEmpty else { return [] }
        var ghosts = Set<String>()
        for c in candidates where !c.isLive && c.createdAt > 0 {
            if liveKeys.contains("\(c.projectID)\u{1f}\(c.createdAt)") {
                ghosts.insert(c.id)
            }
        }
        return ghosts
    }

    private func rebuildTree(projects: [Project], sessions: [SessionEntry]) {
        // A session whose removal is in flight vanishes from the sidebar
        // immediately; the kill/cleanup then runs silently in the background
        // (no dimmed "removing" placeholder row).
        var sessions = sessions.filter { !removingSessionIDs.contains($0.id) }
        // A session being restarted keeps its row throughout the teardown +
        // respawn, so it never blinks out of the sidebar (restart gap fix).
        // The snapshot is injected only once the live scan stops producing
        // the row (i.e. after the old host's manifest is deleted and before
        // the replacement's manifest exists).
        if !restartPlaceholders.isEmpty {
            let present = Set(sessions.map(\.id))
            for (id, snapshot) in restartPlaceholders where !present.contains(id) {
                sessions.append(snapshot)
            }
        }

        var byProject: [String: [SessionEntry]] = [:]
        // A project-override marker files the session under another project
        // (group/worktree folder) — display + ordering only, and only when
        // the target still exists; a stale marker falls back to the manifest
        // project instead of orphaning the row.
        let knownProjectIDs = Set(projects.map(\.id))
        for s in sessions {
            let key = s.projectOverrideID.flatMap {
                knownProjectIDs.contains($0) ? $0 : nil
            } ?? s.projectID
            byProject[key, default: []].append(s)
        }
        for key in byProject.keys {
            // Newest-first, exactly sortSessionsNewestFirst in
            // stores/sessions.ts:164-165 (`b.created_at - a.created_at`).
            // New launches therefore land at the TOP of the regular list
            // (the Svelte store also prepends, sessions.ts:546).
            byProject[key]?.sort { $0.createdAt > $1.createdAt }
            // Native drag-reorder overlay: sessions the user ordered by hand
            // keep that order; ids not in the overlay (newer launches) stay
            // newest-first ABOVE the hand-ordered block.
            byProject[key] = applySessionOrderOverlay(byProject[key]!, projectID: key)
        }

        var childrenOf: [String: [Project]] = [:]
        var topLevel: [Project] = []
        for p in projects {
            if let parent = p.parentProjectID {
                childrenOf[parent, default: []].append(p)
            } else {
                topLevel.append(p)
            }
        }
        topLevel.sort { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
        // Native drag-reorder overlay over the file's sort_order: overlay ids
        // first (in overlay order), unknown/new projects appended in file
        // order — matching reorder_projects/add_project in project.rs, where
        // new projects get max(sort_order)+1 (appended last).
        topLevel = applyProjectOrderOverlay(topLevel, parentID: nil)

        func node(for project: Project) -> ProjectNode {
            // Worktree checkouts AND plain groups (organizational child
            // folders, isFolder + parent, no branch) render as inline
            // folder rows.
            let childProjects = (childrenOf[project.id] ?? [])
                .filter { $0.worktreeBranch != nil || $0.isFolder == true }
                .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
            let kids = applyProjectOrderOverlay(childProjects, parentID: project.id)
                .map { node(for: $0) }
            return ProjectNode(
                project: project,
                sessions: byProject[project.id] ?? [],
                worktrees: kids
            )
        }

        let newNodes = topLevel.map { node(for: $0) }
        var index: [String: SessionEntry] = [:]
        for s in sessions { index[s.id] = s }
        var projIndex: [String: Project] = [:]
        for p in projects { projIndex[p.id] = p }

        sessionsByID = index
        if projIndex != projectsByID {
            projectsByID = projIndex
        }

        // History feed: log the live → exited edge. Keyed off statuses this
        // run has already seen, so a startup rescan over long-dead sessions
        // logs nothing; archiving (which stops the host on purpose) is
        // deliberately silent too.
        for (id, session) in index {
            let previous = activityLoggedStatuses[id]
            activityLoggedStatuses[id] = session.status
            if session.status == .exited,
               let previous, previous != .exited,
               !archivedSessionIDs.contains(id),
               !restartingSessionIDs.contains(id) {
                logActivity(.exited, sessionID: id)
            }
        }
        activityLoggedStatuses = activityLoggedStatuses.filter { index[$0.key] != nil }

        // Drop selection before publishing the tree so RootView's surface
        // pruning sees the final selection state for this rescan. Remote
        // scope owns its own (remote-id) selection, which a local scan must
        // never clear.
        if selectedHostScope == .local, let sel = selectedSessionID, index[sel] == nil {
            selectedSessionID = nil
        }

        // Only publish when something observable changed (cheap diff).
        if newNodes != nodes {
            nodes = newNodes
            invalidateSidebarLists()
        }

        refreshTitlebarBranch()
    }

    /// Publish in-memory starting sessions immediately, without waiting for
    /// the filesystem watcher or manifest poll. The next full rescan will
    /// merge the same pending rows with app-state.json and manifest state.
    private func publishPendingSessions() {
        let currentSessions = sessionsByID.values.filter { pendingSessions[$0.id] == nil }
        rebuildTree(
            projects: currentProjectsInDisplayOrder(),
            sessions: Array(currentSessions) + Array(pendingSessions.values)
        )
        updateBusySweepTimer()
        persistActivitySnapshot()
    }

    private func currentProjectsInDisplayOrder() -> [Project] {
        var projects: [Project] = []
        func walk(_ nodes: [ProjectNode]) {
            for node in nodes {
                projects.append(node.project)
                walk(node.worktrees)
            }
        }
        walk(nodes)
        return projects.isEmpty ? Array(projectsByID.values) : projects
    }

    // MARK: - Expansion

    func toggleProjectExpanded(_ projectID: String) {
        if expandedProjectIDs.contains(projectID) {
            expandedProjectIDs.remove(projectID)
            // Collapsing drops any "keep this hidden row visible" pins for
            // the project — reopening starts from the plain recent window.
            let projectSessionIDs = Set(
                (findDisplayNode(projectID)?.sessions ?? []).map(\.id)
            )
            if !sidebarKeepVisibleSessionIDs.isDisjoint(with: projectSessionIDs) {
                sidebarKeepVisibleSessionIDs.subtract(projectSessionIDs)
            }
        } else {
            expandedProjectIDs.insert(projectID)
        }
    }

    func revealSessionInSidebar(_ sessionID: String) {
        guard selectedHostScope == .local else { return }
        guard let session = sessionsByID[sessionID] else { return }

        closeSettings()
        prepareSidebarToRenderSession(session)

        if selectedSessionID != sessionID {
            selectedSessionID = sessionID
        }

        requestSidebarScroll(to: sessionID, centered: true)
    }

    /// Follow a row that just moved in place (pinning teleports it up into
    /// the project's pinned section): minimal scroll so the row stays in
    /// view, without changing selection or expansion state.
    func followSessionRowInSidebar(_ sessionID: String) {
        requestSidebarScroll(to: sessionID, centered: false)
    }

    private func requestSidebarScroll(to sessionID: String, centered: Bool) {
        sidebarSessionRevealSerial += 1
        let request = SidebarSessionRevealRequest(
            sessionID: sessionID,
            serial: sidebarSessionRevealSerial,
            centered: centered
        )
        DispatchQueue.main.async { [weak self] in
            guard let self, self.sessionsByID[sessionID] != nil else { return }
            self.sidebarSessionRevealRequest = request
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
            guard let self, self.sidebarSessionRevealRequest == request else { return }
            self.sidebarSessionRevealRequest = nil
        }
    }

    private func prepareSidebarToRenderSession(_ session: SessionEntry) {
        // A project-override files the row under a group/worktree node, so
        // expansion and the window check must target that node, not the
        // manifest's launch project.
        let projectID = effectiveProjectID(for: session)
        guard let project = projectsByID[projectID] else { return }

        // Worktree children render inline under their parent: expand every
        // ancestor so the session's project row is actually visible.
        var ancestorID = project.parentProjectID
        while let id = ancestorID {
            expandedProjectIDs.insert(id)
            ancestorID = projectsByID[id]?.parentProjectID
        }
        expandedProjectIDs.insert(projectID)

        guard let node = findNode(projectID) else { return }
        let pinnedIDs = Set(pinnedSessions(in: node).map(\.id))
        guard !pinnedIDs.contains(session.id) else { return }
        let displayedIDs = Set(displayedSessions(in: node).map(\.id))
        if !displayedIDs.contains(session.id) {
            // Beyond the stopped-group window: pin just this row visible
            // (and exempt from the overflow auto-archive).
            sidebarKeepVisibleSessionIDs.insert(session.id)
        }
    }

    // MARK: - Project folder colors

    func projectFolderColor(for projectID: String) -> ProjectFolderColor? {
        guard let raw = projectFolderColorIDs[projectID] else { return nil }
        return ProjectFolderColor(rawValue: raw)
    }

    func setProjectFolderColor(_ color: ProjectFolderColor?, for projectID: String) {
        if let color {
            projectFolderColorIDs[projectID] = color.rawValue
        } else {
            projectFolderColorIDs.removeValue(forKey: projectID)
        }
        saveProjectFolderColorIDs()
    }

    /// Whether this group's sessions sort by date (recently updated first)
    /// instead of the manual drag order.
    func isDateSorted(projectID: String) -> Bool {
        if let summary = remoteProjectSummariesByID[projectID] {
            return summary.dateSorted == true
        }
        return dateSortedProjectIDs.contains(projectID)
    }

    /// Flip a group between date sort and custom order. The mode lives in
    /// app-state.json (`session_sort_modes`) so the TUI reads and writes the
    /// same truth; the manual order in session-order.json stays untouched,
    /// so switching back to custom restores the old arrangement.
    func setSessionDateSorted(_ dateSorted: Bool, for projectID: String) {
        // No remote sort-mode operation exists yet; the menu hides remotely.
        guard remoteProjectSummariesByID[projectID] == nil else { return }
        let wrote = editPresetStateAnnouncing { object in
            var modes = (object["session_sort_modes"] as? [String: Any])?
                .compactMapValues { $0 as? String } ?? [:]
            if dateSorted {
                modes[projectID] = "date"
            } else {
                modes.removeValue(forKey: projectID)
            }
            object["session_sort_modes"] = modes
        }
        guard wrote else { return }
        if dateSorted {
            dateSortedProjectIDs.insert(projectID)
        } else {
            dateSortedProjectIDs.remove(projectID)
        }
        invalidateSidebarLists()
        withAnimation(.easeInOut(duration: 0.18)) { rebuildTreeFromLastScan() }
    }

    private static func loadProjectFolderColorIDs() -> [String: String] {
        let raw = AppDefaults.shared.dictionary(forKey: nativeProjectFolderColorsKey) ?? [:]
        return raw.compactMapValues { value in
            guard let string = value as? String,
                  ProjectFolderColor(rawValue: string) != nil
            else { return nil }
            return string
        }
    }

    private func saveProjectFolderColorIDs() {
        if projectFolderColorIDs.isEmpty {
            AppDefaults.shared.removeObject(forKey: Self.nativeProjectFolderColorsKey)
        } else {
            AppDefaults.shared.set(
                projectFolderColorIDs, forKey: Self.nativeProjectFolderColorsKey
            )
        }
    }

    // MARK: - Pins

    /// Native-side pin intent persisted until the matching shared-state write
    /// succeeds. Older builds kept these entries forever; `removedAt` and the
    /// reconciliation below let newer shared changes supersede those stale
    /// overlays while preserving an intent whose disk write actually failed.
    struct NativePinOverrides: Codable, Equatable {
        var added: [PinnedSidebarSession] = []
        var removedKeys: [String] = []
        var removedAt: [String: UInt64] = [:]

        init(
            added: [PinnedSidebarSession] = [],
            removedKeys: [String] = [],
            removedAt: [String: UInt64] = [:]
        ) {
            self.added = added
            self.removedKeys = removedKeys
            self.removedAt = removedAt
        }

        private enum CodingKeys: String, CodingKey {
            case added, removedKeys, removedAt
        }

        init(from decoder: Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            added = try container.decodeIfPresent(
                [PinnedSidebarSession].self, forKey: .added
            ) ?? []
            removedKeys = try container.decodeIfPresent(
                [String].self, forKey: .removedKeys
            ) ?? []
            // Absent in every previously shipped overlay. A legacy tombstone
            // is retired as soon as readable shared state confirms either the
            // pin or its absence; see `reconciledPinOverrides`.
            removedAt = try container.decodeIfPresent(
                [String: UInt64].self, forKey: .removedAt
            ) ?? [:]
        }
    }

    private func loadPinOverrides() -> NativePinOverrides {
        guard let data = AppDefaults.shared.data(forKey: Self.nativePinsKey),
              let overrides = try? JSONDecoder().decode(NativePinOverrides.self, from: data)
        else { return NativePinOverrides() }
        return overrides
    }

    private func savePinOverrides(_ overrides: NativePinOverrides) {
        if overrides.added.isEmpty && overrides.removedKeys.isEmpty {
            AppDefaults.shared.removeObject(forKey: Self.nativePinsKey)
            return
        }
        if let data = try? JSONEncoder().encode(overrides) {
            AppDefaults.shared.set(data, forKey: Self.nativePinsKey)
        }
    }

    static func reconciledPinOverrides(
        _ overrides: NativePinOverrides,
        sharedPins: [String: PinnedSidebarSession],
        sharedStateModifiedAt: UInt64?
    ) -> NativePinOverrides {
        var reconciled = overrides

        // An added overlay is pending only while it is newer than what the
        // shared file proves. A same/newer shared pin confirms the write; a
        // newer shared file that omits it represents an external unpin.
        reconciled.added.removeAll { nativePin in
            if let sharedPin = sharedPins[nativePin.key] {
                return max(sharedPin.pinnedAt, sharedStateModifiedAt ?? 0)
                    >= nativePin.pinnedAt
            }
            guard let sharedStateModifiedAt else { return false }
            return sharedStateModifiedAt >= nativePin.pinnedAt
        }

        var seen = Set<String>()
        reconciled.removedKeys = reconciled.removedKeys.filter { key in
            guard seen.insert(key).inserted else { return false }
            guard let removedAt = reconciled.removedAt[key] else {
                // Legacy removals had no timestamp. Once shared state is
                // readable it is the only safe authority: presence means a
                // later repin, absence means the old unpin already landed.
                return sharedStateModifiedAt == nil
            }
            if let sharedPin = sharedPins[key] {
                return max(sharedPin.pinnedAt, sharedStateModifiedAt ?? 0)
                    <= removedAt
            }
            guard let sharedStateModifiedAt else { return true }
            return sharedStateModifiedAt < removedAt
        }
        let retainedRemovalKeys = Set(reconciled.removedKeys)
        reconciled.removedAt = reconciled.removedAt.filter {
            retainedRemovalKeys.contains($0.key)
        }
        return reconciled
    }

    private static func appStateModifiedAtUnixMs() -> UInt64? {
        guard let stamp = statFile(LaunchConfig.appStateFile.path),
              stamp.mtimeSec >= 0,
              stamp.mtimeNsec >= 0
        else { return nil }
        return UInt64(stamp.mtimeSec) * 1_000
            + UInt64(stamp.mtimeNsec) / 1_000_000
    }

    private static func nextPinIntentTimestamp() -> UInt64 {
        let now = UInt64(Date().timeIntervalSince1970 * 1_000)
        guard let shared = appStateModifiedAtUnixMs(), shared >= now else { return now }
        return shared == UInt64.max ? shared : shared + 1
    }

    func isPinned(sessionID: String, projectID: String) -> Bool {
        pinnedByProject[projectID]?.contains {
            $0.key == PinnedSidebarSession.key(forSessionID: sessionID)
        } ?? false
    }

    /// Project whose sidebar node currently owns the session. A valid shared
    /// override wins; a removed/stale group falls back to the manifest's
    /// launch project, exactly like `rebuildTree`.
    private func effectiveProjectID(for session: SessionEntry) -> String {
        session.projectOverrideID.flatMap {
            projectsByID[$0] != nil ? $0 : nil
        } ?? session.projectID
    }

    func pinSession(projectID: String, sessionID: String) {
        if remoteSummariesByID[sessionID] != nil {
            performRemoteVerb("Couldn't pin the session") { runtime in
                try await runtime.setSessionPinned(sessionID, pinned: true)
            }
            return
        }
        let key = PinnedSidebarSession.key(forSessionID: sessionID)
        var overrides = loadPinOverrides()
        overrides.removedKeys.removeAll { $0 == key }
        overrides.removedAt.removeValue(forKey: key)
        overrides.added.removeAll { $0.key == key }
        overrides.added.append(PinnedSidebarSession(
            key: key,
            projectID: projectID,
            sessionID: sessionID,
            pinnedAt: Self.nextPinIntentTimestamp()
        ))
        savePinOverrides(overrides)
        rebuildPins(tauriPins: loadAppState()?.pinnedSessions ?? [:])
        announceStateChange("app-state")
    }

    func unpinSession(projectID _: String, sessionID: String) {
        if remoteSummariesByID[sessionID] != nil {
            performRemoteVerb("Couldn't unpin the session") { runtime in
                try await runtime.setSessionPinned(sessionID, pinned: false)
            }
            return
        }
        let key = PinnedSidebarSession.key(forSessionID: sessionID)
        var overrides = loadPinOverrides()
        overrides.added.removeAll { $0.key == key }
        if !overrides.removedKeys.contains(key) {
            overrides.removedKeys.append(key)
        }
        overrides.removedAt[key] = Self.nextPinIntentTimestamp()
        savePinOverrides(overrides)
        rebuildPins(tauriPins: loadAppState()?.pinnedSessions ?? [:])
        announceStateChange("app-state")
    }

    /// Merge shared pins with native write-failure fallbacks, then retire the
    /// fallbacks once the merged state is durably mirrored. Shared changes
    /// newer than an overlay win, so App -> TUI -> App handoff cannot revive
    /// an old pin or unpin. Rendering remains oldest-first so a newly pinned
    /// session lands at the bottom of the pin list.
    private func rebuildPins(tauriPins: [String: [PinnedSidebarSession]]) {
        let loadedOverrides = loadPinOverrides()
        var sharedByKey: [String: PinnedSidebarSession] = [:]
        for pin in tauriPins.values.joined() {
            if let current = sharedByKey[pin.key], current.pinnedAt > pin.pinnedAt {
                continue
            }
            sharedByKey[pin.key] = pin
        }
        var overrides = Self.reconciledPinOverrides(
            loadedOverrides,
            sharedPins: sharedByKey,
            sharedStateModifiedAt: Self.appStateModifiedAtUnixMs()
        )
        let removed = Set(overrides.removedKeys)

        var merged: [String: PinnedSidebarSession] = [:]
        for pin in sharedByKey.values where !removed.contains(pin.key) {
            merged[pin.key] = pin
        }
        for pin in overrides.added {
            merged[pin.key] = pin
        }
        if mirrorPinsToSharedState(overrides) {
            // The pending intents are now reflected in the latest locked
            // shared object. Clear the write-ahead fallback only after that
            // succeeds; a corrupt/unwritable file leaves it recoverable.
            overrides = NativePinOverrides()
        }

        // Garbage-collect native overrides for sessions whose artifact dirs are
        // gone for good. Do not key this off `sessionsByID`: a torn manifest
        // write or transient scan miss must not permanently unpin a session.
        let beforeAdded = overrides.added.count
        overrides.added.removeAll { pin in
            guard let sessionID = pin.sessionID else { return true }
            return sessionsByID[sessionID] == nil
                && pendingSessions[sessionID] == nil
                && !sessionArtifactsExist(sessionID)
        }
        if overrides.added.count != beforeAdded {
            overrides.removedAt = overrides.removedAt.filter { key, _ in
                overrides.removedKeys.contains(key)
            }
        }
        if overrides != loadedOverrides {
            savePinOverrides(overrides)
        }

        var grouped: [String: [PinnedSidebarSession]] = [:]
        for pin in merged.values {
            guard let sessionID = pin.sessionID,
                  let session = sessionsByID[sessionID],
                  effectiveProjectID(for: session) == pin.projectID
            else { continue }
            grouped[pin.projectID, default: []].append(pin)
        }
        for key in grouped.keys {
            grouped[key]?.sort {
                $0.pinnedAt != $1.pinnedAt ? $0.pinnedAt < $1.pinnedAt : $0.key < $1.key
            }
            // The combined cross-frontend order wins when it contains pin
            // ranks; the legacy native-only pin overlay remains a migration
            // fallback. Newly-pinned sessions append below the ordered block.
            grouped[key] = applyPinnedOrderOverlay(grouped[key] ?? [], projectID: key)
        }

        if grouped != pinnedByProject {
            pinnedByProject = grouped
            invalidateSidebarLists()
        }
    }

    private func sessionArtifactsExist(_ sessionID: String) -> Bool {
        FileManager.default.fileExists(
            atPath: LaunchConfig.appSessionsDir
                .appendingPathComponent(sessionID, isDirectory: true)
                .path
        )
    }

    /// Reorder pins by the `pinnedOrder` overlay (mirrors
    /// `applySessionOrderOverlay`): overlay entries form a hand-ordered block,
    /// pins not in the overlay keep oldest-first BELOW it so a freshly pinned
    /// session lands at the bottom.
    private func applyPinnedOrderOverlay(
        _ base: [PinnedSidebarSession], projectID: String
    ) -> [PinnedSidebarSession] {
        Self.orderedPinnedSessions(
            base,
            sharedOrder: sessionOrderPreviews[projectID]
                ?? Self.sharedSessionOrder(projectID: projectID),
            localOrder: AppDefaults.shared.stringArray(forKey: Self.pinnedOrderKey(projectID))
        )
    }

    static func orderedPinnedSessions(
        _ base: [PinnedSidebarSession],
        sharedOrder: [String]?,
        localOrder: [String]?
    ) -> [PinnedSidebarSession] {
        let baseIDs = Set(base.compactMap(\.sessionID))
        let sharedKnown = sharedOrder?.filter { baseIDs.contains($0) } ?? []
        let overlay = !sharedKnown.isEmpty ? sharedKnown : (localOrder ?? [])
        guard !overlay.isEmpty else { return base }
        let known = overlay.filter { baseIDs.contains($0) }
        guard !known.isEmpty else { return base }
        var rank: [String: Int] = [:]
        for (index, id) in known.enumerated() { rank[id] = index }
        let rest = base.filter { $0.sessionID.map { rank[$0] == nil } ?? true }
        let ordered = base.filter { $0.sessionID.map { rank[$0] != nil } ?? false }
            .sorted { rank[$0.sessionID ?? ""]! < rank[$1.sessionID ?? ""]! }
        return ordered + rest
    }

    // MARK: - Drag-reorder overlays (native; app-state.json is read-only)

    /// The Svelte app persists project order as `sort_order` via the Tauri
    /// `reorder_projects` command (project.rs:227-260) and has NO
    /// within-project session reordering (dragging a session there moves it
    /// to another project). Natively both orders are UserDefaults overlays
    /// merged over the file/derived order at read time:
    /// - `unpeel.native.projectOrder`             = [top-level project ids]
    /// - `unpeel.native.projectOrder.<projectID>` = [worktree child project ids]
    /// - `unpeel.native.sessionOrder.<projectID>` = [session ids]
    /// Ids absent from an overlay keep file/derived order (projects append
    /// last like add_project's max+1 sort_order; sessions stay newest-first
    /// on top). Stale ids are GC'd when a new order is persisted and lazily
    /// pruned at read time.
    static let projectOrderKey = "unpeel.native.projectOrder"

    static func projectOrderKey(forParent parentID: String?) -> String {
        guard let parentID else { return projectOrderKey }
        return "\(projectOrderKey).\(parentID)"
    }

    static func sessionOrderKey(_ projectID: String) -> String {
        "unpeel.native.sessionOrder.\(projectID)"
    }

    /// One combined shared rank list. Each frontend filters it into pinned,
    /// running, and stopped buckets, so publishing one bucket must preserve
    /// the ranks belonging to all the others.
    static func combinedSessionOrder(pinnedIDs: [String], regularIDs: [String]) -> [String] {
        var seen = Set<String>()
        return (pinnedIDs + regularIDs).filter { seen.insert($0).inserted }
    }

    static func replacingSessionID(
        in order: [String]?, oldID: String, newID: String
    ) -> [String]? {
        guard var order, let rank = order.firstIndex(of: oldID) else { return nil }
        order[rank] = newID
        return order
    }

    /// Legacy native pin-order fallback. New writes also publish the combined
    /// shared session order so the TUI sees them; this key remains separate
    /// from the regular local overlay for migration and bucket isolation.
    static func pinnedOrderKey(_ projectID: String) -> String {
        "unpeel.native.pinnedOrder.\(projectID)"
    }

    /// `~/.unpeel/project-order.json` — a flat rank list for every project.
    /// Filtering it by parent yields top-level or child-folder sibling order,
    /// shared with the terminal UI exactly as `session-order.json` is. Cached
    /// against the file's modification date: this is read on every sidebar
    /// rebuild, so the unchanged case must cost a stat.
    private nonisolated(unsafe) static var sharedProjectOrderCache: (stamp: Date, ids: [String])?

    static var sharedProjectOrderURL: URL {
        LaunchConfig.unpeelDir.appendingPathComponent("project-order.json")
    }

    static func sharedProjectOrder() -> [String]? {
        let url = sharedProjectOrderURL
        let stamp = (try? FileManager.default.attributesOfItem(atPath: url.path))?[.modificationDate] as? Date
        guard let stamp else {
            sharedProjectOrderCache = nil
            return nil
        }
        if sharedProjectOrderCache?.stamp != stamp {
            let ids = (try? Data(contentsOf: url))
                .flatMap { try? JSONSerialization.jsonObject(with: $0) } as? [String] ?? []
            sharedProjectOrderCache = (stamp, ids)
        }
        let ids = sharedProjectOrderCache?.ids ?? []
        return ids.isEmpty ? nil : ids
    }

    /// Tell every other Unpeel that shared state moved — the Swift half of
    /// `unpeel-core::state_bus`. Same registry (`~/.unpeel/app-ports`), same
    /// route, our own port skipped. Fire-and-forget: a peer that has gone is
    /// normal, and nothing here may delay a UI action.
    nonisolated static func announceStateChange(_ change: String, ownPort: UInt16?) {
        let registry = LaunchConfig.unpeelDir.appendingPathComponent("app-ports")
        guard let raw = try? String(contentsOf: registry, encoding: .utf8) else { return }
        let ports = raw.split(whereSeparator: \.isNewline)
            .compactMap { UInt16($0.trimmingCharacters(in: .whitespaces)) }
            .filter { $0 != 0 && $0 != ownPort }
        guard !ports.isEmpty else { return }
        let body = #"{"change":"\#(change)"}"#
        DispatchQueue.global(qos: .utility).async {
            for port in Set(ports) {
                guard let url = URL(string: "http://127.0.0.1:\(port)/state-changed") else {
                    continue
                }
                var request = URLRequest(url: url)
                request.httpMethod = "POST"
                request.timeoutInterval = 0.25
                request.setValue("application/json", forHTTPHeaderField: "Content-Type")
                request.httpBody = Data(body.utf8)
                URLSession.shared.dataTask(with: request).resume()
            }
        }
    }

    func announceStateChange(_ change: String) {
        Self.announceStateChange(change, ownPort: hookServer?.port)
    }

    @discardableResult
    static func writeSharedProjectOrder(
        siblingIDs: [String], fallbackAllIDs: [String]
    ) -> Bool {
        // Merge under the cross-frontend lock. A project drag and a child
        // drag can happen in different frontends at once; replacing only
        // this sibling set's occupied ranks keeps both edits.
        let wrote = PresetStateFile.withExclusiveLock(on: sharedProjectOrderURL) {
            var shared = (try? Data(contentsOf: sharedProjectOrderURL))
                .flatMap { try? JSONSerialization.jsonObject(with: $0) } as? [String]
                ?? fallbackAllIDs
            for id in fallbackAllIDs where !shared.contains(id) { shared.append(id) }
            let siblingSet = Set(siblingIDs)
            let slots = shared.indices.filter { siblingSet.contains(shared[$0]) }
            guard slots.count == siblingIDs.count else { return false }
            for (slot, id) in zip(slots, siblingIDs) { shared[slot] = id }
            guard let merged = try? JSONSerialization.data(withJSONObject: shared) else {
                return false
            }
            do {
                try merged.write(to: sharedProjectOrderURL, options: .atomic)
                return true
            } catch {
                return false
            }
        } ?? false
        sharedProjectOrderCache = nil
        return wrote
    }

    private func applyProjectOrderOverlay(_ base: [Project], parentID: String?) -> [Project] {
        let baseIDs = Set(base.map(\.id))
        // An in-flight drag preview outranks everything, then the
        // cross-frontend file when it knows this sibling set. Older files
        // contain top-level ids only, so retain the existing per-parent
        // UserDefaults overlay as a migration fallback for child folders.
        let preview = projectOrderPreview.flatMap {
            $0.parentID == parentID ? $0.ids : nil
        }
        let shared = Self.sharedProjectOrder()?.filter { baseIDs.contains($0) }
        guard let overlay = preview
                ?? (shared?.isEmpty == false ? shared : nil)
                ?? AppDefaults.shared.stringArray(
                    forKey: Self.projectOrderKey(forParent: parentID)
                ),
              !overlay.isEmpty
        else { return base }
        // Unknown ids are skipped, NOT GC'd here: a project can be merely
        // not-yet-known at read time (ephemeral test projects register
        // after the first rescan). Stale ids are dropped when the next
        // drag persists a fresh order (setProjectOrder writes current ids
        // only), so the overlay cannot grow unbounded.
        let known = overlay.filter { baseIDs.contains($0) }
        guard !known.isEmpty else { return base }
        var rank: [String: Int] = [:]
        for (index, id) in known.enumerated() { rank[id] = index }
        let ordered = base.filter { rank[$0.id] != nil }
            .sorted { rank[$0.id]! < rank[$1.id]! }
        let rest = base.filter { rank[$0.id] == nil }
        return ordered + rest
    }

    private func pruneProjectOrderOverlays(removing removedIDs: Set<String>) {
        guard !removedIDs.isEmpty else { return }
        let defaults = AppDefaults.shared
        let childOrderPrefix = Self.projectOrderKey + "."
        let keys = defaults.dictionaryRepresentation().keys.filter {
            $0 == Self.projectOrderKey || $0.hasPrefix(childOrderPrefix)
        }
        for key in keys {
            if key.hasPrefix(childOrderPrefix),
               removedIDs.contains(String(key.dropFirst(childOrderPrefix.count))) {
                defaults.removeObject(forKey: key)
                continue
            }
            guard var ids = defaults.stringArray(forKey: key),
                  ids.contains(where: removedIDs.contains)
            else { continue }
            ids.removeAll { removedIDs.contains($0) }
            if ids.isEmpty {
                defaults.removeObject(forKey: key)
            } else {
                defaults.set(ids, forKey: key)
            }
        }
    }

    private func applySessionOrderOverlay(
        _ base: [SessionEntry], projectID: String
    ) -> [SessionEntry] {
        // Date sort ignores the manual order entirely (the stored order
        // survives for a switch back) and uses the same shape as Recent:
        // working rows first, then every other lifecycle event newest-first.
        if dateSortedProjectIDs.contains(projectID) {
            return Self.sessionsSortedByRecentActivity(
                base,
                restartingSessionIDs: restartingSessionIDs
            )
        }
        // Same precedence as `sidebarSessionBlocks`: the cross-frontend file
        // wins, the local overlay is the fallback.
        let key = Self.sessionOrderKey(projectID)
        guard let overlay = sessionOrderPreviews[projectID]
                ?? Self.sharedSessionOrder(projectID: projectID)
                ?? AppDefaults.shared.stringArray(forKey: key),
              !overlay.isEmpty
        else { return base }
        let baseIDs = Set(base.map(\.id))
        // Unknown ids are skipped, NOT GC'd at read: a session can vanish
        // from one rescan transiently (torn manifest decode mid-heartbeat).
        // Stale ids drop out when the next drag persists a fresh order
        // (setSessionOrder), and removeSession prunes its id explicitly.
        let known = overlay.filter { baseIDs.contains($0) }
        guard !known.isEmpty else { return base }
        var rank: [String: Int] = [:]
        for (index, id) in known.enumerated() { rank[id] = index }
        // Sessions NOT in the overlay are newer than every overlay entry
        // (the overlay snapshots the whole visible list at drag time), so
        // they keep newest-first order ABOVE the hand-ordered block —
        // preserving "new sessions appear at the top".
        let rest = base.filter { rank[$0.id] == nil }
        let ordered = base.filter { rank[$0.id] != nil }
            .sorted { rank[$0.id]! < rank[$1.id]! }
        return rest + ordered
    }

    /// Pure shared rank for the Recent page shape and per-group "Recently
    /// updated" mode. A live-but-idle session is NOT privileged over a more
    /// recent exited one; only work currently in progress gets the leading
    /// tier. Id is the deterministic final tie-break across rescans/frontends.
    static func sessionsSortedByRecentActivity(
        _ sessions: [SessionEntry],
        restartingSessionIDs: Set<String> = []
    ) -> [SessionEntry] {
        func isWorking(_ session: SessionEntry) -> Bool {
            session.status == .starting
                || session.status == .busy
                || restartingSessionIDs.contains(session.id)
        }
        func stamp(_ session: SessionEntry) -> Int64 {
            max(session.createdAt, session.lifecycleAtMs ?? 0)
        }
        return sessions.sorted { lhs, rhs in
            let lhsWorking = isWorking(lhs)
            let rhsWorking = isWorking(rhs)
            if lhsWorking != rhsWorking { return lhsWorking }
            let lhsStamp = stamp(lhs)
            let rhsStamp = stamp(rhs)
            if lhsStamp != rhsStamp { return lhsStamp > rhsStamp }
            return lhs.id < rhs.id
        }
    }

    /// Durable project/worktree sibling move used by tests and non-drag
    /// callers: move `draggedID` to `targetID`'s position among siblings and
    /// persist that sibling order. The desktop drag path uses
    /// `previewProjectMove` and commits only on drop. Remote projects route
    /// through the Host's `project.organization.set` — never local state.
    func moveProject(draggedID: String, over targetID: String) {
        guard let (parentID, ids) = projectSiblingMove(
            draggedID: draggedID, over: targetID
        ) else { return }
        if remoteProjectSummariesByID[draggedID] != nil {
            commitRemoteProjectOrder(draggedID: draggedID, ids: ids)
            return
        }
        setProjectOrder(ids, parentID: parentID)
    }

    /// In-memory move used by the desktop drag path. Rows still animate live,
    /// but no shared/local state is written until `commitProjectReorder`.
    /// Works identically in remote scope: the preview reorders the remote
    /// projection in memory (`projectRemoteScope` reads it) and nothing local
    /// is touched.
    func previewProjectMove(draggedID: String, over targetID: String) {
        guard let (parentID, ids) = projectSiblingMove(
            draggedID: draggedID, over: targetID
        ) else { return }
        guard projectOrderPreview?.parentID != parentID
            || projectOrderPreview?.ids != ids
        else { return }
        projectOrderPreview = (parentID, ids, draggedID)
        if remoteProjectSummariesByID[draggedID] != nil {
            withAnimation(.easeInOut(duration: 0.18)) {
                projectRemoteScope(snapshot: remoteHostRuntime.snapshot)
            }
            return
        }
        withAnimation(.easeInOut(duration: 0.18)) { rebuildTreeFromLastScan() }
    }

    /// Persist the final desktop drag preview exactly once. Local scope
    /// writes the shared order files; remote scope commits the SAME drag
    /// through the Host's `project.organization.set` (one-project patch: the
    /// dragged project's final sibling index) and keeps the optimistic order
    /// on screen until the refreshed bootstrap confirms it. No local
    /// project-order state is ever written for remote entities.
    func commitProjectReorder() {
        guard let preview = projectOrderPreview else { return }
        // The displayed tree still carries the live preview. Capture the
        // final visible sibling order before removing its precedence.
        let ids = projectOrderIDs(parentID: preview.parentID)
        projectOrderPreview = nil
        if remoteProjectSummariesByID[preview.draggedID] != nil {
            commitRemoteProjectOrder(draggedID: preview.draggedID, ids: ids)
            return
        }
        setProjectOrder(ids, parentID: preview.parentID)
    }

    /// Remote half of a project reorder: send the dragged project's new
    /// sibling index; the Host applies it to its own display order through
    /// the same choke point a local drag uses, and the runtime's bootstrap
    /// refresh reconciles the projection.
    private func commitRemoteProjectOrder(draggedID: String, ids: [String]) {
        guard let index = ids.firstIndex(of: draggedID) else {
            withAnimation(.easeInOut(duration: 0.18)) {
                projectRemoteScope(snapshot: remoteHostRuntime.snapshot)
            }
            return
        }
        // Hold the dropped order on screen until a bootstrap confirms it —
        // a poll captured before the Host applied the write must not snap
        // the drag back. A failed verb rolls back visibly with its alert.
        remoteCommittedOrderHold =
            (remoteProjectsByID[draggedID]?.parentProjectID, ids, Date())
        performRemoteVerb("Couldn't reorder the projects", onFailure: { [weak self] in
            guard let self else { return }
            self.remoteCommittedOrderHold = nil
            withAnimation(.easeInOut(duration: 0.18)) {
                self.projectRemoteScope(snapshot: self.remoteHostRuntime.snapshot)
            }
        }) { runtime in
            try await runtime.setProjectSortOrder(
                projectID: draggedID,
                sortOrder: index
            )
        }
    }

    /// Roll back a drag that left the sidebar or otherwise did not produce an
    /// accepted drop. The persisted order was never touched.
    func cancelProjectReorder() {
        guard let preview = projectOrderPreview else { return }
        projectOrderPreview = nil
        if remoteProjectSummariesByID[preview.draggedID] != nil {
            withAnimation(.easeInOut(duration: 0.18)) {
                projectRemoteScope(snapshot: remoteHostRuntime.snapshot)
            }
            return
        }
        withAnimation(.easeInOut(duration: 0.18)) { rebuildTreeFromLastScan() }
    }

    /// Shared sibling-reorder math: the dragged project takes the target's
    /// slot among same-parent siblings (a cross-parent pair is a no-op).
    /// Scope-neutral: reads the displayed tree, so the same drag works over
    /// local nodes and the remote projection alike.
    private func projectSiblingMove(
        draggedID: String, over targetID: String
    ) -> (parentID: String?, ids: [String])? {
        guard draggedID != targetID else { return nil }
        guard let dragged = displayProjectsByID[draggedID],
              let target = displayProjectsByID[targetID],
              dragged.parentProjectID == target.parentProjectID
        else { return nil }
        let parentID = dragged.parentProjectID
        var ids = projectOrderIDs(parentID: parentID)
        guard let from = ids.firstIndex(of: draggedID),
              let to = ids.firstIndex(of: targetID)
        else { return nil }
        ids.remove(at: from)
        ids.insert(draggedID, at: to)
        return (parentID, ids)
    }

    private func projectOrderIDs(parentID: String?) -> [String] {
        guard let parentID else { return displayNodes.map(\.id) }
        return findDisplayNode(parentID)?.worktrees.map(\.id) ?? []
    }

    private func flattenedProjectOrderIDs() -> [String] {
        var ids: [String] = []
        func append(_ nodes: [ProjectNode]) {
            for node in nodes {
                ids.append(node.id)
                append(node.worktrees)
            }
        }
        append(nodes)
        return ids
    }

    func setProjectOrder(_ ids: [String], parentID: String? = nil) {
        let key = Self.projectOrderKey(forParent: parentID)
        if ids.isEmpty {
            AppDefaults.shared.removeObject(forKey: key)
        } else {
            AppDefaults.shared.set(ids, forKey: key)
        }
        // Publish every sibling reorder. The shared representation is one
        // flat list: replace just this sibling set's occupied slots, leaving
        // every other parent/root rank untouched.
        if !ids.isEmpty {
            if Self.writeSharedProjectOrder(
                siblingIDs: ids,
                fallbackAllIDs: flattenedProjectOrderIDs()
            ) {
                announceStateChange("order")
            }
        }
        withAnimation(.easeInOut(duration: 0.18)) { rebuildTreeFromLastScan() }
    }

    /// Order-overlay-only rebuild for the drag-reorder path: reapplies the
    /// native order overlays over the last scan's inputs. Session previews
    /// fire on every drag-hover tick, and a full rescan (directory scan +
    /// every UserDefaults overlay + activity snapshot) per tick made dragging
    /// lag.
    private func rebuildTreeFromLastScan() {
        guard hasCompletedScan else { return rescan() }
        rebuildTree(projects: lastScanProjects, sessions: lastScanSessions)
        rebuildPins(tauriPins: lastScanTauriPins)
    }

    /// In-memory move used by the desktop drag path. Rows still animate live,
    /// but no shared/local state is written until `commitSessionReorder`.
    func previewSessionMove(projectID: String, draggedID: String, over targetID: String) {
        guard draggedID != targetID, let node = findDisplayNode(projectID) else { return }
        let pinnedIDs = pinnedSessions(in: node).map(\.id)
        let pinned = Set(pinnedIDs)
        var regularIDs = node.sessions.map(\.id).filter { !pinned.contains($0) }
        guard let from = regularIDs.firstIndex(of: draggedID),
              let to = regularIDs.firstIndex(of: targetID)
        else { return }
        regularIDs.remove(at: from)
        regularIDs.insert(draggedID, at: to)
        let preview = Self.combinedSessionOrder(
            pinnedIDs: pinnedIDs, regularIDs: regularIDs
        )
        guard sessionOrderPreviews[projectID] != preview else { return }
        sessionOrderPreviews[projectID] = preview
        refreshAfterOrderPreviewChange(projectID: projectID)
    }

    /// Durable regular-section move used by tests and non-drag callers. The
    /// desktop drag path uses `previewSessionMove` and commits only on drop.
    func moveSession(projectID: String, draggedID: String, over targetID: String) {
        guard draggedID != targetID, let node = findNode(projectID) else { return }
        let pinnedIDs = Set(
            (pinnedByProject[projectID] ?? []).compactMap(\.sessionID)
        )
        var ids = node.sessions.map(\.id).filter { !pinnedIDs.contains($0) }
        guard let from = ids.firstIndex(of: draggedID),
              let to = ids.firstIndex(of: targetID)
        else { return }
        ids.remove(at: from)
        ids.insert(draggedID, at: to)
        setSessionOrder(projectID: projectID, ids: ids)
    }

    func setSessionOrder(projectID: String, ids: [String]) {
        let pinnedIDs = (pinnedByProject[projectID] ?? []).compactMap(\.sessionID)
        let shared = Self.combinedSessionOrder(pinnedIDs: pinnedIDs, regularIDs: ids)
        if Self.writeSharedSessionOrder(projectID: projectID, ids: shared) {
            announceStateChange("order")
        }
        AppDefaults.shared.set(ids, forKey: Self.sessionOrderKey(projectID))
        withAnimation(.easeInOut(duration: 0.18)) { rebuildTreeFromLastScan() }
    }

    /// Pinned-section counterpart to `previewSessionMove`.
    func previewPinnedSessionMove(
        projectID: String, draggedID: String, over targetID: String
    ) {
        guard draggedID != targetID, let node = findDisplayNode(projectID) else { return }
        var pinnedIDs = pinnedSessions(in: node).map(\.id)
        guard let from = pinnedIDs.firstIndex(of: draggedID),
              let to = pinnedIDs.firstIndex(of: targetID)
        else { return }
        pinnedIDs.remove(at: from)
        pinnedIDs.insert(draggedID, at: to)
        let pinned = Set(pinnedIDs)
        let regularIDs = node.sessions.map(\.id).filter {
            !pinned.contains($0)
        }
        let preview = Self.combinedSessionOrder(
            pinnedIDs: pinnedIDs, regularIDs: regularIDs
        )
        guard sessionOrderPreviews[projectID] != preview else { return }
        sessionOrderPreviews[projectID] = preview
        refreshAfterOrderPreviewChange(projectID: projectID)
    }

    /// Durable pinned-section move used by tests and non-drag callers. The
    /// desktop drag path uses `previewPinnedSessionMove`; pinned and regular
    /// rows remain separate buckets.
    func movePinnedSession(projectID: String, draggedID: String, over targetID: String) {
        guard draggedID != targetID else { return }
        var ids = (pinnedByProject[projectID] ?? []).compactMap(\.sessionID)
        guard let from = ids.firstIndex(of: draggedID),
              let to = ids.firstIndex(of: targetID)
        else { return }
        ids.remove(at: from)
        ids.insert(draggedID, at: to)
        setPinnedOrder(projectID: projectID, ids: ids)
    }

    func setPinnedOrder(projectID: String, ids: [String]) {
        AppDefaults.shared.set(ids, forKey: Self.pinnedOrderKey(projectID))
        let pinned = Set(ids)
        let regularIDs = findNode(projectID)?.sessions.map(\.id).filter {
            !pinned.contains($0)
        } ?? []
        let shared = Self.combinedSessionOrder(pinnedIDs: ids, regularIDs: regularIDs)
        if Self.writeSharedSessionOrder(projectID: projectID, ids: shared) {
            announceStateChange("order")
        }
        withAnimation(.easeInOut(duration: 0.18)) { rebuildTreeFromLastScan() }
    }

    /// Persist the final desktop drag preview exactly once. The section flag
    /// chooses which legacy UserDefaults fallback is kept in sync; the shared
    /// file always receives the combined pin + regular order.
    func commitSessionReorder(projectID: String, pinned: Bool) {
        guard sessionOrderPreviews[projectID] != nil else { return }
        // Remote nodes commit the combined pinned + regular visible order
        // through the Host's `session.order.set`, exactly as a desktop drag
        // commits it locally. The optimistic order keeps the rows in place
        // until the next bootstrap confirms it.
        if remoteProjectSummariesByID[projectID] != nil {
            guard let node = findDisplayNode(projectID) else {
                sessionOrderPreviews.removeValue(forKey: projectID)
                return
            }
            let orderedIDs = (renderedPinnedSessions(in: node)
                + renderedDisplayedSessions(in: node)).map(\.id)
            remoteSessionOrderByProject[projectID] = orderedIDs
            sessionOrderPreviews.removeValue(forKey: projectID)
            invalidateSidebarLists()
            performRemoteVerb("Couldn't reorder the sessions") { runtime in
                try await runtime.setSessionOrder(
                    projectID: projectID,
                    orderedSessionIDs: orderedIDs
                )
            }
            return
        }
        // `nodes` and `pinnedByProject` still carry the live preview. Capture
        // those final visible orders before removing its precedence.
        let pinnedIDs = (pinnedByProject[projectID] ?? []).compactMap(\.sessionID)
        let pinnedSet = Set(pinnedIDs)
        let regularIDs = findNode(projectID)?.sessions.map(\.id).filter {
            !pinnedSet.contains($0)
        } ?? []
        sessionOrderPreviews.removeValue(forKey: projectID)
        if pinned {
            setPinnedOrder(projectID: projectID, ids: pinnedIDs)
        } else {
            setSessionOrder(projectID: projectID, ids: regularIDs)
        }
    }

    /// Roll back a drag that left the sidebar or otherwise did not produce an
    /// accepted drop. The persisted order was never touched.
    func cancelSessionReorder(projectID: String) {
        guard sessionOrderPreviews.removeValue(forKey: projectID) != nil else { return }
        if remoteProjectSummariesByID[projectID] != nil {
            withAnimation(.easeInOut(duration: 0.18)) {
                projectRemoteScope(snapshot: remoteHostRuntime.snapshot)
            }
            return
        }
        withAnimation(.easeInOut(duration: 0.18)) { rebuildTreeFromLastScan() }
    }

    private func findNode(_ projectID: String) -> ProjectNode? {
        func search(_ nodes: [ProjectNode]) -> ProjectNode? {
            for node in nodes {
                if node.id == projectID { return node }
                if let found = search(node.worktrees) { return found }
            }
            return nil
        }
        return search(nodes)
    }

    // MARK: - Rename session (rename_session, pty_manager.rs:2197-2215)

    /// Session id whose sidebar row shows the inline rename editor (one at
    /// a time, like `editingSessionId` in ProjectItem.svelte:146).
    @Published var editingSessionID: String?

    /// Legacy native rename fallback: [session id: custom title]. New writes
    /// publish `title.json`; pre-marker values are migrated there on scan and
    /// retained here only if that shared write fails. We also mirror the
    /// resolved title into manifest.json so the Rust host and Sessions MCP
    /// report the same title as the sidebar.
    private func loadSessionTitleOverrides() -> [String: String] {
        (AppDefaults.shared.dictionary(forKey: NativeOverlay.sessionTitlesKey)
            as? [String: String]) ?? [:]
    }

    private func saveSessionTitleOverrides(_ overrides: [String: String]) {
        if overrides.isEmpty {
            AppDefaults.shared.removeObject(forKey: NativeOverlay.sessionTitlesKey)
        } else {
            AppDefaults.shared.set(overrides, forKey: NativeOverlay.sessionTitlesKey)
        }
    }

    static func decodedPendingTitleWrites(_ stored: Any?) -> [String: UInt64] {
        if let data = stored as? Data,
           let decoded = try? JSONDecoder().decode([String: UInt64].self, from: data) {
            return decoded
        }
        if let dictionary = stored as? [String: Any] {
            return dictionary.reduce(into: [:]) { result, item in
                if let timestamp = jsonUInt64(item.value) {
                    result[item.key] = timestamp
                }
            }
        }
        if let legacySessionIDs = stored as? [String] {
            // The first uncommitted implementation stored only a pending bit.
            // Zero means "unknown age": it may retry when no marker exists,
            // but it can never overwrite a valid shared marker.
            return Dictionary(
                legacySessionIDs.map { ($0, UInt64(0)) },
                uniquingKeysWith: { _, newest in newest }
            )
        }
        return [:]
    }

    private func loadPendingTitleWrites() -> [String: UInt64] {
        Self.decodedPendingTitleWrites(
            AppDefaults.shared.object(forKey: Self.nativePendingTitleWritesKey)
        )
    }

    private func savePendingTitleWrites(_ writes: [String: UInt64]) {
        if writes.isEmpty {
            AppDefaults.shared.removeObject(forKey: Self.nativePendingTitleWritesKey)
        } else if let data = try? JSONEncoder().encode(writes) {
            AppDefaults.shared.set(data, forKey: Self.nativePendingTitleWritesKey)
        }
    }

    static func normalizedSessionTitle(_ title: String?) -> String? {
        let normalized = title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return normalized.isEmpty ? nil : normalized
    }

    struct SessionTitleResolution: Equatable {
        let title: String?
        let shouldPublishNative: Bool
    }

    /// Shared markers are the durable App/TUI contract. A failed native write
    /// retries only when its durable intent timestamp is newer. Timestamp-less
    /// legacy pending bits defer to any valid marker, preventing an old native
    /// fallback from overwriting a later TUI/CLI rename after relaunch.
    static func resolvedSessionTitle(
        sharedMarker: SharedTitleMarker?,
        nativeTitle: String?,
        pendingWriteAt: UInt64?
    ) -> SessionTitleResolution {
        let sharedTitle = normalizedSessionTitle(sharedMarker?.title)
        let nativeTitle = normalizedSessionTitle(nativeTitle)
        guard let nativeTitle else {
            return SessionTitleResolution(
                title: sharedTitle,
                shouldPublishNative: false
            )
        }
        guard let sharedTitle else {
            return SessionTitleResolution(
                title: nativeTitle,
                shouldPublishNative: true
            )
        }
        guard let pendingWriteAt,
              pendingWriteAt > 0,
              let sharedUpdatedAt = sharedMarker?.updatedAt,
              pendingWriteAt > sharedUpdatedAt
        else {
            return SessionTitleResolution(
                title: sharedTitle,
                shouldPublishNative: false
            )
        }
        return SessionTitleResolution(
            title: nativeTitle,
            shouldPublishNative: true
        )
    }

    private static func nextTitleIntentTimestamp(
        after previous: UInt64? = nil
    ) -> UInt64 {
        let now = UInt64(Date().timeIntervalSince1970 * 1_000)
        guard let previous, previous >= now else { return now }
        return previous == UInt64.max ? previous : previous + 1
    }

    /// All native title publication goes through this helper so a successful
    /// atomic replacement cannot be hidden by a same-stamp/same-size cache hit
    /// during the immediate rescan.
    @discardableResult
    private func publishTitleMarker(
        sessionID: String,
        title: String,
        updatedAt: UInt64
    ) -> Bool {
        let wrote = Self.writeSharedMarker(
            sessionID,
            .title,
            ["title": title, "updated_at": updatedAt]
        )
        if wrote {
            titleMarkerCache.removeValue(forKey: sessionID)
        }
        return wrote
    }

    private func syncSessionTitleOverrideToManifest(sessionID: String, label: String) {
        // The Host commits the in-place runtime generation with its own
        // whole-manifest update. Do not race that commit with this legacy
        // compatibility mirror; title.json remains authoritative meanwhile.
        guard !resumingAgentSessionIDs.contains(sessionID) else { return }
        writeSessionManifestFields(sessionID: sessionID) { session in
            var changed = false
            if session["label"] as? String != label {
                session["label"] = label
                changed = true
            }
            if session["custom_title"] as? Bool != true {
                session["custom_title"] = true
                changed = true
            }
            return changed
        }
    }

    @discardableResult
    private func writeSessionManifestFields(
        sessionID: String,
        mutate: (inout [String: Any]) -> Bool
    ) -> Bool {
        Self.withSessionManifestLock(sessionID: sessionID) {
            let manifestURL = LaunchConfig.appSessionsDir
                .appendingPathComponent(sessionID)
                .appendingPathComponent("manifest.json")
            guard let data = try? Data(contentsOf: manifestURL),
                  var object = (try? JSONSerialization.jsonObject(with: data))
                    as? [String: Any],
                  var session = object["session"] as? [String: Any]
            else { return false }

            guard mutate(&session) else { return false }
            object["session"] = session
            guard JSONSerialization.isValidJSONObject(object),
                  let encoded = try? JSONSerialization.data(
                    withJSONObject: object,
                    options: [.prettyPrinted]
                  )
            else { return false }

            do {
                try encoded.write(to: manifestURL, options: [.atomic])
                manifestCache.removeValue(forKey: sessionID)
                return true
            } catch {
                NSLog("[UnpeelNative] failed to sync session title to manifest: \(error)")
                return false
            }
        } ?? false
    }

    /// Cross-process counterpart of Rust `manifest_lock_target` plus
    /// `app_state::lock_exclusive`: both sides flock the exact stable
    /// `~/.unpeel/session-manifest-locks/<sha256(session id)>.lock` path
    /// around every manifest read-modify-write cycle.
    private static func withSessionManifestLock<Result>(
        sessionID: String,
        _ operation: () -> Result
    ) -> Result? {
        let directory = LaunchConfig.unpeelDir
            .appendingPathComponent("session-manifest-locks", isDirectory: true)
        do {
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true
            )
        } catch {
            NSLog("[UnpeelNative] failed to create manifest lock directory: \(error)")
            return nil
        }
        let digest = SHA256.hash(data: Data(sessionID.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        let lockURL = directory.appendingPathComponent("\(digest).lock")
        let descriptor = open(
            lockURL.path,
            O_CREAT | O_RDWR | O_CLOEXEC,
            mode_t(0o600)
        )
        guard descriptor >= 0 else { return nil }
        defer { close(descriptor) }
        guard fchmod(descriptor, mode_t(0o600)) == 0,
              flock(descriptor, LOCK_EX) == 0
        else { return nil }
        defer { _ = flock(descriptor, LOCK_UN) }
        return operation()
    }

    // MARK: - Provider conversation ids (resume-on-restart)

    private func loadProviderSessionIDs() -> [String: String] {
        (AppDefaults.shared.dictionary(forKey: NativeOverlay.providerSessionIDsKey)
            as? [String: String]) ?? [:]
    }

    private func saveProviderSessionIDs(_ map: [String: String]) {
        if map.isEmpty {
            AppDefaults.shared.removeObject(forKey: NativeOverlay.providerSessionIDsKey)
        } else {
            AppDefaults.shared.set(map, forKey: NativeOverlay.providerSessionIDsKey)
        }
    }

    private func loadPendingAppendSystemContexts() -> [String: String] {
        (AppDefaults.shared.dictionary(forKey: NativeOverlay.appendedSystemContextsKey)
            as? [String: String]) ?? [:]
    }

    private func savePendingAppendSystemContexts(_ map: [String: String]) {
        if map.isEmpty {
            AppDefaults.shared.removeObject(forKey: NativeOverlay.appendedSystemContextsKey)
        } else {
            AppDefaults.shared.set(map, forKey: NativeOverlay.appendedSystemContextsKey)
        }
    }

    private func loadRestartRecommendationDismissals() -> [String: String] {
        (AppDefaults.shared.dictionary(
            forKey: NativeOverlay.restartRecommendationDismissalsKey
        ) as? [String: String]) ?? [:]
    }

    private func saveRestartRecommendationDismissals(_ map: [String: String]) {
        if map.isEmpty {
            AppDefaults.shared.removeObject(
                forKey: NativeOverlay.restartRecommendationDismissalsKey
            )
        } else {
            AppDefaults.shared.set(
                map,
                forKey: NativeOverlay.restartRecommendationDismissalsKey
            )
        }
    }

    // MARK: - Phone resize override (temporary phone-driven terminal size)

    /// Letterbox a session's desktop terminal to a phone's grid. The pane
    /// resize flows through the normal surface→attach path, so the hosted
    /// PTY follows without extra socket traffic.
    @discardableResult
    func setPhoneResizeOverride(sessionID: String, cols: Int, rows: Int) -> Bool {
        guard sessionsByID[sessionID] != nil else { return false }
        let grid = PhoneResizeOverride(
            cols: max(2, min(cols, 300)),
            rows: max(2, min(rows, 120))
        )
        if phoneResizeOverrides[sessionID] != grid {
            phoneResizeOverrides[sessionID] = grid
        }
        return true
    }

    /// Revert a phone-letterboxed session to its natural full-pane size.
    func clearPhoneResizeOverride(for sessionID: String) {
        guard phoneResizeOverrides[sessionID] != nil else { return }
        phoneResizeOverrides.removeValue(forKey: sessionID)
    }

    /// Open a REMOTE Unpeel's session as a local terminal pane: spawns a
    /// normal local session whose command is the remote attach CLI, so every
    /// existing affordance (sidebar row, pane cache, restart) just works.
    /// Credentials ride in the peer file (`--peer-file`), never the command.
    @discardableResult
    func attachRemoteUnpeelSession(
        remoteSessionID: String,
        remoteLabel: String
    ) -> String? {
        guard let project = nodes.first?.project else { return nil }
        let hostBinary = LaunchConfig.hostBinary
        let peerFile = RemoteUnpeelPeerStore.peerFileURL.path
        let command = "UNPEEL_REMOTE_ATTACH=1 "
            + shellQuote(hostBinary)
            + " __remote_attach__ --peer-file "
            + shellQuote(peerFile)
            + " "
            + shellQuote(remoteSessionID)
        return spawnSession(
            projectID: project.id,
            command: command,
            label: "Remote: \(remoteLabel)",
            customTitle: true,
            createdAt: Int64(Date().timeIntervalSince1970 * 1000),
            cwd: project.path,
            worktreePath: nil,
            worktreeBranch: nil
        )
    }

    private func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    /// /mobile/resize-desktop: phone-driven temporary desktop resize.
    /// Setting letterboxes the pane AND raw-resizes the hosted PTY, so a
    /// session with no mounted surface takes the phone size immediately;
    /// clearing reverts the pane (an unmounted session self-heals on its
    /// next attach, which re-asserts the full size).
    func applyRemoteDesktopResize(_ request: RemoteDesktopResizeRequest) throws {
        let sessionID = request.sessionID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !sessionID.isEmpty, !sessionID.contains("/"), !sessionID.contains("..") else {
            throw MobileRemoteError(400, "invalid session id")
        }
        if request.clear == true {
            clearPhoneResizeOverride(for: sessionID)
            return
        }
        guard let columns = request.columns, let rows = request.rows,
              columns > 0, rows > 0
        else {
            throw MobileRemoteError(400, "missing columns or rows")
        }
        guard setPhoneResizeOverride(sessionID: sessionID, cols: columns, rows: rows) else {
            throw MobileRemoteError(404, "unknown session")
        }
        try? MobileSessionControl.resize(sessionID: sessionID, columns: columns, rows: rows)
    }

    func dismissRestartRecommendation(for sessionID: String) {
        guard let recommendation = restartRecommendations[sessionID] else { return }
        var dismissals = loadRestartRecommendationDismissals()
        dismissals[sessionID] = recommendation.token
        saveRestartRecommendationDismissals(dismissals)
        restartRecommendations.removeValue(forKey: sessionID)
    }

    /// Restart recommendation for a live session, if any. An old hosted PTY
    /// must be replaced to gain current terminal behavior. Launch-context
    /// changes wait for the active agent to end, then offer Resume Agent from
    /// the returned shell. Only a known Host below the essential maintenance
    /// compatibility floor requires a terminal reload.
    static func restartRecommendation(
        for manifest: HostedSessionManifest,
        pendingAppendSystemContext: String?
    ) -> SessionRestartRecommendation? {
        if let version = manifest.hostProtocolVersion,
           version < requiredSessionHostProtocolVersion {
            return SessionRestartRecommendation(
                token: "host-protocol:\(requiredSessionHostProtocolVersion)",
                message: "Reload to use the updated terminal host.",
                action: .reloadTerminal
            )
        }
        if let token = appendedSystemContextRestartToken(for: pendingAppendSystemContext) {
            let command = manifest.session.command ?? ""
            let activeRuntimeID = manifest.runtime?.currentObservation?.id
            let canResumeAgent = ProviderCapabilities.canResumeAgent(
                command: command,
                isLive: manifest.state == "running",
                activeRuntimeID: activeRuntimeID,
                runtimeLaunchPending: manifest.runtimeLaunchPending,
                hostProtocolVersion: manifest.hostProtocolVersion
            )
            // Context is queued for the next launch. Never turn it into an
            // action that interrupts a runtime which still owns the
            // foreground, including on a v2 Host that will need a terminal
            // reload only after the agent returns to its shell.
            if activeRuntimeID != nil || manifest.runtimeLaunchPending {
                return SessionRestartRecommendation(
                    token: token,
                    message: "Appended system context will apply the next time the agent is resumed.",
                    action: nil
                )
            }
            // A current Host can still be transiently unable to prove a safe
            // managed relaunch after returning to the shell. Keep the context
            // pending and re-evaluate instead of replacing that terminal.
            if (manifest.hostProtocolVersion ?? 0)
                >= ProviderCapabilities.resumeAgentHostProtocolVersion,
               !canResumeAgent {
                return nil
            }
            return SessionRestartRecommendation(
                token: token,
                message: canResumeAgent
                    ? "Resume the agent to apply appended system context."
                    : "Reload the terminal to apply appended system context.",
                action: canResumeAgent ? .resumeAgent : .reloadTerminal
            )
        }
        return nil
    }

    private static func appendedSystemContextRestartToken(for context: String?) -> String? {
        guard let context else { return nil }
        let trimmed = context.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let digest = SHA256.hash(data: Data(trimmed.utf8))
        let shortHash = digest.prefix(8).map { String(format: "%02x", $0) }.joined()
        return "append-system-context:\(shortHash)"
    }

    /// Record the provider's own conversation metadata for a session (latest
    /// wins — Claude/Codex keep `session_id` stable across a conversation, and
    /// a fresh id after reset is exactly what a later restart/transcript read
    /// should target).
    private func recordProviderMetadata(
        providerSessionID: String?,
        providerTranscriptPath: String?,
        for sessionID: String
    ) {
        var providerIDChanged = false
        if let providerID = providerSessionID {
            var map = loadProviderSessionIDs()
            if map[sessionID] != providerID {
                map[sessionID] = providerID
                saveProviderSessionIDs(map)
                providerIDChanged = true
            }
        }

        writeProviderMetadataToManifest(
            sessionID: sessionID,
            providerSessionID: providerSessionID,
            providerTranscriptPath: providerTranscriptPath
        )

        // Shared marker: the cross-frontend copy (the overlay above is
        // app-only, the manifest write races the host). Merge so an id-only
        // event never erases a captured transcript path; skip unchanged —
        // hooks fire constantly. No state-bus announce, deliberately: every
        // frontend already heard this hook on the same port broadcast.
        let current = Self.readSharedMarker(sessionID, .providerSession) ?? [:]
        var next = current
        if let providerSessionID {
            next["provider_session_id"] = providerSessionID
        }
        if let providerTranscriptPath {
            next["provider_transcript_path"] = providerTranscriptPath
        }
        let changed = (next["provider_session_id"] as? String)
            != (current["provider_session_id"] as? String)
            || (next["provider_transcript_path"] as? String)
            != (current["provider_transcript_path"] as? String)
        if changed {
            next["captured_at"] = Int64(Date().timeIntervalSince1970 * 1000)
            Self.writeSharedMarker(sessionID, .providerSession, next)
        }

        // The conversation identity moved (in-tool /resume or /clear): if the
        // session is still untitled, title it from the resumed conversation's
        // transcript. After the marker/manifest writes above so the host verb
        // resolves the transcript the capture just pointed at; the host
        // no-ops once titling is settled, so a stale-id → fresh-id flip on an
        // already-titled session costs one short-lived process and nothing
        // else. The label lands via the ordinary manifest rescan.
        if providerIDChanged {
            Self.autoTitleFromProviderTranscript(sessionID: sessionID)
        }
    }

    /// Fire-and-forget `unpeel-host __auto_title__ <id>`
    /// (`transcripts::auto_title_session_from_transcript`): titles an
    /// untitled session from its provider conversation — Claude's summary
    /// record or the conversation's first user prompt.
    private nonisolated static func autoTitleFromProviderTranscript(sessionID: String) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: LaunchConfig.hostBinary)
        process.arguments = ["__auto_title__", sessionID]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            NSLog("[UnpeelNative] failed to spawn auto-title: \(error)")
        }
    }

    private func writeProviderMetadataToManifest(
        sessionID: String,
        providerSessionID: String?,
        providerTranscriptPath: String?
    ) {
        _ = Self.withSessionManifestLock(sessionID: sessionID) {
            let manifestURL = LaunchConfig.appSessionsDir
                .appendingPathComponent(sessionID)
                .appendingPathComponent("manifest.json")
            guard let data = try? Data(contentsOf: manifestURL),
                  var object = (try? JSONSerialization.jsonObject(with: data))
                    as? [String: Any]
            else { return }

            var changed = false
            if let providerSessionID,
               object["provider_session_id"] as? String != providerSessionID {
                object["provider_session_id"] = providerSessionID
                changed = true
            }
            if let providerTranscriptPath,
               object["provider_transcript_path"] as? String != providerTranscriptPath {
                object["provider_transcript_path"] = providerTranscriptPath
                changed = true
            }
            guard changed,
                  JSONSerialization.isValidJSONObject(object),
                  let encoded = try? JSONSerialization.data(
                    withJSONObject: object,
                    options: [.prettyPrinted]
                  )
            else { return }
            try? encoded.write(to: manifestURL, options: [.atomic])
            manifestCache.removeValue(forKey: sessionID)
        }
    }

    /// rename_session parity: the overlay entry is the native stand-in for
    /// `custom_title = true` — once set, the backend's auto-titling of the
    /// manifest label never shows again for this session. Empty labels are
    /// rejected (the view reverts to the original instead, matching
    /// commitEdit in ProjectItem.svelte:958-966).
    func renameSession(_ sessionID: String, to label: String) {
        let trimmed = label.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        if remoteSummariesByID[sessionID] != nil {
            performRemoteVerb("Couldn't rename the session") { runtime in
                try await runtime.renameSession(sessionID, to: trimmed)
            }
            return
        }
        var overrides = loadSessionTitleOverrides()
        overrides[sessionID] = trimmed
        saveSessionTitleOverrides(overrides)
        var pendingTitleWrites = loadPendingTitleWrites()
        let sharedUpdatedAt = Self.readSharedMarker(sessionID, .title)
            .flatMap { Self.jsonUInt64($0["updated_at"]) }
        let previousTimestamp = max(
            pendingTitleWrites[sessionID] ?? 0,
            sharedUpdatedAt ?? 0
        )
        let intentAt = Self.nextTitleIntentTimestamp(after: previousTimestamp)
        pendingTitleWrites[sessionID] = intentAt
        savePendingTitleWrites(pendingTitleWrites)
        syncSessionTitleOverrideToManifest(sessionID: sessionID, label: trimmed)
        // Save the local fallback before publishing. A successful shared
        // marker becomes authoritative on the rescan below; on failure the
        // pending bit keeps this title recoverable and schedules a retry.
        if publishTitleMarker(
            sessionID: sessionID,
            title: trimmed,
            updatedAt: intentAt
        ) {
            // The durable marker is now the only title authority. Clearing the
            // fallback here also covers a rescan that temporarily cannot decode
            // the session manifest.
            overrides.removeValue(forKey: sessionID)
            saveSessionTitleOverrides(overrides)
            pendingTitleWrites.removeValue(forKey: sessionID)
            savePendingTitleWrites(pendingTitleWrites)
        }
        announceStateChange("session-markers")
        rescan()
    }

    // MARK: - Remove session (kill_session, pty_manager.rs:1701-1753)

    /// Session id whose sidebar row is showing the inline "Remove session?"
    /// confirm state (one at a time, like confirmingArchiveId in
    /// ProjectItem.svelte:158/1009-1024 — the native version swaps the whole
    /// row instead of just the button).
    @Published var confirmingRemoveSessionID: String?

    /// Which surface asked for the pending remove-confirm. The sidebar row
    /// and the archive-page card share `confirmingRemoveSessionID`, but only
    /// the requesting surface may render the inline confirm — the confirm UI
    /// mounts a click-away dismiss monitor scoped to its own row, so a
    /// mirrored confirm on the *other* surface cancels the whole thing on
    /// the very mouse-down aimed at this surface's Delete button (that made
    /// every archive-page delete a no-op until 2026-08-06).
    enum RemoveConfirmSurface { case sidebar, archivePage }
    @Published private(set) var confirmingRemoveSurface: RemoveConfirmSurface = .sidebar

    /// Sessions whose kill/cleanup is in flight; rows render disabled.
    @Published private(set) var removingSessionIDs: Set<String> = []

    /// Session dirs deleted by an explicit close/restart, keyed by session id.
    /// A host from an older build writes its final exited manifest up to a
    /// full heartbeat interval (60s) after its child dies — recreating the
    /// dir we deleted — so a closed session could reappear as a stopped row.
    /// scanSessions deletes a tombstoned dir on sight instead of listing it.
    /// (New hosts skip the final write when the dir is gone; this covers
    /// sessions still running an old `unpeel-host`.)
    private var purgedSessionDirs: [String: Date] = [:]
    private static let purgedSessionDirTTL: TimeInterval = 180

    /// Record that a session's dir was deliberately deleted. Call only after
    /// the kill + delete finished — a tombstone on a still-live session would
    /// let scanSessions delete its control socket out from under the kill.
    private func tombstoneSessionDir(_ sessionID: String) {
        let now = Date()
        purgedSessionDirs = purgedSessionDirs.filter {
            now.timeIntervalSince($0.value) < Self.purgedSessionDirTTL
        }
        purgedSessionDirs[sessionID] = now
    }

    func requestRemoveSession(
        _ sessionID: String, from surface: RemoveConfirmSurface = .sidebar
    ) {
        confirmingArchiveSessionID = nil
        confirmingRemoveSurface = surface
        confirmingRemoveSessionID = sessionID
    }

    func cancelRemoveConfirm() {
        confirmingRemoveSessionID = nil
    }

    /// Full removal: kill the host via its control socket ({"type":"kill"} →
    /// SIGTERM to the child's process group, session_host.rs:2043-2052), wait
    /// ≤2s for the host to stop, SIGKILL the host pid's group if it is still
    /// alive (spawn_hosted_session_cleanup parity, pty_manager.rs:1117-1132),
    /// then delete the session dir (cleanup_session_artifacts) and prune
    /// every native trace (pins / order overlays / selection / unread).
    /// Dead sessions skip the kill and just clean up.
    func confirmRemoveSession(_ sessionID: String) {
        if confirmingRemoveSessionID == sessionID {
            confirmingRemoveSessionID = nil
        }
        if remoteSummariesByID[sessionID] != nil
            || remoteArchivedByProject.values.contains(where: { sessions in
                sessions.contains(where: { $0.id == sessionID })
            }) {
            performRemoteVerb("Couldn't remove the session") { runtime in
                try await runtime.removeSession(sessionID)
            }
            return
        }
        guard !removingSessionIDs.contains(sessionID) else { return }
        removingSessionIDs.insert(sessionID)

        // Deselect first so the content pane swaps off the doomed surface
        // before the host dies under it.
        if selectedSessionID == sessionID {
            selectedSessionID = nil
        }
        // Drop the row from the sidebar right now — rebuildTree filters out
        // removingSessionIDs, so the kill/cleanup below runs silently with no
        // visible "removing" state.
        publishPendingSessions()

        let dirURL = LaunchConfig.appSessionsDir.appendingPathComponent(sessionID)
        let manifest = (try? Data(
            contentsOf: dirURL.appendingPathComponent("manifest.json")
        )).flatMap { try? JSONDecoder().decode(HostedSessionManifest.self, from: $0) }
        let pid = manifest?.pid
        // A recycled pid must not count as "live": killing through it would
        // SIGKILL whatever unrelated process group owns the pid now.
        let live = manifest?.state == "running"
            && pid.map { kill($0, 0) == 0 } ?? false
            && Self.manifestPidIdentity(manifest) != .notOurs
        let persistedManagedStoragePath = manifest?.managedStoragePath

        Task { [weak self] in
            // New Hosts publish this provider-neutral path in the manifest.
            // Ask core for old saved Sessions so their runtime adapter can
            // recover the same ownership without native parsing CLI flags.
            let reportedManagedStoragePath: String?
            if let persistedManagedStoragePath {
                reportedManagedStoragePath = persistedManagedStoragePath
            } else {
                reportedManagedStoragePath = await Task.detached(priority: .utility) {
                    ResumeCommand.hostManagedStoragePath(sessionID: sessionID)
                }.value
            }
            let managedStoragePath = Self.validatedManagedStoragePath(
                reportedManagedStoragePath,
                unpeelDir: LaunchConfig.unpeelDir
            )
            await Self.killAndCleanup(
                sessionID: sessionID, dirURL: dirURL, manifest: manifest, live: live
            )
            if let managedStoragePath {
                try? FileManager.default.removeItem(atPath: managedStoragePath)
            }
            await MainActor.run {
                guard let self else { return }
                self.removingSessionIDs.remove(sessionID)
                self.announceStateChange("lifecycle")
                self.tombstoneSessionDir(sessionID)
                self.pruneNativeState(forRemovedSession: sessionID)
                self.rescan()
            }
        }
    }

    // MARK: - Pid identity (anti-recycling guard)

    /// Whether a manifest's recorded pid still refers to the session's own
    /// child process, or the OS has recycled the pid since the child died.
    /// Under agent load the pid counter wraps in well under an hour, so a
    /// stale manifest's pid routinely points at an unrelated live process —
    /// signaling it kills an innocent process group (mirrors PidIdentity in
    /// session_host.rs).
    enum ManifestPidIdentity {
        /// Positively verified: the live process is the recorded child.
        case matches
        /// Positively refuted: the pid was recycled onto an unrelated
        /// process. Treat the session as already dead; never signal.
        case notOurs
        /// Cannot prove either way (legacy manifest without
        /// `pid_started_at` whose child has exec'd away the identifying
        /// argv). Safe default: never force-kill, never declare dead.
        case unknown
    }

    /// Tolerance when comparing the manifest's recorded pid start time
    /// against the kernel-reported one (PID_START_TOLERANCE_MS parity).
    private nonisolated static let pidStartToleranceMs: UInt64 = 10_000

    nonisolated static func processStartTimeMs(_ pid: Int32) -> UInt64? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.stride)
        guard proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size else { return nil }
        return info.pbi_start_tvsec * 1000 + info.pbi_start_tvusec / 1000
    }

    /// Definitive refutation only: true when the live process at the
    /// manifest's pid provably started at a different time than the recorded
    /// child. Cheap (one syscall), safe to poll.
    private nonisolated static func pidProvablyRecycled(
        _ manifest: HostedSessionManifest?
    ) -> Bool {
        guard let pid = manifest?.pid, pid > 1,
              let recorded = manifest?.pidStartedAt,
              let actual = processStartTimeMs(pid)
        else { return false }
        let drift = actual > recorded ? actual - recorded : recorded - actual
        return drift > pidStartToleranceMs
    }

    nonisolated static func manifestPidIdentity(
        _ manifest: HostedSessionManifest?
    ) -> ManifestPidIdentity {
        guard let manifest, let pid = manifest.pid, pid > 1 else { return .unknown }
        if let recorded = manifest.pidStartedAt, let actual = processStartTimeMs(pid) {
            let drift = actual > recorded ? actual - recorded : recorded - actual
            return drift <= pidStartToleranceMs ? .matches : .notOurs
        }
        // Legacy manifests (no recorded start): the hosted child is spawned
        // as `zsh -l -i -c "<script>"` whose script embeds the session id,
        // so a positive argv hit is definitive. A miss is NOT — after the
        // agent exits, the wrapper execs a plain shell whose argv no longer
        // mentions the session.
        if processCommandLine(pid)?.contains(manifest.session.id) == true {
            return .matches
        }
        return .unknown
    }

    /// Full argv via the KERN_PROCARGS2 sysctl — this ran inside the rescan
    /// loop, and the previous `/bin/ps` implementation was a synchronous
    /// process spawn per legacy-manifest session per rescan on the main
    /// actor. Only works for same-user processes; a failure returns nil,
    /// which callers already treat as "cannot prove" (.unknown).
    private nonisolated static func processCommandLine(_ pid: Int32) -> String? {
        var mib: [Int32] = [CTL_KERN, KERN_PROCARGS2, pid]
        var size = 0
        guard sysctl(&mib, 3, nil, &size, nil, 0) == 0,
              size > MemoryLayout<Int32>.size else { return nil }
        var buffer = [UInt8](repeating: 0, count: size)
        guard sysctl(&mib, 3, &buffer, &size, nil, 0) == 0,
              size > MemoryLayout<Int32>.size else { return nil }
        // Layout: argc (Int32), exec path (NUL-terminated), NUL padding,
        // then argc NUL-terminated argv strings, then the environment.
        let argc = buffer.withUnsafeBytes { $0.load(as: Int32.self) }
        guard argc > 0 else { return nil }
        var index = MemoryLayout<Int32>.size
        while index < size, buffer[index] != 0 { index += 1 }
        while index < size, buffer[index] == 0 { index += 1 }
        var args: [String] = []
        var current: [UInt8] = []
        while index < size, args.count < Int(argc) {
            if buffer[index] == 0 {
                args.append(String(decoding: current, as: UTF8.self))
                current.removeAll(keepingCapacity: true)
            } else {
                current.append(buffer[index])
            }
            index += 1
        }
        let command = args.joined(separator: " ")
        return command.isEmpty ? nil : command
    }

    /// Kill the session host's PTY: {"type":"kill"} over the control socket
    /// (→ SIGTERM to the child's process group), wait ≤2s, then SIGKILL the
    /// recorded pid's group if it is still alive (spawn_hosted_session_cleanup
    /// parity, pty_manager.rs:1117-1132). Leaves the session directory on
    /// disk — callers decide whether to also delete it.
    ///
    /// The escalation only ever signals a pid whose identity is positively
    /// verified against the manifest (`manifestPidIdentity == .matches`). A
    /// stale manifest's pid is routinely recycled onto an unrelated live
    /// process, and `kill(-pid, SIGKILL)` on such a pid took out innocent
    /// agent sessions' whole process groups.
    private nonisolated static func terminateHost(
        dirURL: URL, manifest: HostedSessionManifest?
    ) async {
        let pid = manifest?.pid
        func hostStopped() -> Bool {
            if let pid, pid > 1 {
                // EPERM still means "exists" (hosted_process_exists parity).
                if kill(pid, 0) != 0, errno != EPERM { return true }
                // The pid exists but provably belongs to an unrelated
                // process: nothing of ours is left to wait for.
                return pidProvablyRecycled(manifest)
            }
            let manifest = (try? Data(
                contentsOf: dirURL.appendingPathComponent("manifest.json")
            )).flatMap { try? JSONDecoder().decode(HostedSessionManifest.self, from: $0) }
            return manifest.map { $0.state != "running" } ?? true
        }

        let delivered = sendSocketCommand(
            socketPath: dirURL.appendingPathComponent("session.sock").path,
            payload: "{\"type\":\"kill\"}\n"
        )
        NSLog("[unpeel-killtrace] terminateHost dir=%@ pid=%d delivered=%d identity=%d",
              dirURL.lastPathComponent, pid ?? -1, delivered ? 1 : 0,
              { switch manifestPidIdentity(manifest) { case .matches: return 2; case .notOurs: return 0; case .unknown: return 1 } }())
        // No reachable host and no positive proof the recorded pid is still
        // the session's own child: the session is already dead, and there is
        // nothing that is safe to signal.
        if !delivered, manifestPidIdentity(manifest) != .matches { return }
        // Wait up to 2s for the host to exit (the wrapped shell can ignore
        // SIGTERM), then SIGKILL the child's process group — identity
        // re-verified immediately beforehand — and give it up to 2 more
        // seconds.
        var stopped = false
        for _ in 0..<20 {
            try? await Task.sleep(nanoseconds: 100_000_000)
            if hostStopped() { stopped = true; break }
        }
        if !stopped, let pid, pid > 1,
            manifestPidIdentity(manifest) == .matches {
            NSLog("[unpeel-killtrace] terminateHost SIGKILL group pid=%d", pid)
            kill(-pid, SIGKILL)
            for _ in 0..<20 {
                try? await Task.sleep(nanoseconds: 100_000_000)
                if hostStopped() { break }
            }
        }
    }

    /// Freeze a settled title across the host teardown. Hosts from builds
    /// before 2026-07-21 rebuilt their final exit manifest from the
    /// launch-time session record, reverting the auto-title (or a
    /// manifest-level custom title) to the preset label the moment the host
    /// drained. New hosts preserve the on-disk record — but a live session
    /// keeps whatever host binary spawned it across app updates, so every
    /// user has a window of old-host sessions after updating. Snapshotting
    /// the title into the rename overlay (which always wins over the
    /// manifest label, and already carries across Restart) makes the title
    /// stick no matter which host generation writes last.
    private func preserveSettledTitleBeforeStop(_ session: SessionEntry) {
        var titles = loadSessionTitleOverrides()
        guard titles[session.id] == nil else { return }
        let label = session.label.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !label.isEmpty else { return }
        if session.command.isEmpty {
            // Blank terminal still in its untitled state.
            guard label != "Terminal" else { return }
        } else if session.command.hasPrefix(label) {
            // The label is still the display command — untitled, nothing
            // worth freezing.
            return
        }
        titles[session.id] = label
        saveSessionTitleOverrides(titles)
    }

    /// User-visible stop: kill the hosted PTY but keep the session directory
    /// and output history, so it remains in the sidebar as restartable.
    func stopSession(_ sessionID: String) -> Bool {
        guard let session = sessionsByID[sessionID], session.isLive else { return false }
        preserveSettledTitleBeforeStop(session)
        let dirURL = LaunchConfig.appSessionsDir.appendingPathComponent(sessionID)
        let manifest = (try? Data(
            contentsOf: dirURL.appendingPathComponent("manifest.json")
        )).flatMap { try? JSONDecoder().decode(HostedSessionManifest.self, from: $0) }
        let pid = manifest?.pid
        let live = manifest?.state == "running"
            && pid.map { kill($0, 0) == 0 } ?? false
            && Self.manifestPidIdentity(manifest) != .notOurs
        guard live else {
            rescan()
            return false
        }

        Task { [weak self] in
            await Self.terminateHost(dirURL: dirURL, manifest: manifest)
            // A stopped session must not keep burning resources: the browser
            // engine daemon (and its Chrome) deliberately outlives the CLI,
            // so reap it like Archive/Remove do. No-op for browserless
            // sessions; a later Restart re-spawns it lazily on first use.
            Self.cleanupBrowserDaemon(sessionID: sessionID)
            await MainActor.run { self?.rescan() }
        }
        return true
    }

    /// Off-main kill + artifact cleanup (the blocking part).
    private nonisolated static func killAndCleanup(
        sessionID: String, dirURL: URL, manifest: HostedSessionManifest?, live: Bool
    ) async {
        if live {
            await terminateHost(dirURL: dirURL, manifest: manifest)
        }

        // The Browser MCP engine daemon (and its Chrome) deliberately outlives
        // the provider CLI, so closing the session must reap it explicitly.
        // Fire-and-forget: a session that never used the browser exits fast.
        cleanupBrowserDaemon(sessionID: sessionID)

        try? FileManager.default.removeItem(at: dirURL)

        // The manifest pid is the child SHELL, so "stopped" can race the
        // HOST's final manifest write (state=exited), which recreates the
        // dir we just deleted. Sweep again briefly so the session cannot
        // reappear as a dead row.
        for _ in 0..<10 {
            try? await Task.sleep(nanoseconds: 300_000_000)
            if FileManager.default.fileExists(atPath: dirURL.path) {
                try? FileManager.default.removeItem(at: dirURL)
            }
        }
    }

    /// Copy the session's conversation transcript to the clipboard as Markdown.
    /// Rendered by `unpeel-host __transcript__ markdown <id>`, which reads the
    /// shared Settings ▸ Transcripts options from `app-state.json` — so the
    /// clipboard content and the Sessions MCP `read_transcript` output stay
    /// aligned. Runs off the main thread; the host resolves the provider
    /// transcript, so no path/args are needed here.
    /// `entries` overrides the Settings ▸ Transcripts range for this copy
    /// (the context menu's flyout picks): a count keeps that many most-recent
    /// entries, 0 means the whole conversation, nil uses the Settings default.
    func copyTranscriptMarkdown(_ sessionID: String, entries: Int? = nil) {
        if remoteSummary(for: sessionID) != nil {
            performRemoteVerb("Couldn't copy transcript") { runtime in
                let markdown = try await runtime.transcriptMarkdown(
                    sessionID: sessionID,
                    entries: entries
                )
                let trimmed = markdown.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !trimmed.isEmpty else {
                    throw RemoteHostVerbError(
                        operation: "copy transcript",
                        message: "This session has no readable conversation transcript yet.",
                        outcomeIsUnknown: false
                    )
                }
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(trimmed, forType: .string)
            }
            return
        }
        Task.detached {
            let outcome = Self.runTranscriptMarkdown(sessionID: sessionID, entries: entries)
            await MainActor.run {
                if let error = outcome.error {
                    Self.showErrorAlert(title: "Couldn't copy transcript", message: error)
                    return
                }
                let trimmed = outcome.markdown.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !trimmed.isEmpty else {
                    Self.showErrorAlert(
                        title: "No transcript to copy",
                        message: "This session has no readable conversation transcript yet."
                    )
                    return
                }
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(trimmed, forType: .string)
            }
        }
    }

    /// Also serves the phone's "Copy transcript" (`/mobile/transcript-markdown`
    /// in MobileRemoteServer), so keep it callable off the main actor.
    /// `entries` maps to the CLI's `--entries` override (0 = whole
    /// conversation, matching TranscriptSettings.maxEntries semantics).
    nonisolated static func runTranscriptMarkdown(
        sessionID: String,
        entries: Int? = nil
    ) -> (markdown: String, error: String?) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: LaunchConfig.hostBinary)
        var arguments = ["__transcript__", "markdown", sessionID]
        if let entries {
            arguments += ["--entries", String(max(0, entries))]
        }
        process.arguments = arguments
        process.standardInput = FileHandle.nullDevice
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        do {
            try process.run()
        } catch {
            return ("", "Failed to run unpeel-host: \(error.localizedDescription)")
        }
        let outData = stdout.fileHandleForReading.readDataToEndOfFile()
        let errData = stderr.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        if process.terminationStatus != 0 {
            let message = String(data: errData, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            if let message, !message.isEmpty {
                return ("", message)
            }
            return ("", "unpeel-host exited with code \(process.terminationStatus).")
        }
        return (String(data: outData, encoding: .utf8) ?? "", nil)
    }

    /// Close a session's Browser MCP engine daemon via
    /// `unpeel-host __browser_cleanup__ <id>` (browser_mcp.rs run_cleanup).
    /// Detached — the host does its own bounded close and file removal, and a
    /// session without a browser returns immediately.
    private nonisolated static func cleanupBrowserDaemon(sessionID: String) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: LaunchConfig.hostBinary)
        process.arguments = ["__browser_cleanup__", sessionID]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            NSLog("[UnpeelNative] failed to spawn browser cleanup: \(error)")
        }
        cleanupComputerSession(sessionID: sessionID)
    }

    /// End a session's cua-driver session (overlay cursor + scope state) via
    /// `unpeel-host __computer_cleanup__ <id>` (computer_mcp.rs run_cleanup).
    /// Piggybacks on every browser-cleanup site — both are "session is going
    /// away, reap its engine state" — and tolerates a stopped daemon.
    private nonisolated static func cleanupComputerSession(sessionID: String) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: LaunchConfig.hostBinary)
        process.arguments = ["__computer_cleanup__", sessionID]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            NSLog("[UnpeelNative] failed to spawn computer cleanup: \(error)")
        }
    }

    /// One-shot Unix-socket request to the session host's control socket
    /// (newline-framed JSON, send_command_for_response in session_host.rs).
    private nonisolated static func sendSocketCommand(
        socketPath: String, payload: String
    ) -> Bool {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { return false }
        defer { close(fd) }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let maxLen = MemoryLayout.size(ofValue: addr.sun_path) - 1
        let pathBytes = Array(socketPath.utf8)
        guard pathBytes.count <= maxLen else { return false }
        withUnsafeMutableBytes(of: &addr.sun_path) { dst in
            dst.copyBytes(from: pathBytes)
        }

        var tv = timeval(tv_sec: 1, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &tv, socklen_t(MemoryLayout<timeval>.size))
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, socklen_t(MemoryLayout<timeval>.size))

        let connected = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connected == 0 else { return false }

        let sent = payload.withCString { send(fd, $0, strlen($0), 0) }
        guard sent > 0 else { return false }
        // Best-effort ack read so the host finishes handling before we
        // start polling the manifest.
        var buffer = [UInt8](repeating: 0, count: 256)
        _ = recv(fd, &buffer, buffer.count, 0)
        return true
    }

    /// Drop every native-side trace of a removed session: pin overlay
    /// entries, the session-order overlay slot, and the confirm state.
    private func pruneNativeState(
        forRemovedSession sessionID: String,
        preserveSharedOrder: Bool = false,
        appendedContextAlreadyStaged: Bool = false
    ) {
        // Pin overlay (Tauri pins for missing sessions are already filtered
        // out by rebuildPins; the file itself is Tauri-owned).
        pendingSessions.removeValue(forKey: sessionID)

        // Resume-failure watcher + flag (in-memory; restart starts a fresh
        // watcher for the replacement id after this prune).
        resumeFailureWatchers[sessionID]?.cancel()
        resumeFailureWatchers.removeValue(forKey: sessionID)
        resumeFailureWatcherTokens.removeValue(forKey: sessionID)
        resumeFailures.remove(sessionID)
        deferredStopEffects.removeValue(forKey: sessionID)?.task.cancel()

        // Session access override (keyed by session id) — drop it so a dead
        // session never lingers in app-state.json. Restart carries it to the
        // new id before this runs, so the live conversation keeps its grant.
        if mcpOrchestrators[sessionID] != nil {
            mcpOrchestrators[sessionID] = nil
            persistMcpGrants()
        }
        // Remembered write approvals involving this session (either side).
        // Restart repoints them to the new id before this prune runs.
        pruneMcpWriteApprovals(forRemovedSession: sessionID)
        pruneComputerApprovals(forRemovedSession: sessionID)
        pruneBrowserApprovals(forRemovedSession: sessionID)
        if notifyWhenDoneSessionIDs.contains(sessionID) {
            setNotifyWhenDone(sessionID, enabled: false)
        }
        // Archived flag: gone on true removal. Restart deliberately does NOT
        // carry it — restarting an archived session is "bring it back".
        if archivedSessionIDs.contains(sessionID) {
            archivedSessionIDs.remove(sessionID)
            persistArchivedSessionIDs()
        }
        let pinKey = PinnedSidebarSession.key(forSessionID: sessionID)
        var overrides = loadPinOverrides()
        let hadPin = overrides.added.contains { $0.key == pinKey }
            || overrides.removedKeys.contains(pinKey)
        if hadPin {
            overrides.added.removeAll { $0.key == pinKey }
            overrides.removedKeys.removeAll { $0 == pinKey }
            savePinOverrides(overrides)
        }

        // Session-order + pinned-order overlays (the session knows its project
        // via sessionsByID, but be thorough in case the index already dropped).
        let defaults = AppDefaults.shared
        for (key, value) in defaults.dictionaryRepresentation()
        where key.hasPrefix("unpeel.native.sessionOrder.")
            || key.hasPrefix("unpeel.native.pinnedOrder.") {
            guard var ids = value as? [String], ids.contains(sessionID) else { continue }
            ids.removeAll { $0 == sessionID }
            if ids.isEmpty {
                defaults.removeObject(forKey: key)
            } else {
                defaults.set(ids, forKey: key)
            }
        }
        if !preserveSharedOrder,
           Self.removeSessionFromSharedOrders(sessionID) {
            announceStateChange("order")
        }

        // Rename overlay entry (also GC'd by scanSessions when the session
        // dir disappears; this is the eager path for native removals).
        var titles = loadSessionTitleOverrides()
        if titles.removeValue(forKey: sessionID) != nil {
            saveSessionTitleOverrides(titles)
        }
        var pendingTitleWrites = loadPendingTitleWrites()
        if pendingTitleWrites.removeValue(forKey: sessionID) != nil {
            savePendingTitleWrites(pendingTitleWrites)
        }

        // Provider conversation-id overlay (resume-on-restart). Note: restart
        // reads this BEFORE pruning the old id, then the replacement session
        // re-captures its own id from the first hook event.
        var providerIDs = loadProviderSessionIDs()
        if providerIDs.removeValue(forKey: sessionID) != nil {
            saveProviderSessionIDs(providerIDs)
        }

        // Pending appended system context (restart-only launch flag).
        var pendingAppendContexts = loadPendingAppendSystemContexts()
        if pendingAppendContexts.removeValue(forKey: sessionID) != nil {
            savePendingAppendSystemContexts(pendingAppendContexts)
        }
        // Replacement Resume holds this marker's cross-process lock across
        // teardown/spawn and moved the exact consumed bytes aside before this
        // cleanup. Re-locking here would deadlock on our own file description.
        if !appendedContextAlreadyStaged {
            Self.removeSharedMarker(sessionID, .appendedContext)
        }

        restartRecommendations.removeValue(forKey: sessionID)
        clearPhoneResizeOverride(for: sessionID)
        var restartDismissals = loadRestartRecommendationDismissals()
        if restartDismissals.removeValue(forKey: sessionID) != nil {
            saveRestartRecommendationDismissals(restartDismissals)
        }

        if confirmingRemoveSessionID == sessionID {
            confirmingRemoveSessionID = nil
        }
        if editingSessionID == sessionID {
            editingSessionID = nil
        }
    }

    // MARK: - Restart session (restartSession, stores/sessions.ts:554-590)

    /// Sessions whose restart is in flight (App.svelte restartingSessions):
    /// their rows/buttons render disabled until the replacement appears.
    @Published private(set) var restartingSessionIDs: Set<String> = [] {
        // A restarting (resuming) block counts as ACTIVE in the sidebar
        // partition, so the row jumps to its active-group spot the moment
        // Resume is clicked — the cache must see the flag flip.
        didSet { if restartingSessionIDs != oldValue { invalidateSidebarLists() } }
    }

    /// In-place managed-agent resumes. Unlike `restartingSessionIDs`, these
    /// never unmount the terminal: the Session id, PTY, attach surface, and
    /// scrollback all remain live while the ended provider is resumed.
    @Published private(set) var resumingAgentSessionIDs: Set<String> = [] {
        didSet { if resumingAgentSessionIDs != oldValue { invalidateSidebarLists() } }
    }

    /// Snapshot of each restarting session, kept so rebuildTree can hold its
    /// row in place across the window where the old host's manifest is gone
    /// and the replacement hasn't been spawned yet. Without it the row blinks
    /// out of the sidebar for the duration of kill+cleanup.
    private var restartPlaceholders: [String: SessionEntry] = [:]

    /// Resume only the stable managed agent inside a live terminal. The Host
    /// re-derives the command and verifies the foreground owner; Swift never
    /// promotes a passively observed runtime or supplies relaunch argv.
    @discardableResult
    func resumeAgent(_ sessionID: String) -> Bool {
        guard sessionCanResumeAgent(sessionID),
              !resumingAgentSessionIDs.contains(sessionID),
              !restartingSessionIDs.contains(sessionID),
              !removingSessionIDs.contains(sessionID)
        else { return false }

        resumingAgentSessionIDs.insert(sessionID)

        if remoteSummariesByID[sessionID] != nil {
            performRemoteVerb(
                "Couldn't resume the agent",
                onFailure: { [weak self] in
                    self?.resumingAgentSessionIDs.remove(sessionID)
                }
            ) { [weak self] runtime in
                try await runtime.resumeAgent(sessionID)
                self?.resumingAgentSessionIDs.remove(sessionID)
            }
            return true
        }

        Task { [weak self] in
            let failure = await Task.detached(priority: .userInitiated) {
                Self.runResumeAgentHostCommand(sessionID: sessionID)
            }.value
            guard let self else { return }
            if let failure {
                self.resumingAgentSessionIDs.remove(sessionID)
                Self.showErrorAlert(title: "Couldn't resume the agent", message: failure.message)
            } else {
                // Success stays marked until rescan observes the committed
                // runtime generation. That closes the old-Stop hook race.
                self.consumePendingAppendedContext(afterResumingAgent: sessionID)
            }
            self.rescan()
        }
        return true
    }

    /// The visible primary action: live managed launches restart only their
    /// agent; stopped Sessions retain the legacy replacement-based Resume.
    @discardableResult
    func resumeAgentOrSession(_ sessionID: String) -> Bool {
        guard let session = displaySessionsByID[sessionID] else { return false }
        return session.isLive ? resumeAgent(sessionID) : restartSession(sessionID)
    }

    nonisolated static func runResumeAgentHostCommand(
        sessionID: String
    ) -> ResumeAgentHostCommandFailure? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: LaunchConfig.hostBinary)
        process.arguments = ["__resume_agent__", sessionID]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        let stderr = Pipe()
        process.standardError = stderr
        do {
            try process.run()
        } catch {
            return ResumeAgentHostCommandFailure(
                status: 500,
                message: error.localizedDescription
            )
        }
        process.waitUntilExit()
        guard process.terminationStatus != 0 else { return nil }
        let data = (try? stderr.fileHandleForReading.readToEnd()) ?? Data()
        let rawMessage = String(decoding: data, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let message = normalizedResumeAgentFailureMessage(rawMessage)
        return ResumeAgentHostCommandFailure(
            status: resumeAgentFailureHTTPStatus(message),
            message: message.isEmpty
                ? "The terminal Host rejected the agent resume."
                : message
        )
    }

    /// Await the synchronous Host CLI receipt without occupying MainActor.
    /// The injectable runner keeps the executor boundary deterministic under
    /// test while production always invokes the real `unpeel-host` command.
    nonisolated static func runResumeAgentHostCommandOffMainActor(
        sessionID: String,
        runner: @escaping @Sendable (String) -> ResumeAgentHostCommandFailure? = {
            runResumeAgentHostCommand(sessionID: $0)
        }
    ) async -> ResumeAgentHostCommandFailure? {
        await Task.detached(priority: .userInitiated) {
            runner(sessionID)
        }.value
    }

    /// Keep native compatibility routing aligned with Rust
    /// `classify_restart_agent_failure`: only stable eligibility/concurrency
    /// races are 409. Transport, signal, support-install, PTY-write, timeout,
    /// and post-submit manifest failures are infrastructure/ambiguous 500s.
    nonisolated static func resumeAgentFailureHTTPStatus(_ message: String) -> Int {
        let message = normalizedResumeAgentFailureMessage(message)
        if message.hasPrefix("session "), message.hasSuffix(" is not running") {
            return 409
        }
        let exactConflicts = [
            "Agent restart requires a nonblank, known resumable launch command",
            "An agent restart is already in progress",
            "Session host no longer has an owned shell process",
            "Session host process identity could not be verified",
            "Session host has no verifiable process start time",
            "Session terminal has no foreground process group",
            "Terminal foreground is outside the owned session",
            "Agent foreground changed before restart escalation",
            "Agent stopped, but the owned shell did not regain the terminal",
            "Owned shell changed while restarting the agent",
            "Owned shell lost the terminal before agent relaunch",
        ]
        if message.hasPrefix("Agent restart generation changed")
            || message.hasPrefix("Refusing to restart ")
            || message.hasPrefix("Refusing to resume ")
            || (message.hasPrefix("session ")
                && message.hasSuffix(" host does not support shell-only agent resume"))
            || exactConflicts.contains(message) {
            return 409
        }
        return 500
    }

    /// `unpeel-host` prefixes command errors for CLI readability. Strip only
    /// that stable wrapper before classifying or returning the receipt body.
    nonisolated static func normalizedResumeAgentFailureMessage(_ message: String) -> String {
        let trimmed = message.trimmingCharacters(in: .whitespacesAndNewlines)
        for prefix in ["agent resume failed: ", "agent restart failed: "]
        where trimmed.hasPrefix(prefix) {
            return String(trimmed.dropFirst(prefix.count))
                .trimmingCharacters(in: .whitespacesAndNewlines)
        }
        return trimmed
    }

    nonisolated static func hookReceiptPredatesRuntimeLaunch(
        receivedAt: Date,
        launchedAt: Date?
    ) -> Bool {
        launchedAt.map { receivedAt < $0 } ?? false
    }

    /// Bind an owned hook to the Host's managed-runtime generation before it
    /// can mutate any native state. Exact generation provenance wins. Legacy
    /// hooks remain compatible for the initial launch. Immediately after an
    /// in-place edge a Stop cannot prove which process emitted it: a
    /// current-turn Start/UserPromptSubmit received after the launch boundary
    /// establishes ownership. The 30-second fallback bounds degradation for a
    /// permanently old hook install that emits no recognizable opener.
    nonisolated static func hookRuntimeDecision(
        eventGeneration: UInt64?,
        hookEventName: String,
        receivedAt: Date,
        currentGeneration: UInt64?,
        runtimeLaunchedAt: Date?,
        currentGenerationOwned: Bool
    ) -> HookRuntimeDecision {
        if let eventGeneration {
            if let currentGeneration, eventGeneration < currentGeneration {
                return .reject
            }
            // A hook can beat the manifest rescan (and, briefly, the manifest
            // commit visible to this process). Retain its exact future
            // generation; resetForRuntimeLaunch will bind it when the edge is
            // observed.
            return .accept(effectiveGeneration: eventGeneration)
        }

        guard let currentGeneration, currentGeneration > 1 else {
            return .accept(effectiveGeneration: nil)
        }
        if hookReceiptPredatesRuntimeLaunch(
            receivedAt: receivedAt,
            launchedAt: runtimeLaunchedAt
        ) {
            return .reject
        }

        switch hookEventName {
        case "Start", "UserPromptSubmit":
            return .accept(effectiveGeneration: currentGeneration)
        case "Stop", "StopFailure":
            if currentGenerationOwned {
                return .accept(effectiveGeneration: currentGeneration)
            }
            if let runtimeLaunchedAt,
               receivedAt.timeIntervalSince(runtimeLaunchedAt)
                   < Self.legacyGenerationStopGuard {
                return .reject
            }
            return .accept(effectiveGeneration: currentGeneration)
        default:
            // Non-completion events cannot create legacy Stop ownership. They
            // may still drive their own current activity transition after the
            // launch boundary (for example PermissionRequest).
            return .accept(effectiveGeneration: currentGeneration)
        }
    }

    nonisolated static func reconciledPendingAppendedContexts(
        _ pending: [String: String],
        sessionID: String,
        currentSharedContext: String?
    ) -> [String: String] {
        var reconciled = pending
        if let currentSharedContext {
            reconciled[sessionID] = currentSharedContext
        } else {
            reconciled.removeValue(forKey: sessionID)
        }
        return reconciled
    }

    private func consumePendingAppendedContext(afterResumingAgent sessionID: String) {
        // Core atomically stages and consumes only the marker snapshot it
        // actually launched with. Never delete the marker here: another
        // frontend may already have published the next generation's context.
        // Mirror the marker's CURRENT value into the native compatibility
        // overlay. Core may have consumed the old marker while another
        // frontend atomically published the next context at the same path.
        let pending = loadPendingAppendSystemContexts()
        let currentContext = Self.readSharedMarker(
            sessionID, .appendedContext
        )?["context"] as? String
        savePendingAppendSystemContexts(Self.reconciledPendingAppendedContexts(
            pending,
            sessionID: sessionID,
            currentSharedContext: currentContext
        ))
        restartRecommendations.removeValue(forKey: sessionID)
        var dismissals = loadRestartRecommendationDismissals()
        if dismissals.removeValue(forKey: sessionID) != nil {
            saveRestartRecommendationDismissals(dismissals)
        }
    }

    /// Svelte restart semantics (restartSession + handleRestartSession,
    /// App.svelte:1213-1261): the old session is fully removed
    /// (kill_session for live hosts / close_saved_session for dead ones —
    /// both drop the entry from UI and persisted state), then a FRESH
    /// session (new id) is spawned with the session's original command and
    /// label. custom_title carries over; worktree sessions restart inside
    /// their worktree, not the project root; a pinned session is re-pinned;
    /// and created_at is stabilized to the old value so the row keeps its
    /// sidebar position. The provider conversation IS resumed where the CLI
    /// supports it, but only after the hosted PTY has received input. A
    /// never-written session has no provider conversation to resume, so it is
    /// relaunched with the original command. `forceFresh` strips every resume
    /// marker regardless — the recovery path when the provider conversation
    /// is gone from disk (see `resumeFailures`).
    @discardableResult
    func restartSession(
        _ sessionID: String,
        forceFresh: Bool = false,
        stoppedOnly: Bool = true
    ) -> Bool {
        if remoteSummariesByID[sessionID] != nil {
            performRemoteVerb("Couldn't restart the session") { runtime in
                try await runtime.restartSession(sessionID)
            }
            return true
        }
        guard let session = sessionsByID[sessionID] else { return false }
        let groupProjectID = effectiveProjectID(for: session)
        guard let project = projectsByID[groupProjectID] else { return false }
        guard session.status != .starting else { return false }
        guard !restartingSessionIDs.contains(sessionID),
              !removingSessionIDs.contains(sessionID)
        else { return false }

        // Serialize the state check and the complete replacement against the
        // TUI, CLI, MCP, and another app process. Nonblocking flock keeps the
        // MainActor responsive; contention is an ordinary retryable refusal.
        guard let lifecycleLease = Self.acquireSessionLifecycleLease(
            unpeelDir: LaunchConfig.unpeelDir,
            sessionID: sessionID
        ) else { return false }

        let wasPinned = isPinned(sessionID: sessionID, projectID: groupProjectID)
        // The native rename overlay is the stand-in for custom_title; carry
        // the title to the new session id before the old entry is pruned.
        let titleOverride = loadSessionTitleOverrides()[sessionID]

        let dirURL = LaunchConfig.appSessionsDir.appendingPathComponent(sessionID)
        let manifest = (try? Data(
            contentsOf: dirURL.appendingPathComponent("manifest.json")
        )).flatMap { try? JSONDecoder().decode(HostedSessionManifest.self, from: $0) }
        let pid = manifest?.pid
        let childProcessExists = Self.hostedChildProcessExists(pid)
        let pidIdentity = Self.manifestPidIdentity(manifest)

        // Resume is stopped-only. Reload Terminal and the explicit
        // resume-failure recovery pass `stoppedOnly: false`; their live
        // replacement is intentional maintenance, not a stale Resume race.
        // A crashed Host may leave state=running, so pair state with a
        // definitive child-existence/identity decision under this lease.
        if !Self.replacementRestartAllowsState(
            manifest?.state,
            stoppedOnly: stoppedOnly,
            childProcessExists: childProcessExists,
            pidIdentity: pidIdentity
        ) {
            lifecycleLease.release()
            return false
        }

        let legacyPendingContext = loadPendingAppendSystemContexts()[sessionID]
        guard let contextSnapshot = Self.replacementContextSnapshot(
            sessionID: sessionID,
            fallbackContext: legacyPendingContext
        ) else {
            lifecycleLease.release()
            return false
        }
        restartingSessionIDs.insert(sessionID)
        // Hold the row in the sidebar across the teardown → respawn gap.
        restartPlaceholders[sessionID] = session
        // The Sessions MCP grant is keyed by session id and restart mints a new
        // id, so carry it over (like the title/pin). Read before
        // pruneNativeState drops the old entry; relaunching with it set injects
        // the client. Browser access is app-wide, so nothing to carry.
        let accessGrant = mcpOrchestrators[sessionID]
        // The "notify when done" opt-in is likewise keyed by id; carry it over.
        let carryNotifyWhenDone = notifyWhenDoneSessionIDs.contains(sessionID)
        // Manual sidebar order is keyed by session id: snapshot the old row's
        // rank so the replacement id can take the exact same slot. Without
        // this the resumed row first moves to its (ranked) active spot, then
        // jumps again when the unranked new id sorts above the hand-ordered
        // block — the "bouncing" resume.
        let manualOrderKey = Self.sessionOrderKey(groupProjectID)
        let manualOrderSnapshot = AppDefaults.shared.stringArray(forKey: manualOrderKey)
        let sharedManualOrderSnapshot = Self.sharedSessionOrder(projectID: groupProjectID)
        // Remembered write approvals are keyed by id on both sides; snapshot
        // before pruneNativeState drops the old id's pairs.
        let writeApprovalSnapshot = mcpWriteApprovals
        // Same for the computer-use and browser approvals (plain session ids).
        let hadComputerApproval = computerApprovals.contains(sessionID)
        let hadBrowserApproval = browserApprovals.contains(sessionID)

        // Keep the session SELECTED across the restart so the content area
        // doesn't flash to the empty state and back. The live surface is torn
        // down a different way: TerminalArea treats a restarting session as
        // non-mountable (shouldMountTerminal), so it unmounts the Ghostty
        // surface off the dying host and shows the DeadSessionView (with its
        // restart spinner) in place. spawnSession then moves selection to the
        // replacement id in the same synchronous turn — no empty frame.

        // A recycled pid must not count as "live" (see confirmRemoveSession).
        let live = manifest?.state == "running"
            && childProcessExists == true
            && pidIdentity != .notOurs

        Task { [weak self, lifecycleLease] in
            // Derivation invokes the Host CLI and waits for its exact result;
            // keep that process off MainActor while the lifecycle lease keeps
            // the source Session stable.
            let relaunchPlan = await Task.detached(priority: .userInitiated) {
                ResumeCommand.hostRelaunchPlan(
                    sessionID: sessionID,
                    forceFresh: forceFresh
                )
            }.value
            guard let relaunchPlan else {
                lifecycleLease.release()
                guard let self else { return }
                self.restartingSessionIDs.remove(sessionID)
                self.restartPlaceholders.removeValue(forKey: sessionID)
                Self.showErrorAlert(
                    title: "Couldn't resume the session",
                    message: "The bundled terminal Host could not derive a safe relaunch plan."
                )
                self.rescan()
                return
            }
            let relaunchCommand = relaunchPlan.command

            // A context writer may have published while the command was being
            // derived. Compare exact bytes, stage the consumed marker, and
            // retain its lock through both teardown and replacement spawn.
            guard let contextLease = Self.stageReplacementContext(
                sessionID: sessionID,
                snapshot: contextSnapshot
            ) else {
                lifecycleLease.release()
                guard let self else { return }
                self.restartingSessionIDs.remove(sessionID)
                self.restartPlaceholders.removeValue(forKey: sessionID)
                Self.showErrorAlert(
                    title: "Couldn't resume the session",
                    message: "Pending appended context changed while Resume was being prepared. Try again."
                )
                self.rescan()
                return
            }
            defer {
                contextLease.release()
                lifecycleLease.release()
            }
            // Svelte awaits killSession before spawning the replacement.
            await Self.killAndCleanup(
                sessionID: sessionID, dirURL: dirURL, manifest: manifest, live: live
            )
            await MainActor.run {
                guard let self else { return }
                self.restartingSessionIDs.remove(sessionID)
                self.tombstoneSessionDir(sessionID)
                // Keep the shared slot until the replacement id exists; the
                // write below swaps ids atomically, so peers never observe an
                // intentional reorder being discarded during restart.
                self.pruneNativeState(
                    forRemovedSession: sessionID,
                    preserveSharedOrder: true,
                    appendedContextAlreadyStaged: true
                )
                // Drop the placeholder in the same synchronous turn as the
                // respawn: spawnSession publishes the new .starting row, so
                // the old row is replaced in one rebuild with no empty frame.
                self.restartPlaceholders.removeValue(forKey: sessionID)

                // Only steal focus to the replacement if the user is still
                // looking at the session being restarted. If they switched
                // away while it churned, leave their selection alone.
                let wasViewingRestart = self.selectedSessionID == sessionID

                let newID = self.spawnSession(
                    projectID: groupProjectID,
                    command: relaunchCommand,
                    label: session.label,
                    customTitle: session.customTitle,
                    // Same logical thread: keep the old created_at so the
                    // row doesn't jump to the top of the project list
                    // (sessions.ts:580-588 stabilization).
                    createdAt: session.createdAt,
                    cwd: session.worktreePath ?? project.path,
                    worktreePath: session.worktreePath,
                    worktreeBranch: session.worktreeBranch,
                    spawnedBy: session.spawnedBy,
                    role: session.role,
                    task: session.task,
                    accessGrant: accessGrant,
                    activateUI: wasViewingRestart
                )
                if let newID {
                    // Give the replacement id the old row's hand-ordered
                    // rank (pruneNativeState just dropped the old id, so
                    // write the corrected snapshot back).
                    if let order = Self.replacingSessionID(
                        in: manualOrderSnapshot,
                        oldID: sessionID,
                        newID: newID
                    ) {
                        AppDefaults.shared.set(order, forKey: manualOrderKey)
                        self.invalidateSidebarLists()
                    }
                    if let order = Self.replacingSessionID(
                        in: sharedManualOrderSnapshot,
                        oldID: sessionID,
                        newID: newID
                    ) {
                        if Self.writeSharedSessionOrder(
                            projectID: groupProjectID,
                            ids: order
                        ) {
                            self.announceStateChange("order")
                        }
                    }
                    if let titleOverride {
                        var titles = self.loadSessionTitleOverrides()
                        titles[newID] = titleOverride
                        self.saveSessionTitleOverrides(titles)
                    }
                    if wasPinned {
                        self.pinSession(projectID: groupProjectID, sessionID: newID)
                    }
                    // Re-register the carried-over grant under the new id (the
                    // session already launched with the MCP client, so no
                    // restart — write the map directly).
                    if let accessGrant {
                        self.mcpOrchestrators[newID] = accessGrant
                        self.persistMcpGrants()
                    }
                    // Same for remembered write approvals: the pair survives a
                    // restart on either side.
                    self.carryMcpWriteApprovals(
                        snapshot: writeApprovalSnapshot, from: sessionID, to: newID
                    )
                    self.carryComputerApproval(approved: hadComputerApproval, to: newID)
                    self.carryBrowserApproval(approved: hadBrowserApproval, to: newID)
                    if carryNotifyWhenDone {
                        self.setNotifyWhenDone(newID, enabled: true)
                    }
                    // If the relaunch resumes a conversation whose provider
                    // storage was deleted out from under us (Claude Code
                    // auto-cleanup — see resumeFailures), the CLI dies
                    // instantly to a bare shell. Watch the replacement's
                    // first output and offer a fresh start when that happens.
                    self.watchForResumeFailure(
                        sessionID: newID,
                        markers: relaunchPlan.failureMarkers
                    )
                } else if Self.removeSessionFromSharedOrders(sessionID) {
                    // A failed restart has no replacement id to inherit the
                    // slot; clean up the old rank exactly like true removal.
                    self.announceStateChange("order")
                }
                self.rescan()
            }
        }
        return true
    }

    // MARK: - Resume failure detection (ResumeFailedBar)

    /// Watch a freshly-launched runtime's earliest output for the runtime
    /// adapter's Host-published "conversation not found" markers.
    /// Replacement Sessions begin at byte zero; in-place Resume Agent uses
    /// the Host-committed generation boundary so old scrollback cannot match.
    private func watchForResumeFailure(
        sessionID: String,
        markers: [String],
        startOffset: UInt64 = 0
    ) {
        resumeFailureWatchers[sessionID]?.cancel()
        resumeFailureWatchers.removeValue(forKey: sessionID)
        resumeFailureWatcherTokens.removeValue(forKey: sessionID)
        resumeFailures.remove(sessionID)
        guard !markers.isEmpty else { return }
        let outputURL = LaunchConfig.appSessionsDir
            .appendingPathComponent(sessionID)
            .appendingPathComponent("output.bin")
        let token = UUID()
        resumeFailureWatcherTokens[sessionID] = token
        resumeFailureWatchers[sessionID] = Task { [weak self] in
            defer {
                Task { @MainActor [weak self] in
                    guard self?.resumeFailureWatcherTokens[sessionID] == token else {
                        return
                    }
                    self?.resumeFailureWatchers.removeValue(forKey: sessionID)
                    self?.resumeFailureWatcherTokens.removeValue(forKey: sessionID)
                }
            }
            // The error lands within a second or two of the CLI starting;
            // poll briefly and give up quietly (a successful resume, a slow
            // machine past the window, or a removed session all just end the
            // watch with no flag).
            for _ in 0..<15 {
                try? await Task.sleep(nanoseconds: 2_000_000_000)
                if Task.isCancelled { return }
                guard let launchOutput = Self.readFileWindow(
                    outputURL, fromOffset: startOffset, maxBytes: 8192
                ) else {
                    continue
                }
                let text = String(decoding: launchOutput, as: UTF8.self)
                guard markers.allSatisfy(text.contains) else { continue }
                await MainActor.run { [weak self] in
                    guard let self,
                          self.resumeFailureWatcherTokens[sessionID] == token,
                          self.sessionsByID[sessionID] != nil
                    else { return }
                    self.resumeFailures.insert(sessionID)
                }
                return
            }
        }
    }

    func dismissResumeFailure(for sessionID: String) {
        resumeFailures.remove(sessionID)
    }

    /// ResumeFailedBar's action: relaunch without any resume marker. The dead
    /// conversation is unrecoverable (its provider storage is gone), so a
    /// fresh start — with a newly minted conversation id where the provider
    /// supports one — is the only forward path. The flag is not cleared here:
    /// a successful restart prunes it with the old session id
    /// (pruneNativeState), and a refused restart should keep the bar up for
    /// another try.
    func startFreshAfterResumeFailure(_ sessionID: String) {
        restartSession(sessionID, forceFresh: true, stoppedOnly: false)
    }

    /// Up to `maxBytes` at a stable output-generation boundary, nil when the
    /// file is unreadable. Seeking past the current end yields empty data and
    /// lets the watcher retry as output arrives.
    nonisolated static func readFileWindow(
        _ url: URL,
        fromOffset offset: UInt64,
        maxBytes: Int
    ) -> Data? {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? handle.close() }
        let logicalEnd = (try? handle.seekToEnd()) ?? 0
        let retentionURL = url.deletingLastPathComponent()
            .appendingPathComponent("output-retention.json")
        let retainedFrom: UInt64 = {
            guard let data = try? Data(contentsOf: retentionURL),
                  let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  (object["version"] as? NSNumber)?.uint32Value == 1,
                  let retained = (object["retained_from"] as? NSNumber)?.uint64Value
            else { return 0 }
            return min(retained, logicalEnd)
        }()
        let readableOffset = max(offset, retainedFrom)
        do {
            try handle.seek(toOffset: readableOffset)
        } catch {
            return nil
        }
        return try? handle.read(upToCount: maxBytes)
    }

    /// Defense in depth for destructive cleanup. The Host already validates
    /// the runtime-owned path before publishing it; native independently
    /// resolves symlinks and accepts only a strict descendant of this
    /// instance's UNPEEL_HOME.
    nonisolated static func validatedManagedStoragePath(
        _ path: String?,
        unpeelDir: URL
    ) -> String? {
        guard let path, !path.isEmpty else { return nil }
        let root = unpeelDir.standardizedFileURL.resolvingSymlinksInPath()
        let candidate = URL(fileURLWithPath: path)
            .standardizedFileURL
            .resolvingSymlinksInPath()
        let rootPrefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
        guard candidate.path.hasPrefix(rootPrefix) else { return nil }
        return candidate.path
    }

    // Per-verb gates for the session context menu (and, via
    // RemoteDTOAdapters, the phone's session sheet). All derive from
    // ProviderCapabilities — the one place that knows what each CLI supports.

    /// Resume is offered only for a stopped Session where relaunching
    /// continues the conversation (or a blank shell, which has none to lose).
    /// Live managed launches that returned to their shell use the separate
    /// Resume Agent gate below.
    func sessionCanRestart(_ sessionID: String) -> Bool {
        if let summary = remoteSummary(for: sessionID) {
            guard summary.status == .exited else { return false }
            return (summary.capabilities?.restart ?? true)
                && remoteHostRuntime.supportsHostOperation(
                    RemoteHostRuntime.HostOperation.restart
                )
        }
        guard let session = sessionsByID[sessionID] else { return false }
        guard session.status == .exited else { return false }
        return ProviderCapabilities.canRestart(command: session.command)
    }

    /// Archive remains available for any resumable launch regardless of live
    /// state. It is separate from stopped-only Resume and live Resume Agent.
    func sessionCanArchive(_ sessionID: String) -> Bool {
        if let summary = remoteSummariesByID[sessionID] {
            return summary.capabilities?.archive == true
                && remoteHostRuntime.supportsHostOperation(
                    RemoteHostRuntime.HostOperation.archive
                )
        }
        guard let session = sessionsByID[sessionID] else { return false }
        return ProviderCapabilities.canRestart(command: session.command)
    }

    /// Whether the live Session can resume its stable managed launch inside
    /// the same terminal. Remote scope requires both the Host-level operation
    /// and the Host-computed per-Session capability; absence from an older
    /// bootstrap is unsupported, never a cue to fall back to destructive
    /// legacy Session restart.
    func sessionCanResumeAgent(_ sessionID: String) -> Bool {
        if let summary = remoteSummariesByID[sessionID] {
            guard summary.status == .running,
                  summary.activity != .starting,
                  summary.activeRuntimeID == nil,
                  !summary.runtimeLaunchPending,
                  summary.capabilities?.resumeAgent == true
            else { return false }
            return remoteHostRuntime.supportsHostOperation(
                RemoteHostRuntime.HostOperation.resumeAgent
            )
        }
        guard let session = sessionsByID[sessionID], session.status != .starting else {
            return false
        }
        return ProviderCapabilities.canResumeAgent(
            command: session.command,
            isLive: session.isLive,
            activeRuntimeID: session.activeRuntimeID,
            runtimeLaunchPending: session.runtimeLaunchPending,
            hostProtocolVersion: session.hostProtocolVersion
        )
    }

    /// Whether this session can be **forked** into an independent conversation
    /// branch. The runtime catalog declares whether a provider exposes a
    /// native fork primitive, and forking a session that never received
    /// input has nothing to branch — but that written-to check reads the
    /// manifest off disk, so the cheap sidebar gate stays provider-only and
    /// `forkSession` does the final written-to guard.
    func sessionCanFork(_ sessionID: String) -> Bool {
        // The Host contract has no remote fork operation yet; the menu item
        // hides rather than offering a verb that cannot be carried.
        guard remoteSummariesByID[sessionID] == nil else { return false }
        guard let session = sessionsByID[sessionID] else { return false }
        guard session.status != .starting else { return false }
        return ProviderCapabilities.canFork(command: session.command)
    }

    func sessionCanAppendSystemContext(_ sessionID: String) -> Bool {
        // No remote append-system-context operation exists yet.
        guard remoteSummariesByID[sessionID] == nil else { return false }
        guard let session = sessionsByID[sessionID] else { return false }
        guard session.status != .starting else { return false }
        return ProviderCapabilities.canAppendSystemContext(command: session.command)
    }

    /// "Notify when done" needs a reliable hook Stop signal; sessions on the
    /// output heuristic (pi, shells, unknown commands) don't get the toggle.
    func sessionCanNotifyWhenDone(_ sessionID: String) -> Bool {
        // notifyWhenDone is a platform-owned Host capability that has no
        // remote operation yet; the toggle hides in remote scope.
        guard remoteSummariesByID[sessionID] == nil else { return false }
        guard let session = sessionsByID[sessionID] else { return false }
        return ProviderCapabilities.canNotifyWhenDone(command: session.command)
    }

    /// "Clear attention" is a Controller-local activity-engine escape hatch;
    /// there is no remote operation for it.
    func sessionCanClearAttention(_ sessionID: String) -> Bool {
        remoteSummariesByID[sessionID] == nil
    }

    func promptAppendSystemContext(sessionID: String) {
        guard let session = sessionsByID[sessionID],
              session.status != .starting,
              ProviderCapabilities.canAppendSystemContext(command: session.command)
        else { return }

        let alert = NSAlert()
        alert.messageText = "Append system context"
        alert.informativeText = """
        Save system context for “\(session.label)”. It will be applied the \
        next time this agent resumes.
        """

        let width: CGFloat = 420
        let height: CGFloat = 130
        let scrollView = NSScrollView(frame: NSRect(x: 0, y: 0, width: width, height: height))
        scrollView.borderType = .bezelBorder
        scrollView.hasVerticalScroller = true

        let textView = NSTextView(frame: NSRect(x: 0, y: 0, width: width, height: height))
        textView.font = .systemFont(ofSize: 12)
        textView.isRichText = false
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.string = pendingAppendedContext(sessionID) ?? ""
        scrollView.documentView = textView

        alert.accessoryView = scrollView
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Cancel")
        alert.window.initialFirstResponder = textView

        guard alert.runModal() == .alertFirstButtonReturn else { return }

        let context = textView.string.trimmingCharacters(in: .whitespacesAndNewlines)
        var contexts = loadPendingAppendSystemContexts()
        if context.isEmpty {
            contexts.removeValue(forKey: sessionID)
            Self.removeSharedMarker(sessionID, .appendedContext)
        } else {
            contexts[sessionID] = context
            Self.writeSharedMarker(
                sessionID, .appendedContext,
                ["context": context,
                 "updated_at": Int64(Date().timeIntervalSince1970 * 1000)]
            )
        }
        savePendingAppendSystemContexts(contexts)
        announceStateChange("session-markers")

        var dismissals = loadRestartRecommendationDismissals()
        if dismissals.removeValue(forKey: sessionID) != nil {
            saveRestartRecommendationDismissals(dismissals)
        }
        rescan()
    }

    /// Fork a session: spawn a NEW session that branches the provider's current
    /// conversation into an independent copy, leaving the original running.
    ///
    /// Unlike `restartSession` this does **not** kill the source. The fork is a
    /// new logical thread, so it gets a fresh `created_at` (its own sidebar row)
    /// and does not inherit the source's pin. It keeps the source's cwd/worktree
    /// (branching the conversation, not the files), stamps `spawned_by` as
    /// "fork", carries the source's Sessions MCP grant, and — for a
    /// titled source — labels itself "… (fork)" so it reads as a branch, not a
    /// duplicate. Same-CLI only: a fork resumes a provider-owned transcript, so
    /// there is no Claude→Codex fork (`ResumeCommand.forked` returns nil for
    /// providers without a native fork primitive, making this a no-op).
    @discardableResult
    func forkSession(_ sessionID: String) -> Bool {
        guard let session = sessionsByID[sessionID],
              let project = projectsByID[session.projectID]
        else { return false }
        guard session.status != .starting else { return false }

        let dirURL = LaunchConfig.appSessionsDir.appendingPathComponent(sessionID)
        let manifest = (try? Data(
            contentsOf: dirURL.appendingPathComponent("manifest.json")
        )).flatMap { try? JSONDecoder().decode(HostedSessionManifest.self, from: $0) }
        // A session that never received input has no conversation to branch.
        guard manifest?.hasBeenWrittenTo != false else { return false }

        guard let forkCommand = ResumeCommand.hostRelaunchPlan(
            sessionID: sessionID,
            fork: true
        )?.command else { return false }

        // The Sessions MCP grant is keyed by session id; carry the source's to
        // the fork (like restart does). Browser access is app-wide.
        let accessGrant = mcpOrchestrators[sessionID]

        let newID = spawnSession(
            projectID: session.projectID,
            command: forkCommand,
            label: session.label,
            customTitle: session.customTitle,
            // A branch is its own thread: fresh timestamp so it sorts as a new
            // row rather than colliding with the source's position.
            createdAt: Int64(Date().timeIntervalSince1970 * 1000),
            cwd: session.worktreePath ?? project.path,
            worktreePath: session.worktreePath,
            worktreeBranch: session.worktreeBranch,
            spawnedBy: SessionEntry.forkSpawnMarker,
            role: session.role,
            task: session.task,
            accessGrant: accessGrant,
            activateUI: true
        )
        guard let newID else { return false }

        if let titleOverride = loadSessionTitleOverrides()[sessionID] {
            var titles = loadSessionTitleOverrides()
            titles[newID] = "\(titleOverride) (fork)"
            saveSessionTitleOverrides(titles)
        }
        if let accessGrant {
            mcpOrchestrators[newID] = accessGrant
            persistMcpGrants()
        }
        rescan()
        return true
    }

    // MARK: - Project context menu actions (ProjectItem.svelte:818-941)

    /// Project id whose sidebar row shows the inline "Remove project?"
    /// confirm (native pattern — the Svelte app removes immediately for
    /// plain projects and uses a native dialog only for worktrees).
    @Published var confirmingRemoveProjectID: String?

    static let removedProjectsKey = "unpeel.native.removedProjects"

    /// Tombstoned project ids (native "Remove project"), pruned of ids that
    /// no longer exist in any source — a project re-added later (new id in
    /// Tauri) must not stay hidden, and the list must not grow unbounded.
    private func removedProjectIDs(prunedAgainst knownIDs: Set<String>) -> Set<String> {
        let stored = AppDefaults.shared.stringArray(forKey: Self.removedProjectsKey) ?? []
        let kept = stored.filter { knownIDs.contains($0) }
        if kept.count != stored.count {
            if kept.isEmpty {
                AppDefaults.shared.removeObject(forKey: Self.removedProjectsKey)
            } else {
                AppDefaults.shared.set(kept, forKey: Self.removedProjectsKey)
            }
        }
        return Set(kept)
    }

    func requestRemoveProject(_ projectID: String) {
        confirmingRemoveProjectID = projectID
    }

    func cancelRemoveProjectConfirm() {
        confirmingRemoveProjectID = nil
    }

    /// Remove a project from the sidebar. Parity with remove_project
    /// (project.rs:164-183): the project disappears from the tree along
    /// with its worktree children, its per-project UI state is dropped, and
    /// live hosted sessions are NOT killed (the Tauri app leaves hosts
    /// running too — it only forgets the project). Tauri-owned projects get
    /// a tombstone in `unpeel.native.removedProjects`; natively-added
    /// projects are actually deleted from `unpeel.native.projects`.
    func removeProject(_ projectID: String) {
        if confirmingRemoveProjectID == projectID {
            confirmingRemoveProjectID = nil
        }
        guard let project = projectsByID[projectID] else { return }

        // Natively-added project → drop the record itself.
        var records = loadNativeProjects()
        if records.contains(where: { $0.id == projectID }) {
            records.removeAll { $0.id == projectID }
            if records.isEmpty {
                AppDefaults.shared.removeObject(forKey: Self.nativeProjectsKey)
            } else if let data = try? JSONEncoder().encode(records) {
                AppDefaults.shared.set(data, forKey: Self.nativeProjectsKey)
            }
            mirrorProjectsToSharedState()
        } else {
            // Tauri-owned (or ephemeral) → tombstone overlay, and delete the
            // entry from the shared file: the tombstone only hides it from
            // THIS app, and a project the user removed must disappear from
            // every frontend, not linger in the terminal's sidebar.
            var removed = AppDefaults.shared.stringArray(forKey: Self.removedProjectsKey) ?? []
            if !removed.contains(projectID) {
                removed.append(projectID)
                AppDefaults.shared.set(removed, forKey: Self.removedProjectsKey)
            }
            ephemeralProjects.removeAll { $0.id == projectID }
            editPresetStateAnnouncing { object in
                var projects = (object["projects"] as? [[String: Any]]) ?? []
                let before = projects.count
                projects.removeAll { ($0["id"] as? String) == projectID }
                if projects.count != before {
                    object["projects"] = projects
                }
            }
        }

        // Per-project native state: expansion, reveal, order overlay, and
        // the selection if it lived in this project (or one of its worktree
        // children).
        let removedIDs = projectIDAndWorktreeDescendants(projectID)
        var removedFolderColor = false
        for id in removedIDs {
            expandedProjectIDs.remove(id)
            if projectFolderColorIDs.removeValue(forKey: id) != nil {
                removedFolderColor = true
            }
            AppDefaults.shared.removeObject(forKey: Self.sessionOrderKey(id))
            AppDefaults.shared.removeObject(forKey: Self.pinnedOrderKey(id))
            if archivedProjectID == id { archivedProjectID = nil }
            if let selected = selectedSessionID,
               sessionsByID[selected]?.projectID == id {
                selectedSessionID = nil
            }
        }
        if removedFolderColor {
            saveProjectFolderColorIDs()
        }
        // Project-order overlay slots: top-level order, per-parent worktree
        // orders, and any list owned by a removed project.
        pruneProjectOrderOverlays(removing: removedIDs)

        _ = project
        // Remove is the destructive verb: the subtree's sessions (project
        // root, worktree checkouts, plain groups) go with the project.
        // Leaving their dirs on disk made them unreachable here — the tree
        // only renders buckets for known projects — while the terminal UI
        // resurrected them as phantom cwd-named projects (2026-08-13).
        var doomedProjectIDs = removedIDs
        var stack = Array(removedIDs)
        while let current = stack.popLast() {
            for child in projectsByID.values
            where child.parentProjectID == current && !doomedProjectIDs.contains(child.id) {
                doomedProjectIDs.insert(child.id)
                stack.append(child.id)
            }
        }
        // Same bucket keying as rebuildTree: a valid override target wins
        // over the manifest project, so a session filed elsewhere by the
        // user survives its launch project's removal (and vice versa).
        let knownProjectIDs = Set(projectsByID.keys)
        for session in sessionsByID.values {
            let bucket = session.projectOverrideID.flatMap {
                knownProjectIDs.contains($0) ? $0 : nil
            } ?? session.projectID
            if doomedProjectIDs.contains(bucket) {
                confirmRemoveSession(session.id)
            }
        }
        rescan()
    }

    /// Reveal in Finder (project-menu-reveal → reveal_in_finder, which is
    /// `open -R` on the project path).
    func revealInFinder(path: String) {
        NSWorkspace.shared.activateFileViewerSelecting(
            [URL(fileURLWithPath: path)]
        )
    }

    /// Synchronous `.git` presence check standing in for the Tauri
    /// `is_git_repo` command (worktree.rs) that gates the worktree menu
    /// items. `.git` can be a dir (checkout) or a file (worktree/submodule).
    /// Cached with a short TTL: the sidebar calls this per project row per
    /// render pass, and a stat per row per publish adds up. A `git init`
    /// after launch still shows up within the TTL.
    nonisolated static func isGitRepo(path: String) -> Bool {
        let now = Date()
        return gitRepoCacheLock.withLock {
            if let cached = gitRepoCache[path],
               now.timeIntervalSince(cached.at) < 10 {
                return cached.isRepo
            }
            let isRepo = FileManager.default.fileExists(atPath: path + "/.git")
            gitRepoCache[path] = (now, isRepo)
            return isRepo
        }
    }

    private nonisolated(unsafe) static var gitRepoCache: [String: (at: Date, isRepo: Bool)] = [:]
    private nonisolated static let gitRepoCacheLock = NSLock()

    /// Stop all (project-menu-stop-all): stop every live session of THIS
    /// project — worktree children keep theirs, like the Svelte per-project
    /// session map. Stop is non-destructive: each host is killed through the
    /// identity-guarded `stopSession` path (the same verb the phone's session
    /// sheet uses), but the session dir AND its sidebar row survive — the row
    /// settles into the exited state, from where Restart resumes the
    /// conversation. (Until 2026-07-21 this reused the remove path and
    /// destroyed the sessions outright.)
    func stopAllSessions(projectID: String) {
        if remoteProjectSummariesByID[projectID] != nil {
            let live = remoteSessionsByID.values
                .filter { $0.projectID == projectID && $0.isLive }
                .map(\.id)
                .sorted()
            guard !live.isEmpty else { return }
            performRemoteVerb("Couldn't stop the sessions") { runtime in
                for sessionID in live {
                    try await runtime.stopSession(sessionID)
                }
            }
            return
        }
        let live = sessionsByID.values
            .filter { $0.projectID == projectID && $0.isLive }
        for session in live {
            _ = stopSession(session.id)
        }
    }

    // MARK: - Open in editor (open_in_editor, project.rs:577-615)

    /// Menu label for the configured editor id (editorLabel map,
    /// ProjectItem.svelte:824-831); unknown ids show the raw command.
    nonisolated static func editorDisplayName(_ editor: String) -> String {
        switch editor {
        case "code": return "VS Code"
        case "cursor": return "Cursor"
        case "zed": return "Zed"
        case "idea": return "IntelliJ"
        case "webstorm": return "WebStorm"
        case "xcode": return "Xcode"
        default: return editor
        }
    }

    /// Bundled CLI shims tried before anything else
    /// (preferred_editor_command_candidates, project.rs): VS Code's and
    /// Cursor's `code`/`cursor` CLIs live inside the app bundle, so they
    /// work even when the user never ran "install command in PATH".
    private nonisolated static func bundledEditorCLIs(_ editor: String) -> [String] {
        let home = NSHomeDirectory()
        switch editor {
        case "cursor":
            return [
                "/Applications/Cursor.app/Contents/Resources/app/bin/cursor",
                "\(home)/Applications/Cursor.app/Contents/Resources/app/bin/cursor",
            ]
        case "code":
            return [
                "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
                "\(home)/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
            ]
        default:
            return []
        }
    }

    /// App-name fallback (fallback_editor_app_name, project.rs tests):
    /// `open -a <App>` when the CLI shim isn't found.
    private nonisolated static func fallbackEditorApp(_ editor: String) -> String? {
        switch editor {
        case "cursor": return "Cursor"
        case "code": return "Visual Studio Code"
        case "zed": return "Zed"
        case "idea": return "IntelliJ IDEA"
        case "webstorm": return "WebStorm"
        case "xcode": return "Xcode"
        default: return nil
        }
    }

    /// Same resolution order as project.rs open_in_editor: bundled CLI →
    /// `open -a <App>` → the editor id as a PATH command. Errors surface in
    /// an alert (the Svelte app shows a toast).
    func openInEditor(path: String) {
        Self.openInEditor(editor: codeEditor, path: path, line: nil, column: nil)
    }

    /// Opens a file from a cmd-click in the terminal at the given line/column,
    /// in the user's selected editor. Static so the terminal pane can call it
    /// without a store instance.
    nonisolated static func openFileInPreferredEditor(path: String, line: Int?, column: Int?) {
        openInEditor(editor: preferredCodeEditor(), path: path, line: line, column: column)
    }

    private nonisolated static func openInEditor(
        editor: String,
        path: String,
        line: Int?,
        column: Int?
    ) {
        Task {
            let error = await Task.detached(priority: .userInitiated) {
                openInEditorImpl(editor: editor, path: path, line: line, column: column)
            }.value
            if let error {
                await MainActor.run {
                    showErrorAlert(
                        title: "Couldn't open \(editorDisplayName(editor))",
                        message: error
                    )
                }
            }
        }
    }

    /// Per-editor arguments to open `path`, jumping to `line`/`column` when the
    /// editor's CLI supports it. Editors without a goto flag just get the path.
    private nonisolated static func editorOpenArguments(
        editor: String,
        path: String,
        line: Int?,
        column: Int?
    ) -> [String] {
        guard let line else { return [path] }
        switch editor {
        case "code", "cursor", "zed":
            // `code -g file:line[:col]`; Zed accepts the same `file:line:col`.
            var location = "\(path):\(line)"
            if let column { location += ":\(column)" }
            return editor == "zed" ? [location] : ["-g", location]
        case "idea", "webstorm":
            var args = ["--line", "\(line)"]
            if let column { args += ["--column", "\(column)"] }
            return args + [path]
        default:
            return [path]
        }
    }

    /// Blocking launch helper; returns an error message or nil on success.
    private nonisolated static func openInEditorImpl(
        editor: String,
        path: String,
        line: Int?,
        column: Int?
    ) -> String? {
        guard FileManager.default.fileExists(atPath: path) else {
            return "Not found: \(path)"
        }

        let args = editorOpenArguments(editor: editor, path: path, line: line, column: column)

        func run(_ executable: String, _ args: [String]) -> String? {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: executable)
            process.arguments = args
            process.standardInput = FileHandle.nullDevice
            process.standardOutput = FileHandle.nullDevice
            let stderr = Pipe()
            process.standardError = stderr
            do {
                try process.run()
            } catch {
                return error.localizedDescription
            }
            let errData = stderr.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            if process.terminationStatus == 0 { return nil }
            let message = (String(data: errData, encoding: .utf8) ?? "")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return message.isEmpty ? "\(executable) exited with status \(process.terminationStatus)" : message
        }

        for cli in bundledEditorCLIs(editor)
        where FileManager.default.isExecutableFile(atPath: cli) {
            return run(cli, args)
        }
        if let app = fallbackEditorApp(editor) {
            // `open -a` can't pass a line; only the path survives. The bundled
            // CLI (above) is what carries line/column for code/cursor.
            if run("/usr/bin/open", ["-a", app, path]) == nil { return nil }
            // App launch failed → try the editor id as a CLI before giving up.
            return run("/usr/bin/env", [editor] + args)
        }
        return run("/usr/bin/env", [editor] + args)
    }

    // MARK: - Open workspace target (titlebar dropdown)

    func openWorkspace(path: String, in target: WorkspaceOpenTarget) {
        if target == .finder {
            revealInFinder(path: path)
            return
        }

        Task {
            let error = await Task.detached(priority: .userInitiated) {
                Self.openWorkspaceImpl(target: target, path: path)
            }.value
            if let error {
                await MainActor.run {
                    Self.showErrorAlert(
                        title: "Couldn't open \(target.title)",
                        message: error
                    )
                }
            }
        }
    }

    private nonisolated static func openWorkspaceImpl(
        target: WorkspaceOpenTarget,
        path: String
    ) -> String? {
        guard FileManager.default.fileExists(atPath: path) else {
            return "Folder not found: \(path)"
        }

        func run(_ args: [String]) -> String? {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/bin/open")
            process.arguments = args
            process.standardInput = FileHandle.nullDevice
            process.standardOutput = FileHandle.nullDevice
            let stderr = Pipe()
            process.standardError = stderr
            do {
                try process.run()
            } catch {
                return error.localizedDescription
            }
            let errData = stderr.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            if process.terminationStatus == 0 { return nil }
            let message = (String(data: errData, encoding: .utf8) ?? "")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return message.isEmpty ? "open exited with status \(process.terminationStatus)" : message
        }

        var lastError: String?
        for bundleID in target.bundleIdentifiers {
            if let error = run(["-b", bundleID, path]) {
                lastError = error
            } else {
                return nil
            }
        }
        for app in target.appNames {
            if let error = run(["-a", app, path]) {
                lastError = error
            } else {
                return nil
            }
        }
        return lastError ?? "No launch target found for \(target.title)."
    }

    // MARK: - Worktrees (create/remove; worktree.rs via WorktreeGit)

    /// "New worktree…" (project-menu-create-workspace → NewWorkspaceView in
    /// the Svelte app). The native stand-in is a dialog with a branch combo
    /// box (pick an existing local branch to check it out, or type a new
    /// name) plus a base-branch popup showing what a new branch forks from
    /// (default: the repo's mainline — `origin/<default>` when there is a
    /// remote, else local main/master, else the current branch).
    func promptCreateWorktree(projectID: String) {
        guard let project = projectsByID[projectID] else { return }
        let repoPath = project.path
        Task { [weak self] in
            // Branch enumeration shells out to git; keep it off the main actor.
            let info = await Task.detached(priority: .userInitiated) {
                (all: WorktreeGit.listBranches(repoPath: repoPath),
                 remote: WorktreeGit.listRemoteBranches(repoPath: repoPath),
                 current: WorktreeGit.currentBranch(repoPath: repoPath),
                 defaultBase: WorktreeGit.defaultBaseRef(repoPath: repoPath),
                 checkedOut: WorktreeGit.checkedOutBranches(repoPath: repoPath))
            }.value
            self?.showCreateWorktreeDialog(
                project: project,
                branches: info.all,
                remoteBranches: info.remote,
                currentBranch: info.current,
                defaultBase: info.defaultBase,
                checkedOutBranches: info.checkedOut
            )
        }
    }

    private func showCreateWorktreeDialog(
        project: Project,
        branches: [String],
        remoteBranches: [String],
        currentBranch: String?,
        defaultBase: String?,
        checkedOutBranches: Set<String>
    ) {
        let alert = NSAlert()
        alert.messageText = "New worktree"
        alert.informativeText = """
        Give the worktree a readable name for the sidebar and folder, then \
        pick an existing branch or type a new branch name. If name is blank, \
        the branch name is used.
        """

        let width: CGFloat = 320
        let labelWidth: CGFloat = 64
        let fieldWidth = width - labelWidth
        // Locals first (recency-sorted), then remote-tracking refs; the
        // popup pre-selects the mainline so new branches fork from it
        // instead of whatever the main checkout happens to have open.
        let baseChoices = branches + remoteBranches.filter { !branches.contains($0) }
        let hasBasePicker = !baseChoices.isEmpty
        let nameY: CGFloat = hasBasePicker ? 72 : 38
        let branchY: CGFloat = hasBasePicker ? 36 : 0

        let nameLabel = NSTextField(labelWithString: "Name")
        nameLabel.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        nameLabel.textColor = .secondaryLabelColor
        nameLabel.alignment = .right
        nameLabel.frame = NSRect(x: 0, y: nameY + 5, width: labelWidth - 8, height: 17)

        let nameField = NSTextField(frame: NSRect(
            x: labelWidth, y: nameY, width: fieldWidth, height: 24
        ))
        nameField.placeholderString = "Plugin refactor"

        let branchLabel = NSTextField(labelWithString: "Branch")
        branchLabel.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        branchLabel.textColor = .secondaryLabelColor
        branchLabel.alignment = .right
        branchLabel.frame = NSRect(x: 0, y: branchY + 5, width: labelWidth - 8, height: 17)

        // Branches checked out in another worktree can't be checked out
        // again, but any branch can be the base of a new one.
        let available = branches.filter { !checkedOutBranches.contains($0) }
        let combo = NSComboBox(frame: NSRect(
            x: labelWidth, y: branchY, width: fieldWidth, height: 26
        ))
        combo.placeholderString = "feature/plugin-refactor"
        combo.completes = true
        combo.addItems(withObjectValues: available)

        let baseLabel = NSTextField(labelWithString: "Start from")
        baseLabel.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        baseLabel.textColor = .secondaryLabelColor
        baseLabel.alignment = .right
        baseLabel.frame = NSRect(x: 0, y: 5, width: labelWidth - 8, height: 17)

        let basePopup = NSPopUpButton(
            frame: NSRect(x: labelWidth, y: 0, width: fieldWidth, height: 25),
            pullsDown: false
        )
        basePopup.addItems(withTitles: baseChoices)
        if let defaultBase, baseChoices.contains(defaultBase) {
            basePopup.selectItem(withTitle: defaultBase)
        } else if let currentBranch {
            basePopup.selectItem(withTitle: currentBranch)
        }

        let container = NSView(frame: NSRect(
            x: 0, y: 0, width: width, height: hasBasePicker ? 98 : 64
        ))
        container.addSubview(nameLabel)
        container.addSubview(nameField)
        container.addSubview(branchLabel)
        container.addSubview(combo)
        if hasBasePicker {
            container.addSubview(baseLabel)
            container.addSubview(basePopup)
        }

        alert.accessoryView = container
        alert.addButton(withTitle: "Create Worktree")
        alert.addButton(withTitle: "Cancel")
        alert.window.initialFirstResponder = combo
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        let branch = combo.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !branch.isEmpty else { return }
        let name = nameField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        let baseRef = hasBasePicker ? basePopup.titleOfSelectedItem : nil
        createWorktree(
            parentProject: project,
            branch: branch,
            name: name.isEmpty ? nil : name,
            baseRef: baseRef
        )
    }

    private func createWorktree(
        parentProject: Project, branch: String, name: String?, baseRef: String?
    ) {
        let repoPath = parentProject.path
        Task { [weak self] in
            let result = await Task.detached(priority: .userInitiated) {
                WorktreeGit.createWorktree(
                    repoPath: repoPath, branch: branch, baseRef: baseRef, folderName: name
                )
            }.value
            await MainActor.run {
                guard let self else { return }
                switch result {
                case .created(let path):
                    self.registerWorktreeProject(
                        parentID: parentProject.id, path: path, branch: branch, name: name
                    )
                case .failed(let message):
                    Self.showErrorAlert(title: "Couldn't create worktree", message: message)
                }
            }
        }
    }

    /// Whether the new-session menus offer "In a new worktree" for this
    /// project — same gate as the row's "New worktree…" context-menu item:
    /// a real project (not a folder), not itself a worktree child, and the
    /// path is a git repo.
    func canOfferWorktreeSession(projectID: String) -> Bool {
        guard isExperimentalEnabled(.worktrees) else { return false }
        guard let project = projectsByID[projectID] else { return false }
        return project.isFolder != true
            && project.worktreeBranch == nil
            && Self.isGitRepo(path: project.path)
    }

    // MARK: - Experimental features (Settings ▸ Experimental)

    /// Whether an experimental feature is active for this store. Reads the
    /// published set so SwiftUI views that gate on it recompute when it flips.
    func isExperimentalEnabled(_ feature: ExperimentalFeature) -> Bool {
        enabledExperimentalKeys.contains(feature.key)
    }

    /// Toggle an experimental feature: persist the preference and update the
    /// published set (which republishes the store so dependent UI re-evaluates).
    func setExperimental(_ enabled: Bool, for feature: ExperimentalFeature) {
        UnpeelFeatureFlags.setEnabled(enabled, for: feature)
        if enabled {
            enabledExperimentalKeys.insert(feature.key)
        } else {
            enabledExperimentalKeys.remove(feature.key)
        }
    }

    /// One-shot "new session in a new worktree" from the new-session menus:
    /// ask what the session is working on, then create (or reuse) a worktree
    /// named after the answer and launch the preset inside it — the same
    /// resolution used by controller-driven session starts.
    /// The branch and folder derive from the name; the existing
    /// "New worktree…" dialog remains the place to pick an explicit branch
    /// or base ref.
    func promptNewWorktreeSession(projectID: String, preset: Preset) {
        guard let project = projectsByID[projectID] else { return }

        let alert = NSAlert()
        alert.messageText = "New \(preset.label) session in a worktree"
        alert.informativeText = """
        The session runs in its own copy of “\(project.name)” on a separate \
        branch, so it won't touch files other sessions are using. The \
        worktree and its branch are named after this.
        """
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 280, height: 24))
        field.placeholderString = "Plugin refactor"
        alert.accessoryView = field
        alert.addButton(withTitle: "Start Session")
        alert.addButton(withTitle: "Cancel")
        alert.window.initialFirstResponder = field
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        let name = field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return }
        startWorktreeSession(project: project, preset: preset, name: name)
    }

    /// Resolve/create the worktree via the shared `sessionLaunchTarget`
    /// (reuse an existing child project or checkout for the derived branch,
    /// else create branch + worktree), then spawn like a normal preset
    /// launch. Runs synchronously on the main actor, matching the internal
    /// controller start route that shares the resolution logic.
    private func startWorktreeSession(project: Project, preset: Preset, name: String) {
        let branch = WorktreeGit.branchSlug(name).lowercased()
        switch sessionLaunchTarget(
            project: project,
            worktreeBranch: branch,
            worktreeName: name,
            baseRef: nil
        ) {
        case .success(let target):
            spawnSession(
                projectID: target.projectID,
                command: preset.command,
                label: preset.command.isEmpty ? "Terminal" : preset.command,
                customTitle: false,
                createdAt: Int64(Date().timeIntervalSince1970 * 1000),
                cwd: target.cwd,
                worktreePath: target.worktreePath,
                worktreeBranch: target.worktreeBranch
            )
        case .failure(let failure):
            Self.showErrorAlert(
                title: "Couldn't start the worktree session",
                message: failure.message
            )
        }
    }

    /// ensure_worktree_project parity (project.rs:102-160): the worktree
    /// becomes a child project named after its custom name or branch so it groups sessions
    /// and reuses all project UI. Stored natively; never written to
    /// app-state.json.
    @discardableResult
    func registerWorktreeProject(
        parentID: String, path: String, branch: String, name: String? = nil
    ) -> String? {
        let canonicalPath = URL(fileURLWithPath: path).resolvingSymlinksInPath().path
        let trimmedName = name?.trimmingCharacters(in: .whitespacesAndNewlines)
        let projectName = trimmedName?.isEmpty == false ? trimmedName! : branch
        var projectID = projectsByID.values.first {
            URL(fileURLWithPath: $0.path).resolvingSymlinksInPath().path == canonicalPath
        }?.id
        if projectID == nil {
            var records = loadNativeProjects()
            projectID = records.first {
                URL(fileURLWithPath: $0.path).resolvingSymlinksInPath().path == canonicalPath
            }?.id
            if projectID == nil {
                let id = "native-\(UUID().uuidString.lowercased())"
                records.append(NativeProjectRecord(
                    id: id,
                    name: projectName,
                    path: path,
                    parentProjectID: parentID,
                    worktreeBranch: branch
                ))
                projectID = id
                if let data = try? JSONEncoder().encode(records) {
                    AppDefaults.shared.set(data, forKey: Self.nativeProjectsKey)
                    mirrorProjectsToSharedState()
                }
            }
        }
        // Show the result like handleWorkspacesEnabled (Sidebar.svelte:131):
        // expand the parent so the new inline worktree folder row is visible
        // (the row itself lands via rescan).
        expandedProjectIDs.insert(parentID)
        rescan()
        return projectID ?? projectsByID.values.first {
            URL(fileURLWithPath: $0.path).resolvingSymlinksInPath().path == canonicalPath
        }?.id
    }

    /// Rename the Unpeel worktree project. This deliberately does not rename
    /// the git branch or move an existing checkout folder; live sessions and
    /// saved manifests continue to point at the same path.
    func promptRenameWorktreeProject(_ projectID: String) {
        guard let project = projectsByID[projectID],
              project.parentProjectID != nil
        else { return }
        let branch = project.worktreeBranch

        let alert = NSAlert()
        alert.messageText = branch != nil ? "Rename worktree" : "Rename group"
        alert.informativeText = branch.map {
            """
            This changes the name shown in Unpeel. The git branch stays \
            "\($0)".
            """
        } ?? "This changes the name shown in Unpeel."
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 280, height: 24))
        field.stringValue = project.name
        field.placeholderString = branch ?? project.name
        alert.accessoryView = field
        alert.addButton(withTitle: "Rename")
        alert.addButton(withTitle: "Cancel")
        alert.window.initialFirstResponder = field
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        if project.acceptsSessionDrop {
            // Plain groups can live only in the shared file (TUI-created,
            // `tui-` ids) — the worktree path's native-record lookup would
            // silently drop the rename for those.
            renameGroupProject(projectID, to: field.stringValue)
        } else {
            renameWorktreeProject(projectID, to: field.stringValue)
        }
    }

    private func renameWorktreeProject(_ projectID: String, to name: String) {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty,
              let project = projectsByID[projectID],
              project.parentProjectID != nil,
              trimmed != project.name
        else { return }

        let canonicalPath = URL(fileURLWithPath: project.path).resolvingSymlinksInPath().path
        var records = loadNativeProjects()
        // Groups share the parent's path, so the path fallback (worktree
        // records whose id drifted) only applies to branch-backed children.
        guard let index = records.firstIndex(where: { $0.id == projectID })
            ?? (project.worktreeBranch != nil
                ? records.firstIndex(where: {
                    URL(fileURLWithPath: $0.path).resolvingSymlinksInPath().path == canonicalPath
                })
                : nil)
        else { return }
        records[index].name = trimmed
        if let data = try? JSONEncoder().encode(records) {
            AppDefaults.shared.set(data, forKey: Self.nativeProjectsKey)
            mirrorProjectsToSharedState()
        }
        rescan()
    }

    // MARK: - Groups (plain organizational child folders)

    /// "New group…" on a project: name prompt, then a child project record
    /// with the parent's path, no branch, `isFolder` set. Groups render as
    /// inline folder rows beside the worktrees and exist purely to organize
    /// sessions — moving a session in is a `project-override.json` marker,
    /// never a manifest edit.
    func promptCreateGroup(projectID: String) {
        guard let project = projectsByID[projectID] else { return }
        let alert = NSAlert()
        alert.messageText = "New group"
        alert.informativeText = "Group sessions under a named folder in the sidebar. Sessions keep running where they are — this only organizes the list."
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 280, height: 24))
        field.placeholderString = "Research"
        alert.accessoryView = field
        alert.addButton(withTitle: "Create")
        alert.addButton(withTitle: "Cancel")
        alert.window.initialFirstResponder = field
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        let name = field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return }
        var records = loadNativeProjects()
        records.append(NativeProjectRecord(
            id: "native-\(UUID().uuidString.lowercased())",
            name: name,
            path: project.path,
            parentProjectID: projectID,
            isFolder: true
        ))
        if let data = try? JSONEncoder().encode(records) {
            AppDefaults.shared.set(data, forKey: Self.nativeProjectsKey)
            mirrorProjectsToSharedState()
        }
        expandedProjectIDs.insert(projectID)
        rescan()
    }

    /// Rename either a native-record group or a shared-file group. The latter
    /// is how a group created in the TUI remains editable while the app is
    /// running; native records still mirror their updated name into the file.
    @discardableResult
    func renameGroupProject(_ projectID: String, to rawName: String) -> Bool {
        let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty,
              let project = projectsByID[projectID],
              project.acceptsSessionDrop
        else { return false }
        var records = loadNativeProjects()
        if let index = records.firstIndex(where: { $0.id == projectID }) {
            records[index].name = name
            guard let data = try? JSONEncoder().encode(records) else { return false }
            AppDefaults.shared.set(data, forKey: Self.nativeProjectsKey)
            mirrorProjectsToSharedState()
        } else {
            let changed = editPresetStateAnnouncing { object in
                var projects = (object["projects"] as? [[String: Any]]) ?? []
                guard let index = projects.firstIndex(where: {
                    ($0["id"] as? String) == projectID
                        && ($0["is_folder"] as? Bool) == true
                        && $0["parent_project_id"] is String
                        && $0["worktree_branch"] == nil
                }) else { return }
                projects[index]["name"] = name
                object["projects"] = projects
            }
            guard changed else { return false }
        }
        rescan()
        return true
    }

    /// Remove a group: unpin, move, and archive every session under its
    /// parent first, then forget the record. Rehoming before removal keeps
    /// the archived conversations reachable even for sessions launched
    /// directly in the group (their manifest project remains the group id).
    /// Controller callers already confirmed and pass `confirm: false`.
    @discardableResult
    func removeGroupProject(_ projectID: String, confirm: Bool = true) -> Int? {
        guard let project = projectsByID[projectID],
              let parentID = project.parentProjectID,
              project.acceptsSessionDrop
        else { return nil }
        let members = sessionsByID.values
            .filter { effectiveProjectID(for: $0) == projectID }
        if confirm && !members.isEmpty {
            let count = members.count
            let noun = count == 1 ? "session" : "sessions"
            let parentName = projectsByID[parentID]?.name ?? "the parent project"
            let alert = NSAlert()
            alert.messageText = "Remove group?"
            alert.informativeText = "\(count) \(noun) will be stopped and archived under \(parentName)."
            alert.addButton(withTitle: "Remove Group")
            alert.addButton(withTitle: "Cancel")
            guard alert.runModal() == .alertFirstButtonReturn else { return nil }
        }
        for session in members {
            // Pins deliberately win over archive everywhere else. Removing
            // the container is the exception: every contained session must
            // actually land in the archive.
            unpinSession(projectID: projectID, sessionID: session.id)
            moveSession(session.id, toProjectID: parentID)
            archiveSession(session.id)
        }
        var records = loadNativeProjects()
        if records.contains(where: { $0.id == projectID }) {
            records.removeAll { $0.id == projectID }
            if let data = try? JSONEncoder().encode(records) {
                AppDefaults.shared.set(data, forKey: Self.nativeProjectsKey)
                mirrorProjectsToSharedState()
            }
        } else {
            editPresetStateAnnouncing { object in
                var projects = (object["projects"] as? [[String: Any]]) ?? []
                projects.removeAll { ($0["id"] as? String) == projectID }
                object["projects"] = projects
            }
        }
        rescan()
        return members.count
    }

    /// File a session under a plain organizational group, or back at its root
    /// project. Git worktrees are deliberately rejected because changing a
    /// checkout needs restart/resume, not this display-only override.
    /// Cross-frontend via the shared `project-override.json` marker.
    func moveSession(_ sessionID: String, toProjectID targetID: String) {
        // Filing is a shared-marker write with no remote operation yet.
        guard remoteSummariesByID[sessionID] == nil else { return }
        guard let session = sessionsByID[sessionID] else { return }
        var rootID = session.projectID
        var hops = 0
        while let parent = projectsByID[rootID]?.parentProjectID, hops < 16 {
            rootID = parent
            hops += 1
        }
        guard let target = projectsByID[targetID] else { return }
        let targetIsRoot = targetID == rootID
        let targetIsPlainGroup = target.acceptsSessionDrop
            && target.parentProjectID == rootID
        guard targetIsRoot || targetIsPlainGroup else { return }
        let sourceProjectID = effectiveProjectID(for: session)
        guard sourceProjectID != targetID else { return }
        let wasPinned = isPinned(sessionID: sessionID, projectID: sourceProjectID)
        if targetID == session.projectID {
            Self.removeSharedMarker(sessionID, .projectOverride)
        } else {
            Self.writeSharedMarker(sessionID, .projectOverride, [
                "project_id": targetID,
                "moved_at": Int64(Date().timeIntervalSince1970 * 1000),
            ])
            expandedProjectIDs.insert(targetID)
        }
        announceStateChange("session-markers")
        rescan()
        // Pins are project-scoped. Preserve that intent when filing a pinned
        // session by moving its pin record to the destination after the
        // synchronous rescan has adopted the new override.
        if wasPinned {
            pinSession(projectID: targetID, sessionID: sessionID)
        }
    }

    /// Destinations for a session's display-only "Move to" menu: its root
    /// project plus plain organizational groups. Git worktrees stay out of
    /// this menu because entering one requires an explicit restart/resume.
    func moveDestinations(forSession sessionID: String) -> [(id: String, name: String)] {
        // Filing between groups is a shared-marker write with no remote
        // operation yet; the "Move to" menu hides in remote scope.
        guard remoteSummariesByID[sessionID] == nil else { return [] }
        guard let session = sessionsByID[sessionID] else { return [] }
        let effective = effectiveProjectID(for: session)
        var rootID = session.projectID
        var hops = 0
        while let parent = projectsByID[rootID]?.parentProjectID, hops < 16 {
            rootID = parent
            hops += 1
        }
        guard let root = projectsByID[rootID] else { return [] }
        let children = projectsByID.values
            .filter {
                $0.parentProjectID == rootID
                    && $0.acceptsSessionDrop
            }
            .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
        return ([root] + children)
            .filter { $0.id != effective }
            .map { ($0.id, $0.name) }
    }

    /// "Remove worktree" on a worktree child project: native confirm
    /// dialog, `git worktree remove` (refuses while dirty), then forget the
    /// project. A dirty refusal comes back as a second, destructive
    /// "Force Delete" confirmation that retries with --force. Committed
    /// work stays on the branch either way
    /// (handleRemoveProject, Sidebar.svelte:74-106).
    func removeWorktreeProject(_ projectID: String) {
        guard let project = projectsByID[projectID],
              let branch = project.worktreeBranch
        else { return }
        let parentPath = project.parentProjectID.flatMap { projectsByID[$0]?.path }

        let alert = NSAlert()
        alert.messageText = "Remove worktree \"\(project.name)\"?"
        alert.informativeText = """
        This deletes the worktree folder from disk. Committed work stays on \
        the "\(branch)" branch, and git will refuse if there are unsaved \
        changes.
        """
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Remove Worktree")
        alert.addButton(withTitle: "Cancel")
        guard alert.runModal() == .alertFirstButtonReturn else { return }

        runWorktreeRemoval(
            projectID: projectID, projectName: project.name, branch: branch,
            parentPath: parentPath, worktreePath: project.path, force: false
        )
    }

    private func runWorktreeRemoval(
        projectID: String, projectName: String, branch: String,
        parentPath: String?, worktreePath: String, force: Bool
    ) {
        Task { [weak self] in
            let outcome = await Task.detached(priority: .userInitiated) {
                WorktreeGit.removeWorktree(
                    repoPath: parentPath, worktreePath: worktreePath, force: force
                )
            }.value
            await MainActor.run {
                guard let self else { return }
                switch outcome {
                case .removed, .alreadyGone:
                    self.removeProject(projectID)
                case .dirty:
                    guard Self.confirmForceRemoveWorktree(
                        name: projectName, branch: branch
                    ) else { return }
                    self.runWorktreeRemoval(
                        projectID: projectID, projectName: projectName, branch: branch,
                        parentPath: parentPath, worktreePath: worktreePath, force: true
                    )
                case .failed(let message):
                    Self.showErrorAlert(title: "Couldn't remove worktree", message: message)
                }
            }
        }
    }

    private static func confirmForceRemoveWorktree(name: String, branch: String) -> Bool {
        let alert = NSAlert()
        alert.messageText = "\"\(name)\" has unsaved changes"
        alert.informativeText = """
        The worktree contains modified or untracked files that are not \
        committed. Force deleting discards them permanently. Committed work \
        stays on the "\(branch)" branch.
        """
        alert.alertStyle = .critical
        let forceButton = alert.addButton(withTitle: "Force Delete")
        forceButton.hasDestructiveAction = true
        alert.addButton(withTitle: "Cancel")
        return alert.runModal() == .alertFirstButtonReturn
    }

    private static func showErrorAlert(title: String, message: String) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.alertStyle = .warning
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }

    // MARK: - Presets (flat ordered list; preset.rs list_presets)

    /// Native-side preset overrides persisted in UserDefaults. The native
    /// app is GLOBAL-presets-only. The Tauri app owns
    /// ~/.unpeel/app-state.json, so we never write it; native mutations
    /// are merged over the file presets at read time, native wins by id
    /// (same pattern as the pin overrides).
    struct PresetOverlay: Codable {
        var added: [Preset] = []
        var edited: [Preset] = []
        var removedIDs: [String] = []
    }

    /// Legacy on-disk shape: the overlay used to be keyed by scope
    /// ("global" or a project id). Only the "global" entry is migrated;
    /// project-scope entries are dropped (project presets were removed).
    private func loadPresetOverlay() -> PresetOverlay {
        guard let data = AppDefaults.shared.data(forKey: Self.nativePresetsKey)
        else { return PresetOverlay() }
        if let overlay = try? JSONDecoder().decode(PresetOverlay.self, from: data) {
            return overlay
        }
        if let legacy = try? JSONDecoder().decode([String: PresetOverlay].self, from: data) {
            return legacy["global"] ?? PresetOverlay()
        }
        return PresetOverlay()
    }

    private func savePresetOverlay(_ overlay: PresetOverlay) {
        if let data = try? JSONEncoder().encode(overlay) {
            AppDefaults.shared.set(data, forKey: Self.nativePresetsKey)
        }
    }

    /// Apply the overlay to the file-based presets:
    /// removedIDs hide file entries, edited replaces file entries by id,
    /// added appends. Everything is quick-launch-sanitized on the way out
    /// (sanitize_preset_quick_launch parity, like list_presets_for_project).
    private func overlaid(_ base: [Preset], overlay: PresetOverlay) -> [Preset] {
        let removed = Set(overlay.removedIDs)
        var editedByID: [String: Preset] = [:]
        for preset in overlay.edited { editedByID[preset.id] = preset }

        var result = base
            .filter { !removed.contains($0.id) }
            .map { editedByID[$0.id] ?? $0 }
        let baseIDs = Set(base.map(\.id))
        result.append(contentsOf: overlay.added.filter {
            !removed.contains($0.id) && !baseIDs.contains($0.id)
        })
        return result.map { $0.sanitized() }
    }

    /// Build the preset lists and quick strip (shared by every project).
    /// Migrated installs read app-state.json alone — the array order IS the
    /// display order, shared with the TUI. Un-migrated installs still layer
    /// the legacy UserDefaults overlay, then fold it into the file one-shot.
    private func rebuildPresets(
        globalPresets: [Preset],
        setupDone: Bool,
        overlayMigrated: Bool,
        allowFold: Bool
    ) {
        let merged: [Preset]
        if overlayMigrated {
            presetsInSharedFile = true
            merged = globalPresets.map { $0.sanitized() }
        } else {
            migrateCLIPreferencesIfNeeded(globalPresets: globalPresets, setupDone: setupDone)
            merged = orderApplied(overlaid(globalPresets, overlay: loadPresetOverlay()))
            // A file that exists but did not decode must never be folded
            // over — `globalPresets` would be the builtin fallback, not the
            // user's list.
            presetsInSharedFile = allowFold && migrateOverlayPresetsToSharedFile(merged)
        }
        let enabled = merged.filter { $0.enabled }
        // Sidebar-visible subset: presets whose CLI is installed on this
        // machine (unknown-head custom commands are always shown). Hiding a
        // preset is per-preset now — disable it — so no separate CLI
        // availability filter. Pre-scan → everything counts as installed.
        let available = enabled.filter { preset in
            guard let cli = SetupTool.detect(in: preset.command) else { return true }
            return isCLIInstalled(cli) ?? true
        }
        // Starred presets grouped by CLI, in flat-list order; the strip shows
        // one chip per group (dropdown when a CLI has 2+ starred presets).
        let groups = collectQuickPresetGroups(available)
        let quick = groups.map(\.leader) + [.newTerminal]

        if merged != mergedPresets {
            mergedPresets = merged
        }
        if enabled != enabledPresets {
            enabledPresets = enabled
        }
        if available != availablePresets {
            availablePresets = available
        }
        if groups != quickPresetGroups {
            quickPresetGroups = groups
        }
        if quick != quickPresets {
            quickPresets = quick
        }
    }

    /// nil = the PATH scan hasn't completed yet.
    func isCLIInstalled(_ cli: SetupTool) -> Bool? {
        setupToolReport?.status(for: cli)?.installed
    }

    /// One-time fold of the UserDefaults preset overlay (adds/edits/removals
    /// plus the flat display order) into app-state.json, which from then on
    /// is the single preset truth both UIs read and write. The overlay keys
    /// are left in place — defaults are shared by bundle id, so an older
    /// build running side by side must keep its state — but this build never
    /// reads them again once the file carries the marker. On write failure
    /// the overlay stays authoritative and the fold retries next rescan.
    private func migrateOverlayPresetsToSharedFile(_ merged: [Preset]) -> Bool {
        editPresetStateAnnouncing { object in
            let existing = PresetStateFile.rawPresets(of: object)
            var byID: [String: [String: Any]] = [:]
            for dict in existing {
                if let id = dict["id"] as? String { byID[id] = dict }
            }
            let mergedIDs = Set(merged.map(\.id))
            var rewritten = merged.map { preset in
                PresetStateFile.apply(preset, to: byID[preset.id] ?? ["project_id": NSNull()])
            }
            // Keep rows this build does not model (Tauri-era project-scoped
            // presets) — only global rows the overlay removed are meant to
            // disappear.
            rewritten.append(contentsOf: existing.filter { dict in
                guard let id = dict["id"] as? String, !mergedIDs.contains(id) else {
                    return false
                }
                let projectID = dict["project_id"]
                return projectID != nil && !(projectID is NSNull)
            })
            object["presets"] = rewritten
            object[PresetStateFile.migratedKey] = true
        }
    }

    // MARK: - Flat preset order (native UserDefaults overlay)

    /// Sort presets by the saved flat order. Ids missing from the order (new
    /// presets, or an empty order) append at the end in their incoming
    /// (app-state.json) order.
    private func orderApplied(_ presets: [Preset]) -> [Preset] {
        guard !presetOrder.isEmpty else { return presets }
        let rank = Dictionary(
            uniqueKeysWithValues: presetOrder.enumerated().map { ($1, $0) }
        )
        return presets.enumerated()
            .sorted { lhs, rhs in
                let lhsRank = rank[lhs.element.id] ?? Int.max
                let rhsRank = rank[rhs.element.id] ?? Int.max
                if lhsRank != rhsRank { return lhsRank < rhsRank }
                return lhs.offset < rhs.offset
            }
            .map(\.element)
    }

    /// Reorder the preset list. `currentOrder` is the preset-id order the list
    /// was showing (so drag indices line up with what the user sees).
    /// `currentOrder` may be a visible subset (not-installed CLIs' presets are
    /// hidden from the lists); presets outside it keep their prior relative
    /// order, appended after the reordered visible ones.
    func movePresets(_ currentOrder: [String], from offsets: IndexSet, to destination: Int) {
        var order = currentOrder
        order.move(fromOffsets: offsets, toOffset: destination)
        let visible = Set(order)
        let rest = mergedPresets.map(\.id).filter { !visible.contains($0) }
        applyPresetOrder(order + rest)
        rescan()
    }

    /// Persist a full flat order. Migrated installs rewrite the file's
    /// presets array — the array order IS the display order everywhere,
    /// including the TUI; rows missing from `fullOrder` (project-scoped or
    /// unmodelled) keep their relative position at the end. Un-migrated
    /// installs keep the legacy presetOrder overlay.
    private func applyPresetOrder(_ fullOrder: [String]) {
        if presetsInSharedFile {
            let rank = Dictionary(
                uniqueKeysWithValues: fullOrder.enumerated().map { ($1, $0) }
            )
            editPresetStateAnnouncing { object in
                object["presets"] = PresetStateFile.rawPresets(of: object)
                    .enumerated()
                    .sorted { lhs, rhs in
                        let lhsRank = (lhs.element["id"] as? String)
                            .flatMap { rank[$0] } ?? Int.max
                        let rhsRank = (rhs.element["id"] as? String)
                            .flatMap { rank[$0] } ?? Int.max
                        if lhsRank != rhsRank { return lhsRank < rhsRank }
                        return lhs.offset < rhs.offset
                    }
                    .map(\.element)
            }
        } else {
            presetOrder = fullOrder
            savePresetOrder()
        }
    }

    private static func loadPresetOrder() -> [String] {
        guard let data = AppDefaults.shared.data(forKey: nativePresetOrderKey),
              let order = try? JSONDecoder().decode([String].self, from: data)
        else { return [] }
        return order
    }

    private func savePresetOrder() {
        if let data = try? JSONEncoder().encode(presetOrder) {
            AppDefaults.shared.set(data, forKey: Self.nativePresetOrderKey)
        }
    }

    /// One-time fold of the legacy per-CLI preferences (display order, hide
    /// toggles, MCP default choices) into the flat preset list:
    /// - the initial `presetOrder` reproduces the old derived order (CLI rank
    ///   over the saved CLI order, custom commands last),
    /// - each explicit per-CLI default preset is hoisted above its CLI
    ///   siblings (order-derived defaults keep the same MCP behavior),
    /// - presets of CLIs hidden via the old availability toggle are disabled
    ///   (and unstarred) — per-preset Enabled is the hide mechanism now.
    /// Runs once: guarded on the presetOrder key being absent. Fresh machines
    /// (no legacy keys, setup not completed) skip it entirely so first-run
    /// usage-based seeding can produce the first order instead.
    private func migrateCLIPreferencesIfNeeded(globalPresets: [Preset], setupDone: Bool) {
        guard AppDefaults.shared.object(forKey: Self.nativePresetOrderKey) == nil else { return }
        let defaults = AppDefaults.shared
        let hasLegacy = defaults.object(forKey: Self.legacyCLIOrderKey) != nil
            || defaults.object(forKey: Self.legacyCLIDefaultsKey) != nil
            || defaults.object(forKey: Self.legacyCLIAvailabilityKey) != nil
        guard hasLegacy || setupDone else { return }

        func decode<T: Decodable>(_ type: T.Type, key: String) -> T? {
            defaults.data(forKey: key).flatMap { try? JSONDecoder().decode(type, from: $0) }
        }
        let legacyOrder = decode([String].self, key: Self.legacyCLIOrderKey) ?? []
        let legacyDefaults = decode([String: String].self, key: Self.legacyCLIDefaultsKey) ?? [:]
        let legacyHidden = decode([String: Bool].self, key: Self.legacyCLIAvailabilityKey) ?? [:]

        var overlay = loadPresetOverlay()
        let merged = overlaid(globalPresets, overlay: overlay)

        // Old derived order: saved CLI order first, then declaration order;
        // custom commands last, ties keep app-state order.
        let savedCLIs = legacyOrder.compactMap { SetupTool(rawValue: $0) }
        let savedCLISet = Set(savedCLIs)
        let orderedCLIs = savedCLIs + SetupTool.allCases.filter { !savedCLISet.contains($0) }
        let cliRank = Dictionary(uniqueKeysWithValues: orderedCLIs.enumerated().map { ($1, $0) })
        var ordered = merged.enumerated()
            .sorted { lhs, rhs in
                let lhsRank = SetupTool.detect(in: lhs.element.command).flatMap { cliRank[$0] } ?? Int.max
                let rhsRank = SetupTool.detect(in: rhs.element.command).flatMap { cliRank[$0] } ?? Int.max
                if lhsRank != rhsRank { return lhsRank < rhsRank }
                return lhs.offset < rhs.offset
            }
            .map(\.element)

        // Hoist each explicitly-chosen default above its CLI siblings.
        for (cliRaw, presetID) in legacyDefaults {
            guard let cli = SetupTool(rawValue: cliRaw),
                  let from = ordered.firstIndex(where: { $0.id == presetID }),
                  let to = ordered.firstIndex(where: { SetupTool.detect(in: $0.command) == cli }),
                  to < from
            else { continue }
            let preset = ordered.remove(at: from)
            ordered.insert(preset, at: to)
        }

        // Hidden CLIs → disable (and unstar) their presets.
        let hiddenCLIs = Set(
            legacyHidden.filter { !$0.value }.keys.compactMap { SetupTool(rawValue: $0) }
        )
        if !hiddenCLIs.isEmpty {
            for preset in merged
            where SetupTool.detect(in: preset.command).map(hiddenCLIs.contains) == true {
                var flipped = preset
                flipped.enabled = false
                flipped.quickLaunch = false
                record(flipped, into: &overlay)
            }
            savePresetOverlay(overlay)
        }

        presetOrder = ordered.map(\.id)
        savePresetOrder()
        // The legacy keys are deliberately NOT deleted: UserDefaults is
        // shared by bundle id, so an older build running side by side (the
        // installed release app next to a dev build) would lose its CLI
        // order/defaults/hidden state mid-flight. The presetOrder-key guard
        // above already makes this migration one-shot.
    }

    // MARK: - Per-CLI default preset (order-derived)

    /// The preset the Unpeel Sessions MCP launches for a new session of `cli`:
    /// the topmost enabled preset of that CLI in the flat list order
    /// (reordering the list is how the default is chosen). Falls back to the
    /// topmost disabled one so a bare CLI id still resolves.
    func defaultPreset(for cli: SetupTool) -> Preset? {
        let presets = mergedPresets.filter { SetupTool.detect(in: $0.command) == cli }
        return presets.first(where: { $0.enabled }) ?? presets.first
    }

    func isDefaultPreset(_ preset: Preset, for cli: SetupTool) -> Bool {
        defaultPreset(for: cli)?.id == preset.id
    }

    // MARK: - Preset editing (shared app-state.json; PresetsPanel.svelte
    // semantics — legacy overlay paths only until the one-shot migration)

    /// add_preset (preset.rs:170-197) via PresetsPanel handleAdd: label =
    /// command, enabled, not quick-launch.
    @discardableResult
    func addPreset(command: String) -> Preset? {
        let cmd = command.trimmingCharacters(in: .whitespaces)
        guard !cmd.isEmpty else { return nil }
        let preset = Preset(
            id: "native-\(UUID().uuidString.lowercased())",
            label: cmd,
            command: cmd,
            enabled: true,
            quickLaunch: false
        )
        if presetsInSharedFile {
            let wrote = editPresetStateAnnouncing { object in
                var list = (object["presets"] as? [Any]) ?? []
                list.append(PresetStateFile.apply(preset, to: ["project_id": NSNull()]))
                object["presets"] = list
            }
            guard wrote else { return nil }
        } else {
            var overlay = loadPresetOverlay()
            overlay.added.append(preset)
            savePresetOverlay(overlay)
        }
        rescan()
        return preset
    }

    /// update_preset (preset.rs:222-285), evaluated on the MERGED view and
    /// recorded into the overlay. Any number of presets can be starred —
    /// same-CLI stars collapse into one quick-strip dropdown chip, so there
    /// is no sibling-disable rule anymore.
    func updatePreset(
        id: String,
        command: String? = nil,
        enabled: Bool? = nil,
        quickLaunch: Bool? = nil
    ) {
        guard var preset = mergedPresets.first(where: { $0.id == id }) else { return }

        if let command {
            let cmd = command.trimmingCharacters(in: .whitespaces)
            guard !cmd.isEmpty else { return }
            preset.command = cmd
            // PresetsPanel keeps the label mirrored to the command.
            preset.label = cmd
        }
        if let enabled { preset.enabled = enabled }
        if let quickLaunch { preset.quickLaunch = quickLaunch }
        preset = preset.sanitized()

        if presetsInSharedFile {
            let updated = preset
            editPresetStateAnnouncing { object in
                object["presets"] = PresetStateFile.rawPresets(of: object).map { dict in
                    (dict["id"] as? String) == id
                        ? PresetStateFile.apply(updated, to: dict)
                        : dict
                }
            }
        } else {
            var overlay = loadPresetOverlay()
            record(preset, into: &overlay)
            savePresetOverlay(overlay)
        }
        rescan()
    }

    /// remove_preset (preset.rs:200-220). Migrated installs delete the row
    /// from the shared file; legacy installs drop natively-added presets from
    /// the overlay and tombstone file-based ones.
    func removePreset(id: String) {
        if presetsInSharedFile {
            editPresetStateAnnouncing { object in
                object["presets"] = PresetStateFile.rawPresets(of: object)
                    .filter { ($0["id"] as? String) != id }
            }
            rescan()
            return
        }
        var overlay = loadPresetOverlay()
        if let index = overlay.added.firstIndex(where: { $0.id == id }) {
            overlay.added.remove(at: index)
        } else {
            overlay.edited.removeAll { $0.id == id }
            if !overlay.removedIDs.contains(id) {
                overlay.removedIDs.append(id)
            }
        }
        savePresetOverlay(overlay)
        if presetOrder.contains(id) {
            presetOrder.removeAll { $0 == id }
            savePresetOrder()
        }
        rescan()
    }

    private func record(_ preset: Preset, into overlay: inout PresetOverlay) {
        if let index = overlay.added.firstIndex(where: { $0.id == preset.id }) {
            overlay.added[index] = preset
        } else {
            overlay.edited.removeAll { $0.id == preset.id }
            overlay.edited.append(preset)
        }
    }

    // MARK: - Native-added projects

    /// Projects added from the native footer "+" (Sidebar.svelte footer →
    /// App.svelte handleAddProject) plus natively-created worktree child
    /// projects (ensure_worktree_project parity). The Tauri app owns
    /// app-state.json, so native additions are persisted in UserDefaults
    /// and merged at read time, mirroring the pin-overrides approach.
    /// The worktree fields are optional so records written by older builds
    /// keep decoding.
    private struct NativeProjectRecord: Codable {
        let id: String
        var name: String
        let path: String
        var parentProjectID: String?
        var worktreeBranch: String?
        /// Plain group child folders (parent set, no branch); optional so
        /// records written by older builds keep decoding.
        var isFolder: Bool?
    }

    private static let nativeProjectsKey = "unpeel.native.projects"

    private func loadNativeProjects() -> [NativeProjectRecord] {
        guard let data = AppDefaults.shared.data(forKey: Self.nativeProjectsKey),
              let records = try? JSONDecoder().decode([NativeProjectRecord].self, from: data)
        else { return [] }
        return records
    }

    private func nativeProjects(
        excludingPaths existing: Set<String>, excludingIDs existingIDs: Set<String> = []
    ) -> [Project] {
        let normalizedExisting = Set(existing.map(Self.normalizedProjectPath))
        return loadNativeProjects()
            .filter { record in
                // The file's copy of a mirrored record wins (same id).
                guard !existingIDs.contains(record.id) else { return false }
                // Child records (groups share the parent's path by design)
                // skip the path dedup; top-level records dedupe by path.
                return record.parentProjectID != nil
                    || !normalizedExisting.contains(Self.normalizedProjectPath(record.path))
            }
            .map { record in
                Project(
                    id: record.id,
                    name: record.name,
                    path: record.path,
                    parentProjectID: record.parentProjectID,
                    sortOrder: Int.max, // append after Tauri-ordered projects
                    isFolder: record.isFolder,
                    worktreeBranch: record.worktreeBranch,
                    workspacesEnabled: nil,
                    mcpBlocked: nil
                )
            }
    }

    /// Footer "+" — Add Project (Sidebar.svelte:568-581 → App.svelte
    /// handleAddProject:1099-1120): folder picker, then the project appears
    /// in the tree. Stored natively; never written to app-state.json.
    func addProjectFolder() {
        guard selectedHostScope == .local else { return }
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.title = "Select project folder"
        panel.prompt = "Add Project"
        panel.begin { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            Task { @MainActor in
                // Open the newly added project's launcher by default.
                self?.openLauncher(forFolder: url.path)
            }
        }
    }

    @discardableResult
    func addProjectFolders(from providers: [NSItemProvider]) -> Bool {
        guard selectedHostScope == .local else { return false }
        let fileURLProviders = providers.filter {
            $0.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier)
        }
        guard !fileURLProviders.isEmpty else { return false }

        for provider in fileURLProviders {
            provider.loadItem(
                forTypeIdentifier: UTType.fileURL.identifier,
                options: nil
            ) { [weak self] item, _ in
                guard let url = Self.fileURL(fromDropItem: item) else { return }
                Task { @MainActor [weak self] in
                    self?.addDroppedProjectFolder(url)
                }
            }
        }

        return true
    }

    /// Reuse-or-add a permanent native project for `path`, returning its id.
    /// Reuses any existing project (app-state, native, or worktree) whose
    /// path matches; otherwise adds a permanent native project like the
    /// "+ Add Project" button (App.svelte:1113-1118 "already added" guard).
    @discardableResult
    func ensureProject(path: String) -> String {
        let normalizedPath = Self.normalizedProjectPath(path)
        if let existing = projectsByID.values.first(where: {
            Self.normalizedProjectPath($0.path) == normalizedPath
        }) {
            return existing.id
        }
        var records = loadNativeProjects()
        if let existing = records.first(where: {
            Self.normalizedProjectPath($0.path) == normalizedPath
        }) {
            return existing.id
        }
        let id = "native-\(UUID().uuidString.lowercased())"
        records.append(NativeProjectRecord(
            id: id,
            name: URL(fileURLWithPath: normalizedPath).lastPathComponent,
            path: normalizedPath
        ))
        if let data = try? JSONEncoder().encode(records) {
            AppDefaults.shared.set(data, forKey: Self.nativeProjectsKey)
            mirrorProjectsToSharedState()
        }
        rescan()
        return id
    }

    private func addDroppedProjectFolder(_ url: URL) {
        // The asynchronous NSItemProvider load may complete after the user
        // switches Hosts. Recheck at the mutation boundary, not only when
        // the drop was accepted.
        guard selectedHostScope == .local else { return }
        guard url.isFileURL else { return }
        let path = Self.normalizedProjectPath(url.path)
        var isDirectory = ObjCBool(false)
        guard FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory),
              isDirectory.boolValue
        else { return }

        let projectID = ensureProject(path: path)
        settingsVisible = false
        archivedProjectID = nil
        expandedProjectIDs.insert(projectID)
    }

    nonisolated private static func normalizedProjectPath(_ path: String) -> String {
        URL(fileURLWithPath: path)
            .standardizedFileURL
            .resolvingSymlinksInPath()
            .path
    }

    nonisolated private static func fileURL(fromDropItem item: NSSecureCoding?) -> URL? {
        if let url = item as? URL {
            return url
        }
        if let data = item as? Data,
           let string = String(data: data, encoding: .utf8) {
            return URL(string: string)
        }
        if let string = item as? String {
            if let url = URL(string: string) {
                return url
            }
            return URL(fileURLWithPath: string)
        }
        return nil
    }

    /// Finder "New Unpeel Session Here" service entry point: reuse-or-add the
    /// project for `path`, then show the main-screen session launcher for it
    /// (the user picks a tool there). The launcher gives way to the terminal
    /// as soon as a tile is launched.
    func openLauncher(forFolder path: String) {
        // Covers Finder Services, the folder picker, and delayed callbacks.
        // Remote scope must never inspect/mirror a Controller-local path.
        guard selectedHostScope == .local else { return }
        let projectID = ensureProject(path: path)
        settingsVisible = false
        archivedProjectID = nil
        selectedSessionID = nil
        expandedProjectIDs.insert(projectID)
        launcherProjectID = projectID
    }

    /// Project context-menu destination for archived sessions. This is a
    /// main-pane library, not another sidebar accordion.
    func openArchivedSessions(projectID: String) {
        guard displayProjectsByID[projectID] != nil else { return }
        settingsVisible = false
        launcherProjectID = nil
        recentActivityVisible = false
        archivedProjectID = projectID
        // Remote projects fetch their archive library from the Host
        // (`session.archive.list`); the page renders from the cached fetch.
        if remoteProjectSummariesByID[projectID] != nil {
            refreshRemoteArchivedSessions(projectID: projectID)
        }
    }

    func closeArchivedSessions() {
        archivedProjectID = nil
    }

    /// "All recent" destination from the titlebar/menu-bar activity
    /// dropdowns: the app-wide history page in the main pane, same shell as
    /// the archived-sessions library.
    func openRecentActivity() {
        guard selectedHostScope == .local else { return }
        settingsVisible = false
        launcherProjectID = nil
        archivedProjectID = nil
        recentActivityVisible = true
    }

    func closeRecentActivity() {
        recentActivityVisible = false
    }

    func toggleRecentActivity() {
        if recentActivityVisible {
            closeRecentActivity()
        } else {
            openRecentActivity()
        }
    }

    // MARK: - Ephemeral projects (verification only)

    /// In-memory projects for hook verification (UNPEEL_TEST_LAUNCH with a
    /// `path:` spec). Never persisted anywhere, so other Unpeel instances
    /// and future launches see no residue.
    private var ephemeralProjects: [Project] = []

    @discardableResult
    func addEphemeralProject(path: String) -> String {
        if let existing = ephemeralProjects.first(where: { $0.path == path }) {
            return existing.id
        }
        // Deterministic id so sessions launched into this project in an
        // earlier run still resolve to it after an app restart.
        let slug = path.lowercased().map { $0.isLetter || $0.isNumber ? $0 : "-" }
        let id = "ephemeral-\(String(slug))"
        ephemeralProjects.append(Project(
            id: id,
            name: URL(fileURLWithPath: path).lastPathComponent,
            path: path,
            parentProjectID: nil,
            sortOrder: Int.max,
            isFolder: nil,
            worktreeBranch: nil,
            workspacesEnabled: nil,
            mcpBlocked: nil
        ))
        rescan()
        return id
    }

    /// Ephemeral worktree CHILD project (verification of the worktrees link
    /// row / spinner without touching any real project). Adding the child is
    /// enough for the parent's link row to render — worktrees show purely by
    /// existence. In-memory only, like addEphemeralProject.
    @discardableResult
    func addEphemeralWorktreeProject(
        parentPath: String, path: String, branch: String
    ) -> String {
        let parentID = addEphemeralProject(path: parentPath)
        if let existing = ephemeralProjects.first(where: { $0.path == path }) {
            return existing.id
        }
        let slug = path.lowercased().map { $0.isLetter || $0.isNumber ? $0 : "-" }
        let id = "ephemeral-\(String(slug))"
        ephemeralProjects.append(Project(
            id: id,
            name: URL(fileURLWithPath: path).lastPathComponent,
            path: path,
            parentProjectID: parentID,
            sortOrder: Int.max,
            isFolder: nil,
            worktreeBranch: branch,
            workspacesEnabled: nil,
            mcpBlocked: nil
        ))
        rescan()
        return id
    }

    // MARK: - Derived

    var selectedSession: SessionEntry? {
        selectedSessionID.flatMap { displaySessionsByID[$0] }
    }

    /// Sessions that currently own visible work: row spinners show for
    /// starting/busy sessions, plus restart placeholders while a relaunch is
    /// in flight.
    var activeJobSessions: [SessionEntry] {
        var result: [SessionEntry] = []
        var seen = Set<String>()
        func append(_ session: SessionEntry) {
            guard !seen.contains(session.id) else { return }
            seen.insert(session.id)
            result.append(session)
        }
        func collect(_ nodes: [ProjectNode]) {
            for node in nodes {
                for session in node.sessions
                where session.status == .starting || session.status == .busy {
                    append(session)
                }
                collect(node.worktrees)
            }
        }
        collect(nodes)
        for id in restartingSessionIDs.sorted() {
            if let session = sessionsByID[id] {
                append(session)
            }
        }
        return Self.sessionsSortedByRecentActivity(
            result,
            restartingSessionIDs: restartingSessionIDs
        )
    }

    var activeJobCount: Int {
        activeJobSessions.count
    }

    /// Product-facing status word for the activity dropdowns (titlebar + menu
    /// bar). Rows may still choose to render this as a spinner or unread dot.
    func activityStatusLabel(for session: SessionEntry) -> String {
        if restartingSessionIDs.contains(session.id) {
            return session.isLive ? "Restarting" : "Resuming"
        }
        switch session.activityStatus(unread: unreadSessionIDs.contains(session.id)) {
        case .starting: return "Starting"
        case .working: return "Working"
        case .blocked: return "Blocked"
        case .done: return "Done"
        case .idle: return "Idle"
        case .exited: return "Exited"
        }
    }

    /// Display name for a project id, used by the activity dropdowns.
    /// Plain group folders carry the full path — Project › Folder — matching
    /// the titlebar; worktrees keep their own name (the branch identifies
    /// them elsewhere, and the parent prefix would just be noise here).
    func activityProjectName(_ id: String) -> String {
        guard let project = projectsByID[id] else { return "Unknown project" }
        if project.worktreeBranch == nil,
           let parentID = project.parentProjectID,
           let parent = projectsByID[parentID] {
            return "\(parent.name) › \(project.name)"
        }
        return project.name
    }

    /// Sessions that recently settled with an unread badge (the #60a5fa dot)
    /// and are no longer doing visible work. Surfaced in the titlebar
    /// activity popover beneath the active jobs as "recently finished".
    var unreadJobSessions: [SessionEntry] {
        let active = Set(activeJobSessions.map(\.id))
        var result: [SessionEntry] = []
        var seen = Set<String>()
        func collect(_ nodes: [ProjectNode]) {
            for node in nodes {
                for session in node.sessions
                where unreadSessionIDs.contains(session.id)
                    && !active.contains(session.id)
                    && !seen.contains(session.id) {
                    seen.insert(session.id)
                    result.append(session)
                }
                collect(node.worktrees)
            }
        }
        collect(nodes)
        return result
    }

    /// Title for the titlebar: project of the selected session (or first
    /// project). Worktree projects display as "parent / worktree".
    /// The project whose name/branch the titlebar shows: the selected
    /// session's project. Nil when nothing is selected — the titlebar then
    /// shows the app name with no branch.
    private var titlebarProject: Project? {
        guard let s = selectedSession else { return nil }
        return displayProjectsByID[s.projectID]
    }

    var titlebarSegments: [String] {
        // Remote scope leads with the Host's name; the rest of the path is
        // built exactly like the local titlebar.
        let hostPrefix: [String] = selectedHostScope == .local
            ? []
            : [remoteHostRuntime.snapshot?.macName
                ?? remoteHostStore.selectedRecord?.name
                ?? "Remote Host"]
        // No session selected: just the app/Host name, no project/branch.
        guard let project = titlebarProject else {
            return hostPrefix.isEmpty ? ["Unpeel"] : hostPrefix
        }
        // Worktree projects show only the parent name; the worktree itself
        // is identified by the muted branch suffix instead of its name.
        if let parentID = project.parentProjectID,
           let parent = displayProjectsByID[parentID] {
            if project.worktreeBranch != nil {
                return hostPrefix + [parent.name]
            }
            // A plain group folder has no branch suffix to identify it, so
            // the title carries the full path: Project › Group.
            return hostPrefix + [parent.name, project.name]
        }
        return hostPrefix + [project.name]
    }

    /// Muted current-branch suffix for the titlebar. Worktree projects know
    /// their branch from the model; a plain git project resolves the branch
    /// checked out at its path off-main (cached) so the titlebar never
    /// blocks on git. Nil for non-git projects.
    @Published private(set) var titlebarBranchName: String?

    /// Whether the titlebar branch belongs to a worktree (drives which branch
    /// glyph the titlebar shows: "split" for worktrees, "git-branch" else).
    @Published private(set) var titlebarBranchIsWorktree = false

    /// Guards async branch resolution against stale results after a switch.
    private var titlebarBranchPath: String?

    func refreshTitlebarBranch() {
        guard let project = titlebarProject else {
            titlebarBranchPath = nil
            if titlebarBranchName != nil { titlebarBranchName = nil }
            return
        }
        // Remote scope: the Host resolved the branch already; a Controller
        // must never run git against a Host-side path.
        if let summary = remoteProjectSummariesByID[project.id] {
            titlebarBranchPath = nil
            let isWorktree = summary.worktreeBranch != nil
            let branch = summary.worktreeBranch ?? summary.gitBranch
            if titlebarBranchIsWorktree != isWorktree { titlebarBranchIsWorktree = isWorktree }
            if titlebarBranchName != branch { titlebarBranchName = branch }
            return
        }
        // Worktree: branch is already in the model, no git call needed.
        if let branch = project.worktreeBranch {
            titlebarBranchPath = project.path
            if !titlebarBranchIsWorktree { titlebarBranchIsWorktree = true }
            if titlebarBranchName != branch { titlebarBranchName = branch }
            return
        }
        if titlebarBranchIsWorktree { titlebarBranchIsWorktree = false }
        guard UnpeelStore.isGitRepo(path: project.path) else {
            titlebarBranchPath = nil
            if titlebarBranchName != nil { titlebarBranchName = nil }
            return
        }
        // Plain git project: resolve HEAD off-main, keeping the prior value
        // visible until the new one arrives (no flash) and dropping stale
        // results if the title project changed meanwhile.
        let path = project.path
        titlebarBranchPath = path
        DispatchQueue.global(qos: .userInitiated).async {
            let branch = WorktreeGit.currentBranch(repoPath: path)
            DispatchQueue.main.async { [weak self] in
                guard let self, self.titlebarBranchPath == path else { return }
                if self.titlebarBranchName != branch { self.titlebarBranchName = branch }
            }
        }
    }

    // MARK: - Sidebar session lists (shared with ProjectNodeView)

    /// Whether this session is hidden from the regular sidebar lists.
    /// Archives whose host stop is still in flight stay visible (as a muted
    /// "archiving…" row) until the stop completes.
    private func isHiddenArchived(_ sessionID: String) -> Bool {
        // Remote rows: the Host already filtered/windowed the bootstrap, so
        // a Controller never applies its own local archive overlay to them.
        if remoteSummariesByID[sessionID] != nil { return false }
        return archivedSessionIDs.contains(sessionID)
            && !archivingSessionIDs.contains(sessionID)
    }

    /// Pins resolved against the node's sessions, mirroring
    /// resolvedPinnedItems in ProjectItem.svelte:405-423.
    func pinnedSessions(in node: ProjectNode) -> [SessionEntry] {
        // Remote nodes: pins are Host-resolved flags on the summaries, in
        // the Host's own row order; an in-flight drag previews on top.
        if remoteProjectSummariesByID[node.id] != nil {
            let pinned = node.sessions.filter { remoteSummariesByID[$0.id]?.pinned == true }
            guard let preview = sessionOrderPreviews[node.id] else { return pinned }
            var rank: [String: Int] = [:]
            for (index, id) in preview.enumerated() { rank[id] = index }
            return pinned.sorted {
                (rank[$0.id] ?? Int.max, $0.id) < (rank[$1.id] ?? Int.max, $1.id)
            }
        }
        return (pinnedByProject[node.id] ?? []).compactMap { pin in
            // Pin wins over archive: a pinned session keeps its row in the
            // pinned section even when archived (the affordances swap to
            // Restore via `isArchived`). Archiving must never empty the
            // pinned section — Archive became the stop verb in beta.27, so
            // filtering archived pins here made "stop a pinned session"
            // silently remove its row while the phone kept showing it.
            guard let id = pin.sessionID else { return nil }
            return node.sessions.first { $0.id == id }
        }
    }

    /// Regular sessions exclude pinned and archived ones. Archived sessions
    /// stay out of the sidebar entirely and live in the project's main-pane
    /// archive library.
    func regularSessions(in node: ProjectNode) -> [SessionEntry] {
        let pinnedIDs = Set(pinnedSessions(in: node).map(\.id))
        return node.sessions.filter {
            !pinnedIDs.contains($0.id) && !isHiddenArchived($0.id)
        }
    }

    /// This project's archived sessions, in the node's (newest-first +
    /// reorder-overlay) order. In-flight archives are excluded until their
    /// host stop completes (they still render as a sidebar row).
    func archivedSessions(in node: ProjectNode) -> [SessionEntry] {
        node.sessions.filter { isHiddenArchived($0.id) }
    }

    func archivedSessions(projectID: String) -> [SessionEntry] {
        // Remote projects: the fetched Host archive library (empty until the
        // fetch lands or when the Host has none).
        if remoteProjectSummariesByID[projectID] != nil {
            return remoteArchivedByProject[projectID] ?? []
        }
        return localArchivedSessions(projectID: projectID)
    }

    /// Local-only archive lookup for the /mobile Host serving path, which
    /// must keep serving THIS Mac's archives even while a remote Host is
    /// selected in the picker.
    func localArchivedSessions(projectID: String) -> [SessionEntry] {
        guard let node = findNode(projectID) else { return [] }
        return archivedSessions(in: node)
    }

    /// The sidebar's row list for a group: pins first, then live sessions,
    /// then the five most recently stopped/archived sessions. Recently
    /// updated uses the shared lifecycle rank inside each section; Custom
    /// keeps manual order for live rows. Selected, unread, and in-flight
    /// stopped rows always stay past the window.
    func displayedSessions(in node: ProjectNode) -> [SessionEntry] {
        sidebarLists(in: node).displayed
    }

    func renderedPinnedSessions(in node: ProjectNode) -> [SessionEntry] {
        sidebarLists(in: node).pinned
    }

    func renderedDisplayedSessions(in node: ProjectNode) -> [SessionEntry] {
        sidebarLists(in: node).displayed
    }

    /// Flat session rows plus the stopped-only projection used for windowing
    /// and auto-archive. The array-of-arrays shape remains because those
    /// paths historically operated on blocks; each block is one session now.
    private func sidebarSessionBlocks(
        in node: ProjectNode
    ) -> (ordered: [[SessionEntry]], stopped: [[SessionEntry]]) {
        let pinnedRenderedIDs = Set(pinnedSessions(in: node).map(\.id))
        // Archived rows remain eligible for the five-row stopped preview at
        // the bottom of the sidebar. Pins stay in their separate section and
        // win over archive.
        let candidates = node.sessions.filter {
            !pinnedRenderedIDs.contains($0.id)
        }
        // The drag-reorder overlay must survive the recency sort, or a hand-
        // dragged row snaps straight back. Rows absent from the overlay stay
        // newest-first above the hand-ordered block, mirroring
        // `applySessionOrderOverlay`.
        // An in-flight desktop drag previews in memory. Otherwise the shared
        // file wins so a TUI (or any other frontend) drag shows up here; the
        // local overlay remains the fallback for installs that predate it.
        // Date sort ignores every manual-order source (drags are disabled
        // for the group, so no preview can exist either).
        let isRemoteNode = remoteProjectSummariesByID[node.id] != nil
        let dateSorted = isRemoteNode
            ? remoteProjectSummariesByID[node.id]?.dateSorted == true
            : dateSortedProjectIDs.contains(node.id)
        // Remote nodes never consult local shared order files: the Host's
        // bootstrap row order IS the committed order, and an in-flight drag
        // previews on top of it exactly like local.
        let manualOrder = dateSorted
            ? []
            : isRemoteNode
                ? sessionOrderPreviews[node.id]
                    ?? remoteSessionOrderByProject[node.id]
                    ?? []
                : sessionOrderPreviews[node.id]
                    ?? Self.sharedSessionOrder(projectID: node.id)
                    ?? AppDefaults.shared.stringArray(forKey: Self.sessionOrderKey(node.id))
                    ?? []
        var ordered = orderedSessions(candidates, manualOrder: manualOrder)
        if dateSorted {
            ordered = Self.sessionsSortedByRecentActivity(
                ordered,
                restartingSessionIDs: restartingSessionIDs
            )
        }
        let blocks = ordered.map { [$0] }
        var active: [[SessionEntry]] = []
        var stopped: [[SessionEntry]] = []
        for block in blocks {
            // A restarting (Resume clicked) block is active-in-waiting: it
            // moves to its active-group position immediately instead of
            // sitting in the stopped group until the replacement spawns —
            // restart stabilizes created_at, so this IS its final spot.
            let isActive = block.contains {
                $0.isLive || restartingSessionIDs.contains($0.id)
            }
            if isActive {
                active.append(block)
            } else {
                stopped.append(block)
            }
        }
        // Stopped/archived rows always form the bottom preview. Rank that
        // section by the lifecycle event which stopped the host, with an
        // explicit archive stamp as an optional newer filing event.
        let recency: ([SessionEntry]) -> Int64 = { [archivedAtBySession] block in
            let lifecycle = block.map {
                max($0.createdAt, $0.lifecycleAtMs ?? 0)
            }.max() ?? 0
            let stamped = block.compactMap { archivedAtBySession[$0.id] }.max() ?? 0
            return max(lifecycle, stamped)
        }
        let indexed = stopped.enumerated().sorted { a, b in
            let ra = recency(a.element)
            let rb = recency(b.element)
            if ra != rb { return ra > rb }
            return a.offset < b.offset
        }
        let sortedStopped = indexed.map(\.element)
        return (active + sortedStopped, sortedStopped)
    }

    /// Whether a stopped row must stay in the sidebar past the recent
    /// window (and stay exempt from the overflow auto-archive).
    private func stoppedBlockMustStayVisible(_ block: [SessionEntry]) -> Bool {
        block.contains { session in
            session.id == selectedSessionID
                || sessionIsUnread(session.id)
                || sidebarKeepVisibleSessionIDs.contains(session.id)
                || archivingSessionIDs.contains(session.id)
                || removingSessionIDs.contains(session.id)
                || restartingSessionIDs.contains(session.id)
                // Archive-page confirms render on the archive page only; no
                // need to drag the row into the sidebar for them.
                || (confirmingRemoveSessionID == session.id
                    && confirmingRemoveSurface == .sidebar)
                || confirmingArchiveSessionID == session.id
                || editingSessionID == session.id
        }
    }

    /// Memoized per-project sidebar row lists. Every store publish re-runs
    /// each visible ProjectNodeView body, which asked for both lists —
    /// ordering plus a UserDefaults read per project per render pass. The
    /// inputs only change on tree/pin rebuilds and the explicit mutations
    /// that call `invalidateSidebarLists()`.
    private func sidebarLists(
        in node: ProjectNode
    ) -> (pinned: [SessionEntry], displayed: [SessionEntry]) {
        if let cached = sidebarListsCache[node.id] { return cached }
        let pinned = pinnedSessions(in: node)
        let (ordered, stopped) = sidebarSessionBlocks(in: node)
        var visibleStoppedIDs = Set<String>()
        for (index, block) in stopped.enumerated()
        where index < Self.sidebarVisibleSessionLimit
            || stoppedBlockMustStayVisible(block) {
            visibleStoppedIDs.formUnion(block.map(\.id))
        }
        // Preserve the live-then-stopped section boundary while the stopped-
        // only projection decides which preview rows fit.
        let displayedBlocks = ordered.filter { block in
            block.contains {
                $0.isLive
                    || restartingSessionIDs.contains($0.id)
                    || visibleStoppedIDs.contains($0.id)
            }
        }
        let rendered = displayedBlocks.flatMap { $0 }
        sidebarListsCache[node.id] = (pinned, rendered)
        return (pinned, rendered)
    }

    /// Stopped blocks past the recent window that nothing is holding
    /// visible — the overflow the auto-archive sweep files away.
    private func stoppedOverflowBlocks(in node: ProjectNode) -> [[SessionEntry]] {
        let stopped = sidebarSessionBlocks(in: node).stopped
        guard stopped.count > Self.sidebarVisibleSessionLimit else { return [] }
        return stopped[Self.sidebarVisibleSessionLimit...].filter {
            !stoppedBlockMustStayVisible($0)
        }
    }

    func invalidateSidebarLists() {
        sidebarListsCache.removeAll(keepingCapacity: true)
    }

    private func orderedSessions(
        _ sessions: [SessionEntry],
        manualOrder: [String]
    ) -> [SessionEntry] {
        guard !manualOrder.isEmpty else {
            return sessions.sorted { $0.createdAt > $1.createdAt }
        }
        var rank: [String: Int] = [:]
        for (index, id) in manualOrder.enumerated() { rank[id] = index }
        return sessions.sorted { a, b in
            switch (rank[a.id], rank[b.id]) {
            case let (ra?, rb?): return ra < rb
            case (nil, .some): return true
            case (.some, nil): return false
            case (nil, nil): return a.createdAt > b.createdAt
            }
        }
    }

    // MARK: - Pane pre-warming (native-only; no Svelte counterpart)

    /// Sessions whose Ghostty pane should be created and replayed ahead of
    /// selection (mounted hidden by WarmPaneHostView). Fed by sidebar hover
    /// intent and by the first ⌘1–9 targets while ⌘ is held; ordered oldest
    /// first, capped, pruned to live sessions on rescan.
    @Published private(set) var prewarmSessionIDs: [String] = []

    static let prewarmLimit = 3

    /// Request a warm pane for a live, not-currently-shown session.
    func prewarmSession(_ sessionID: String) {
        guard sessionID != selectedSessionID,
              let session = sessionsByID[sessionID], session.isAttachable,
              !removingSessionIDs.contains(sessionID),
              !restartingSessionIDs.contains(sessionID)
        else { return }
        var ids = prewarmSessionIDs.filter { $0 != sessionID }
        ids.append(sessionID)
        if ids.count > Self.prewarmLimit {
            ids.removeFirst(ids.count - Self.prewarmLimit)
        }
        if ids != prewarmSessionIDs {
            prewarmSessionIDs = ids
        }
    }

    // MARK: - ⌘1–9 session switching (ProjectItem.svelte:502-528, 680-755)

    /// SESSION_SHORTCUT_LIMIT — at most ⌘1…⌘9.
    static let sessionShortcutLimit = 9

    /// True while ⌘ is held and the app is frontmost; visible session rows
    /// of the shortcut project show ⌘N hints in place of the age
    /// (`showCommandShortcuts` in ProjectItem.svelte).
    @Published private(set) var commandHintsVisible = false

    private var shortcutKeyMonitor: Any?
    private var shortcutFlagsMonitor: Any?

    /// The project whose rows answer ⌘1–9: the selected session's project,
    /// else the first top-level project (the Svelte `isActive` project; the
    /// same fallback the titlebar and collapsed "+" use).
    var shortcutProjectID: String? {
        if let session = selectedSession { return session.projectID }
        return displayNodes.first?.project.id
    }

    /// Pinned rows first, then the displayed (truncation-aware) regular
    /// rows, skipping in-flight restart/remove rows, capped at 9 — exactly
    /// `sessionShortcutTargets`. Empty while settings covers the workspace
    /// (sessionShortcutsEnabled gate, App.svelte:1410) or while the project
    /// is collapsed (`showSessionList` gate).
    var sessionShortcutTargets: [SessionEntry] {
        guard !settingsVisible,
              let projectID = shortcutProjectID,
              expandedProjectIDs.contains(projectID),
              let node = findDisplayNode(projectID)
        else { return [] }
        let rows = renderedPinnedSessions(in: node) + renderedDisplayedSessions(in: node)
        return Array(
            rows.filter {
                !restartingSessionIDs.contains($0.id)
                    && !removingSessionIDs.contains($0.id)
            }
            .prefix(Self.sessionShortcutLimit)
        )
    }

    /// [session id: 1-based ⌘ index] for one project's rows — empty unless
    /// the hints are showing and the project is the shortcut target, so the
    /// sidebar can render `⌘N` without recomputing targets per row.
    func sessionShortcutHintIndices(forProject projectID: String) -> [String: Int] {
        guard commandHintsVisible, projectID == shortcutProjectID else { return [:] }
        var indices: [String: Int] = [:]
        for (offset, session) in sessionShortcutTargets.enumerated() {
            indices[session.id] = offset + 1
        }
        return indices
    }

    /// Local NSEvent monitors for ⌘1–9 + the held-⌘ hint state. Installed
    /// once by the app delegate on the app's real store — never on the
    /// throwaway stores the snapshot self-tests create.
    func installSessionShortcutMonitors() {
        guard shortcutKeyMonitor == nil else { return }
        shortcutFlagsMonitor = NSEvent.addLocalMonitorForEvents(
            matching: .flagsChanged
        ) { [weak self] event in
            // Local monitors fire on the main thread before dispatch.
            MainActor.assumeIsolated {
                guard let self else { return }
                let flags = event.modifierFlags
                    .intersection(.deviceIndependentFlagsMask)
                let held = flags.contains(.command)
                let targets = self.sessionShortcutTargets
                let visible = held && !targets.isEmpty
                if self.commandHintsVisible != visible {
                    self.commandHintsVisible = visible
                }
                // Holding ⌘ telegraphs a switch: warm the most likely target
                // (just one — surface creation is synchronous main-thread
                // work, and a 3-pane burst caused visible stalls).
                if visible, let first = targets.first {
                    self.prewarmSession(first.id)
                }
                // ⌃ drives the project hints (delayed — ⌃C etc. are constant
                // terminal traffic) and commits an open ⌃Tab switcher on
                // release.
                if flags.contains(.control) {
                    self.scheduleControlHints()
                } else {
                    if self.sessionSwitcher != nil { self.commitSessionSwitcher() }
                    self.setControlHintsVisible(false)
                }
            }
            return event
        }
        shortcutKeyMonitor = NSEvent.addLocalMonitorForEvents(
            matching: .keyDown
        ) { [weak self] event in
            let consumed = MainActor.assumeIsolated { () -> Bool in
                guard let self else { return false }
                if self.handleShortcutKeyDown(event) { return true }
                self.noteTerminalKeystroke()
                return false
            }
            return consumed ? nil : event
        }
    }

    /// Record that the user just typed into the focused terminal, so the
    /// output heuristic can treat the resulting echo as input rather than
    /// agent work (input-aware busy suppression). Skipped while a text field
    /// holds focus (rename editor, settings) — those keystrokes never reach a
    /// terminal surface, so they must not arm echo suppression.
    private func noteTerminalKeystroke() {
        guard let sessionID = observedSessionID, editingSessionID == nil else { return }
        if NSApp.keyWindow?.firstResponder is NSTextView { return }
        lastUserInputAt[sessionID] = Date()
    }

    /// True when recent output growth for `sessionID` is most likely the echo
    /// of the user's own keystrokes rather than agent work: it is the observed
    /// session and a keystroke landed within the echo window. Continuous typing
    /// keeps re-arming this, so the spinner stays suppressed throughout typing
    /// and clears `inputEchoWindow` after the last keystroke — by which point
    /// any sustained output is genuine agent work.
    private func outputIsLikelyEcho(_ sessionID: String, now: Date) -> Bool {
        guard sessionID == observedSessionID,
              let typedAt = lastUserInputAt[sessionID] else { return false }
        return now.timeIntervalSince(typedAt) <= inputEchoWindow
    }

    /// True when the event was consumed as an app shortcut (⌘K palette,
    /// ⌘T terminal, ⌘1–9 sessions, ⌃1–9 projects, ⌃Tab MRU switcher).
    /// While the palette is open its own monitor owns the list keys; only
    /// ⌘K (close) is handled here.
    private func handleShortcutKeyDown(_ event: NSEvent) -> Bool {
        // Remote Ghostty owns its complete key stream. Local command palette,
        // launch, session, and project shortcuts must neither consume those
        // keys nor mutate the still-loaded Local workspace behind it.
        guard selectedHostScope == .local else { return false }
        let mods = event.modifierFlags
            .intersection([.command, .option, .control, .shift])
        let char = event.charactersIgnoringModifiers?.lowercased()

        // ⌘K/⌘T live in the Session menu for discoverability, but the
        // focused Ghostty surface claims key equivalents that collide with
        // ghostty keybindings before the menu sees them (AppTerminalView.
        // performKeyEquivalent) — so the monitor owns the actual keys.
        if mods == .command, char == "k" {
            toggleCommandPalette()
            return true
        }

        guard !commandPaletteVisible else { return false }

        if mods == .command, char == "t" {
            guard !settingsVisible, editingSessionID == nil else { return false }
            if NSApp.keyWindow?.firstResponder is NSTextView { return false }
            launchDefaultTerminal()
            return true
        }

        // ⌃Tab / ⌃⇧Tab — MRU session switcher (kVK_Tab = 48). Commit
        // happens when ⌃ is released (flags monitor above).
        if event.keyCode == 48, mods == .control || mods == [.control, .shift] {
            guard !settingsVisible, editingSessionID == nil else { return false }
            cycleSessionSwitcher(backward: mods.contains(.shift))
            return true
        }
        // Esc cancels an armed switcher without switching (kVK_Escape = 53).
        if sessionSwitcher != nil, event.keyCode == 53 {
            cancelSessionSwitcher()
            return true
        }

        if mods == .command { return handleSessionShortcutKeyDown(event) }
        if mods == .control { return handleProjectShortcutKeyDown(event) }
        return false
    }

    /// True when the event was consumed as a session shortcut.
    private func handleSessionShortcutKeyDown(_ event: NSEvent) -> Bool {
        // ⌘ alone — ⌥/⌃/⇧ chords pass through untouched
        // (sessionShortcutIndexFromEvent, ProjectItem.svelte:697-701).
        guard event.modifierFlags
            .intersection([.command, .option, .control, .shift]) == .command
        else { return false }
        guard !settingsVisible, editingSessionID == nil else { return false }
        // Digits typed into a focused text field (rename editor uses
        // isEditing above, but settings/sheets may have fields too) keep
        // their normal meaning — the field editor is an NSTextView
        // (shouldIgnoreSessionShortcutTarget parity).
        if NSApp.keyWindow?.firstResponder is NSTextView { return false }
        guard let digit = Self.shortcutDigit(for: event),
              (1...Self.sessionShortcutLimit).contains(digit)
        else { return false }
        let targets = sessionShortcutTargets
        guard digit <= targets.count else { return false }
        selectedSessionID = targets[digit - 1].id
        return true
    }

    /// Physical digit-row and keypad key codes (kVK_ANSI_1…9 / Keypad1…9):
    /// the fallback for layouts where digits are shifted characters (AZERTY)
    /// — the Svelte handler accepts `Digit1-9`/`Numpad1-9` codes the same
    /// way (ProjectItem.svelte:707-713).
    private static let digitKeyCodes: [UInt16: Int] = [
        18: 1, 19: 2, 20: 3, 21: 4, 23: 5, 22: 6, 26: 7, 28: 8, 25: 9,
        83: 1, 84: 2, 85: 3, 86: 4, 87: 5, 88: 6, 89: 7, 91: 8, 92: 9,
    ]

    private static func shortcutDigit(for event: NSEvent) -> Int? {
        if let characters = event.charactersIgnoringModifiers,
           characters.count == 1, let digit = Int(characters) {
            return digit
        }
        return digitKeyCodes[event.keyCode]
    }

    // MARK: - ⌃1–9 project switching

    /// True while ⌃ has been held long enough (see `scheduleControlHints`)
    /// and the app is frontmost; top-level project rows show ⌃N hints.
    @Published private(set) var controlHintsVisible = false
    private var controlHintsWorkItem: DispatchWorkItem?

    /// Top-level (non-folder) sidebar projects answering ⌃1–9, in sidebar
    /// order — the project mirror of `sessionShortcutTargets`.
    var projectShortcutTargets: [ProjectNode] {
        guard selectedHostScope == .local, !settingsVisible else { return [] }
        return Array(
            nodes.filter { $0.project.isFolder != true }
                .prefix(Self.sessionShortcutLimit)
        )
    }

    /// 1-based ⌃ index for one project row — nil unless the hints are
    /// showing and the project is a target.
    func projectShortcutHintIndex(forProject projectID: String) -> Int? {
        guard controlHintsVisible else { return nil }
        return projectShortcutTargets
            .firstIndex { $0.id == projectID }
            .map { $0 + 1 }
    }

    /// ⌃N: expand the project and select its most recently used session
    /// (fallback: first rendered row). A project with no sessions opens the
    /// main-screen launcher instead, so ⌃N is never a dead keystroke.
    func focusProject(_ projectID: String) {
        guard selectedHostScope == .local else { return }
        guard let node = findNode(projectID) else { return }
        expandedProjectIDs.insert(projectID)
        let rows = renderedPinnedSessions(in: node) + renderedDisplayedSessions(in: node)
        let target = sessionMRU.first { id in rows.contains { $0.id == id } }
            ?? rows.first?.id
        if let target {
            revealSessionInSidebar(target)
        } else {
            settingsVisible = false
            archivedProjectID = nil
            selectedSessionID = nil
            launcherProjectID = projectID
        }
    }

    /// True when the event was consumed as a project shortcut (⌃ alone).
    private func handleProjectShortcutKeyDown(_ event: NSEvent) -> Bool {
        guard !settingsVisible, editingSessionID == nil else { return false }
        if NSApp.keyWindow?.firstResponder is NSTextView { return false }
        guard let digit = Self.shortcutDigit(for: event),
              (1...Self.sessionShortcutLimit).contains(digit)
        else { return false }
        let targets = projectShortcutTargets
        guard digit <= targets.count else { return false }
        focusProject(targets[digit - 1].id)
        return true
    }

    /// ⌃ is constant terminal traffic (⌃C, ⌃R, …), so unlike the instant ⌘
    /// hints the ⌃ project hints only appear after the modifier has been
    /// held ~a third of a second on its own.
    private func scheduleControlHints() {
        guard controlHintsWorkItem == nil, !controlHintsVisible else { return }
        let item = DispatchWorkItem { [weak self] in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.controlHintsWorkItem = nil
                guard NSEvent.modifierFlags.contains(.control),
                      self.sessionSwitcher == nil,
                      !self.projectShortcutTargets.isEmpty
                else { return }
                self.controlHintsVisible = true
            }
        }
        controlHintsWorkItem = item
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35, execute: item)
    }

    private func setControlHintsVisible(_ visible: Bool) {
        controlHintsWorkItem?.cancel()
        controlHintsWorkItem = nil
        if controlHintsVisible != visible { controlHintsVisible = visible }
    }

    // MARK: - ⌃Tab MRU session switcher

    /// Selected-session history, newest first, capped. Feeds the ⌃Tab
    /// switcher and ⌃N's "most recent session in this project". Ids are
    /// validated against `sessionsByID` at read time, so no prune hook is
    /// needed. In-memory only, by design (like the Svelte selection).
    private(set) var sessionMRU: [String] = []
    private static let sessionMRULimit = 24

    private func noteSessionMRU(_ id: String) {
        sessionMRU.removeAll { $0 == id }
        sessionMRU.insert(id, at: 0)
        if sessionMRU.count > Self.sessionMRULimit {
            sessionMRU.removeLast(sessionMRU.count - Self.sessionMRULimit)
        }
    }

    struct SessionSwitcherState: Equatable {
        var sessionIDs: [String]
        var index: Int
    }

    /// Non-nil while ⌃Tab is cycling (⌃ still held). RootView renders the
    /// overlay from this; releasing ⌃ commits, Esc cancels.
    @Published private(set) var sessionSwitcher: SessionSwitcherState?

    /// Sidebar-visible sessions: MRU first, then tree order, capped at 9 —
    /// a switcher longer than that stops being glanceable.
    private var sessionSwitcherCandidates: [String] {
        var treeOrder: [String] = []
        func walk(_ node: ProjectNode) {
            for session in renderedPinnedSessions(in: node)
                + renderedDisplayedSessions(in: node) {
                treeOrder.append(session.id)
            }
            node.worktrees.forEach(walk)
        }
        nodes.forEach(walk)
        let valid = Set(treeOrder)
        var ordered: [String] = []
        var seen = Set<String>()
        for id in sessionMRU where valid.contains(id) && seen.insert(id).inserted {
            ordered.append(id)
        }
        for id in treeOrder where seen.insert(id).inserted {
            ordered.append(id)
        }
        return Array(ordered.prefix(Self.sessionShortcutLimit))
    }

    func cycleSessionSwitcher(backward: Bool) {
        if var state = sessionSwitcher {
            let count = state.sessionIDs.count
            guard count > 0 else { return }
            state.index = (state.index + (backward ? count - 1 : 1)) % count
            sessionSwitcher = state
            prewarmSession(state.sessionIDs[state.index])
        } else {
            let ids = sessionSwitcherCandidates
            guard ids.count > 1 else { return }
            let index = backward ? ids.count - 1 : 1
            sessionSwitcher = SessionSwitcherState(sessionIDs: ids, index: index)
            prewarmSession(ids[index])
        }
    }

    func commitSessionSwitcher() {
        guard let state = sessionSwitcher else { return }
        sessionSwitcher = nil
        guard state.sessionIDs.indices.contains(state.index) else { return }
        revealSessionInSidebar(state.sessionIDs[state.index])
    }

    func cancelSessionSwitcher() {
        sessionSwitcher = nil
    }

    // MARK: - ⌘K command palette

    /// Whether the palette overlay is up. The palette view owns its own
    /// query/selection state and key monitor; the store only hosts
    /// visibility so the menu item and Esc agree.
    @Published var commandPaletteVisible = false

    func toggleCommandPalette() {
        guard selectedHostScope == .local else { return }
        commandPaletteVisible.toggle()
    }

    // MARK: - Launching

    /// Session ▸ New Session (⌘N): launch the leading favorite preset in the
    /// selected session's project, falling back to the first sidebar project.
    /// Prefers an agent favorite over the blank-terminal pseudo-preset so ⌘N
    /// means "new agent session" whenever any agent CLI is favorited.
    func launchDefaultSession() {
        guard selectedHostScope == .local else { return }
        guard let projectID = defaultLaunchProjectID else { return }
        let preset = quickPresets.first { !$0.command.isEmpty } ?? .newTerminal
        launchSession(projectID: projectID, command: preset.command)
    }

    /// Session ▸ New Terminal (⌘T): a plain shell in the same project ⌘N
    /// targets — the blank-terminal pseudo-preset's keyboard path.
    func launchDefaultTerminal() {
        guard selectedHostScope == .local else { return }
        guard let projectID = defaultLaunchProjectID else { return }
        launchSession(projectID: projectID, command: "")
    }

    /// The project ⌘N/⌘T launch into: the selected session's project,
    /// falling back to the first sidebar project. The ⌘K palette reads it
    /// too, for its "New session: <preset>" rows.
    var defaultLaunchProjectID: String? {
        let projectID = selectedSessionID.flatMap { sessionsByID[$0]?.projectID }
            ?? nodes.first(where: { $0.project.isFolder != true })?.id
        guard let projectID, projectsByID[projectID] != nil else { return nil }
        return projectID
    }

    /// Writes a launch file and spawns unpeel-host detached, then polls for
    /// the manifest (≤2s) and selects the new session.
    func launchSession(projectID: String, command: String) {
        // Remote scope: session creation is a Host operation. The matching
        // Host preset id is preferred so the Host resolves its own catalog;
        // a bare command travels as-is. No local spawn can happen here.
        if remoteProjectSummariesByID[projectID] != nil {
            let presetID = remotePresetSummaries.first { $0.command == command }?.id
            performRemoteVerb("Couldn't start the session") { runtime in
                try await runtime.createSession(
                    projectID: projectID,
                    presetID: presetID,
                    command: presetID == nil ? command : nil
                )
            }
            return
        }
        guard let project = projectsByID[projectID] else { return }
        let worktreePath = project.worktreeBranch == nil ? nil : project.path
        spawnSession(
            projectID: projectID,
            command: command,
            label: command.isEmpty ? "Terminal" : command,
            customTitle: false,
            createdAt: Int64(Date().timeIntervalSince1970 * 1000),
            cwd: project.path,
            worktreePath: worktreePath,
            worktreeBranch: project.worktreeBranch
        )
    }

    /// Core spawn (spawn_session parity): label/custom_title/created_at and
    /// the worktree target are caller-controlled so restart can carry the
    /// original session's identity over. Foreground launches focus the new
    /// session; MCP launches can pass `activateUI: false` to run in the
    /// background without stealing the user's current view. Returns the new
    /// session id, or nil when the launch file/host spawn failed.
    @discardableResult
    func spawnSession(
        projectID: String,
        command: String,
        label: String,
        customTitle: Bool,
        createdAt: Int64,
        cwd: String,
        worktreePath: String?,
        worktreeBranch: String?,
        spawnedBy: String? = nil,
        role: String? = nil,
        task: String? = nil,
        accessGrant: McpGrant? = nil,
        activateUI: Bool = true
    ) -> String? {
        guard LocalExecutionPolicy.permits(.spawnSession, in: selectedHostScope) else {
            assertionFailure(
                "Refusing to spawn a local session while remote Host scope is selected"
            )
            return nil
        }
        if activateUI {
            // UI launches land in the workspace (Svelte startSessionOrToast
            // navigates back from the settings shell view too). Background
            // MCP launches deliberately avoid changing selection or settings.
            settingsVisible = false
        }
        let sessionID = UUID().uuidString.lowercased()

        let payload: [String: Any] = [
            "session": [
                "id": sessionID,
                "project_id": projectID,
                "label": label,
                "custom_title": customTitle,
                "command": command,
                "created_at": createdAt,
                "tag_id": NSNull(),
                "worktree_path": worktreePath ?? NSNull() as Any,
                "worktree_branch": worktreeBranch ?? NSNull() as Any,
                "spawned_by": spawnedBy ?? NSNull() as Any,
                "role": role ?? NSNull() as Any,
                "task": task ?? NSNull() as Any,
            ],
            "cwd": cwd,
            "dark_mode": currentAppDarkMode(),
            // Defense in depth: the Rust host validates this before it writes
            // a manifest or refreshes provider hook assets. Legacy launch
            // files omit it and decode as local.
            "execution_scope": selectedHostScope.sessionLaunchWireValue,
            // unpeel-host exports UNPEEL_APP_PORT / UNPEEL_SESSION_ID
            // into the session when hook_port is set
            // (integrations/mod.rs configure_host_command:109-122), which is
            // what makes provider hook scripts post back to this instance.
            "hook_port": hookServer.map { Int($0.port) } ?? NSNull() as Any,
            // Foreground provider launches should wait until the first
            // Ghostty attach client is relaying input before the CLI starts.
            // Codex probes terminal colors at startup; if it starts before
            // attach, those probes get replay-muted and the composer loses
            // terminal-background-aware styling. Background MCP sessions skip
            // the wait so they can start without a visible surface.
            "wait_for_attach": activateUI,
            // Launch the PTY at the terminal area's current grid so the
            // workload's first paint already matches the surface. A guessed
            // grid makes full-screen TUIs (codex especially) draw at the
            // wrong size and bet on repainting off the attach client's
            // corrective resize — a SIGWINCH they sometimes miss during
            // startup, leaving a broken layout until the user resizes the
            // window. Fallback for the first-ever launch, before any pane
            // has reported a grid.
            "initial_cols": GhosttyTerminalPane.lastDisplayedGrid?.cols ?? 120,
            "initial_rows": GhosttyTerminalPane.lastDisplayedGrid?.rows ?? 32,
            // Register the Sessions MCP for capable providers only while the
            // experimental feature is on (Settings ▸ Experimental); the host's
            // per-call gate enforces Read vs Read/create/write. Project blocks
            // still suppress the client.
            "mcp_enabled": isExperimentalEnabled(.sessionsMcp) && !projectMcpBlocked(projectID),
            // Advertise the `browser` domain while the experimental feature
            // is on and Browser access isn't Off (Allow is the default; Ask
            // gates per call with an approval alert, like computer use).
            "browser_mcp_enabled": isExperimentalEnabled(.browserMcp)
                && browserDefaultAccess != .off,
            // Advertise the `computer` domain only while the experimental
            // feature is on and Computer access isn't Off. The Ask/Allow
            // policy and the per-call gate handle everything after launch;
            // the engine daemon itself is app-owned (ComputerEngineManager).
            "computer_mcp_enabled": UnpeelFeatureFlags.isEnabled(.computerUse)
                && computerDefaultAccess != .off,
        ]

        let launchFile = FileManager.default.temporaryDirectory
            .appendingPathComponent("unpeel-launch-\(sessionID).json")
        do {
            let data = try JSONSerialization.data(withJSONObject: payload)
            try data.write(to: launchFile)
        } catch {
            NSLog("[UnpeelNative] failed to write launch file: \(error)")
            return nil
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: LaunchConfig.hostBinary)
        process.arguments = [launchFile.path]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            NSLog("[UnpeelNative] failed to spawn unpeel-host: \(error)")
            try? FileManager.default.removeItem(at: launchFile)
            return nil
        }

        let pending = SessionEntry(
            id: sessionID,
            projectID: projectID,
            label: label,
            command: command,
            createdAt: createdAt,
            status: .starting,
            customTitle: customTitle,
            worktreePath: worktreePath,
            worktreeBranch: worktreeBranch,
            spawnedBy: spawnedBy,
            role: role,
            task: task
        )
        pendingSessions[sessionID] = pending
        announceStateChange("lifecycle")
        publishPendingSessions()
        logActivity(.started, sessionID: sessionID)
        if activateUI {
            selectedSessionID = sessionID
            // Reveal the new session's project/group — and every ancestor, so a
            // session added into a worktree/group child isn't left hidden behind
            // a collapsed parent header.
            var ancestorID = projectsByID[projectID]?.parentProjectID
            while let id = ancestorID {
                expandedProjectIDs.insert(id)
                ancestorID = projectsByID[id]?.parentProjectID
            }
            expandedProjectIDs.insert(projectID)
        }

        // Poll for the manifest, then replace the pending row with the real
        // attachable session.
        let manifestURL = LaunchConfig.appSessionsDir
            .appendingPathComponent(sessionID)
            .appendingPathComponent("manifest.json")
        Task { @MainActor [weak self] in
            for _ in 0..<40 {
                try? await Task.sleep(nanoseconds: 50_000_000)
                if FileManager.default.fileExists(atPath: manifestURL.path) {
                    self?.rescan()
                    return
                }
            }
            NSLog("[UnpeelNative] session \(sessionID) manifest never appeared")
            guard let self, var pending = self.pendingSessions[sessionID] else { return }
            pending.status = .exited
            self.pendingSessions[sessionID] = pending
            self.publishPendingSessions()
        }
        return sessionID
    }
}

// MARK: - Remote Host scope: display projection and verb plumbing

extension UnpeelStore {
    // MARK: Display accessors (the single seam the views read)

    private func remoteSummary(for sessionID: String) -> RemoteSessionSummary? {
        remoteSummariesByID[sessionID] ?? remoteArchivedSummaryCache[sessionID]
    }

    /// The sidebar tree for the selected Host scope. Identical shape in both
    /// scopes so the views never branch.
    var displayNodes: [ProjectNode] {
        selectedHostScope == .local ? nodes : remoteNodes
    }

    var displaySessionsByID: [String: SessionEntry] {
        selectedHostScope == .local ? sessionsByID : remoteSessionsByID
    }

    var displayProjectsByID: [String: Project] {
        selectedHostScope == .local ? projectsByID : remoteProjectsByID
    }

    var displayAvailablePresets: [Preset] {
        selectedHostScope == .local ? availablePresets : remotePresets
    }

    var displayQuickPresetGroups: [QuickPresetGroup] {
        selectedHostScope == .local ? quickPresetGroups : remoteQuickPresetGroups
    }

    /// Unread badge for one row, whichever Host owns it.
    func sessionIsUnread(_ sessionID: String) -> Bool {
        if let summary = remoteSummary(for: sessionID) { return summary.unread }
        return unreadSessionIDs.contains(sessionID)
    }

    /// An archived row that still renders in the five-row stopped preview (or
    /// because it is pinned); its affordances swap to Restore.
    func sessionIsRecentArchived(_ sessionID: String) -> Bool {
        if let summary = remoteSummary(for: sessionID) { return summary.archived }
        return archivedSessionIDs.contains(sessionID)
            && !archivingSessionIDs.contains(sessionID)
    }

    /// Archive-library size for the project context menu. Remote projects
    /// carry the Host-computed count; local counts the archived rows.
    func archivedSessionCount(in node: ProjectNode) -> Int {
        if let summary = remoteProjectSummariesByID[node.id] {
            return summary.archivedSessionCount ?? 0
        }
        return archivedSessions(in: node).count
    }

    /// Scope-neutral node lookup for verbs that operate on the displayed tree.
    func findDisplayNode(_ projectID: String) -> ProjectNode? {
        func search(_ nodes: [ProjectNode]) -> ProjectNode? {
            for node in nodes {
                if node.id == projectID { return node }
                if let found = search(node.worktrees) { return found }
            }
            return nil
        }
        return search(displayNodes)
    }

    /// Host-resolved terminal canvas color for a remote row (nil locally).
    func remoteTerminalBackgroundColor(for sessionID: String) -> NSColor? {
        guard let hex = remoteSummariesByID[sessionID]?.terminalBackgroundHex,
              (0 ... 0xFF_FF_FF).contains(hex)
        else { return nil }
        return NSColor(hex: UInt32(hex))
    }

    /// Host-resolved spinner tint for a remote row (nil locally).
    func remoteSpinnerColor(for sessionID: String) -> Color? {
        guard let hex = remoteSummariesByID[sessionID]?.spinnerColorHex,
              (0 ... 0xFF_FF_FF).contains(hex)
        else { return nil }
        return Color(hex: UInt32(hex))
    }

    /// Whether new-session affordances should offer anything for a project.
    /// Remote projects require the Host's `session.create` operation.
    func canCreateSessions(inProject projectID: String) -> Bool {
        if remoteProjectSummariesByID[projectID] != nil {
            return remoteHostRuntime.supportsHostOperation(
                RemoteHostRuntime.HostOperation.create
            )
        }
        return true
    }

    // MARK: Projection

    /// Rebuild the remote display projection from the runtime's latest
    /// bootstrap. Local truth is untouched: the projection is a parallel,
    /// in-memory view (never persisted, never served to paired phones).
    func projectRemoteScope(snapshot: RemoteBootstrapSnapshot?) {
        guard selectedHostScope != .local else { return }
        guard let snapshot else {
            // The runtime clears its snapshot only for a fresh selection or
            // a Host switch — never during a same-Host reconnect (the last
            // valid snapshot stays published there). Clear the projection so
            // a different Host's rows can never linger under the new scope.
            if remoteHostRuntime.snapshot == nil {
                clearRemoteScopeProjectionState()
                invalidateSidebarLists()
            }
            return
        }

        var projects: [Project] = []
        var projectSummaries: [String: RemoteProjectSummary] = [:]
        for (index, summary) in snapshot.projects.enumerated() {
            // Legacy folder membership renders flat, like the local sidebar.
            let parentID = summary.parentProjectID
            projects.append(Project(
                id: summary.id,
                name: summary.name,
                path: summary.path,
                parentProjectID: parentID,
                sortOrder: index,
                isFolder: summary.isGroup == true ? true : nil,
                worktreeBranch: summary.worktreeBranch,
                workspacesEnabled: nil,
                mcpBlocked: summary.mcpBlocked ? true : nil
            ))
            projectSummaries[summary.id] = summary
        }
        let knownProjectIDs = Set(projects.map(\.id))

        var summaries: [String: RemoteSessionSummary] = [:]
        var entries: [SessionEntry] = []
        var orderByProject: [String: [String]] = [:]
        for summary in snapshot.sessions {
            summaries[summary.id] = summary
            entries.append(Self.sessionEntry(fromRemote: summary))
            if knownProjectIDs.contains(summary.projectID) {
                orderByProject[summary.projectID, default: []].append(summary.id)
            }
        }

        remoteProjectSummariesByID = projectSummaries
        remoteSummariesByID = summaries
        remoteSessionOrderByProject = orderByProject
        remotePresetSummaries = snapshot.presets

        // Presets: the Host's enabled list, in Host order; stars become the
        // same quick-launch chips the local strip renders.
        let presets = snapshot.presets
            .filter(\.enabled)
            .map { summary in
                Preset(
                    id: summary.id,
                    label: summary.label,
                    command: summary.command,
                    enabled: summary.enabled,
                    quickLaunch: summary.quickLaunch
                )
            }
        remotePresets = presets
        remoteQuickPresetGroups = Self.quickPresetGroups(from: presets)

        // Build the tree with the exact local algorithm: top-level projects,
        // inline child folders (worktrees + groups), sessions in Host order
        // (bootstrap row order IS the Host's committed order).
        var byProject: [String: [SessionEntry]] = [:]
        for entry in entries where knownProjectIDs.contains(entry.projectID) {
            byProject[entry.projectID, default: []].append(entry)
        }
        var childrenOf: [String: [Project]] = [:]
        var topLevel: [Project] = []
        for project in projects {
            if let parent = project.parentProjectID, knownProjectIDs.contains(parent) {
                childrenOf[parent, default: []].append(project)
            } else {
                topLevel.append(project)
            }
        }
        // A committed-but-unconfirmed reorder holds its order until the
        // snapshot itself carries it (or the hold expires) — drop the hold
        // as soon as the Host's natural order matches, so the Host stays
        // truth for every later change.
        if projectOrderPreview == nil, let hold = remoteCommittedOrderHold {
            let siblings: [Project]
            if let parent = hold.parentID {
                siblings = (childrenOf[parent] ?? [])
                    .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
            } else {
                siblings = topLevel.sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
            }
            let siblingIDs = Set(siblings.map(\.id))
            let natural = siblings.map(\.id).filter { hold.ids.contains($0) }
            let held = hold.ids.filter { siblingIDs.contains($0) }
            if natural == held || Date().timeIntervalSince(hold.heldAt) > 15 {
                remoteCommittedOrderHold = nil
            }
        }
        // An in-flight project drag preview outranks the snapshot order for
        // its sibling set — the same precedence the local overlay path gives
        // `projectOrderPreview` — so remote rows animate live during a drag.
        // Behind it, the committed hold keeps the dropped order stable across
        // stale bootstraps. Never persisted; a confirming bootstrap is truth.
        let orderPreview: (parentID: String?, ids: [String])? =
            projectOrderPreview.map { ($0.parentID, $0.ids) }
                ?? remoteCommittedOrderHold.map { ($0.parentID, $0.ids) }
        func applyOrderPreview(_ base: [Project], parentID: String?) -> [Project] {
            guard let orderPreview, orderPreview.parentID == parentID else { return base }
            var rank: [String: Int] = [:]
            for (index, id) in orderPreview.ids.enumerated() { rank[id] = index }
            let ordered = base.filter { rank[$0.id] != nil }
                .sorted { rank[$0.id]! < rank[$1.id]! }
            guard !ordered.isEmpty else { return base }
            let rest = base.filter { rank[$0.id] == nil }
            return ordered + rest
        }
        func node(for project: Project) -> ProjectNode {
            let childProjects = applyOrderPreview(
                (childrenOf[project.id] ?? [])
                    .filter { $0.worktreeBranch != nil || $0.isFolder == true }
                    .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) },
                parentID: project.id
            )
            return ProjectNode(
                project: project,
                sessions: byProject[project.id] ?? [],
                worktrees: childProjects.map { node(for: $0) }
            )
        }
        let newNodes = applyOrderPreview(
            topLevel.sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) },
            parentID: nil
        ).map { node(for: $0) }

        remoteProjectsByID = Dictionary(uniqueKeysWithValues: projects.map { ($0.id, $0) })
        remoteSessionsByID = Dictionary(uniqueKeysWithValues: entries.map { ($0.id, $0) })
        if newNodes != remoteNodes {
            remoteNodes = newNodes
        }
        invalidateSidebarLists()

        // Entering this Host: swap the expansion set to the Host's own
        // persisted state so open/closed folders are remembered per Host.
        // First visit ever (nothing stored): open every root project once,
        // so selecting a Host never lands on an all-collapsed tree.
        let hostKey = snapshot.macID ?? selectedHostScope.remoteHostID ?? "remote"
        if remoteAutoExpandedHostKey != hostKey {
            remoteAutoExpandedHostKey = hostKey
            remoteRevealedSelectionID = nil
            remoteCommittedOrderHold = nil
            expandedProjectsStorageKey = Self.expandedProjectsKey + "." + hostKey
            if let stored = AppDefaults.shared.stringArray(forKey: expandedProjectsStorageKey) {
                expandedProjectIDs = Set(stored)
            } else {
                expandedProjectIDs = Set(newNodes.map(\.id))
            }
        }
        // Keep the selected session's project reachable — but only when the
        // selection changes. The projection also re-runs on every drag-preview
        // hover, and re-expanding a deliberately collapsed project mid-drag
        // pops it open under the cursor.
        if let selected = selectedSessionID,
           selected != remoteRevealedSelectionID,
           let projectID = remoteSessionsByID[selected]?.projectID {
            remoteRevealedSelectionID = selected
            var reveal: Set<String> = [projectID]
            var parent = remoteProjectsByID[projectID]?.parentProjectID
            var hops = 0
            while let current = parent, hops < 16 {
                reveal.insert(current)
                parent = remoteProjectsByID[current]?.parentProjectID
                hops += 1
            }
            if !reveal.isSubset(of: expandedProjectIDs) {
                expandedProjectIDs.formUnion(reveal)
            }
        }

        // Adopt the runtime's selection (it owns default selection).
        if selectedSessionID != remoteHostRuntime.selectedSessionID {
            selectedSessionID = remoteHostRuntime.selectedSessionID
        }
        // A removed/archived-away id can linger in the archived cache; prune
        // fetched libraries for projects that vanished from the Host.
        remoteArchivedByProject = remoteArchivedByProject.filter {
            knownProjectIDs.contains($0.key)
        }
        remoteArchivedSummaryCache.retainProjects(knownProjectIDs)
        refreshTitlebarBranch()
    }

    func clearRemoteScopeProjection() {
        clearRemoteScopeProjectionState()
        remoteAutoExpandedHostKey = nil
        remoteRevealedSelectionID = nil
        remoteCommittedOrderHold = nil
        // Back to Local: restore the local expansion set from its own key.
        if expandedProjectsStorageKey != Self.expandedProjectsKey {
            expandedProjectsStorageKey = Self.expandedProjectsKey
            expandedProjectIDs =
                Set(AppDefaults.shared.stringArray(forKey: Self.expandedProjectsKey) ?? [])
        }
        invalidateSidebarLists()
    }

    private func clearRemoteScopeProjectionState() {
        remoteNodes = []
        remoteSessionsByID = [:]
        remoteProjectsByID = [:]
        remoteSummariesByID = [:]
        remoteArchivedSummaryCache.removeAll()
        remoteProjectSummariesByID = [:]
        remoteSessionOrderByProject = [:]
        remoteArchivedByProject = [:]
        remotePresetSummaries = []
        remotePresets = []
        remoteQuickPresetGroups = []
    }

    private static func sessionEntry(fromRemote summary: RemoteSessionSummary) -> SessionEntry {
        let status: SessionStatus
        switch summary.status {
        case .exited:
            status = .exited
        case .running:
            switch summary.activity {
            case .starting: status = .starting
            case .working: status = .busy
            case .blocked: status = .attention
            case .done, .idle, .unknown: status = .idle
            }
        }
        var entry = SessionEntry(
            id: summary.id,
            projectID: summary.projectID,
            label: summary.title.isEmpty ? summary.command : summary.title,
            command: summary.command,
            createdAt: summary.createdAtUnixMs,
            status: status,
            activeRuntimeID: summary.activeRuntimeID,
            runtimeLaunchPending: summary.runtimeLaunchPending,
            lifecycleAtMs: max(
                summary.createdAtUnixMs,
                summary.updatedAtUnixMs ?? 0
            )
        )
        entry.worktreePath = summary.worktreePath
        entry.worktreeBranch = summary.worktreeBranch
        return entry
    }

    /// Quick-strip groups from a flat preset list — same rules as the local
    /// strip: starred presets grouped per CLI, flat-list order.
    private static func quickPresetGroups(from presets: [Preset]) -> [QuickPresetGroup] {
        var groups: [SetupTool: [Preset]] = [:]
        var order: [SetupTool] = []
        for preset in presets where preset.quickLaunch {
            guard let cli = SetupTool.detect(in: preset.command) else { continue }
            if groups[cli] == nil { order.append(cli) }
            groups[cli, default: []].append(preset)
        }
        return order.compactMap { cli in
            guard let presets = groups[cli], !presets.isEmpty else { return nil }
            return QuickPresetGroup(cli: cli, presets: presets)
        }
    }

    // MARK: Remote verbs

    /// Run one user-initiated remote verb; failures surface through the
    /// app's normal error alert with the runtime's failure message. Nothing
    /// is ever retried automatically (an outcome-unknown effect already
    /// triggered a bootstrap refresh inside the runtime).
    func performRemoteVerb(
        _ failureTitle: String,
        onFailure: (@MainActor () -> Void)? = nil,
        _ operation: @escaping @MainActor (RemoteHostRuntime) async throws -> Void
    ) {
        let runtime = remoteHostRuntime
        Task { @MainActor in
            do {
                try await operation(runtime)
            } catch {
                onFailure?()
                let message = (error as? RemoteHostVerbError)?.message
                    ?? error.localizedDescription
                Self.showRemoteVerbFailure(title: failureTitle, message: message)
            }
        }
    }

    private static func showRemoteVerbFailure(title: String, message: String) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.alertStyle = .warning
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }

    /// Repaint the sidebar after an in-flight drag preview changed. Remote
    /// nodes only need the list caches busted (the preview overlay is read
    /// at render time); local rows re-apply the order overlay over the last
    /// scan exactly as before.
    func refreshAfterOrderPreviewChange(projectID: String) {
        if remoteProjectSummariesByID[projectID] != nil {
            withAnimation(.easeInOut(duration: 0.18)) {
                invalidateSidebarLists()
                objectWillChange.send()
            }
            return
        }
        withAnimation(.easeInOut(duration: 0.18)) { rebuildTreeFromLastScan() }
    }

    /// Fetch (or refresh) one remote project's archive library.
    func refreshRemoteArchivedSessions(projectID: String) {
        let pageGeneration = remoteArchivePageGeneration
        let hostID = selectedHostScope.remoteHostID
        performRemoteVerb("Couldn't load archived sessions") { [weak self] runtime in
            let sessions = try await runtime.archivedSessions(projectID: projectID)
            guard let self,
                  self.selectedHostScope.remoteHostID == hostID,
                  self.archivedProjectID == projectID,
                  self.remoteArchivePageGeneration == pageGeneration
            else { return }
            self.remoteArchivedSummaryCache.replaceProject(
                projectID,
                summaries: sessions
            )
            self.remoteArchivedByProject[projectID] = sessions.map {
                Self.sessionEntry(fromRemote: $0)
            }
        }
    }
}
