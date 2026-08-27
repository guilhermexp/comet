import SwiftUI
import UnpeelShared
#if canImport(UIKit)
import UIKit
#endif

/// Resume affordance rendered by the session organizer. This is deliberately
/// independent of SwiftUI so protocol/session-state gates are unit-testable.
enum SessionOrganizeResumePresentation: Equatable {
    case none
    case resumeAgent
    case resumeSession
    case restore
    case restoreAndResume
}

func sessionOrganizeResumePresentation(
    session: RemoteSessionSummary,
    hostProtocol: RemoteHostProtocolDescriptor?
) -> SessionOrganizeResumePresentation {
    if session.archived {
        return session.status == .exited && (session.capabilities?.restart ?? true)
            ? .restoreAndResume
            : .restore
    }
    if session.status == .running {
        guard session.activeRuntimeID == nil,
              !session.runtimeLaunchPending,
              session.capabilities?.resumeAgent == true,
              hostProtocol?.isCompatible() == true,
              hostProtocol?.supports(
                RemoteControlProtocol.sessionRuntimeResumeCapability
              ) == true
        else { return .none }
        return .resumeAgent
    }
    return (session.capabilities?.restart ?? true) ? .resumeSession : .none
}

/// Rename + pin editor, opened by long-pressing the terminal title bar.
/// Saves through POST /mobile/session-organization, which lands in the
/// desktop's own title/pin overlays — the refreshed bootstrap snapshot is
/// what updates this screen.
struct SessionOrganizeSheet: View {
    var store: RemotePreviewStore
    let session: RemoteSessionSummary
    @Environment(\.dismiss) private var dismiss

    @State private var title: String
    @State private var pinned: Bool
    @State private var notifyWhenDone: Bool
    @State private var saving = false
    @State private var saveFailed = false
    @State private var actionInFlight: SessionSheetAction?
    @State private var confirmingAction: SessionSheetAction?
    @State private var actionFailed: String?
    @State private var copyingTranscript = false
    @State private var transcriptCopied = false
    @State private var transcriptError: String?
    @FocusState private var titleFocused: Bool

    init(store: RemotePreviewStore, session: RemoteSessionSummary) {
        self.store = store
        self.session = session
        _title = State(initialValue: session.title)
        _pinned = State(initialValue: session.pinned)
        _notifyWhenDone = State(initialValue: session.notifyWhenDone)
    }

    private var trimmedTitle: String {
        title.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Only send what changed; an unchanged (or emptied) title stays nil so
    /// the Mac keeps its current label.
    private var titlePatch: String? {
        trimmedTitle.isEmpty || trimmedTitle == session.title ? nil : trimmedTitle
    }

    private var pinnedPatch: Bool? {
        pinned == session.pinned ? nil : pinned
    }

    private var notifyWhenDonePatch: Bool? {
        notifyWhenDone == session.notifyWhenDone ? nil : notifyWhenDone
    }

    private var isBusy: Bool {
        saving || actionInFlight != nil
    }

    private var removeLabel: String {
        session.status == .running ? "Remove session" : "Remove from list"
    }

    private var resumePresentation: SessionOrganizeResumePresentation {
        sessionOrganizeResumePresentation(
            session: session,
            hostProtocol: store.snapshot.hostProtocol
        )
    }

    // Verb support computed on the Mac (ProviderCapabilities → the
    // `capabilities` field): the sheet offers exactly what the desktop
    // context menu offers for this CLI. Absent on older Macs ⇒ fall back to
    // the permissive pre-capability behavior for stopped Session Resume.
    private var canResume: Bool {
        resumePresentation == .resumeSession
            || resumePresentation == .restoreAndResume
    }

    private var canResumeAgent: Bool {
        resumePresentation == .resumeAgent
    }

    private var canNotifyWhenDone: Bool {
        session.capabilities?.notifyWhenDone ?? true
    }

    /// Archive needs the Mac to understand `archived` on the organization
    /// patch — older Macs silently ignore it, so the fallback here is
    /// restrictive (hide the verb), unlike the permissive verbs above.
    private var canArchive: Bool {
        session.capabilities?.archive ?? false
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Session name", text: $title)
                        .focused($titleFocused)
                        .submitLabel(.done)
                        .onSubmit(save)
                    Toggle(isOn: $pinned) {
                        Label("Pin in sidebar", systemImage: "pin")
                    }
                    if canNotifyWhenDone {
                        Toggle(isOn: $notifyWhenDone) {
                            Label("Notify when done", systemImage: "bell")
                        }
                    }
                } footer: {
                    if saveFailed {
                        Text("Could not update the session. Check the connection to your Mac.")
                            .foregroundStyle(.red)
                    } else if canNotifyWhenDone {
                        Text("Get a notification when this session finishes a turn.")
                    }
                }

                Section {
                    Menu {
                        Button("Last 20 entries") { copyTranscript(entries: 20) }
                        Button("Last 50 entries") { copyTranscript(entries: 50) }
                        Button("Whole conversation") { copyTranscript(entries: 0) }
                    } label: {
                        HStack {
                            Label(
                                copyingTranscript ? "Copying transcript..." : "Copy transcript",
                                systemImage: transcriptCopied ? "checkmark" : "doc.on.doc"
                            )
                            Spacer()
                            if copyingTranscript {
                                ProgressView()
                                    .controlSize(.small)
                            }
                        }
                    }
                    .disabled(copyingTranscript)
                } footer: {
                    if let transcriptError {
                        Text(transcriptError)
                            .foregroundStyle(.red)
                    } else if transcriptCopied {
                        Text("Transcript copied to the clipboard as Markdown.")
                    } else {
                        Text("Copy the conversation as Markdown, using your Mac's Settings \u{25B8} Transcripts content options.")
                    }
                }

                Section {
                    if canResumeAgent {
                        Button {
                            performAction(.resumeAgent)
                        } label: {
                            actionLabel(
                                title: actionInFlight == .resumeAgent
                                    ? "Resuming Agent..." : "Resume Agent",
                                systemImage: "arrow.clockwise",
                                action: .resumeAgent
                            )
                        }
                        .disabled(isBusy)
                    } else if canResume {
                        Button {
                            performAction(.restart)
                        } label: {
                            actionLabel(
                                title: actionInFlight == .restart
                                    ? (session.archived ? "Restoring & Resuming..." : "Resuming...")
                                    : (session.archived ? "Restore & Resume" : "Resume"),
                                systemImage: "arrow.clockwise",
                                action: .restart
                            )
                        }
                        .disabled(isBusy)
                    }

                    if session.archived {
                        // A recent archive still showing in the stopped
                        // group: offer the way back (desktop context menu's
                        // "Restore from archive").
                        Button {
                            performAction(.restore)
                        } label: {
                            actionLabel(
                                title: actionInFlight == .restore ? "Restoring..." : "Restore from archive",
                                systemImage: "arrow.uturn.backward",
                                action: .restore
                            )
                        }
                        .disabled(isBusy)
                    } else if canArchive {
                        Button {
                            // A running session gets a confirm (archiving
                            // stops the agent); an ended one archives
                            // directly — mirroring the desktop rule.
                            if session.status == .running {
                                confirmingAction = .archive
                            } else {
                                performAction(.archive)
                            }
                        } label: {
                            actionLabel(
                                title: actionInFlight == .archive
                                    ? "Archiving..."
                                    : (session.status == .running ? "Stop and archive" : "Archive"),
                                systemImage: "archivebox",
                                action: .archive
                            )
                        }
                        .disabled(isBusy)
                    }

                    // Archive IS the stop verb (desktop folded "Stop" into
                    // "Stop and archive"); the standalone Stop survives only
                    // against older Macs that can't archive.
                    if session.status == .running, !canArchive {
                        Button(role: .destructive) {
                            confirmingAction = .stop
                        } label: {
                            actionLabel(
                                title: actionInFlight == .stop ? "Stopping..." : "Stop session",
                                systemImage: "stop.circle",
                                action: .stop
                            )
                        }
                        .disabled(isBusy)
                    }

                    Button(role: .destructive) {
                        confirmingAction = .remove
                    } label: {
                        actionLabel(
                            title: actionInFlight == .remove ? "Removing..." : removeLabel,
                            systemImage: "trash",
                            action: .remove
                        )
                    }
                    .disabled(isBusy)
                } footer: {
                    if let actionFailed {
                        Text(actionFailed)
                            .foregroundStyle(.red)
                    } else if session.archived, canResume {
                        Text("Restore & Resume puts the session back in the regular list and continues the conversation. Remove deletes it.")
                    } else if session.archived {
                        Text("Restore puts the session back in the regular list. Remove deletes it.")
                    } else if session.status == .running {
                        if canResumeAgent {
                            Text(canArchive
                                ? "Resume Agent continues in this terminal. Stop and archive files it away; Remove deletes it."
                                : "Resume Agent continues in this terminal. Stop keeps the transcript; Remove deletes the session.")
                        } else {
                            Text(canArchive
                                ? "Stop and archive stops the session and files it away (restorable anytime). Remove deletes it."
                                : "Stop keeps the transcript and lets you resume later. Remove deletes the session from the list.")
                        }
                    } else if canResume {
                        Text(canArchive
                            ? "Resume continues the conversation. Archive files it away. Remove deletes it."
                            : "Resume continues the conversation. Remove clears it from the list.")
                    } else {
                        Text("Remove clears the session from the list.")
                    }
                }
            }
            .navigationTitle("Edit Session")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .disabled(isBusy)
                }
                ToolbarItem(placement: .confirmationAction) {
                    if isBusy {
                        ProgressView()
                    } else {
                        Button("Save", action: save)
                            .disabled(trimmedTitle.isEmpty)
                    }
                }
            }
        }
        .interactiveDismissDisabled(isBusy)
        .confirmationDialog(
            confirmingAction?.confirmationTitle(for: session) ?? "",
            isPresented: Binding(
                get: { confirmingAction != nil },
                set: { if !$0 { confirmingAction = nil } }
            ),
            titleVisibility: .visible
        ) {
            if let action = confirmingAction {
                // Archive is non-destructive (everything is kept), so its
                // confirm button is not styled as a destructive action.
                Button(
                    action.confirmButtonTitle(for: session),
                    role: action == .archive ? nil : .destructive
                ) {
                    let confirmed = action
                    confirmingAction = nil
                    performAction(confirmed)
                }
            }
            Button("Cancel", role: .cancel) {
                confirmingAction = nil
            }
        } message: {
            if let action = confirmingAction {
                Text(action.confirmationMessage(for: session))
            }
        }
    }

    @ViewBuilder
    private func actionLabel(
        title: String,
        systemImage: String,
        action: SessionSheetAction
    ) -> some View {
        HStack {
            Label(title, systemImage: systemImage)
            Spacer()
            if actionInFlight == action {
                ProgressView()
                    .controlSize(.small)
            }
        }
    }

    private func save() {
        guard !isBusy else { return }
        guard titlePatch != nil || pinnedPatch != nil || notifyWhenDonePatch != nil else {
            dismiss()
            return
        }
        saving = true
        saveFailed = false
        Task {
            let ok = await store.updateSessionOrganization(
                sessionID: session.id,
                title: titlePatch,
                pinned: pinnedPatch,
                notifyWhenDone: notifyWhenDonePatch
            )
            saving = false
            if ok {
                dismiss()
            } else {
                saveFailed = true
            }
        }
    }

    /// Fetch the Markdown transcript from the Mac and put it on the phone's
    /// clipboard. `entries` is the menu's range pick (count, 0 = whole
    /// conversation). Deliberately does not dismiss: the copied/failed footer
    /// is the feedback, and the user may still want the other actions.
    private func copyTranscript(entries: Int) {
        guard !copyingTranscript else { return }
        copyingTranscript = true
        transcriptCopied = false
        transcriptError = nil
        Task {
            let markdown = await store.transcriptMarkdown(sessionID: session.id, entries: entries)
            copyingTranscript = false
            if let markdown {
                #if canImport(UIKit)
                UIPasteboard.general.string = markdown
                #endif
                transcriptCopied = true
            } else {
                transcriptError = "Could not copy the transcript. The session may have no readable conversation yet."
            }
        }
    }

    private func performAction(_ action: SessionSheetAction) {
        guard !isBusy else { return }
        actionInFlight = action
        actionFailed = nil
        saveFailed = false
        Task {
            let ok: Bool
            if let remoteAction = action.remoteAction {
                ok = await store.performSessionAction(
                    sessionID: session.id,
                    action: remoteAction
                )
            } else {
                // Archive/restore ride the organization patch (`archived`),
                // not the kill-verb action endpoint.
                ok = await store.updateSessionOrganization(
                    sessionID: session.id,
                    title: nil,
                    pinned: nil,
                    archived: action == .archive
                )
            }
            actionInFlight = nil
            if ok {
                dismiss()
            } else {
                actionFailed = "Could not \(action.failureVerb) the session. Check the connection to your Mac."
            }
        }
    }
}

private enum SessionSheetAction: String, Identifiable {
    case restart
    case resumeAgent
    case stop
    case archive
    case restore
    case remove

    var id: String { rawValue }

    /// nil for archive/restore, which travel on the organization patch
    /// instead of the session-action endpoint.
    var remoteAction: RemoteSessionAction? {
        switch self {
        case .restart: return .restart
        case .resumeAgent: return .resumeAgent
        case .stop: return .stop
        case .archive, .restore: return nil
        case .remove: return .remove
        }
    }

    var failureVerb: String {
        switch self {
        case .restart: return "restart"
        case .resumeAgent: return "resume agent"
        case .stop: return "stop"
        case .archive: return "archive"
        case .restore: return "restore"
        case .remove: return "remove"
        }
    }

    func confirmationTitle(for session: RemoteSessionSummary) -> String {
        switch self {
        case .restart:
            return "Resume Session?"
        case .resumeAgent:
            return "Resume Agent?"
        case .stop:
            return "Stop Session?"
        case .archive:
            return "Stop and Archive Session?"
        case .restore:
            // Restore is non-destructive and never confirms.
            return "Restore Session?"
        case .remove:
            return session.status == .running ? "Remove Session?" : "Remove From List?"
        }
    }

    func confirmButtonTitle(for session: RemoteSessionSummary) -> String {
        switch self {
        case .restart:
            return "Resume"
        case .resumeAgent:
            return "Resume Agent"
        case .stop:
            return "Stop Session"
        case .archive:
            return "Archive"
        case .restore:
            return "Restore"
        case .remove:
            return session.status == .running ? "Remove Session" : "Remove From List"
        }
    }

    func confirmationMessage(for session: RemoteSessionSummary) -> String {
        switch self {
        case .restart:
            return "This starts the session again and continues its conversation."
        case .resumeAgent:
            return "This resumes the managed agent inside the same terminal. The Session and terminal history stay open."
        case .stop:
            return "This stops the running process but keeps the transcript so you can resume later."
        case .archive:
            return "This stops the running process and files the session into the project's archive on your Mac. Everything is kept — restore and resume it anytime."
        case .restore:
            return "This puts the archived session back in the regular list."
        case .remove:
            if session.status == .running {
                return "This stops the running process and removes the session from the list."
            }
            return "This removes the ended session from the list."
        }
    }
}
