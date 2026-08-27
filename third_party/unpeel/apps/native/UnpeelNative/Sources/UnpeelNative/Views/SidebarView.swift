//
//  SidebarView.swift
//  UnpeelNative
//
//  The project/session tree (DESIGN.md §4): project rows, session rows,
//  pinned sessions, worktree child projects, the worktrees slide-in view,
//  and the footer strip.
//
//  Motion values are extracted from the Svelte app:
//  - Worktrees slide-in: fly x ±140, 200ms cubicOut (Sidebar.svelte:465,491)
//  - Accordion open: height 340ms cubic-bezier(0.16,1,0.3,1); content fade
//    220ms ease-out from translateY(-6) scale(0.992) (ProjectItem.svelte:2225-2262)
//  - Accordion close: 240ms cubicInOut, content fade 140ms (ProjectItem.svelte:568-613)
//  - Row entrance: 380ms cubic-bezier(0.18,0.86,0.26,1), stagger 14ms/row,
//    from translate(-5,-4) scale(0.988) (ProjectItem.svelte:2264-2270, 2468-2482)
//

import AppKit
import SwiftUI
import UniformTypeIdentifiers

// MARK: - Drag-to-reorder state

/// Tracks the in-flight sidebar drag (one at a time): either a project/worktree
/// sibling row or a regular session row. The Svelte app reorders projects with
/// svelte-dnd-action (Sidebar.svelte:492-503 → reorder_projects); session
/// dragging there MOVES a session to another project — within-project session
/// reordering is native-only.
///
/// `.onDrag` has no "drag ended" callback, so a drag cancelled outside any
/// drop target would leave the source row dimmed forever; `arm()` installs
/// a one-shot local event monitor that cancels the in-memory reorder on the
/// next normal mouse/key event (none are delivered while the drag session is
/// active). A successful drop calls `finish()` instead, which persists once.
@MainActor
final class SidebarDragState: ObservableObject {
    struct SessionDrag: Equatable {
        let projectID: String
        let sessionID: String
        /// Whether the drag started from a pinned row. A pinned drag only
        /// reorders over other pinned rows, and a regular drag only over
        /// regular rows (the drop delegates gate on this).
        let pinned: Bool
    }

    @Published private(set) var projectID: String?
    @Published private(set) var sessionDrag: SessionDrag?
    /// Group row currently accepting the in-flight session drag. Worktree
    /// rows never become targets: moving a running session there would imply
    /// changing its working directory, which requires an explicit restart.
    @Published private(set) var sessionDropTargetProjectID: String?

    /// Number of rows currently reporting a Finder folder drag hovering over
    /// them. A counter (not a Bool) so that moving between adjacent rows —
    /// where AppKit may fire the new row's `dropEntered` before the old row's
    /// `dropExited` — never dips to zero and flickers the drop highlight off.
    @Published private(set) var folderHoverCount = 0

    private var monitor: Any?
    private var commitReorder: (() -> Void)?
    private var cancelReorder: (() -> Void)?

    var isActive: Bool { projectID != nil || sessionDrag != nil }

    var isFolderHovered: Bool { folderHoverCount > 0 }

    func folderHoverEnter() { folderHoverCount += 1 }
    func folderHoverExit() { folderHoverCount = max(0, folderHoverCount - 1) }
    func folderHoverReset() { folderHoverCount = 0 }

    func beginProject(
        _ id: String,
        commitReorder: @escaping () -> Void,
        cancelReorder: @escaping () -> Void
    ) {
        end()
        projectID = id
        self.commitReorder = commitReorder
        self.cancelReorder = cancelReorder
        arm()
    }

    func beginSession(
        projectID: String,
        sessionID: String,
        pinned: Bool,
        commitReorder: @escaping () -> Void,
        cancelReorder: @escaping () -> Void
    ) {
        end()
        sessionDrag = SessionDrag(
            projectID: projectID, sessionID: sessionID, pinned: pinned
        )
        self.commitReorder = commitReorder
        self.cancelReorder = cancelReorder
        arm()
    }

    func setSessionDropTarget(_ projectID: String, hovering: Bool) {
        if hovering {
            sessionDropTargetProjectID = projectID
        } else if sessionDropTargetProjectID == projectID {
            sessionDropTargetProjectID = nil
        }
    }

    /// Complete a drop accepted by the sidebar: both drag kinds persist
    /// their in-memory preview exactly once here.
    func finish() {
        let commit = commitReorder
        commitReorder = nil
        cancelReorder = nil
        clear()
        commit?()
    }

    /// Cancel a drag that ended outside an accepting drop target. Any
    /// preview is removed so the rows return to their last persisted order.
    func end() {
        let cancel = cancelReorder
        commitReorder = nil
        cancelReorder = nil
        clear()
        cancel?()
    }

    private func clear() {
        projectID = nil
        sessionDrag = nil
        sessionDropTargetProjectID = nil
        disarm()
    }

    private func arm() {
        disarm()
        monitor = NSEvent.addLocalMonitorForEvents(
            matching: [.leftMouseDown, .mouseMoved, .keyDown]
        ) { [weak self] event in
            Task { @MainActor in self?.end() }
            return event
        }
    }

    private func disarm() {
        if let monitor {
            NSEvent.removeMonitor(monitor)
            self.monitor = nil
        }
    }
}

/// Row-level drop delegate that previews a live reorder on hover (the moving
/// rows provide the target gap) and commits it when the drop lands.
///
/// It also accepts a Finder **folder** dragged directly onto the row as an
/// "add project" drop. A row must register a UTI to claim its area, and a
/// Finder drag exposes a plain-text path alongside its file URL — so without
/// this the row would swallow the folder drag and reject it (the internal
/// reorder isn't active), and the drop would never fall through to the list's
/// `.onDrop(of: [.fileURL])`. That is why folders only dropped in the gaps
/// between rows before.
private struct SidebarReorderDropDelegate: DropDelegate {
    /// Whether an internal sidebar reorder is in flight (vs. a foreign drag).
    let isReorderActive: () -> Bool
    /// Returns true when the hovered row accepted the dragged id.
    let moveOver: () -> Bool
    /// A session may be dropped on a plain group row. This is deliberately a
    /// separate path from live reorder: filing happens on release, not merely
    /// because the pointer crossed the row.
    let isSessionMoveActive: () -> Bool
    let setSessionMoveHover: (Bool) -> Void
    let performSessionMove: () -> Bool
    /// Successful reorder drop: persist the drag's preview (session or
    /// project) exactly once, then clear the drag state.
    let finishReorder: () -> Void
    /// A cross-project session drop is a move, not a reorder. Discard any
    /// origin-list preview before clearing the drag state.
    let cancelReorder: () -> Void
    /// Toggles the list-wide folder-drop highlight while a folder hovers.
    let setFolderHover: (Bool) -> Void
    /// Adds the dragged folders as projects; returns true if any were accepted.
    let addFolders: ([NSItemProvider]) -> Bool

    /// A foreign Finder folder drag (not our own reorder) carrying a file URL.
    private func isFolderDrop(_ info: DropInfo) -> Bool {
        !isReorderActive()
            && !isSessionMoveActive()
            && info.hasItemsConforming(to: [.fileURL])
    }

    func validateDrop(info: DropInfo) -> Bool {
        isReorderActive() || isSessionMoveActive() || isFolderDrop(info)
    }

    func dropEntered(info: DropInfo) {
        if isReorderActive() {
            _ = moveOver()
        } else if isSessionMoveActive() {
            setSessionMoveHover(true)
        } else if isFolderDrop(info) {
            setFolderHover(true)
        }
    }

    func dropExited(info: DropInfo) {
        if isSessionMoveActive() {
            setSessionMoveHover(false)
        } else if isFolderDrop(info) {
            setFolderHover(false)
        }
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        if isReorderActive() || isSessionMoveActive() {
            return DropProposal(operation: .move)
        }
        if isFolderDrop(info) { return DropProposal(operation: .copy) }
        return DropProposal(operation: .cancel)
    }

    func performDrop(info: DropInfo) -> Bool {
        if isSessionMoveActive() {
            setSessionMoveHover(false)
            let moved = performSessionMove()
            cancelReorder()
            return moved
        }
        if isReorderActive() {
            finishReorder()
            return true
        }
        guard isFolderDrop(info) else { return false }
        setFolderHover(false)
        return addFolders(info.itemProviders(for: [.fileURL]))
    }
}

/// Container-level fallback covering the whole sidebar list area. Without
/// it, the gaps BETWEEN rows (LazyVStack spacing), non-draggable rows and
/// the empty space below the tree have no drop delegate, so AppKit falls
/// back to the default `.copy` proposal — the green "+" badge on the
/// cursor. During an active sidebar drag this proposes `.move` everywhere
/// (the live preview already happened in the row delegates' dropEntered);
/// foreign drags (e.g. files from Finder) are not claimed.
private struct SidebarContainerDropDelegate: DropDelegate {
    /// Whether a sidebar row drag is in flight.
    let isDragActive: () -> Bool
    let finish: () -> Void

    func validateDrop(info _: DropInfo) -> Bool { isDragActive() }

    func dropUpdated(info _: DropInfo) -> DropProposal? {
        DropProposal(operation: isDragActive() ? .move : .cancel)
    }

    func performDrop(info _: DropInfo) -> Bool {
        let active = isDragActive()
        finish()
        return active
    }
}

/// Invisible 1×1 drag preview: hides the floating row-snapshot ghost so the
/// only drag feedback is the live gap animation in the list itself (macOS
/// renders whatever preview view we hand it — a clear pixel reads as none).
private struct EmptyDragPreview: View {
    var body: some View {
        Color.clear.frame(width: 1, height: 1)
    }
}

// MARK: - Motion constants (Svelte parity)

enum SidebarMotion {
    /// Svelte `fly` default easing is cubicOut ≈ cubic-bezier(0.33, 1, 0.68, 1).
    static let slide = Animation.timingCurve(0.33, 1, 0.68, 1, duration: 0.2)
    /// Accordion open: 340ms cubic-bezier(0.16, 1, 0.3, 1).
    static let accordionOpen = Animation.timingCurve(0.16, 1, 0.3, 1, duration: 0.34)
    /// Accordion close: 240ms cubicInOut ≈ cubic-bezier(0.65, 0, 0.35, 1).
    static let accordionClose = Animation.timingCurve(0.65, 0, 0.35, 1, duration: 0.24)
    /// Row entrance: 380ms cubic-bezier(0.18, 0.86, 0.26, 1).
    static func rowEnter(index: Int) -> Animation {
        .timingCurve(0.18, 0.86, 0.26, 1, duration: 0.38)
            .delay(Double(index) * 0.014)
    }

    static var reduceMotion: Bool {
        NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
    }
}

/// Session-row entrance: opacity 0 → 1, translate(-5px, -4px) → 0,
/// scale 0.988 → 1 (ProjectItem.svelte @keyframes session-list-item-enter).
private struct SessionRowEnterModifier: ViewModifier {
    let active: Bool

    func body(content: Content) -> some View {
        content
            .opacity(active ? 0 : 1)
            .scaleEffect(active ? 0.988 : 1, anchor: .topLeading)
            .offset(x: active ? -5 : 0, y: active ? -4 : 0)
    }
}

/// Session-list content fade: opacity + translateY(-6) scale(0.992)
/// (ProjectItem.svelte .session-list.native-accordion-list).
private struct SessionListContentModifier: ViewModifier {
    let active: Bool

    func body(content: Content) -> some View {
        content
            .opacity(active ? 0 : 1)
            .scaleEffect(active ? 0.992 : 1, anchor: .top)
            .offset(y: active ? -6 : 0)
    }
}

extension AnyTransition {
    /// Per-row staggered entrance; rows simply fade with the collapsing
    /// shell on removal (140ms, ProjectItem.svelte sessionListContentOutro).
    static func sessionRowStagger(index: Int) -> AnyTransition {
        if SidebarMotion.reduceMotion { return .opacity }
        return .asymmetric(
            insertion: .modifier(
                active: SessionRowEnterModifier(active: true),
                identity: SessionRowEnterModifier(active: false)
            )
            .animation(SidebarMotion.rowEnter(index: index)),
            removal: .opacity.animation(.easeOut(duration: 0.14))
        )
    }

    /// The session-list container: fade in 220ms ease-out from
    /// translateY(-6) scale(0.992); fade out 140ms.
    static var sessionListContent: AnyTransition {
        if SidebarMotion.reduceMotion { return .opacity }
        return .asymmetric(
            insertion: .modifier(
                active: SessionListContentModifier(active: true),
                identity: SessionListContentModifier(active: false)
            )
            .animation(.easeOut(duration: 0.22)),
            removal: .opacity.animation(.easeOut(duration: 0.14))
        )
    }

    /// Sidebar pane slide (Sidebar.svelte:465,491): the project tree flies
    /// to x:-140, the settings nav flies in from x:+140; both fade.
    static func sidebarPanel(fromTrailing: Bool) -> AnyTransition {
        .offset(x: fromTrailing ? 140 : -140).combined(with: .opacity)
    }
}

// MARK: - Sidebar

struct SidebarView: View {
    @ObservedObject var store: UnpeelStore

    /// Shared drag-reorder state for the whole tree (top-level projects,
    /// inline worktree children, sessions).
    @StateObject private var dragState = SidebarDragState()
    @State private var folderDropTargeted = false

    var body: some View {
        VStack(spacing: 0) {
            // The list area slides between the project tree and the settings
            // nav; the footer below stays put (Sidebar.svelte .sidebar-views).
            // The animation is scoped to this ZStack on purpose: the settings
            // open/close must never be a window-wide transaction, or the
            // content pane's Metal-backed terminal would get pulled into a
            // transition. (Worktrees render inline in the tree — there is no
            // worktrees pane anymore.)
            // Remote scope renders through the SAME tree: the store projects
            // the selected Host's bootstrap into the display nodes, so there
            // is no separate remote sidebar hierarchy.
            ZStack {
                if store.settingsVisible {
                    SettingsSidebarPanel(store: store)
                        .transition(.sidebarPanel(fromTrailing: true))
                } else {
                    projectTreePanel
                        .transition(.sidebarPanel(fromTrailing: false))
                }
            }
            .animation(SidebarMotion.slide, value: store.settingsVisible)
            .environmentObject(dragState)
            .frame(maxHeight: .infinity)
            .onDrop(of: [.fileURL], isTargeted: $folderDropTargeted) { providers in
                guard store.selectedHostScope == .local else { return false }
                dragState.folderHoverReset()
                return store.addProjectFolders(from: providers)
            }
            // Catch-all delegate over the whole list area: commits the final
            // session preview on drop anywhere (the visual order was already
            // applied by row-level dropEntered moves) and keeps the cursor on the
            // `.move` proposal in row gaps / empty space — the closure-based
            // onDrop used here before let AppKit fall back to `.copy`,
            // which showed the green "+" badge between rows.
            .onDrop(of: [.plainText], delegate: SidebarContainerDropDelegate(
                isDragActive: { [weak dragState] in dragState?.isActive ?? false },
                finish: { [weak dragState] in dragState?.finish() }
            ))
            .overlay {
                // Highlight whether the folder hovers empty list space (parent
                // onDrop `folderDropTargeted`) or directly over a row (the row
                // delegates' hover counter) — both mean "drop to add project".
                if folderDropTargeted || dragState.isFolderHovered {
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .fill(Theme.accent.opacity(0.08))
                        .overlay(
                            RoundedRectangle(cornerRadius: 12, style: .continuous)
                                .stroke(Theme.accent.opacity(0.55), lineWidth: 1)
                        )
                        .padding(6)
                        .allowsHitTesting(false)
                        .transition(.opacity)
                }
            }
            .animation(.easeInOut(duration: 0.12), value: folderDropTargeted)
            .animation(.easeInOut(duration: 0.12), value: dragState.isFolderHovered)
            // The footer hides wholesale while the settings nav occupies the
            // list area. In remote scope the SAME footer renders; the host
            // button turns green with the remote Host's name (the one
            // intended visible scope difference), and Add Project — a
            // local-filesystem verb — hides.
            if !store.settingsVisible {
                SidebarFooter(
                    localVerbsVisible: store.selectedHostScope == .local,
                    remoteHostName: store.selectedHostScope == .local
                        ? nil
                        : (store.remoteHostRuntime.snapshot?.macName
                            ?? store.remoteHostStore.selectedRecord?.name
                            ?? "Remote Host"),
                    collapseAllDisabled: store.expandedProjectIDs.isEmpty,
                    onCollapseAll: {
                        withAnimation(SidebarMotion.accordionClose) {
                            store.expandedProjectIDs = []
                            // Collapsing drops the "keep hidden row visible"
                            // pins, like the per-project collapse does.
                            store.clearSidebarKeepVisiblePins()
                        }
                    },
                    onAddProject: { store.addProjectFolder() },
                    onOpenSettings: { store.openSettings() },
                    onOpenRemoteSettings: { store.openSettings(tab: .mobile) }
                )
                .transition(.opacity)
            }
        }
        // Animate the footer's hide/show alongside the pane slide. Scoped to
        // this sidebar VStack only — the content pane's Metal-backed terminal
        // lives outside it, so this stays a sidebar-local transaction.
        .animation(SidebarMotion.slide, value: store.settingsVisible)
        // Fixed top drag chrome stays outside the sliding ZStack so it remains
        // stable as the list area slides between the project tree, worktrees
        // view and settings nav. It is visually transparent; row dissolution
        // is owned by `SidebarListFadeMask`.
        .overlay(alignment: .top) {
            SidebarTopGlassOverlay()
        }
    }

    private var projectTreePanel: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 1) {
                    ForEach(store.displayNodes) { node in
                        ProjectNodeView(store: store, node: node, depth: 0)
                    }
                }
                // Keep rows entering the fixed top chrome inside the fade ramp,
                // so they dissolve before reaching the titlebar controls. The
                // bottom padding must cover SessionScrollTarget.margin, or
                // auto-scrolls to the last rows clamp back under the bottom
                // fade.
                .padding(EdgeInsets(
                    top: 48, leading: 8,
                    bottom: SessionScrollTarget.margin + 12, trailing: 8
                ))
            }
            .scrollIndicators(.hidden)
            .mask(SidebarListFadeMask())
            .overlay {
                if store.displayNodes.isEmpty {
                    if store.selectedHostScope == .local {
                        SidebarEmptyProjectsView {
                            store.pickProjectFolder { _ in }
                        }
                    } else {
                        RemoteScopeEmptySidebarView(
                            hostName: store.remoteHostRuntime.snapshot?.macName
                                ?? store.remoteHostStore.selectedRecord?.name
                                ?? "Remote Host",
                            state: store.remoteHostRuntime.connectionState
                        )
                    }
                }
            }
            .onChange(of: store.selectedSessionID) { newValue in
                if let id = newValue {
                    withAnimation(.easeOut(duration: 0.15)) {
                        proxy.scrollTo(SessionScrollTarget.id(id))
                    }
                }
            }
            .onChange(of: store.sidebarSessionRevealRequest) { request in
                if let request {
                    scrollToSession(
                        request.sessionID, proxy: proxy,
                        anchor: request.centered ? .center : nil
                    )
                }
            }
            .onAppear {
                if let request = store.sidebarSessionRevealRequest {
                    scrollToSession(
                        request.sessionID, proxy: proxy,
                        anchor: request.centered ? .center : nil
                    )
                }
            }
        }
    }

    private func scrollToSession(
        _ sessionID: String,
        proxy: ScrollViewProxy,
        anchor: UnitPoint? = nil
    ) {
        DispatchQueue.main.async {
            withAnimation(.easeOut(duration: 0.22)) {
                proxy.scrollTo(SessionScrollTarget.id(sessionID), anchor: anchor)
            }
        }
    }
}

private struct SidebarEmptyProjectsView: View {
    let onAddProject: () -> Void

    var body: some View {
        VStack(spacing: 14) {
            ChromeIconView(icon: .folderClosed, size: 40)
                .foregroundStyle(Theme.foreground)

            Button {
                onAddProject()
            } label: {
                Label("Add Project", systemImage: "folder")
                    .font(.system(size: 13, weight: .semibold))
            }
            .buttonStyle(.borderedProminent)
            .tint(Theme.ctaTint)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.horizontal, 18)
        .padding(.top, Theme.titlebarHeight)
    }
}

/// Scroll-target geometry for session rows. Each row registers an invisible
/// scroll target that extends `margin` past the row on both ends, so a
/// nil-anchor (minimal) `scrollTo` stops with the row clear of the top chrome
/// veil and the bottom fade instead of flush against the sidebar edges.
enum SessionScrollTarget {
    static let margin: CGFloat = 48
    static func id(_ sessionID: String) -> String { "scroll-target:\(sessionID)" }
}

/// Vertical fade mask over the sidebar lists. The top stays faintly visible so
/// rows blur into the chrome instead of disappearing behind a hard transparent
/// cut.
struct SidebarListFadeMask: View {
    private static let topMinOpacity: CGFloat = 0
    private static let opaqueAt: CGFloat = 76

    var body: some View {
        VStack(spacing: 0) {
            LinearGradient(
                gradient: Gradient(stops: Self.topStops),
                startPoint: .top, endPoint: .bottom
            )
            .frame(height: Self.opaqueAt)
            Color.black
            LinearGradient(
                colors: [.black, .clear],
                startPoint: .top, endPoint: .bottom
            )
            .frame(height: 26)
        }
    }

    /// Gradient stops sample the smoothstep densely enough to stay smooth
    /// (SwiftUI interpolates linearly between stops).
    private static let topStops: [Gradient.Stop] = {
        let steps = 8
        var stops: [Gradient.Stop] = [
            .init(color: .black.opacity(topMinOpacity), location: 0)
        ]
        for step in 0 ... steps {
            let t = CGFloat(step) / CGFloat(steps)
            let alpha = topMinOpacity + (1 - topMinOpacity) * smoothstep(t)
            stops.append(.init(
                color: .black.opacity(alpha),
                location: t
            ))
        }
        return stops
    }()
}

private struct SidebarTopGlassOverlay: View {
    var body: some View {
        WindowDragArea()
            .frame(height: Theme.titlebarHeight)
    }
}

/// Hermite smoothstep: zero first derivative at both ends, so gradient
/// ramps built from it start and finish without visible edges.
private func smoothstep(_ t: CGFloat) -> CGFloat {
    let x = min(max(t, 0), 1)
    return x * x * (3 - 2 * x)
}

/// Text-glyph chevron (›/‹) vertically centered on its INK, not its line
/// box. Guillemets sit on the x-height midline, so inside an HStack(.center)
/// the bare Text lands ~1pt below the row's visual centerline (badges and
/// labels centered on cap height) — measured 2.5 device px off in the
/// worktrees link row. The offset is computed once per (glyph, size) from
/// the CoreText ink bounds, so it tracks the system font's real metrics.
struct ChevronGlyph: View {
    let glyph: String
    let size: CGFloat

    var body: some View {
        Text(glyph)
            .font(.system(size: size))
            .offset(y: Self.inkCenterOffset(glyph: glyph, size: size))
    }

    /// +down offset that moves the glyph's ink center onto the line-box
    /// center: inkRect.midY − (ascender + descender)/2, both baseline-rel.
    @MainActor private static var cache: [String: CGFloat] = [:]

    @MainActor
    static func inkCenterOffset(glyph: String, size: CGFloat) -> CGFloat {
        let key = "\(glyph)@\(size)"
        if let cached = cache[key] { return cached }
        var offset: CGFloat = 0
        let font = NSFont.systemFont(ofSize: size)
        var chars = Array(glyph.utf16)
        var glyphs = [CGGlyph](repeating: 0, count: chars.count)
        if CTFontGetGlyphsForCharacters(font, &chars, &glyphs, chars.count),
           let first = glyphs.first {
            var g = first
            let ink = CTFontGetBoundingRectsForGlyphs(font, .default, &g, nil, 1)
            let frameCenter = (font.ascender + font.descender) / 2
            offset = ink.midY - frameCenter
        }
        cache[key] = offset
        return offset
    }
}

// MARK: - Project node (project row + sessions + worktree children)

private struct SessionRenderRow: Identifiable {
    let session: SessionEntry

    var id: String { session.id }
}

/// A destination in the session context menu's "Move to" flyout.
struct SessionMoveTarget: Identifiable {
    let id: String
    let name: String
}

struct ProjectNodeView: View {
    @ObservedObject var store: UnpeelStore
    @EnvironmentObject private var dragState: SidebarDragState
    let node: ProjectNode
    let depth: Int

    private var isExpanded: Bool { store.expandedProjectIDs.contains(node.id) }

    /// Top-level projects reorder in the main tree; child folders (worktrees
    /// AND groups) reorder among their inline siblings under the parent
    /// (previewProjectMove only ever reorders same-parent siblings, so a
    /// cross-parent drop is a no-op).
    private var isProjectDraggable: Bool { true }

    /// Plain organizational groups accept session drops. Worktree children
    /// do not: changing a live session's checkout needs the explicit
    /// restart/resume flow, not a display-only drag.
    private var acceptsSessionDrops: Bool {
        node.project.acceptsSessionDrop
    }

    /// Pinned/regular/displayed session lists live on the store now. The
    /// rendered lists are parent-ordered so ⌘1–9 shortcut targets match the
    /// visual rows.
    private var pinnedSessions: [SessionEntry] {
        store.pinnedSessions(in: node)
    }

    private var renderedPinnedSessions: [SessionEntry] {
        store.renderedPinnedSessions(in: node)
    }

    private var renderedDisplayedSessions: [SessionEntry] {
        store.renderedDisplayedSessions(in: node)
    }

    private var archivedSessionCount: Int {
        store.archivedSessionCount(in: node)
    }

    private func sessionRenderRows(_ sessions: [SessionEntry]) -> [SessionRenderRow] {
        sessions.map { SessionRenderRow(session: $0) }
    }

    /// Collapsed-project state rollup over this project's sessions plus
    /// worktree descendants. Precedence attention > unread mirrors the
    /// `project-state-dot` markup (ProjectItem.svelte:1228-1232).
    private var aggregateHasAttention: Bool {
        func check(_ node: ProjectNode) -> Bool {
            node.sessions.contains { $0.status == .attention }
                || node.worktrees.contains(where: check)
        }
        return check(node)
    }

    /// projectHasUnread (ProjectItem.svelte:352-354) incl. worktree children.
    private var aggregateHasUnread: Bool {
        func check(_ node: ProjectNode) -> Bool {
            node.sessions.contains { store.sessionIsUnread($0.id) }
                || node.worktrees.contains(where: check)
        }
        return check(node)
    }

    var body: some View {
        projectRow

        if isExpanded {
            sessionList
                .transition(.sessionListContent)
        }
    }

    /// Busy shimmer on the project NAME (ProjectItem.svelte:1268,
    /// `class:shimmer={aggregateState === 'busy'}`). Rolls up over the OWN
    /// sessions always, and over worktree descendants while collapsed —
    /// matching the attention/unread dots, so a busy session hidden inside
    /// a folded subtree still reads as activity on the visible row.
    /// Attention outranks busy, so attention suppresses the shimmer.
    private var showsBusyShimmer: Bool {
        func busy(_ n: ProjectNode) -> Bool {
            n.sessions.contains { $0.status == .starting || $0.status == .busy }
                || n.worktrees.contains(where: busy)
        }
        let busyNow = node.sessions.contains { $0.status == .starting || $0.status == .busy }
            || (!isExpanded && node.worktrees.contains(where: busy))
        return !aggregateHasAttention && busyNow
    }

    @ViewBuilder
    private var projectRow: some View {
        // Local-filesystem verbs (Finder/editor/worktrees/groups/colors/
        // removal) are Host-side operations the remote protocol does not
        // carry; their menu items hide while a remote Host is selected.
        let isLocalScope = store.selectedHostScope == .local
        let worktreesEnabled = store.isExperimentalEnabled(.worktrees)
        let isGitProject = isLocalScope
            && node.project.isFolder != true
            && UnpeelStore.isGitRepo(path: node.project.path)
        let canHostWorktreeSessions = worktreesEnabled
            && isGitProject && node.project.worktreeBranch == nil
        let row = ProjectRowView(
            node: node,
            depth: depth,
            isExpanded: isExpanded,
            showsAttentionDot: !isExpanded && aggregateHasAttention,
            showsUnreadDot: !isExpanded && aggregateHasUnread,
            showsBusyShimmer: showsBusyShimmer,
            isSessionDropTarget: dragState.sessionDropTargetProjectID == node.id,
            quickGroups: store.displayQuickPresetGroups,
            menuPresets: store.displayAvailablePresets,
            folderColor: isLocalScope ? store.projectFolderColor(for: node.id) : nil,
            // Worktree menu toggle gate (ProjectItem.svelte:843-848):
            // real project (not a plain folder), not a worktree child,
            // and the path is a git repo.
            showsWorkspacesToggle: canHostWorktreeSessions,
            showsLocalProjectVerbs: isLocalScope,
            isConfirmingRemove: store.confirmingRemoveProjectID == node.id,
            shortcutHint: store.projectShortcutHintIndex(forProject: node.id),
            hasLiveSessions: node.sessions.contains(where: \.isLive),
            archivedSessionCount: archivedSessionCount,
            editorName: UnpeelStore.editorDisplayName(store.codeEditor),
            // The drag starts from the row's folder-mark grip handle (inside
            // ProjectRowView); the whole row stays the drop target.
            dragProvider: isProjectDraggable
                ? { [dragState, weak store] in
                    dragState.beginProject(
                        node.id,
                        commitReorder: { store?.commitProjectReorder() },
                        cancelReorder: { store?.cancelProjectReorder() }
                    )
                    return NSItemProvider(object: node.id as NSString)
                }
                : nil,
            onToggle: toggleExpansion,
            onLaunchPreset: { preset in
                // startSessionOrToast via launchPresetForProject
                // (App.svelte:1168-1175): command = preset command,
                // label = command for non-blank / "Terminal" for blank —
                // which is exactly what launchSession derives.
                store.launchSession(projectID: node.id, command: preset.command)
            },
            onLaunchPresetInWorktree: canHostWorktreeSessions ? { preset in
                store.promptNewWorktreeSession(projectID: node.id, preset: preset)
            } : nil,
            onCreateWorktree: { store.promptCreateWorktree(projectID: node.id) },
            onCreateGroup: { store.promptCreateGroup(projectID: node.id) },
            onRemoveGroup: { store.removeGroupProject(node.id) },
            onStopAll: { store.stopAllSessions(projectID: node.id) },
            onOpenArchived: { store.openArchivedSessions(projectID: node.id) },
            onRevealInFinder: { store.revealInFinder(path: node.project.path) },
            onOpenInEditor: { store.openInEditor(path: node.project.path) },
            onRequestRemove: { store.requestRemoveProject(node.id) },
            onConfirmRemove: { store.removeProject(node.id) },
            onCancelRemove: { store.cancelRemoveProjectConfirm() },
            onRenameWorktree: { store.promptRenameWorktreeProject(node.id) },
            onRemoveWorktree: { store.removeWorktreeProject(node.id) },
            onSetFolderColor: { color in
                store.setProjectFolderColor(color, for: node.id)
            },
            isDateSorted: store.isDateSorted(projectID: node.id),
            onSetSessionDateSorted: { dateSorted in
                store.setSessionDateSorted(dateSorted, for: node.id)
            },
            onManagePresets: { store.openSettings(tab: .presets) }
        )

        if isProjectDraggable {
            row
                // Row lift: the dragged source dims; the drag preview is an
                // invisible 1×1 (no floating row-snapshot ghost), so the
                // live gap animation is the only drag feedback.
                .opacity(dragState.projectID == node.id ? 0.45 : 1)
                .onDrop(of: [.plainText, .fileURL], delegate: SidebarReorderDropDelegate(
                    isReorderActive: { [weak dragState] in dragState?.projectID != nil },
                    moveOver: { [weak store, weak dragState] in
                        guard let dragged = dragState?.projectID,
                              dragged != node.id
                        else { return false }
                        store?.previewProjectMove(draggedID: dragged, over: node.id)
                        return true
                    },
                    isSessionMoveActive: { [weak dragState] in
                        guard acceptsSessionDrops,
                              let drag = dragState?.sessionDrag
                        else { return false }
                        return drag.projectID != node.id
                    },
                    setSessionMoveHover: { [weak dragState] hovering in
                        dragState?.setSessionDropTarget(node.id, hovering: hovering)
                    },
                    performSessionMove: { [weak store, weak dragState] in
                        guard acceptsSessionDrops,
                              let drag = dragState?.sessionDrag,
                              store != nil
                        else { return false }
                        // Filing into another group supersedes any live
                        // reorder preview made in the origin list.
                        store?.cancelSessionReorder(projectID: drag.projectID)
                        store?.moveSession(drag.sessionID, toProjectID: node.id)
                        return true
                    },
                    finishReorder: { [weak dragState] in dragState?.finish() },
                    cancelReorder: { [weak dragState] in dragState?.end() },
                    setFolderHover: { [weak dragState] hovering in
                        hovering ? dragState?.folderHoverEnter() : dragState?.folderHoverExit()
                    },
                    addFolders: { [weak store, weak dragState] providers in
                        dragState?.folderHoverReset()
                        return store?.addProjectFolders(from: providers) ?? false
                    }
                ))
        } else {
            row
        }
    }

    /// Whole-row click toggles expansion (ProjectItem.svelte:654-659);
    /// open 340ms cubic-bezier(0.16,1,0.3,1), close 240ms cubicInOut.
    private func toggleExpansion() {
        let opening = !isExpanded
        withAnimation(opening ? SidebarMotion.accordionOpen : SidebarMotion.accordionClose) {
            store.toggleProjectExpanded(node.id)
        }
    }

    /// One container so the accordion content transition applies to the
    /// whole block, like .session-list in ProjectItem.svelte (worktree
    /// child rows live inside the same shell there too).
    private var sessionList: some View {
        // The outer project tree is lazy, but each expanded project is one of
        // its children. A regular VStack here therefore constructed every
        // Session row in that project at once. Keep the hierarchy/accordion
        // behavior while virtualizing rows inside large expanded projects.
        LazyVStack(alignment: .leading, spacing: 0) {
            let pinned = pinnedSessions
            let displayed = renderedDisplayedSessions
            // ⌘N hints while ⌘ is held and this is the shortcut project
            // (empty otherwise).
            let shortcutHints = store.sessionShortcutHintIndices(forProject: node.id)

            // Worktrees: inline collapsible child folders, one level deeper
            // than this project's header, shown whenever this project has at
            // least one worktree (created via "New worktree…") and hidden when
            // it has none. Folders come FIRST — above the pinned sessions and
            // the regular session list. Order matches the per-parent order
            // overlay (applied when the tree was built). Each child recurses
            // through ProjectNodeView, so its own sessions, context menu, and
            // accordion state all reuse the exact machinery the parent uses;
            // children default collapsed (expandedProjectIDs only contains
            // explicitly expanded ids). Creating a worktree lives on the
            // project context menu ("New worktree…") — no inline ghost row.
            // Gated behind the experimental worktrees flag so disabling it
            // hides the surface.
            if store.isExperimentalEnabled(.worktrees), !node.worktrees.isEmpty {
                ForEach(node.worktrees) { child in
                    ProjectNodeView(store: store, node: child, depth: depth + 1)
                }
            }

            let pinnedRows = sessionRenderRows(renderedPinnedSessions)
            ForEach(Array(pinnedRows.enumerated()), id: \.element.id) { index, row in
                sessionRow(
                    row.session,
                    staggerIndex: index,
                    pinnedRow: true,
                    shortcutHint: shortcutHints[row.session.id]
                )
            }

            let displayedRows = sessionRenderRows(displayed)
            ForEach(Array(displayedRows.enumerated()), id: \.element.id) { index, row in
                sessionRow(
                    row.session,
                    staggerIndex: pinnedRows.count + index,
                    pinnedRow: false,
                    shortcutHint: shortcutHints[row.session.id]
                )
            }

            // Empty state: archived sessions live in the main pane, so a
            // project with only archived history still reads as empty here.
            if pinned.isEmpty && displayed.isEmpty && node.worktrees.isEmpty {
                EmptySessionsPlaceholderRow(
                    label: archivedSessionCount == 0 ? "No sessions yet." : "No active sessions.",
                    depth: depth,
                    indentBase: 28,
                    menuPresets: store.displayAvailablePresets,
                    onLaunch: { store.launchSession(projectID: node.id, command: $0.command) },
                    onManagePresets: { store.openSettings(tab: .presets) },
                    showsManagePresets: store.selectedHostScope == .local,
                    onLaunchInWorktree: store.canOfferWorktreeSession(projectID: node.id)
                        ? { store.promptNewWorktreeSession(projectID: node.id, preset: $0) }
                        : nil,
                    archivedCount: archivedSessionCount,
                    onOpenArchived: { store.openArchivedSessions(projectID: node.id) }
                )
            }

            // Everything past the recent stopped window lives in the archive
            // library; the way in is the project context menu's "Archived (N)"
            // (plus the empty-state placeholder's own archived link) — no
            // sidebar footer row.
        }
        // Rows keep stable ids across the active → stopped move, so a
        // just-archived session glides down into the stopped group instead
        // of teleporting.
        .animation(
            SidebarMotion.accordionOpen,
            value: renderedDisplayedSessions.map(\.id)
        )
    }

    @ViewBuilder
    private func sessionRow(
        _ session: SessionEntry,
        staggerIndex: Int,
        pinnedRow: Bool,
        shortcutHint: Int? = nil
    ) -> some View {
        // A row being renamed must not be a drag source: mouse drags inside
        // the TextField are text selection. A date-sorted group has no
        // manual order to write, so re-ordering is off entirely.
        let isReorderable = !store.isDateSorted(projectID: node.id)
            && store.confirmingRemoveSessionID != session.id
            && store.editingSessionID != session.id
        // Native-only within-project drag reorder (the Svelte app has no
        // within-project reorder). Pinned and regular rows each reorder
        // only within their own section — `drag.pinned == pinnedRow` keeps
        // the two sections from mixing. The drag STARTS from the row's
        // leading grip handle (inside SessionRowView); the whole row stays
        // the drop target.
        let dragProvider: (() -> NSItemProvider)? = isReorderable
            ? { [dragState, weak store] in
                dragState.beginSession(
                    projectID: node.id,
                    sessionID: session.id,
                    pinned: pinnedRow,
                    commitReorder: {
                        store?.commitSessionReorder(
                            projectID: node.id, pinned: pinnedRow
                        )
                    },
                    cancelReorder: {
                        store?.cancelSessionReorder(projectID: node.id)
                    }
                )
                return NSItemProvider(object: session.id as NSString)
            }
            : nil
        let row = SessionRowView(
            session: session,
            depth: depth,
            // Depth alone supplies the standard 14pt nesting step beneath a
            // child folder; no extra child-folder offset is needed.
            indentBase: 9,
            isSelected: store.selectedSessionID == session.id,
            isPinned: pinnedRow,
            shortcutHint: shortcutHint,
            isUnread: store.sessionIsUnread(session.id),
            // Archive-page confirms render (and monitor click-away) on the
            // archive page only — a mirrored sidebar confirm would cancel
            // them on the mouse-down aimed at the archive card's buttons.
            isConfirmingRemove: store.confirmingRemoveSessionID == session.id
                && store.confirmingRemoveSurface == .sidebar,
            isConfirmingArchive: store.confirmingArchiveSessionID == session.id,
            isRemoving: store.removingSessionIDs.contains(session.id),
            isRestarting: store.restartingSessionIDs.contains(session.id),
            isResumingAgent: store.resumingAgentSessionIDs.contains(session.id),
            isArchiving: store.archivingSessionIDs.contains(session.id),
            isArchived: store.sessionIsRecentArchived(session.id),
            isEditing: store.editingSessionID == session.id,
            notifyWhenDone: store.notifyWhenDoneSessionIDs.contains(session.id),
            canRestart: store.sessionCanRestart(session.id),
            canResumeAgent: store.sessionCanResumeAgent(session.id),
            canArchive: store.sessionCanArchive(session.id),
            canFork: store.sessionCanFork(session.id),
            canAppendSystemContext: store.sessionCanAppendSystemContext(session.id),
            canNotifyWhenDone: store.sessionCanNotifyWhenDone(session.id),
            canClearAttention: store.sessionCanClearAttention(session.id),
            sessionsMcpEnabled: store.isExperimentalEnabled(.sessionsMcp),
            showsToolIcon: store.showSessionToolIcons,
            ageTimestampMs: store.isDateSorted(projectID: node.id)
                ? max(session.createdAt, session.lifecycleAtMs ?? 0)
                : nil,
            dragProvider: dragProvider,
            onSelect: { store.selectedSessionID = session.id },
            onHoverIntent: { store.prewarmSession(session.id) },
            onSetNotifyWhenDone: { store.setNotifyWhenDone(session.id, enabled: $0) },
            onTogglePin: {
                if pinnedRow {
                    store.unpinSession(projectID: node.id, sessionID: session.id)
                } else {
                    store.pinSession(projectID: node.id, sessionID: session.id)
                    // The row just teleported up into the pinned section —
                    // follow it so it doesn't vanish above the viewport.
                    store.followSessionRowInSidebar(session.id)
                }
            },
            onResume: { store.resumeAgentOrSession(session.id) },
            onClearAttention: { store.clearAttention(session.id) },
            onFork: { store.forkSession(session.id) },
            onAppendSystemContext: { store.promptAppendSystemContext(sessionID: session.id) },
            onRequestRemove: { store.requestRemoveSession(session.id) },
            onConfirmRemove: { store.confirmRemoveSession(session.id) },
            onCancelRemove: { store.cancelRemoveConfirm() },
            onArchive: { store.requestArchiveSession(session.id) },
            onUnarchive: {
                if store.sessionCanRestart(session.id) {
                    store.resumeArchivedSession(session.id)
                } else {
                    store.unarchiveSession(session.id)
                }
            },
            onConfirmArchive: { store.archiveSession(session.id) },
            onCancelArchive: { store.cancelArchiveConfirm() },
            onCopyTranscript: { store.copyTranscriptMarkdown(session.id, entries: $0) },
            onBeginEdit: { store.editingSessionID = session.id },
            onCommitRename: { store.renameSession(session.id, to: $0) },
            onEndEdit: {
                if store.editingSessionID == session.id {
                    store.editingSessionID = nil
                }
            },
            moveTargets: store.isExperimentalEnabled(.worktrees)
                ? store.moveDestinations(forSession: session.id)
                    .map { SessionMoveTarget(id: $0.id, name: $0.name) }
                : [],
            onMoveTo: { store.moveSession(session.id, toProjectID: $0) }
        )
        .id(session.id)
        .background {
            // The `.id()` inside is on the un-padded Color.clear, so the
            // registered frame is the row grown by `margin` on both ends —
            // scroll targets aim at this, not the bare row.
            Color.clear
                .id(SessionScrollTarget.id(session.id))
                .padding(.vertical, -SessionScrollTarget.margin)
        }
        .transition(.sessionRowStagger(index: staggerIndex))

        if isReorderable {
            row
                .opacity(
                    dragState.sessionDrag?.sessionID == session.id ? 0.45 : 1
                )
                .onDrop(of: [.plainText, .fileURL], delegate: SidebarReorderDropDelegate(
                    isReorderActive: { [weak dragState] in dragState?.sessionDrag != nil },
                    moveOver: { [weak store, weak dragState] in
                        guard let drag = dragState?.sessionDrag,
                              drag.projectID == node.id,
                              drag.pinned == pinnedRow,
                              drag.sessionID != session.id
                        else { return false }
                        if pinnedRow {
                            store?.previewPinnedSessionMove(
                                projectID: node.id,
                                draggedID: drag.sessionID,
                                over: session.id
                            )
                        } else {
                            store?.previewSessionMove(
                                projectID: node.id,
                                draggedID: drag.sessionID,
                                over: session.id
                            )
                        }
                        return true
                    },
                    isSessionMoveActive: { false },
                    setSessionMoveHover: { _ in },
                    performSessionMove: { false },
                    finishReorder: { [weak dragState] in dragState?.finish() },
                    cancelReorder: { [weak dragState] in dragState?.end() },
                    setFolderHover: { [weak dragState] hovering in
                        hovering ? dragState?.folderHoverEnter() : dragState?.folderHoverExit()
                    },
                    addFolders: { [weak store, weak dragState] providers in
                        dragState?.folderHoverReset()
                        return store?.addProjectFolders(from: providers) ?? false
                    }
                ))
        } else {
            row
        }
    }
}

/// Empty-project placeholder: a muted status row shown when an expanded
/// project has no active sessions. Clicking it opens the same new-session menu
/// as the project row's "+" (blank terminal + presets + Manage presets).
private struct EmptySessionsPlaceholderRow: View {
    var label = "No sessions yet."
    let depth: Int
    /// 28 aligns with session labels; depth supplies nested indentation.
    var indentBase: CGFloat = 28
    let menuPresets: [Preset]
    let onLaunch: (Preset) -> Void
    var onManagePresets: () -> Void = {}
    var showsManagePresets = true
    var onLaunchInWorktree: ((Preset) -> Void)?
    var archivedCount = 0
    var onOpenArchived: (() -> Void)?

    @State private var hovering = false

    var body: some View {
        HStack(spacing: 0) {
            Menu {
                newSessionMenuContent(
                    menuPresets: menuPresets,
                    onLaunch: onLaunch,
                    onManagePresets: onManagePresets,
                    showsManagePresets: showsManagePresets,
                    onLaunchInWorktree: onLaunchInWorktree,
                    archivedCount: archivedCount,
                    onOpenArchived: onOpenArchived
                )
            } label: {
                Text(label)
                    .font(.system(size: 11))
                    .foregroundStyle(hovering ? Theme.foreground : Theme.mutedForeground)
                    .padding(EdgeInsets(top: 3, leading: 6, bottom: 3, trailing: 6))
                    .background(
                        RoundedRectangle(cornerRadius: 4, style: .continuous)
                            .fill(hovering ? Theme.foreground.opacity(0.08) : .clear)
                    )
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .background(HoverReporter { hovering = $0 })
            .animation(.easeInOut(duration: 0.12), value: hovering)
            Spacer(minLength: 0)
        }
        .padding(EdgeInsets(top: 2, leading: 28 + CGFloat(depth) * 14, bottom: 0, trailing: 0))
    }
}

// MARK: - Project row

struct ProjectRowView: View {
    let node: ProjectNode
    let depth: Int
    let isExpanded: Bool
    /// Collapsed-project state dot on the folder icon
    /// (ProjectItem.svelte:1228-1232): attention (#eab308) wins over unread
    /// (#60a5fa); both only show while the project is collapsed.
    var showsAttentionDot = false
    var showsUnreadDot = false
    /// Gradient sweep over the project NAME while a session inside is busy
    /// (.project-name.shimmer, ProjectItem.svelte:2083-2102).
    var showsBusyShimmer = false
    /// A dragged session is poised to be filed into this plain group.
    var isSessionDropTarget = false
    /// Strip contents: starred presets grouped per CLI, flat-list order (the
    /// strip appends the blank-terminal chip itself).
    let quickGroups: [QuickPresetGroup]
    /// All enabled presets, backing the "+" new-session menu.
    let menuPresets: [Preset]
    /// Optional native-only tint for this project's glass folder glyph.
    var folderColor: ProjectFolderColor?
    /// Whether the context menu offers New worktree… (gate: not a folder,
    /// not a worktree child, is a git repo — ProjectItem.svelte:843-848).
    var showsWorkspacesToggle = false
    /// Whether the Controller-local project verbs (Reveal in Finder, Open in
    /// editor, folder colors, groups, sort mode, rename/remove, Manage
    /// presets…) are offered. False while a remote Host is selected — those
    /// operations have no remote protocol carrier yet, so the items hide.
    var showsLocalProjectVerbs = true
    /// Inline "Remove project?" confirm — the whole row swaps, same
    /// pattern as the session remove confirm.
    var isConfirmingRemove = false
    /// 1-based ⌃N hint while ⌃ is held (project mirror of the session
    /// rows' ⌘N hints; nil = hidden).
    var shortcutHint: Int?
    /// Any live session in THIS project (not worktree children) — gates
    /// "Stop all" (hasLive, ProjectItem.svelte:896).
    var hasLiveSessions = false
    /// Archived sessions owned by this exact project. Recent ones can remain
    /// in the stopped preview; a non-zero count adds the complete main-pane
    /// archive library to the project context menu.
    var archivedSessionCount = 0
    /// Display name of the configured editor ("Open in VS Code" etc.).
    var editorName = "VS Code"
    /// Non-nil when the row can be drag-reordered. The FOLDER MARK is the
    /// drag handle: on hover it swaps to a grip icon and `.onDrag` lives
    /// there — never on the whole row, where it would race the tap-to-toggle
    /// gesture.
    var dragProvider: (() -> NSItemProvider)?
    let onToggle: () -> Void
    let onLaunchPreset: (Preset) -> Void
    /// "In a new worktree" in the new-session menus; nil when the project
    /// can't host worktree sessions (same gate as `showsWorkspacesToggle`).
    var onLaunchPresetInWorktree: ((Preset) -> Void)?
    var onCreateWorktree: () -> Void = {}
    /// "New group…" — a plain organizational child folder (no git).
    var onCreateGroup: () -> Void = {}
    /// Group rows swap Remove for this: forget the group; its sessions
    /// fall back to the parent on the next scan.
    var onRemoveGroup: () -> Void = {}
    var onStopAll: () -> Void = {}
    var onOpenArchived: () -> Void = {}
    var onRevealInFinder: () -> Void = {}
    var onOpenInEditor: () -> Void = {}
    var onRequestRemove: () -> Void = {}
    var onConfirmRemove: () -> Void = {}
    var onCancelRemove: () -> Void = {}
    /// Worktree child rows swap Remove for this: confirm dialog +
    /// `git worktree remove` + forget the project.
    var onRenameWorktree: () -> Void = {}
    var onRemoveWorktree: () -> Void = {}
    var onSetFolderColor: (ProjectFolderColor?) -> Void = { _ in }
    /// Whether this group's sessions sort by date (recently updated first)
    /// instead of the manual drag order — drives the Sort sessions menu
    /// checkmark.
    var isDateSorted = false
    var onSetSessionDateSorted: (Bool) -> Void = { _ in }

    /// Opens settings on the Presets tab, from the bottom of the
    /// new-session preset menus.
    var onManagePresets: () -> Void = {}

    /// Worktree children get "Remove worktree" instead of the plain
    /// Remove confirm (ProjectItem.svelte:933).
    private var isWorktreeChild: Bool { node.project.worktreeBranch != nil }

    /// Any inline child folder row — a worktree checkout OR a plain group.
    /// Drives the shared folder-row presentation (chevron mark, session-
    /// level indent, count capsule); branch-specific bits stay keyed on
    /// `isWorktreeChild`.
    private var isChildFolder: Bool { node.project.parentProjectID != nil }

    /// Inline child-folder NAMES share the parent project's normal session
    /// text column. Their disclosure chevron occupies the session mark gutter
    /// to the left, while sessions inside the folder still step in another
    /// level. Plain project headers keep the 7pt header base.
    private var rowLeading: CGFloat {
        isChildFolder
            ? 10 + CGFloat(max(0, depth - 1)) * 14
            : 7 + CGFloat(depth) * 14
    }

    @State private var hovering = false

    /// UNPEEL_DEBUG_HOVER_PROJECT=<name|id> forces this row's hover state
    /// and the expanded strip, so snapshots can photograph hover-only UI.
    private static let debugHoverProject =
        ProcessInfo.processInfo.environment["UNPEEL_DEBUG_HOVER_PROJECT"]

    private var debugHover: Bool {
        guard let target = Self.debugHoverProject, !target.isEmpty else { return false }
        return target == node.project.name || target == node.project.id
    }

    private var showsActions: Bool { hovering || debugHover }
    private var folderTint: Color { folderColor?.tint ?? Theme.mutedForeground }

    @State private var handleHovering = false

    var body: some View {
        if isConfirmingRemove {
            confirmRemoveRow
        } else {
            normalRow
        }
    }

    /// The row itself becomes the confirmation (same row-swap pattern as
    /// the session remove confirm). The Svelte app removes plain projects
    /// without asking; natively a misclick would tombstone the project, so
    /// the inline confirm stays.
    /// Removing a project also removes every session in its subtree
    /// (UnpeelStore.removeProject), so the confirm names the count.
    /// `node.sessions` already includes archived rows — they are hidden at
    /// display time, not at tree build — so no separate archived term.
    private var removeSessionCount: Int {
        func count(_ n: ProjectNode) -> Int {
            n.sessions.count + n.worktrees.map(count).reduce(0, +)
        }
        return count(node)
    }

    private var confirmRemoveRow: some View {
        HStack(spacing: 7) {
            Text(
                removeSessionCount == 0
                    ? "Remove project?"
                    : removeSessionCount == 1
                        ? "Remove project and 1 session?"
                        : "Remove project and \(removeSessionCount) sessions?"
            )
                .font(Theme.rowLabelFont)
                .foregroundStyle(Theme.foreground)
                .lineLimit(1)

            Spacer(minLength: 4)

            ConfirmPillButton(label: "Cancel", destructive: false, action: onCancelRemove)
            ConfirmPillButton(label: "Remove", destructive: true, action: onConfirmRemove)
        }
        .padding(EdgeInsets(top: 2, leading: rowLeading, bottom: 2, trailing: 5))
        .frame(minHeight: 28)
        .background(
            RoundedRectangle(cornerRadius: 9, style: .continuous)
                .fill(Theme.hoverRow)
        )
        .contentShape(Rectangle())
        .background(RemoveConfirmDismissMonitor(onCancel: onCancelRemove))
    }

    /// Leading mark: Phosphor folder (open/closed by expansion) for plain
    /// projects; worktree children lead with the disclosure chevron in the
    /// parent's folder tint instead (TUI parity). While hovering the mark
    /// itself on a reorderable row, it swaps to the drag grip, which is the
    /// row's only drag source.
    @ViewBuilder
    private var leadingMark: some View {
        let mark = ZStack {
            if showsDragHandle {
                ChromeIconView(icon: .dragHandle, size: 16)
                    .foregroundStyle(Theme.mutedForeground)
            } else if isChildFolder {
                // Inline child folders (worktrees + groups) lead with the
                // disclosure chevron (TUI parity); it rotates open in place
                // of a folder-icon swap.
                ChevronGlyph(glyph: "›", size: 17)
                    .foregroundStyle(Theme.mutedForeground)
                    .opacity(0.6)
                    .rotationEffect(.degrees(isExpanded ? 90 : 0))
                    .animation(.easeInOut(duration: 0.15), value: isExpanded)
            } else {
                ChromeIconView(icon: isExpanded ? .folderOpen : .folderClosed, size: 16)
                    .foregroundStyle(folderTint)
            }
        }
        // The chevron is a narrow glyph — a full 18pt slot reads as a gap
        // between it and the name, so child folder rows tighten the mark.
        .frame(width: isChildFolder ? 11 : 18, height: 18)
        .overlay(alignment: .topTrailing) {
            // .project-state-dot (ProjectItem.svelte:1929-1946):
            // 6px, top/right 1px; attention #eab308 w/ 4px 20% halo.
            if showsAttentionDot {
                AttentionDot(color: Color(hex: 0xEAB308))
                    .padding(EdgeInsets(top: 1, leading: 0, bottom: 0, trailing: -1))
            } else if showsUnreadDot {
                Circle()
                    .fill(Theme.unread)
                    .frame(width: 6, height: 6)
                    .padding(EdgeInsets(top: 1, leading: 0, bottom: 0, trailing: -1))
            }
        }
        .animation(.easeInOut(duration: 0.12), value: showsDragHandle)

        if let dragProvider {
            mark
                // Generous hit area around the 18px mark (28×28) without
                // moving the row layout.
                .contentShape(Rectangle().inset(by: -5))
                .onHover { handleHovering = $0 }
                .onDrag(dragProvider) {
                    EmptyDragPreview()
                }
                .help("Drag to reorder")
        } else {
            mark
        }
    }

    /// The grip appears only while the pointer is over the folder mark
    /// itself — not on whole-row hover — so the folder stays readable
    /// while mousing around the row.
    private var showsDragHandle: Bool {
        handleHovering && dragProvider != nil
    }

    private var normalRow: some View {
        HStack(spacing: 7) {
            leadingMark
                // Keep the disclosure glyph in its current gutter, but move
                // the child-folder name onto the same text column as normal
                // session labels beneath the parent project.
                .padding(.trailing, isChildFolder ? 4 : 0)

            // .project-name: fg at 0.6; while shimmering the CSS sets
            // opacity 1 and sweeps a 80%→100%→80% currentColor gradient
            // across the glyphs (ProjectItem.svelte:2083-2102).
            if showsBusyShimmer {
                ShimmerLabel(
                    text: node.project.name,
                    color: NSColor(Theme.foreground)
                )
            } else {
                Text(node.project.name)
                    .font(Theme.rowLabelFont)
                    .foregroundStyle(
                        isChildFolder ? Theme.foreground : Theme.foreground.opacity(0.6)
                    )
                    .lineLimit(1)
                    .truncationMode(.tail)
            }

            // .project-branch (ProjectItem.svelte:1270-1276, 2195-2215):
            // 12px branch icon + mono branch name at 0.55 opacity; the name
            // is omitted when it equals the project title.
            if let branch = node.project.worktreeBranch {
                HStack(spacing: 3) {
                    ChromeIconView(icon: .branch, size: 12)
                    if branch != node.project.name {
                        Text(branch)
                            .font(.system(size: 10, design: .monospaced))
                            .lineLimit(1)
                            .truncationMode(.tail)
                            .frame(maxWidth: 110, alignment: .leading)
                    }
                }
                .foregroundStyle(Theme.mutedForeground)
                .opacity(0.55)
            }

            Spacer(minLength: 4)

            if showsActions {
                // No disclosure chevron here — the Svelte project row has
                // none either; the whole row toggles open/closed.
                QuickPresetStrip(
                    quickGroups: quickGroups,
                    menuPresets: menuPresets,
                    forceExpanded: debugHover,
                    onLaunch: onLaunchPreset,
                    onManagePresets: onManagePresets,
                    showsManagePresets: showsLocalProjectVerbs,
                    onLaunchInWorktree: onLaunchPresetInWorktree,
                    archivedCount: archivedSessionCount,
                    onOpenArchived: onOpenArchived
                )
            } else if let shortcutHint {
                // Held ⌃ shows the project-switch hint (same 9px/500 @ 0.7
                // treatment as the session rows' ⌘N hint).
                Text("⌃\(shortcutHint)")
                    .font(.system(size: 9, weight: .medium))
                    .foregroundStyle(Theme.mutedForeground)
                    .opacity(0.7)
            } else if isChildFolder {
                // Inline child folder rows keep the session-count capsule
                // as trailing meta; the disclosure chevron leads the row now
                // (leadingMark). Live status reuses the standard project-row
                // indicators — attention/unread dot on the mark while
                // collapsed, busy shimmer on the name. Hover swaps this for
                // the quick-preset strip, like every row.
                if !node.sessions.isEmpty {
                    Text("\(node.sessions.count)")
                        .font(.system(size: 10))
                        .foregroundStyle(Theme.mutedForeground)
                        .frame(minHeight: 16)
                        .padding(.horizontal, 5)
                        .background(Capsule().fill(Theme.hoverRow))
                }
            }
        }
        .padding(EdgeInsets(top: 2, leading: rowLeading, bottom: 2, trailing: 7))
        .frame(minHeight: 28)
        .background(
            RoundedRectangle(cornerRadius: 9, style: .continuous)
                .fill(
                    isSessionDropTarget
                        ? Theme.accent.opacity(0.10)
                        : (showsActions ? Theme.hoverRow : .clear)
                )
        )
        .overlay {
            if isSessionDropTarget {
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .stroke(Theme.accent.opacity(0.55), lineWidth: 1)
            }
        }
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .onTapGesture { onToggle() }
        // Project context menu (openProjectMenu, ProjectItem.svelte:818-941).
        // Order: New session · [New worktree…] · [Stop all] · [Archived] ·
        // ── · Reveal in Finder · Open in <Editor> · ── · Remove. Stop all
        // is non-destructive (stopSession per live session — rows stay as
        // exited). Unpeel Sessions MCP access is set per session.
        .contextMenu {
            if isChildFolder, showsLocalProjectVerbs {
                Button("Rename") {
                    onRenameWorktree()
                }
                Divider()
            }
            Menu("New session") {
                Button("Terminal") { onLaunchPreset(.newTerminal) }
                if !menuPresets.isEmpty {
                    Divider()
                    ForEach(menuPresets) { preset in
                        Button(preset.label) { onLaunchPreset(preset) }
                    }
                }
                if let onLaunchPresetInWorktree {
                    Divider()
                    Menu("In a new worktree") {
                        Button("Terminal") { onLaunchPresetInWorktree(.newTerminal) }
                        if !menuPresets.isEmpty {
                            Divider()
                            ForEach(menuPresets) { preset in
                                Button(preset.label) { onLaunchPresetInWorktree(preset) }
                            }
                        }
                    }
                }
                if showsLocalProjectVerbs {
                    Divider()
                    Button("Manage presets…") { onManagePresets() }
                }
            }
            // Folder color is a MAIN-project verb: groups and worktrees stay
            // neutral so nesting reads by indent, not tint (decided
            // 2026-08-13 after a group picked up a color by accident).
            if node.project.parentProjectID == nil, showsLocalProjectVerbs {
                Menu("Folder color") {
                    Button {
                        onSetFolderColor(nil)
                    } label: {
                        FolderColorMenuRow(
                            title: "Default",
                            color: nil,
                            isSelected: folderColor == nil
                        )
                    }
                    Divider()
                    ForEach(ProjectFolderColor.allCases) { color in
                        Button {
                            onSetFolderColor(color)
                        } label: {
                            FolderColorMenuRow(
                                title: color.title,
                                color: color,
                                isSelected: folderColor == color
                            )
                        }
                    }
                }
            }
            // Per-group sort: custom (the manual drag order, the default) or
            // recently updated (last activity, like All recent). Date sort
            // disables drag re-ordering for the group; the stored manual
            // order survives a switch back. Local-only: sort modes live in
            // app-state.json and have no remote operation yet.
            if showsLocalProjectVerbs {
            Menu("Sort sessions") {
                Picker("Sort sessions", selection: Binding(
                    get: { isDateSorted },
                    set: { onSetSessionDateSorted($0) }
                )) {
                    Text("Custom order").tag(false)
                    Text("Recently updated").tag(true)
                }
                .pickerStyle(.inline)
                .labelsHidden()
            }
            }
            if showsWorkspacesToggle {
                Button("New worktree…") {
                    onCreateWorktree()
                }
            }
            if node.project.parentProjectID == nil, showsLocalProjectVerbs {
                Button("New group…") {
                    onCreateGroup()
                }
            }
            if hasLiveSessions {
                Button("Stop all") {
                    onStopAll()
                }
            }
            if archivedSessionCount > 0 {
                Button("Archived (\(archivedSessionCount))") {
                    onOpenArchived()
                }
            }
            if showsLocalProjectVerbs {
                Divider()
                Button("Reveal in Finder") {
                    onRevealInFinder()
                }
                Button("Open in \(editorName)") {
                    onOpenInEditor()
                }
                Divider()
                if isWorktreeChild {
                    Button("Remove worktree", role: .destructive) {
                        onRemoveWorktree()
                    }
                } else if isChildFolder {
                    // Groups archive their sessions under the parent before
                    // the organizational record is removed.
                    Button("Remove group", role: .destructive) {
                        onRemoveGroup()
                    }
                } else {
                    Button("Remove", role: .destructive) {
                        onRequestRemove()
                    }
                }
            }
        }
        .animation(.easeInOut(duration: 0.12), value: hovering)
        .animation(.easeInOut(duration: 0.12), value: isSessionDropTarget)
    }
}

private struct FolderColorMenuRow: View {
    let title: String
    let color: ProjectFolderColor?
    let isSelected: Bool

    var body: some View {
        Label {
            Text(title)
        } icon: {
            Image(nsImage: FolderColorMenuSwatch.image(color: nsColor, selected: isSelected))
                .renderingMode(.original)
        }
    }

    private var nsColor: NSColor {
        color?.nsColor ?? NSColor(hex: 0xB8BCC8)
    }
}

@MainActor
private enum FolderColorMenuSwatch {
    /// Cached per (resolved color, selected): a fresh NSImage on every menu
    /// re-evaluation gives the items a new identity each time the sidebar
    /// re-renders, which makes an open Folder-color submenu blink.
    private static var cache: [String: NSImage] = [:]

    static func image(color: NSColor, selected: Bool) -> NSImage {
        let rgb = color.usingColorSpace(.sRGB) ?? color
        let key = String(
            format: "%.3f-%.3f-%.3f-%.3f-%@",
            rgb.redComponent, rgb.greenComponent, rgb.blueComponent,
            rgb.alphaComponent, selected ? "on" : "off"
        )
        if let cached = cache[key] { return cached }
        let image = draw(color: color, selected: selected)
        cache[key] = image
        return image
    }

    private static func draw(color: NSColor, selected: Bool) -> NSImage {
        let image = NSImage(size: NSSize(width: 18, height: 18))
        image.lockFocus()
        defer { image.unlockFocus() }

        NSGraphicsContext.current?.imageInterpolation = .high

        let shadow = NSShadow()
        shadow.shadowBlurRadius = 2
        shadow.shadowOffset = NSSize(width: 0, height: -0.5)
        shadow.shadowColor = NSColor.black.withAlphaComponent(0.22)
        shadow.set()

        let chip = NSBezierPath(roundedRect: NSRect(x: 3, y: 3, width: 12, height: 12),
                                xRadius: 4, yRadius: 4)
        color.withAlphaComponent(0.94).setFill()
        chip.fill()

        NSShadow().set()
        NSColor.white.withAlphaComponent(0.58).setStroke()
        chip.lineWidth = 1
        chip.stroke()

        if selected {
            let check = NSBezierPath()
            check.move(to: NSPoint(x: 6.1, y: 8.8))
            check.line(to: NSPoint(x: 8.0, y: 6.7))
            check.line(to: NSPoint(x: 12.2, y: 11.4))
            check.lineWidth = 1.8
            check.lineCapStyle = .round
            check.lineJoinStyle = .round
            NSColor.white.setStroke()
            check.stroke()
        }

        image.isTemplate = false
        return image
    }
}

// MARK: - Quick preset strip (ProjectItem.svelte:1292-1357, 1964-2066)

/// The "+" affordance on project-row hover. Collapsed it is a 24px pill
/// showing just the "+"; hovering the pill expands it leftwards
/// (inline-size 0.28s cubic-bezier(0.22,1,0.36,1)) to reveal one icon
/// button per quick preset. The strip is laid out row-reverse in the web
/// app, so visually it reads terminal → … → codex → claude → "+".
/// Clicking an icon launches that preset; "+" is New Session (the Svelte
/// "+" opens the launcher view — the native stand-in is a preset menu).
struct QuickPresetStrip: View {
    /// Starred presets grouped by CLI — one chip per group. A group with a
    /// single starred preset launches it directly; 2+ starred presets of one
    /// CLI render the chip as a dropdown menu of those presets.
    let quickGroups: [QuickPresetGroup]
    let menuPresets: [Preset]
    var forceExpanded = false
    let onLaunch: (Preset) -> Void
    var onManagePresets: () -> Void = {}
    var showsManagePresets = true
    var onLaunchInWorktree: ((Preset) -> Void)?
    var archivedCount = 0
    var onOpenArchived: (() -> Void)?

    @State private var hovering = false
    @State private var plusHovering = false

    private var expanded: Bool { hovering || forceExpanded }

    /// Expanded width: content is (n+1) 22px chips + n 1px gaps + 3px
    /// horizontal padding each side; add 2px slack so the leftmost chip
    /// keeps its left padding instead of clipping against the pill edge.
    /// (n = CLI chips + the blank-terminal chip.)
    private var expandedWidth: CGFloat { CGFloat(quickGroups.count + 1) * 23 + 30 }

    var body: some View {
        strip
            .onHover { hovering = $0 }
            .animation(
                .timingCurve(0.22, 1, 0.36, 1, duration: 0.28),
                value: expanded
            )
    }

    private var strip: some View {
        HStack(spacing: 1) {
            // row-reverse: the first group (topmost starred CLI) renders
            // rightmost, next to "+"; the blank terminal chip sits leftmost.
            QuickPresetButton(preset: .newTerminal) { onLaunch(.newTerminal) }
            ForEach(quickGroups.reversed()) { group in
                if group.presets.count > 1 {
                    QuickPresetMenuChip(group: group, onLaunch: onLaunch)
                } else {
                    QuickPresetButton(preset: group.leader) { onLaunch(group.leader) }
                }
            }
            newSessionMenu
        }
        .padding(.horizontal, 3)
        .padding(.vertical, 1)
        .frame(width: expanded ? expandedWidth : 28, height: 24, alignment: .trailing)
        .clipped()
    }

    /// The trailing "+" (Svelte: onNewSession → SessionLauncherView, which
    /// lists the blank terminal first, then the presets). The native app
    /// has no launcher screen yet, so this is a menu of the same choices.
    private var newSessionMenu: some View {
        Menu {
            newSessionMenuContent(
                menuPresets: menuPresets,
                onLaunch: onLaunch,
                onManagePresets: onManagePresets,
                showsManagePresets: showsManagePresets,
                onLaunchInWorktree: onLaunchInWorktree,
                archivedCount: archivedCount,
                onOpenArchived: onOpenArchived
            )
        } label: {
            // plusIcon (icons.ts:21) at 16px, centered in the same 22×22
            // radius-8 hover chip as QuickPresetButton so hover matches.
            ChromeIconView(icon: .plus, size: 16)
                .foregroundStyle(plusHovering ? Theme.foreground : Theme.mutedForeground)
                .frame(width: 22, height: 22)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .frame(width: 22, height: 22)
        .background(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(
                    plusHovering
                        ? Theme.foreground.opacity(0.14)
                        : Theme.mutedForeground.opacity(0.10)
                )
        )
        .background(HoverReporter { plusHovering = $0 })
        .help("New session")
    }
}

/// AppKit tracking-area hover reporter. SwiftUI's `.onHover` does not fire
/// over a `Menu`'s label on macOS (the menu swallows the tracking), so we
/// drop a geometric tracking view behind the menu — `mouseEntered/Exited`
/// fire on cursor crossing regardless of what is drawn on top.
struct HoverReporter: NSViewRepresentable {
    let onChange: (Bool) -> Void

    func makeNSView(context: Context) -> NSView { TrackingView(onChange: onChange) }
    func updateNSView(_ nsView: NSView, context: Context) {
        (nsView as? TrackingView)?.onChange = onChange
    }

    final class TrackingView: NSView {
        var onChange: (Bool) -> Void
        init(onChange: @escaping (Bool) -> Void) {
            self.onChange = onChange
            super.init(frame: .zero)
        }
        @available(*, unavailable) required init?(coder: NSCoder) { nil }

        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            trackingAreas.forEach(removeTrackingArea)
            addTrackingArea(NSTrackingArea(
                rect: bounds,
                options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
                owner: self
            ))
        }
        override func mouseEntered(with event: NSEvent) { onChange(true) }
        override func mouseExited(with event: NSEvent) { onChange(false) }
    }
}

struct PresetMenuButton: View {
    let preset: Preset
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label {
                Text(preset.label)
            } icon: {
                ToolIconView(command: preset.command, size: 16)
            }
        }
    }
}

/// Shared "+" new-session dropdown content: blank terminal first, then every
/// available preset, then "Manage presets…". Used by the sidebar project "+"
/// (`QuickPresetStrip.newSessionMenu`), the empty-state placeholder, and the
/// collapsed-sidebar title-bar "+" so all three offer the same presets.
/// `onLaunchInWorktree` (git-repo projects only, nil elsewhere) adds an
/// "In a new worktree" submenu of the same choices — the one-shot
/// session-in-a-fresh-worktree flow (`promptNewWorktreeSession`).
/// `archivedCount`/`onOpenArchived` (project-scoped "+"s only) add an
/// "Archived (N)" entry after the presets that opens the project's archive
/// library in the main pane — same destination as the context-menu item.
@MainActor
@ViewBuilder
func newSessionMenuContent(
    menuPresets: [Preset],
    onLaunch: @escaping (Preset) -> Void,
    onManagePresets: @escaping () -> Void,
    showsManagePresets: Bool = true,
    onLaunchInWorktree: ((Preset) -> Void)? = nil,
    archivedCount: Int = 0,
    onOpenArchived: (() -> Void)? = nil
) -> some View {
    PresetMenuButton(preset: .newTerminal) {
        onLaunch(.newTerminal)
    }
    if !menuPresets.isEmpty {
        Divider()
        ForEach(menuPresets) { preset in
            PresetMenuButton(preset: preset) {
                onLaunch(preset)
            }
        }
    }
    if let onLaunchInWorktree {
        Divider()
        Menu {
            PresetMenuButton(preset: .newTerminal) {
                onLaunchInWorktree(.newTerminal)
            }
            if !menuPresets.isEmpty {
                Divider()
                ForEach(menuPresets) { preset in
                    PresetMenuButton(preset: preset) {
                        onLaunchInWorktree(preset)
                    }
                }
            }
        } label: {
            Label {
                Text("In a new worktree")
            } icon: {
                ChromeIconView(icon: .branch, size: 16)
            }
        }
    }
    Divider()
    if archivedCount > 0, let onOpenArchived {
        Button {
            onOpenArchived()
        } label: {
            Label {
                Text("Archived (\(archivedCount))")
            } icon: {
                Image(systemName: "archivebox")
            }
        }
    }
    if showsManagePresets {
        Button {
            onManagePresets()
        } label: {
            Label {
                Text("Manage presets…")
            } icon: {
                Image(systemName: "slider.horizontal.3")
            }
        }
    }
}

/// 22×22 radius-8 icon button: 14px tool icon at 0.72 opacity, muted →
/// foreground + full opacity on hover (ProjectItem.svelte:2048-2066).
private struct QuickPresetButton: View {
    let preset: Preset
    let action: () -> Void

    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            ToolIconView(command: preset.command)
                .opacity(hovering ? 1 : 0.72)
                .foregroundStyle(hovering ? Theme.foreground : Theme.mutedForeground)
                .frame(width: 22, height: 22)
                .background(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(hovering ? Theme.hoverRow : .clear)
                )
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .help("Start \(preset.label)")
    }
}

/// Chip for a CLI with 2+ starred presets: same 22×22 icon chip as
/// `QuickPresetButton`, but clicking opens a menu of that CLI's starred
/// presets (borderless Menu + HoverReporter, like the strip's "+").
private struct QuickPresetMenuChip: View {
    let group: QuickPresetGroup
    let onLaunch: (Preset) -> Void

    @State private var hovering = false

    var body: some View {
        Menu {
            ForEach(group.presets) { preset in
                PresetMenuButton(preset: preset) {
                    onLaunch(preset)
                }
            }
        } label: {
            ToolIconView(command: group.leader.command)
                .opacity(hovering ? 1 : 0.72)
                .foregroundStyle(hovering ? Theme.foreground : Theme.mutedForeground)
                .frame(width: 22, height: 22)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .frame(width: 22, height: 22)
        .background(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(hovering ? Theme.hoverRow : .clear)
        )
        .background(HoverReporter { hovering = $0 })
        .help("Start \(group.cli.displayName)…")
    }
}

// MARK: - Session row

/// Exact resume affordance rendered by a sidebar row. Keeping this decision
/// pure makes the active-runtime, returned-shell, and archived states
/// testable without reaching through SwiftUI's private view tree.
enum SessionRowResumePresentation: Equatable {
    case none
    case resumeAgent
    case resumeSession
    case restore
    case restoreAndResume

    var title: String? {
        switch self {
        case .none: return nil
        case .resumeAgent: return "Resume Agent"
        case .resumeSession: return "Resume"
        case .restore: return "Restore from archive"
        case .restoreAndResume: return "Restore & Resume"
        }
    }
}

func sessionRowResumePresentation(
    session: SessionEntry,
    isArchived: Bool,
    canRestart: Bool,
    canResumeAgent: Bool
) -> SessionRowResumePresentation {
    if isArchived {
        return canRestart ? .restoreAndResume : .restore
    }
    guard session.status != .starting else { return .none }
    if session.isLive {
        return canResumeAgent ? .resumeAgent : .none
    }
    return canRestart ? .resumeSession : .none
}

struct SessionRowView: View {
    let session: SessionEntry
    let depth: Int
    /// Leading padding base; depth supplies one standard 14pt nesting step.
    var indentBase: CGFloat = 9
    let isSelected: Bool
    let isPinned: Bool
    /// 1-based ⌘ index shown in place of the age while ⌘ is held
    /// (.session-shortcut-hint, ProjectItem.svelte:1536-1540).
    var shortcutHint: Int?
    /// Unread badge: 7px #60a5fa dot in the leading slot (where the busy
    /// spinner sits), lowest precedence after spinner/attention.
    var isUnread = false
    /// Inline remove confirmation: the whole row swaps to
    /// "Remove session?" + Remove/Cancel (the Svelte app swaps only the
    /// hover archive button to a "Confirm" pill, ProjectItem.svelte:1562).
    var isConfirmingRemove = false
    /// Inline archive confirmation, shown only for actively-working sessions
    /// (archiving stops the agent mid-turn); settled rows archive directly.
    var isConfirmingArchive = false
    /// Kill/cleanup in flight: row disabled, meta shows "removing".
    var isRemoving = false
    /// Restart in flight: row disabled, meta shows "restarting"
    /// (ProjectItem.svelte:1534-1535).
    var isRestarting = false
    /// In-place provider resume; the terminal remains mounted.
    var isResumingAgent = false
    /// Archive in flight (live host still stopping): row muted + disabled,
    /// leading slot shows a muted spinner, meta shows "archiving". The row
    /// disappears into the archive once the stop completes.
    var isArchiving = false
    /// Already archived (a recent archive still showing in the stopped
    /// group): the archive affordances swap to Restore.
    var isArchived = false
    /// Inline rename editor (double-click on the label or context-menu
    /// Rename — `editingSessionId` in ProjectItem.svelte:146): the label
    /// swaps to a TextField pre-filled with the current title. Enter
    /// commits, Esc cancels, click-away (focus loss) commits — matching the
    /// Svelte contenteditable's keydown/blur handlers
    /// (ProjectItem.svelte:958-986, 1507-1511). Empty/unchanged input
    /// reverts to the original label.
    var isEditing = false
    /// Whether this session is opted into a "finished" phone push.
    var notifyWhenDone = false
    /// Whether Resume continues this stopped Session's conversation (known
    /// agent CLI, or a blank shell with none to lose). Gates the Resume item
    /// (ProviderCapabilities.canRestart).
    var canRestart = true
    /// Whether an ended managed launch can resume inside this same terminal.
    var canResumeAgent = false
    /// Whether Stop and archive / Archive preserves a resumable launch.
    var canArchive = true
    /// Whether this session's provider CLI supports a native fork (Claude/Codex).
    /// Gates the context-menu Fork item.
    var canFork = false
    /// Whether this session's provider accepts extra system context on launch.
    var canAppendSystemContext = false
    /// Whether this session's provider reports turn completion through hooks,
    /// making the "Notify when done" push reliable. Gates that toggle.
    var canNotifyWhenDone = true
    /// Whether "Clear attention" applies — the Controller-local activity
    /// engine's escape hatch, hidden for remote Hosts.
    var canClearAttention = true
    /// Whether the experimental Sessions MCP feature is on — gates the raw
    /// session-id copy affordance.
    var sessionsMcpEnabled = false
    /// Whether to show the session's agent-CLI logo after the date
    /// (Appearance ▸ "Show agent logos"; on by default).
    var showsToolIcon = false
    /// Recently-updated groups show the lifecycle event age that ranked the
    /// row; custom groups leave this nil and retain creation age.
    var ageTimestampMs: Int64? = nil
    /// Non-nil when the row can be drag-reordered. The LEADING SLOT is the
    /// drag handle: on hover it swaps to a grip icon and `.onDrag` lives
    /// there — never on the whole row, where it would race the tap-to-select
    /// and double-click-rename gestures.
    var dragProvider: (() -> NSItemProvider)?
    let onSelect: () -> Void
    /// Fired only after a deliberate pointer dwell — switch intent. Keeping
    /// this well behind the visual hover prevents a fast sweep through a long
    /// list from mounting terminal panes while the pointer is still moving.
    var onHoverIntent: () -> Void = {}
    /// Opt this session in/out of the "notify when done" push.
    var onSetNotifyWhenDone: (Bool) -> Void = { _ in }
    let onTogglePin: () -> Void
    var onResume: () -> Void = {}
    /// Force-clear a stuck/false attention badge (offered only while the
    /// row shows one).
    var onClearAttention: () -> Void = {}
    /// Fork this session into an independent conversation branch.
    var onFork: () -> Void = {}
    /// Save provider system context to apply the next time it resumes.
    var onAppendSystemContext: () -> Void = {}
    var onRequestRemove: () -> Void = {}
    var onConfirmRemove: () -> Void = {}
    var onCancelRemove: () -> Void = {}
    /// Archive this session (stop it if running, keep everything on disk,
    /// move the row into the stopped group / archive library). Busy sessions
    /// confirm inline.
    var onArchive: () -> Void = {}
    /// Restore a recent archived row, resuming when its launch supports it.
    var onUnarchive: () -> Void = {}
    /// Confirmed archive from the inline row — skips the busy re-check that
    /// `onArchive` routes through (it would just re-arm the confirm).
    var onConfirmArchive: () -> Void = {}
    var onCancelArchive: () -> Void = {}
    /// Copy this session's conversation transcript as Markdown (rendered by the
    /// host using the shared Settings ▸ Transcripts content toggles). The Int
    /// is the flyout's range pick: entry count, or 0 for the whole conversation.
    var onCopyTranscript: (Int) -> Void = { _ in }
    var onBeginEdit: () -> Void = {}
    var onCommitRename: (String) -> Void = { _ in }
    var onEndEdit: () -> Void = {}
    /// "Move to" destinations (plain groups, or back to the root project);
    /// empty hides the menu. Worktrees require restart/resume instead.
    var moveTargets: [SessionMoveTarget] = []
    var onMoveTo: (String) -> Void = { _ in }

    @State private var hovering = false
    @State private var handleHovering = false
    @State private var hoverIntentTask: Task<Void, Never>?
    @State private var renameDraft = ""
    @FocusState private var renameFocused: Bool

    /// UNPEEL_DEBUG_HOVER_SESSION=<session-id> forces this row's hover
    /// state so snapshots can photograph the hover-only pin swap.
    private static let debugHoverSession =
        ProcessInfo.processInfo.environment["UNPEEL_DEBUG_HOVER_SESSION"]
    private static let hoverIntentDelay: UInt64 = 450_000_000

    private var isHovering: Bool {
        hovering || Self.debugHoverSession == session.id
    }

    var body: some View {
        if isConfirmingRemove {
            confirmRemoveRow
        } else if isConfirmingArchive {
            confirmArchiveRow
        } else {
            normalRow
        }
    }

    private var normalRow: some View {
        HStack(spacing: 7) {
            leadingSlot

            if isEditing {
                renameField
            } else {
                HStack(spacing: 5) {
                    Text(session.label)
                        .font(Theme.sessionLabelFont)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        // Double-click the label → inline rename
                        // (ondblclick → startEditing, ProjectItem.svelte:1503).
                        // simultaneousGesture so the row's single-tap select is
                        // never blocked by the pending double-tap.
                        .simultaneousGesture(
                            TapGesture(count: 2).onEnded { onBeginEdit() }
                        )
                }
            }

            Spacer(minLength: 4)

            // Trailing cluster: the pin rides here, just before the meta,
            // instead of the leading slot. On hover it sits next to the
            // archive button; otherwise it sits before the date.
            HStack(spacing: 4) {
                // Resume sits left of the pin on hover. The cluster
                // is trailing-aligned, so it grows leftward — the pin and
                // archive never move.
                if isHovering, showsInlineRestart, !isRemoving,
                   !isRestarting, !isResumingAgent, !isArchiving {
                    RestartActionButton(action: onResume)
                        .padding(.trailing, -2)
                }

                // Pin: resting (0.9) for pinned rows, full on hover so any
                // row can pin/unpin.
                if pinOpacity > 0 {
                    PinActionButton(isPinned: isPinned, action: onTogglePin)
                        .opacity(pinOpacity)
                        // The 22px button centers a 13px glyph, so pull it back
                        // out toward the date to close the visual gap and line
                        // the pin up under the Worktrees branch icon.
                        .padding(.trailing, -4)
                }

                if isRemoving || isRestarting || isResumingAgent || isArchiving {
                    Text(
                        isRemoving
                            ? "removing"
                            : isArchiving
                                ? "archiving"
                                : isResumingAgent
                                    ? "resuming agent"
                                    : (session.isLive ? "reloading" : "resuming")
                    )
                    .font(.system(size: 9))
                    .opacity(0.7)
                } else {
                    // Fixed-width meta slot so the hover swap (age → archive)
                    // never changes the cluster width — the pin stays put and
                    // only the trailing content cross-fades in place.
                    ZStack(alignment: .trailing) {
                        if isHovering, !isArchived {
                            // Hover swap: the age hides and the action
                            // affordances appear. Overflowing the slot via the
                            // negative padding nudges the 13px glyphs to the
                            // row's trailing edge without reflowing the pin.
                            // Non-resumable commands can't meaningfully be
                            // archived (nothing to resume later), so their
                            // clear-it-out affordance is Remove.
                            if canArchive {
                                ArchiveActionButton(
                                    help: session.isLive ? "Stop and archive" : "Archive",
                                    action: onArchive
                                )
                                .padding(.trailing, -4)
                            } else {
                                RemoveActionButton(action: onRequestRemove)
                                    .padding(.trailing, -4)
                            }
                        } else if let shortcutHint {
                            // Held ⌘ swaps the age for the ⌘N hint
                            // (ProjectItem.svelte:1536-1540, 9px/500 @ 0.7).
                            Text("⌘\(shortcutHint)")
                                .font(.system(size: 9, weight: .medium))
                                .opacity(0.7)
                        } else {
                            Text(session.ageString(since: ageTimestampMs))
                                .font(.system(size: 9))
                                .opacity(0.7)
                        }
                    }
                    .frame(width: 24, alignment: .trailing)
                }

                if showsToolIcon {
                    SessionCommandIcon(command: session.presentationCommand)
                        .padding(.leading, 3)
                }
            }
        }
        .foregroundStyle(
            session.isLive && !isArchiving ? Theme.foreground : Theme.mutedForeground
        )
        // Exited rows read as clearly stopped: a hard dim, not the barely
        // visible 0.82 wash they used to get. Hover lifts the dim so the
        // Restart/Archive affordances (and the label) stay readable.
        .opacity(
            (isRemoving || isRestarting || isResumingAgent || isArchiving)
                ? 0.5
                : (session.isLive ? 1 : (isHovering ? 0.9 : 0.55))
        )
        .padding(EdgeInsets(top: 2, leading: indentBase + CGFloat(depth) * 14, bottom: 2, trailing: 9))
        .frame(minHeight: 28)
        .background(
            RoundedRectangle(cornerRadius: 9, style: .continuous)
                .fill(
                    isSelected
                        ? (Self.liquidGlassAvailable ? .clear : Theme.activeRow)
                        : (isHovering ? Theme.hoverRow : .clear)
                )
        )
        // Selected row gets real Liquid Glass instead of a flat fill; hover
        // stays a cheap flat wash (a glass backdrop per hovered row would
        // churn the compositor while scrolling).
        .selectedRowGlass(isSelected)
        .contentShape(Rectangle())
        .onHover { inside in
            guard hovering != inside else { return }
            hovering = inside
            hoverIntentTask?.cancel()
            hoverIntentTask = nil
            if inside, session.isAttachable, !isSelected {
                hoverIntentTask = Task { @MainActor in
                    try? await Task.sleep(nanoseconds: Self.hoverIntentDelay)
                    guard !Task.isCancelled else { return }
                    onHoverIntent()
                }
            }
        }
        .onDisappear {
            hoverIntentTask?.cancel()
            hoverIntentTask = nil
        }
        .onTapGesture { onSelect() }
        .contextMenu {
            regularContextMenuItems
        }
        .allowsHitTesting(!isRemoving && !isRestarting && !isResumingAgent && !isArchiving)
    }

    @ViewBuilder
    private var regularContextMenuItems: some View {
        // Same first item as the Svelte session context menu
        // (session-menu-rename, ProjectItem.svelte:1038-1043).
        Button("Rename") {
            onBeginEdit()
        }
        Button(isPinned ? "Unpin from project" : "Pin in project") {
            onTogglePin()
        }
        // File the session under a plain group (or back to the root project)
        // using a shared project-override marker, so the TUI and phone see
        // the same placement. Worktrees require restart/resume.
        if !moveTargets.isEmpty {
            Menu("Move to") {
                ForEach(moveTargets) { target in
                    Button(target.name) { onMoveTo(target.id) }
                }
            }
        }
        // Escape hatch for a stuck or false attention badge (a missed
        // hook, or menu detection tripping on look-alike screen text) —
        // otherwise nothing short of answering/restarting clears it.
        if session.status == .attention, canClearAttention {
            Button("Clear attention") {
                onClearAttention()
            }
        }
        // Verb items are capability-gated per CLI (ProviderCapabilities —
        // the same answers the phone's session sheet gets): no Resume for
        // commands whose conversation a relaunch would silently lose, no
        // Fork without a native fork primitive, no context append without
        // an append flag, no notify toggle without hook Stop events.
        // (There is no separate "Stop" verb: "Stop and archive" below is
        // the stop — it kills the hosted terminal and the row settles into
        // the stopped group, from where Resume continues the conversation.)
        if session.status != .starting {
            // An ended managed launch can resume from the still-live shell in
            // the existing terminal. A stopped Session uses the legacy
            // replacement operation and presents that honestly as Resume.
            if resumePresentation == .resumeAgent {
                Button("Resume Agent") {
                    onResume()
                }
            } else if resumePresentation == .resumeSession {
                Button("Resume") {
                    onResume()
                }
            }
            // Fork branches the provider's conversation into a NEW session
            // and leaves this one running (Claude/Codex only — the CLIs with
            // a native fork primitive). Same-CLI by nature.
            if canFork {
                Button("Fork") {
                    onFork()
                }
            }
            if canAppendSystemContext {
                Button("Append system context…") {
                    onAppendSystemContext()
                }
            }
        }
        // "Notify when done" — push a phone notification when this session
        // next finishes a turn (paired iPhone with notifications on).
        if canNotifyWhenDone {
            Toggle("Notify when done", isOn: Binding(
                get: { notifyWhenDone },
                set: { onSetNotifyWhenDone($0) }
            ))
        }
        Divider()
        // Range flyout; the content toggles still come from
        // Settings ▸ Transcripts. 0 = whole conversation.
        Menu("Copy transcript") {
            Button("Last 20 entries") {
                onCopyTranscript(20)
            }
            Button("Last 50 entries") {
                onCopyTranscript(50)
            }
            Button("Whole conversation") {
                onCopyTranscript(0)
            }
        }
        .help("Copy the conversation as Markdown, using your Settings ▸ Transcripts content options")
        // The raw session id only matters as a Sessions MCP tool target.
        if sessionsMcpEnabled {
            Button("Copy session ID") {
                copyToPasteboard("Unpeel Session ID: \(session.id)")
            }
        }
        Divider()
        // Archive is the non-destructive "file it away" — and for live
        // sessions it is also the stop verb: kill the hosted terminal, keep
        // the whole session on disk, move the row into the stopped group.
        // Working sessions get an inline confirm first. Already-archived
        // resumable rows offer the combined Restore & Resume action; an
        // auto-archived non-resumable command can still be plainly restored.
        if isArchived {
            Button(resumePresentation.title ?? "Restore from archive") {
                onUnarchive()
            }
        } else if canArchive {
            // Only resumable commands offer Archive; for the rest, Remove
            // below is the sole clear-it-out verb.
            Button(session.isLive ? "Stop and archive" : "Archive") {
                onArchive()
            }
        }
        // Remove stays the explicit destructive verb: it deletes the
        // session directory (transcript and artifacts included).
        Button(session.isLive ? "Remove session" : "Remove from list", role: .destructive) {
            onRequestRemove()
        }
    }

    private func copyToPasteboard(_ value: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(value, forType: .string)
    }

    // MARK: Inline rename (startEditing/commitEdit/cancelEdit,
    // ProjectItem.svelte:943-986)

    /// Same metrics as the label it replaces (Theme.sessionLabelFont). NSTextField
    /// select-alls when it becomes first responder, matching the Svelte
    /// range-selectNodeContents in startEditing.
    private var renameField: some View {
        TextField("", text: $renameDraft)
            .textFieldStyle(.plain)
            .font(Theme.sessionLabelFont)
            .foregroundStyle(Theme.foreground)
            .focused($renameFocused)
            .onSubmit(commitRename) // Enter commits
            .onExitCommand(perform: cancelRename) // Esc cancels
            .onAppear {
                renameDraft = session.label
                // Defer one runloop turn so the field is in the window
                // before claiming first responder (select-all happens then).
                DispatchQueue.main.async { renameFocused = true }
            }
            .onChange(of: renameFocused) { focused in
                // Click-away commits, like the Svelte onblur handler
                // (ProjectItem.svelte:1507-1511). isEditing guards the
                // post-commit/cancel focus resignation.
                if !focused && isEditing { commitRename() }
            }
    }

    /// Empty or unchanged input reverts to the original label
    /// (commitEdit, ProjectItem.svelte:958-966).
    private func commitRename() {
        let trimmed = renameDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty && trimmed != session.label {
            onCommitRename(trimmed)
        }
        onEndEdit()
    }

    private func cancelRename() {
        onEndEdit()
    }

    /// The row itself becomes the confirmation: question + destructive
    /// Remove + Cancel. Esc or clicking anywhere else cancels.
    private var confirmRemoveRow: some View {
        HStack(spacing: 7) {
            Text(session.isLive ? "Remove session?" : "Remove from list?")
                .font(Theme.rowLabelFont)
                .foregroundStyle(Theme.foreground)
                .lineLimit(1)

            Spacer(minLength: 4)

            ConfirmPillButton(label: "Cancel", destructive: false, action: onCancelRemove)
            ConfirmPillButton(label: "Remove", destructive: true, action: onConfirmRemove)
        }
        .padding(EdgeInsets(top: 2, leading: indentBase + CGFloat(depth) * 14, bottom: 2, trailing: 5))
        .frame(minHeight: 28)
        .background(
            RoundedRectangle(cornerRadius: 9, style: .continuous)
                .fill(Theme.hoverRow)
        )
        .contentShape(Rectangle())
        .background(RemoveConfirmDismissMonitor(onCancel: onCancelRemove))
    }

    /// Archive confirmation — only reached for actively-working sessions
    /// (archiving stops the agent mid-turn; settled rows archive without
    /// asking). Same inline pattern as the remove confirm.
    private var confirmArchiveRow: some View {
        HStack(spacing: 7) {
            Text("Stop and archive session?")
                .font(Theme.rowLabelFont)
                .foregroundStyle(Theme.foreground)
                .lineLimit(1)

            Spacer(minLength: 4)

            ConfirmPillButton(label: "Cancel", destructive: false, action: onCancelArchive)
            ConfirmPillButton(label: "Archive", destructive: true, action: onConfirmArchive)
        }
        .padding(EdgeInsets(top: 2, leading: indentBase + CGFloat(depth) * 14, bottom: 2, trailing: 5))
        .frame(minHeight: 28)
        .background(
            RoundedRectangle(cornerRadius: 9, style: .continuous)
                .fill(Theme.hoverRow)
        )
        .contentShape(Rectangle())
        .background(RemoveConfirmDismissMonitor(onCancel: onCancelArchive))
    }

    /// Fixed leading icon slot (ProjectItem.svelte .session-leading,
    /// :2355-2364): EVERY row reserves a constant 16×16 slot so labels align
    /// whether or not a status/pin is showing. Occupants stack and cross-fade
    /// via opacity only — the pin transition is `opacity 0.12s ease` (:2391),
    /// the spinner/status fade likewise (:2418-2423, 2505). The 22×22 pin
    /// button overhangs the slot (inset -5px on the 13px stack, :2376-2386)
    /// without affecting layout.
    @ViewBuilder
    private var leadingSlot: some View {
        let slot = ZStack {
            // Precedence: hovering a reorderable row → the drag grip owns the
            // slot; otherwise busy (or restarting) → spinner; otherwise
            // attention shows the 6px #f59e0b dot. The pin moved to the
            // trailing edge, so the status indicator stays put off-hover.
            if showsDragHandle {
                ChromeIconView(icon: .dragHandle, size: 16)
                    .foregroundStyle(Theme.mutedForeground)
            } else if isArchiving {
                // Stopping on the way into the archive: a muted spinner, not
                // the tool-tinted busy one — the session is winding down.
                BrailleSpinner(color: Theme.mutedForeground)
            } else if isRestarting || isResumingAgent
                        || session.status == .starting || session.status == .busy {
                BrailleSpinner(color: Theme.toolSpinnerColor(forCommand: session.presentationCommand))
            } else if session.status == .attention {
                AttentionDot(color: Theme.attention)
            } else if isUnread {
                // Done-and-not-looked-at: the blue dot takes the spinner's
                // slot, so "working" hands off to "done" in place (and the
                // iOS sidebar's leading column matches).
                Circle()
                    .fill(Theme.unread)
                    .frame(width: 7, height: 7)
            }
            // Exited rows show no marker: the hard dim is signal enough.
        }
        .frame(width: 16, height: 16)
        .animation(.easeInOut(duration: 0.12), value: session.status)
        .animation(.easeInOut(duration: 0.12), value: showsDragHandle)
        .animation(.easeInOut(duration: 0.12), value: isUnread)

        if let dragProvider {
            slot
                // Generous hit area around the 16px glyph (28×28) without
                // moving the row layout.
                .contentShape(Rectangle().inset(by: -6))
                .onHover { handleHovering = $0 }
                .onDrag(dragProvider) {
                    EmptyDragPreview()
                }
                .help("Drag to reorder")
        } else {
            slot
        }
    }

    /// The grip appears only while the pointer is over the leading slot
    /// itself — not on whole-row hover — so the status indicator stays
    /// readable while mousing around the row.
    private var showsDragHandle: Bool {
        handleHovering && dragProvider != nil
    }

    /// Exited rows get an inline hover Restart left of the pin — the primary
    /// "bring it back" verb shouldn't hide in the context menu. Gated by the
    /// same per-CLI capability as the menu item (no restart for commands
    /// whose conversation a relaunch would lose).
    private var showsInlineRestart: Bool {
        resumePresentation == .resumeSession
    }

    private var resumePresentation: SessionRowResumePresentation {
        sessionRowResumePresentation(
            session: session,
            isArchived: isArchived,
            canRestart: canRestart,
            canResumeAgent: canResumeAgent
        )
    }

    /// Pin visibility: hidden at opacity 0 by default; row hover shows it at
    /// 1 so any row can pin/unpin. Pinned rows keep the pin visible at 0.9
    /// regardless of status — the spinner/attention indicator lives in the
    /// leading slot now, so the trailing pin no longer yields to it.
    private var pinOpacity: Double {
        if isHovering { return 1 }
        if isPinned { return 0.9 }
        return 0
    }
}

/// Attention indicator (DESIGN.md §5 / .session-status.attention,
/// ProjectItem.svelte:2492-2512): static 6px dot with a 4px halo at 20%
/// of the dot color. No animation.
struct AttentionDot: View {
    let color: Color

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: 6, height: 6)
            .background(
                Circle()
                    .fill(color.opacity(0.20))
                    .frame(width: 14, height: 14)
            )
    }
}

/// 22×22 radius-6 pin button, 13px glyph (ProjectItem.svelte:2376-2416).
private struct PinActionButton: View {
    let isPinned: Bool
    let action: () -> Void

    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            // Unpinned rows show the thumbtack "push-pin" glyph as the
            // "click to pin" affordance; pinned rows keep the plain pin glyph
            // (clicking it still unpins). 13px in both cases.
            ChromeIconView(icon: isPinned ? .pin : .pushPin, size: 13)
                .foregroundStyle(
                    hovering
                        ? Theme.foreground
                        : (isPinned ? Theme.foreground.opacity(0.88) : Theme.mutedForeground)
                )
                .frame(width: 22, height: 22)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(hovering ? Theme.hoverRow : .clear)
                )
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .help(isPinned ? "Unpin from project" : "Pin in project")
    }
}

// MARK: - Remove session (inline confirm) building blocks

/// 22×22 radius-6 archive button shown in the meta slot on row hover
/// (.session-archive-action, ProjectItem.svelte:2700-2741): 13px archive
/// glyph, muted → foreground + fg-10% bg on hover.
private struct ArchiveActionButton: View {
    var help = "Archive session"
    let action: () -> Void

    @State private var hovering = false

    /// archiveIcon (icons.ts:32), same SVG→template pipeline as ChromeIcons.
    @MainActor private static let archiveImage: NSImage? = {
        let svg = ##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M224,50H32A14,14,0,0,0,18,64V88a14,14,0,0,0,14,14h2v90a14,14,0,0,0,14,14H208a14,14,0,0,0,14-14V102h2a14,14,0,0,0,14-14V64A14,14,0,0,0,224,50ZM210,192a2,2,0,0,1-2,2H48a2,2,0,0,1-2-2V102H210ZM226,88a2,2,0,0,1-2,2H32a2,2,0,0,1-2-2V64a2,2,0,0,1,2-2H224a2,2,0,0,1,2,2ZM98,136a6,6,0,0,1,6-6h48a6,6,0,0,1,0,12H104A6,6,0,0,1,98,136Z"></path></svg>"##
        let image = NSImage(data: Data(svg.utf8))
        image?.isTemplate = true
        return image
    }()

    var body: some View {
        Button {
            action()
        } label: {
            Group {
                if let image = Self.archiveImage {
                    Image(nsImage: image)
                        .resizable()
                        .interpolation(.high)
                        .aspectRatio(contentMode: .fit)
                        .frame(width: 13, height: 13)
                } else {
                    Image(systemName: "archivebox")
                        .font(.system(size: 11, weight: .medium))
                }
            }
            .foregroundStyle(hovering ? Theme.foreground : Theme.mutedForeground)
            .frame(width: 22, height: 22)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(hovering ? Theme.hoverRow : .clear)
            )
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .help(help)
    }
}

/// The ArchiveActionButton's destructive sibling, shown on rows whose
/// command can't resume (nothing to archive FOR): same 22×22 hover
/// treatment, trash glyph, routes to the inline remove confirm.
private struct RemoveActionButton: View {
    let action: () -> Void

    @State private var hovering = false

    var body: some View {
        Button {
            action()
        } label: {
            Image(systemName: "trash")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(hovering ? Theme.foreground : Theme.mutedForeground)
                .frame(width: 22, height: 22)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(hovering ? Theme.hoverRow : .clear)
                )
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .help("Remove session")
    }
}

/// 22×22 radius-6 restart button shown left of the pin when an
/// exited row is hovered: 13px Phosphor "arrow-clockwise" glyph, muted →
/// foreground + fg-10% bg on hover (same treatment as ArchiveActionButton).
private struct RestartActionButton: View {
    let action: () -> Void

    @State private var hovering = false

    @MainActor private static let restartImage: NSImage? = {
        let svg = ##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M240,56v48a8,8,0,0,1-8,8H184a8,8,0,0,1,0-16H211.4L184.81,71.64l-.25-.24a80,80,0,1,0-1.67,114.78,8,8,0,0,1,11,11.63A95.44,95.44,0,0,1,128,224h-1.32A96,96,0,1,1,195.75,60L224,86V56a8,8,0,0,1,16,0Z"></path></svg>"##
        let image = NSImage(data: Data(svg.utf8))
        image?.isTemplate = true
        return image
    }()

    var body: some View {
        Button {
            action()
        } label: {
            Group {
                if let image = Self.restartImage {
                    Image(nsImage: image)
                        .resizable()
                        .interpolation(.high)
                        .aspectRatio(contentMode: .fit)
                        .frame(width: 13, height: 13)
                } else {
                    Image(systemName: "arrow.clockwise")
                        .font(.system(size: 11, weight: .medium))
                }
            }
            .foregroundStyle(hovering ? Theme.foreground : Theme.mutedForeground)
            .frame(width: 22, height: 22)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(hovering ? Theme.hoverRow : .clear)
            )
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .help("Resume session (continues the conversation)")
    }
}

/// Non-interactive agent-CLI mark shown right of the date when Appearance ▸
/// "Show agent logos" is on. Fixed-size so the hover swap in the adjacent
/// meta slot never reflows it.
private struct SessionCommandIcon: View {
    let command: String

    var body: some View {
        ToolIconView(command: command, size: 12)
            .opacity(0.82)
            .frame(width: 14, height: 14)
            .help(command.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? "Terminal"
                : command)
    }
}

/// Small pill button for the inline confirm row. Destructive styling
/// follows .session-archive-action.confirming (ProjectItem.svelte:2745-2756):
/// danger text on danger-15% (25% hovered); Cancel is muted on fg-10%.
private struct ConfirmPillButton: View {
    let label: String
    let destructive: Bool
    let action: () -> Void

    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Text(label)
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(
                    destructive
                        ? Theme.danger
                        : (hovering ? Theme.foreground : Theme.mutedForeground)
                )
                .padding(.horizontal, 8)
                .frame(height: 20)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(
                            destructive
                                ? Theme.danger.opacity(hovering ? 0.25 : 0.15)
                                : Theme.hoverRow.opacity(hovering ? 1 : 0.6)
                        )
                )
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .animation(.easeInOut(duration: 0.12), value: hovering)
    }
}

/// Esc / click-away dismissal for the inline confirm row, mirroring the
/// Svelte window-click handler (ProjectItem.svelte:1019-1024). An invisible
/// background NSView the size of the row installs local monitors while the
/// confirm is visible: any mouse-down outside the row's window frame (or in
/// another window) cancels; Esc cancels and is swallowed.
struct RemoveConfirmDismissMonitor: NSViewRepresentable {
    let onCancel: () -> Void

    func makeNSView(context _: Context) -> MonitorView {
        let view = MonitorView()
        view.onCancel = onCancel
        return view
    }

    func updateNSView(_ view: MonitorView, context _: Context) {
        view.onCancel = onCancel
    }

    final class MonitorView: NSView {
        var onCancel: (() -> Void)?
        private var monitors: [Any] = []

        override func hitTest(_: NSPoint) -> NSView? { nil }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            removeMonitors()
            guard window != nil else { return }
            if let mouse = NSEvent.addLocalMonitorForEvents(
                matching: [.leftMouseDown, .rightMouseDown]
            ) { [weak self] event in
                guard let self else { return event }
                if event.window !== self.window {
                    self.onCancel?()
                } else {
                    let rowFrame = self.convert(self.bounds, to: nil)
                    if !rowFrame.contains(event.locationInWindow) {
                        self.onCancel?()
                    }
                }
                return event
            } {
                monitors.append(mouse)
            }
            let keyHandler: (NSEvent) -> NSEvent? = { [weak self] event in
                if event.keyCode == 53 { // Esc
                    self?.onCancel?()
                    return nil
                }
                return event
            }
            if let key = NSEvent.addLocalMonitorForEvents(
                matching: .keyDown, handler: keyHandler
            ) {
                monitors.append(key)
            }
        }

        private func removeMonitors() {
            for monitor in monitors { NSEvent.removeMonitor(monitor) }
            monitors = []
        }

        deinit {
            // Monitors are normally removed in viewDidMoveToWindow(nil) on
            // unmount; this is the safety net. NSViews live on main.
            MainActor.assumeIsolated {
                for monitor in monitors { NSEvent.removeMonitor(monitor) }
            }
        }
    }
}

// MARK: - Busy project-name shimmer (ProjectItem.svelte:2083-2102)

/// Sidebar footer strip: settings ⚙ + add-project ＋ on the left,
/// collapse-all on the right (disabled while nothing is expanded, exactly
/// like the Svelte button binding `disabled={$expandedProjectIds.size===0}`).
/// The gear opens the full-screen settings view, matching the Svelte
/// footer's `onOpenSettings()` (Sidebar.svelte:565-567). Its old utility
/// menu items live in Settings → Advanced; Quit moved to the app menu.
struct SidebarFooter: View {
    /// False while a remote Host is selected: the session-tree verbs
    /// disappear, leaving only the labeled remote button as the way to the
    /// Remote settings screen (where Host switching lives).
    var localVerbsVisible = true
    /// Selected remote Host's name; non-nil only off-Local, where it renders
    /// as a label next to the remote icon.
    var remoteHostName: String?
    let collapseAllDisabled: Bool
    let onCollapseAll: () -> Void
    let onAddProject: () -> Void
    let onOpenSettings: () -> Void
    let onOpenRemoteSettings: () -> Void

    var body: some View {
        HStack(spacing: 2) {
            FooterButton(icon: .settings, help: "Settings (⌘,)", action: onOpenSettings)
            if localVerbsVisible {
                // Add Project is a Controller-local filesystem verb.
                FooterButton(icon: .addProjectPlus, help: "Add Project", action: onAddProject)
            }
            Spacer()
            if UnpeelFeatureFlags.mobileRemoteControlEnabled || remoteHostName != nil {
                // The host button is the ONE intended visible scope
                // difference: green + the remote Host's name while a remote
                // Host is selected; Local keeps the plain muted look.
                FooterButton(
                    icon: .broadcast,
                    label: remoteHostName,
                    help: remoteHostName == nil ? "Remote access" : "Connected Host",
                    tint: remoteHostName == nil ? nil : Color(nsColor: .systemGreen),
                    action: onOpenRemoteSettings
                )
            }
            FooterButton(
                icon: .collapseAll,
                help: "Collapse all folders",
                disabled: collapseAllDisabled,
                action: onCollapseAll
            )
        }
        .padding(EdgeInsets(top: 0, leading: 7.5, bottom: 7.5, trailing: 7.5))
    }
}

private struct FooterButton: View {
    let icon: ChromeIcon
    /// Optional text next to the icon (the remote button shows the selected
    /// Host's name off-Local); nil keeps the square icon-only button.
    var label: String?
    var help: String = ""
    /// Optional foreground tint; the connected-Host button renders green.
    var tint: Color?
    var disabled: Bool = false
    var action: (() -> Void)?

    @State private var hovering = false

    var body: some View {
        Button {
            action?()
        } label: {
            // Footer icons render at 14px (Sidebar.svelte:811-814).
            HStack(spacing: 5) {
                ChromeIconView(icon: icon, size: 14)
                if let label {
                    Text(label)
                        .font(.system(size: 11, weight: .medium))
                        .lineLimit(1)
                }
            }
            .foregroundStyle(tint ?? Theme.mutedForeground)
            .opacity(disabled ? 0.4 : 1)
            .padding(.horizontal, label == nil ? 0 : 7)
            .frame(width: label == nil ? 22 : nil, height: 22)
            .background(
                RoundedRectangle(cornerRadius: 7, style: .continuous)
                    .fill(hovering && !disabled ? Theme.hoverRow : .clear)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .onHover { hovering = $0 }
        .help(help)
        .animation(.easeInOut(duration: 0.12), value: hovering)
    }
}

// MARK: - Liquid Glass selected row (macOS 26+)

extension SessionRowView {
    /// Resolved once: whether the OS has Liquid Glass; pre-26 keeps the
    /// flat Theme.activeRow fill.
    static let liquidGlassAvailable: Bool = {
        if #available(macOS 26.0, *) { return true }
        return false
    }()
}

private extension View {
    /// Real Liquid Glass behind the selected session row; no-op pre-26 and
    /// for unselected rows. Rendered as a background rather than wrapping
    /// `self` in a branch: branching swaps the row's view identity on every
    /// selection change, re-inserting the row content with a visible fade.
    @ViewBuilder
    func selectedRowGlass(_ active: Bool) -> some View {
        background {
            if #available(macOS 26.0, *), active {
                Color.clear
                    .glassEffect(.regular, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                    .transition(.identity)
            }
        }
    }
}
