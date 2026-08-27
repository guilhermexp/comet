//
//  WorkspacesSettingsPanel.swift
//  UnpeelNative
//
//  Settings ▸ Workspaces (experimental): manage additional, fully isolated
//  Unpeel instances on this Mac. Each workspace is its own UNPEEL_HOME —
//  separate sessions/projects/settings and its own phone-pairing identity —
//  launched as a second instance of this same app binary (UnpeelWorkspaceLauncher).
//

import SwiftUI

struct WorkspacesSettingsPanel: View {
    /// Opens Settings ▸ Remote (home of the Unpeel Link section since the
    /// standalone Link tab merged into it, 2026-08-13) from the unlicensed
    /// upsell.
    var onOpenPro: (() -> Void)?
    @ObservedObject private var license = LicenseManager.shared
    @State private var workspaces: [UnpeelWorkspaceRecord] = []
    /// Workspace id → live pid; refreshed on a light timer so the Open button
    /// and running badges track reality.
    @State private var runningPids: [String: Int32] = [:]
    @State private var newWorkspaceName = ""
    @State private var renamingWorkspace: UnpeelWorkspaceRecord?
    @State private var renameText = ""
    @State private var removalCandidate: UnpeelWorkspaceRecord?
    @State private var removalDeletesData = false
    @State private var errorMessage: String?

    private let refreshTimer = Timer.publish(every: 3, on: .main, in: .common).autoconnect()

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Form {
                Section {} header: {
                    SettingsPaneHeader(
                        title: "Workspaces",
                        description: "Run extra, fully separate copies of Unpeel on this Mac. "
                            + "A workspace has its own sessions, projects, presets, and settings, "
                            + "and pairs with your phone as its own Mac. Updates install from "
                            + "the default workspace; other workspaces pick them up when relaunched."
                    )
                    .padding(.bottom, 4)
                }

                if license.isPro {
                    currentSection
                    workspacesSection
                    createSection
                } else {
                    proRequiredSection
                }
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
        }
        .onAppear(perform: refresh)
        .onReceive(refreshTimer) { _ in refresh() }
        .alert(
            "Rename workspace",
            isPresented: Binding(
                get: { renamingWorkspace != nil },
                set: { if !$0 { renamingWorkspace = nil } }
            ),
            presenting: renamingWorkspace
        ) { workspace in
            TextField("Name", text: $renameText)
            Button("Rename") {
                UnpeelWorkspaceRegistry.rename(id: workspace.id, to: renameText)
                refresh()
            }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            Text("Shows in the menu bar and on paired phones. The phone name fully updates after the workspace restarts.")
        }
        .confirmationDialog(
            "Remove \(removalCandidate?.name ?? "workspace")?",
            isPresented: Binding(
                get: { removalCandidate != nil },
                set: { if !$0 { removalCandidate = nil; removalDeletesData = false } }
            ),
            titleVisibility: .visible,
            presenting: removalCandidate
        ) { workspace in
            Button("Remove from list", role: .destructive) {
                UnpeelWorkspaceRegistry.remove(id: workspace.id, deleteData: false)
                refresh()
            }
            Button("Remove and delete its data", role: .destructive) {
                UnpeelWorkspaceRegistry.remove(id: workspace.id, deleteData: true)
                refresh()
            }
            Button("Cancel", role: .cancel) {}
        } message: { workspace in
            Text(
                "\"Remove from list\" keeps \(workspace.name)'s data at \(workspace.home) so you can "
                    + "re-add it later. Deleting its data also unpairs any phones paired with it."
            )
        }
    }

    // MARK: - Sections

    /// Shown instead of the workspace management sections when Pro isn't
    /// active. Workspaces are a Link feature; already-running workspace instances
    /// are not touched — this only gates the management UI.
    private var proRequiredSection: some View {
        Section {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Image(systemName: "lock.fill")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(Theme.mutedForeground)
                VStack(alignment: .leading, spacing: 4) {
                    Text("Workspaces require Unpeel Link")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(Theme.foreground)
                    Text("Unpeel Link adds workspaces, remote control from your iPhone, and "
                        + "Unpeel Remote access from anywhere — $59 per seat per year.")
                        .font(.system(size: 12))
                        .foregroundStyle(Theme.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                    if let onOpenPro {
                        Button("Open Unpeel Link Settings", action: onOpenPro)
                            .controlSize(.small)
                            .padding(.top, 4)
                    }
                }
            }
            .padding(.vertical, 4)
        }
    }

    private var currentSection: some View {
        Section {
            SettingsValueRow(
                label: "This instance",
                value: UnpeelWorkspaceContext.displayName ?? "Default"
            )
            SettingsValueRow(
                label: "Data folder",
                value: LaunchConfig.unpeelDir.path
            )
        } header: {
            SettingsSectionHeader(title: "Current workspace")
        }
    }

    @ViewBuilder
    private var workspacesSection: some View {
        Section {
            if workspaces.isEmpty {
                Text("No extra workspaces yet.")
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.mutedForeground)
            } else {
                ForEach(workspaces) { workspace in
                    workspaceRow(workspace)
                }
            }
        } header: {
            SettingsSectionHeader(
                title: "Workspaces",
                description: "Open launches the workspace as a second Unpeel instance; "
                    + "quit it from its own menu bar item."
            )
        }
    }

    private func workspaceRow(_ workspace: UnpeelWorkspaceRecord) -> some View {
        let isCurrent = UnpeelWorkspaceContext.currentWorkspace()?.id == workspace.id
        let isRunning = isCurrent || runningPids[workspace.id] != nil
        return HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 1) {
                HStack(spacing: 6) {
                    Text(workspace.name)
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.foreground)
                    if isRunning {
                        Text(isCurrent ? "This instance" : "Running")
                            .font(.system(size: 10, weight: .medium))
                            .foregroundStyle(Theme.mutedForeground)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(Theme.mutedForeground.opacity(0.14), in: Capsule())
                    }
                }
                Text(workspace.home)
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.mutedForeground)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: 12)
            Button("Open") { open(workspace) }
                .disabled(isRunning)
            Button("Rename…") {
                renameText = workspace.name
                renamingWorkspace = workspace
            }
            Button("Remove…") {
                if isRunning {
                    errorMessage = "Quit \(workspace.name) before removing it."
                } else {
                    removalCandidate = workspace
                }
            }
        }
        .padding(.vertical, 2)
    }

    private var createSection: some View {
        Section {
            LabeledContent {
                HStack(spacing: 10) {
                    TextField("Workspace name", text: $newWorkspaceName, prompt: Text("Work"))
                        .labelsHidden()
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 220)
                        .onSubmit(createWorkspace)
                    Button("Create & Open", action: createWorkspace)
                        .disabled(
                            newWorkspaceName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        )
                }
            } label: {
                Text("Workspace name")
            }
            if let errorMessage {
                Text(errorMessage)
                    .font(.system(size: 11))
                    .foregroundStyle(.red.opacity(0.9))
            }
        } header: {
            SettingsSectionHeader(
                title: "New workspace",
                description: "Starts blank, with its own sessions, projects, and "
                    + "settings — you pair your phone with it separately."
            )
        }
    }

    // MARK: - Actions

    private func refresh() {
        workspaces = UnpeelWorkspaceRegistry.load()
        var pids: [String: Int32] = [:]
        for workspace in workspaces {
            let home = URL(fileURLWithPath: workspace.home, isDirectory: true)
            if let pid = UnpeelWorkspaceLauncher.runningPid(home: home) {
                pids[workspace.id] = pid
            }
        }
        runningPids = pids
    }

    private func createWorkspace() {
        do {
            let record = try UnpeelWorkspaceRegistry.create(name: newWorkspaceName)
            newWorkspaceName = ""
            errorMessage = nil
            try UnpeelWorkspaceLauncher.launch(record)
            refresh()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func open(_ workspace: UnpeelWorkspaceRecord) {
        do {
            try UnpeelWorkspaceLauncher.launch(workspace)
            errorMessage = nil
            // The pidfile lands once the child finishes launching; the timer
            // flips the row to Running shortly after.
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
