//
//  PresetsSettingsPanel.swift
//  UnpeelNative
//
//  The Presets settings panel. Based on
//  apps/desktop/src/lib/settings/PresetsPanel.svelte, but GLOBAL-presets
//  only (the native app dropped project presets entirely):
//  - list view: pane header, ONE FLAT drag-reorderable list of presets
//    (CLI auto-detected per row; the order doubles as each CLI's default
//    choice), rows with tool icon + command + Risky/Disabled badges +
//    favorite star, add-preset row
//  - detail view: back button, icon + title + delete-with-confirm,
//    Command field / Enabled toggle / Quick launch toggle (supported
//    commands only), Save + Cancel (PresetsPanel.svelte:136-215)
//
//  PRESENTATION: the list route is a plain reorderable List of card rows
//  (card-stack anatomy — grouped Forms never produce
//  `.onMove` drag-reordering on macOS, so the flat list can't be one);
//  the DETAIL route stays a grouped SwiftUI Form (.formStyle(.grouped)),
//  which on macOS 26 renders the real System Settings anatomy.
//  `.scrollContentBackground(.hidden)` lets everything sit on Unpeel's
//  dark vibrancy background. Preset rows drill into the detail route with
//  a trailing chevron, like System Settings subpanes.
//
//  PERSISTENCE: all mutations go through UnpeelStore's UserDefaults preset
//  overlay. app-state.json is never written.
//

import SwiftUI
import UniformTypeIdentifiers

// MARK: - Drag reordering

/// Live drag-reorder for card stacks in a plain ScrollView. SwiftUI's
/// `List.onMove` is a dead end here: grouped Forms never wire the drag up on
/// macOS at all, and inside a plain List the reorder gesture handling steals
/// clicks from any text field sharing the list. Cards in a ScrollView with
/// `.onDrag`/`.onDrop` have neither problem — rows reorder live as the drag
/// passes over them, and ordinary controls keep working.
struct PresetReorderDropDelegate: DropDelegate {
    /// The row this delegate is attached to.
    let itemID: String
    /// The ids of every row in the stack, in current display order.
    let orderedIDs: [String]
    @Binding var draggingID: String?
    /// Same shape as `UnpeelStore.movePresets` (offsets are `Array.move`
    /// semantics against `orderedIDs`).
    let move: (IndexSet, Int) -> Void

    func dropEntered(info: DropInfo) {
        guard let dragging = draggingID, dragging != itemID,
              let from = orderedIDs.firstIndex(of: dragging),
              let to = orderedIDs.firstIndex(of: itemID)
        else { return }
        move(IndexSet(integer: from), to > from ? to + 1 : to)
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: .move)
    }

    func performDrop(info: DropInfo) -> Bool {
        draggingID = nil
        return true
    }
}

// MARK: - Panel

private enum EditorRoute: Equatable {
    case list
    case detail(presetID: String)
}

struct PresetsSettingsPanel: View {
    @ObservedObject var store: UnpeelStore

    @State private var route: EditorRoute = .list
    @State private var newCommand = ""
    @State private var draggingPresetID: String?

    // Detail edit state (PresetsPanel.svelte:31-33).
    @State private var editCommand = ""
    @State private var editEnabled = true
    @State private var editQuickLaunch = false
    @State private var confirmDelete = false

    var body: some View {
        Group {
            switch route {
            case .list:
                listView
            case .detail(let presetID):
                detailView(presetID: presetID)
            }
        }
        .onAppear(perform: applyDebugRoute)
    }

    /// UNPEEL_DEBUG_OPEN_PRESETS=detail opens the first preset's detail
    /// view, =detail-off does the same but forces both edit toggles OFF
    /// (visual check of switch states without saving) — snapshot hooks.
    private func applyDebugRoute() {
        switch ProcessInfo.processInfo.environment["UNPEEL_DEBUG_OPEN_PRESETS"] {
        case "detail":
            if let first = store.mergedPresets.first {
                openDetail(first)
            }
        case "detail-off":
            if let first = store.mergedPresets.first {
                openDetail(first)
                editEnabled = false
                editQuickLaunch = false
            }
        default:
            break
        }
    }

    // MARK: - List view (PresetsPanel.svelte:217-313)

    /// Card-list layout: fixed
    /// pane header, then a ScrollView stack of preset cards with the
    /// add-command card flowing at the end. Reordering is `.onDrag`/`.onDrop`
    /// (`PresetReorderDropDelegate`) — deliberately NOT `List.onMove`, which
    /// grouped Forms never wire up on macOS and which steals clicks from any
    /// text field sharing the list.
    private var listView: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: 12) {
                SettingsPaneHeader(
                    title: "Presets",
                    description: "Launch commands for your agents. "
                        + "Drag to reorder — a CLI's topmost preset is its default."
                )
                Spacer(minLength: 0)
                Button {
                    store.refreshToolAvailability(showScanning: false)
                } label: {
                    if store.toolScanInProgress {
                        HStack(spacing: 5) {
                            ProgressView()
                                .controlSize(.small)
                                .scaleEffect(0.55)
                                .frame(width: 12, height: 12)
                            Text("Scanning…")
                        }
                    } else {
                        Label("Rescan PATH", systemImage: "arrow.clockwise")
                    }
                }
                .controlSize(.small)
                .disabled(store.toolScanInProgress)
                .help("Re-detect which agent CLIs are installed")
                // Track SettingsPaneHeader's internal top padding so the
                // button stays level with the pane title.
                .padding(.top, 4)
            }
            .padding(EdgeInsets(top: 20, leading: 20, bottom: 10, trailing: 20))

            // One flat, drag-reorderable list. The CLI is auto-detected from
            // each command's head (row icon); commands whose CLI isn't
            // installed stay persisted but are omitted here.
            ScrollView {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(visiblePresets) { preset in
                        presetRow(preset)
                            .opacity(draggingPresetID == preset.id ? 0.45 : 1)
                            .onDrag {
                                draggingPresetID = preset.id
                                return NSItemProvider(object: preset.id as NSString)
                            }
                            .onDrop(of: [.text], delegate: PresetReorderDropDelegate(
                                itemID: preset.id,
                                orderedIDs: visiblePresets.map(\.id),
                                draggingID: $draggingPresetID,
                                move: { offsets, destination in
                                    store.movePresets(
                                        visiblePresets.map(\.id),
                                        from: offsets,
                                        to: destination
                                    )
                                }
                            ))
                    }

                    addRow

                    // Compatible CLIs not found on the PATH, each with an
                    // Install (vendor one-liner) or website action — folded
                    // in from the removed Agent CLI Tools window.
                    if let missing = store.setupToolReport?.missingStatuses,
                       !missing.isEmpty {
                        Text("Not installed")
                            .font(.system(size: 11, weight: .semibold))
                            .textCase(.uppercase)
                            .foregroundStyle(Theme.mutedForeground)
                            .padding(EdgeInsets(top: 16, leading: 4, bottom: 3, trailing: 0))
                        ForEach(missing) { status in
                            MissingToolInstallRow(store: store, tool: status.tool)
                        }
                    }
                }
                .padding(EdgeInsets(top: 0, leading: 20, bottom: 20, trailing: 20))
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .onAppear {
            // Keep the Not-installed section current without blanking the
            // report (which would flash the section away).
            store.refreshToolAvailability(showScanning: false)
        }
    }

    /// The flat list contents: every merged preset whose CLI is installed
    /// (unknown-head custom commands always show). Before the asynchronous
    /// PATH scan completes, keep the prior permissive behavior so the list
    /// does not flash empty.
    private var visiblePresets: [Preset] {
        store.mergedPresets.filter { preset in
            guard let cli = SetupTool.detect(in: preset.command) else { return true }
            return store.isCLIInstalled(cli) != false
        }
    }

    /// Preset card row: CLI icon, command,
    /// badges, favorite star, drag grip, trailing chevron. The main area
    /// opens the detail subpane; the star (quick-launch) is a separate tap
    /// target. The whole row is draggable for reordering.
    private func presetRow(_ preset: Preset) -> some View {
        HStack(spacing: 10) {
            Button {
                openDetail(preset)
            } label: {
                HStack(spacing: 10) {
                    ToolIconView(command: preset.command, size: 15)
                        .foregroundStyle(
                            EditorStyle.toolColor(QuickPresetTool.detect(in: preset.command))
                        )
                        .frame(width: 15, height: 15)
                        .opacity(preset.enabled ? 1 : 0.5)

                    Text(styledCommand(preset.command))
                        .font(.system(size: 13, design: .monospaced))
                        .opacity(preset.enabled ? 1 : 0.5)
                        .lineLimit(1)
                        .truncationMode(.tail)

                    if dangerousCommand(preset.command) {
                        EditorBadge(text: "Risky", color: Theme.danger)
                    }
                    if !preset.enabled {
                        EditorBadge(text: "Disabled", color: Theme.mutedForeground)
                    }

                    Spacer(minLength: 4)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Preset settings")

            // Favorite star: any known CLI can be quick-launched. Starring
            // several presets of one CLI turns its sidebar chip into a menu.
            if SetupTool.detect(in: preset.command) != nil {
                Button {
                    store.updatePreset(id: preset.id, quickLaunch: !preset.quickLaunch)
                } label: {
                    Image(systemName: preset.quickLaunch ? "star.fill" : "star")
                        .font(.system(size: 12))
                        .foregroundStyle(preset.quickLaunch
                            ? Theme.foreground
                            : Theme.mutedForeground.opacity(0.55))
                }
                .buttonStyle(.plain)
                .help(preset.quickLaunch
                    ? "Remove from sidebar quick launch"
                    : "Add to sidebar quick launch")
            }

            // Drag-to-reorder affordance (the whole row is draggable; this
            // is the visual grip).
            Image(systemName: "line.3.horizontal")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(Theme.mutedForeground.opacity(0.5))
                .help("Drag to reorder")

            Image(systemName: "chevron.right")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(Theme.mutedForeground.opacity(0.7))
        }
        .padding(.vertical, 8)
        .padding(.horizontal, 12)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Theme.activeRow.opacity(0.4))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(Theme.resizerLine.opacity(0.6), lineWidth: 1)
        )
    }

    private var addRow: some View {
        HStack(spacing: 10) {
            ToolIconView(
                command: newCommand,
                size: 16
            )
            .foregroundStyle(
                EditorStyle.toolColor(QuickPresetTool.detect(in: newCommand))
            )
            .frame(width: 16, height: 16)
            .opacity(newCommand.trimmingCharacters(in: .whitespaces).isEmpty ? 0.45 : 1)

            TextField(
                "Add command (e.g. claude --plan)",
                text: $newCommand
            )
            .textFieldStyle(.plain)
            .font(.system(size: 13, design: .monospaced))
            .foregroundStyle(Theme.foreground)
            .onSubmit(handleAdd)

            Button("Add", action: handleAdd)
                .buttonStyle(.bordered)
                .controlSize(.small)
                .disabled(newCommand.trimmingCharacters(in: .whitespaces).isEmpty)
        }
        .padding(.vertical, 8)
        .padding(.horizontal, 12)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Theme.activeRow.opacity(0.22))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(
                    Theme.resizerLine.opacity(0.4),
                    style: StrokeStyle(lineWidth: 1, dash: [4, 3])
                )
        )
    }

    private func handleAdd() {
        let command = newCommand.trimmingCharacters(in: .whitespaces)
        guard !command.isEmpty else { return }
        store.addPreset(command: command)
        newCommand = ""
    }

    /// Agent name at full strength, flags/args faded — keeps the tool legible
    /// while de-emphasizing the noise. One AttributedString so the row still
    /// truncates on a single line.
    private func styledCommand(_ command: String) -> AttributedString {
        let parts = command.split(separator: " ", maxSplits: 1, omittingEmptySubsequences: false)
        var head = AttributedString(String(parts.first ?? ""))
        head.foregroundColor = Theme.foreground
        guard parts.count > 1 else { return head }
        var tail = AttributedString(" " + String(parts[1]))
        tail.foregroundColor = Theme.mutedForeground
        return head + tail
    }

    /// Flags that hand the agent unsupervised power (permission/sandbox
    /// bypass): anything the tool author named `--dangerously…`, plus the
    /// common `--yolo` / `--force` "run everything" switches.
    private func dangerousCommand(_ command: String) -> Bool {
        command.split(separator: " ").contains { token in
            let flag = token.lowercased()
            return flag.hasPrefix("--dangerously") || flag == "--yolo"
                || flag == "--force" || flag == "-f"
        }
    }

    // MARK: - Detail view (PresetsPanel.svelte:136-215)

    private func openDetail(_ preset: Preset) {
        editCommand = preset.command
        editEnabled = preset.enabled
        editQuickLaunch = preset.quickLaunch
        confirmDelete = false
        route = .detail(presetID: preset.id)
    }

    private func goBack() {
        route = .list
        confirmDelete = false
    }

    @ViewBuilder
    private func detailView(presetID: String) -> some View {
        if let preset = store.mergedPresets.first(where: { $0.id == presetID }) {
            detailContent(preset: preset)
        } else {
            // Preset vanished underneath us (e.g. file edit) — fall back.
            Color.clear.frame(height: 1).onAppear { goBack() }
        }
    }

    /// System Settings subpane: fixed back affordance + pane title above
    /// the grouped card, Save/Cancel CTA row pinned bottom-trailing (the
    /// System Settings action-row position).
    private func detailContent(preset: Preset) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 10) {
                EditorBackButton(action: goBack)

                HStack(spacing: 12) {
                    HStack(spacing: 8) {
                        ToolIconView(command: preset.command, size: 20)
                            .foregroundStyle(EditorStyle.toolColor(preset.tool))
                            .frame(width: 20, height: 20)
                        Text(preset.label.isEmpty ? preset.command : preset.label)
                            .font(.system(size: 20, weight: .bold))
                            .foregroundStyle(Theme.foreground)
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                    Spacer(minLength: 4)
                    if confirmDelete {
                        HStack(spacing: 6) {
                            Text("Delete this preset?")
                                .font(.system(size: 12))
                                .foregroundStyle(Theme.mutedForeground)
                            EditorButton(title: "Delete", variant: .danger, size: .small) {
                                store.removePreset(id: preset.id)
                                goBack()
                            }
                            EditorButton(title: "Cancel", variant: .secondary, size: .small) {
                                confirmDelete = false
                            }
                        }
                    } else {
                        EditorIconButton(systemName: "trash", help: "Delete preset") {
                            confirmDelete = true
                        }
                    }
                }
            }
            .padding(EdgeInsets(top: 20, leading: 20, bottom: 0, trailing: 20))

            detailForm(preset: preset)

            HStack(spacing: 8) {
                Spacer(minLength: 0)
                EditorButton(title: "Cancel", variant: .secondary, action: goBack)
                EditorButton(title: "Save", variant: .primary) {
                    saveDetail(preset: preset)
                }
            }
            .padding(EdgeInsets(top: 8, leading: 20, bottom: 20, trailing: 20))
        }
    }

    private func detailForm(preset: Preset) -> some View {
        Form {
            Section {
                TextField(
                    "Command",
                    text: $editCommand,
                    prompt: Text("e.g. claude --dangerously-skip-permissions")
                )
                .textFieldStyle(.plain)
                .font(.system(size: 12, design: .monospaced))
                .multilineTextAlignment(.trailing)
                .foregroundStyle(Theme.foreground)

                Toggle(isOn: $editEnabled) {
                    Text("Enabled")
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.foreground)
                }
                .toggleStyle(.switch)
                .controlSize(.small)

                if SetupTool.detect(in: editCommand) != nil {
                    Toggle(isOn: $editQuickLaunch) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Quick launch")
                                .font(.system(size: 13))
                                .foregroundStyle(Theme.foreground)
                            Text("Show in the sidebar for one-click access. Several starred presets of one CLI become a menu.")
                                .font(.system(size: 11))
                                .foregroundStyle(Theme.mutedForeground)
                        }
                    }
                    .toggleStyle(.switch)
                    .controlSize(.small)
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
    }

    /// handleSaveDetail (PresetsPanel.svelte:83-122).
    private func saveDetail(preset: Preset) {
        let command = editCommand.trimmingCharacters(in: .whitespaces)
        guard !command.isEmpty else { return }
        let supportsQuick = SetupTool.detect(in: command) != nil
        store.updatePreset(
            id: preset.id,
            command: command,
            enabled: editEnabled,
            quickLaunch: supportsQuick ? editQuickLaunch : false
        )
        goBack()
    }
}

// MARK: - Missing-CLI install rows (from the removed Agent CLI Tools window)

/// One-line row for a compatible CLI that wasn't found on the PATH: dimmed
/// icon + name, then Install (runs the vendor's official one-liner in the
/// login shell) or a website link when no trusted install command exists.
private struct MissingToolInstallRow: View {
    @ObservedObject var store: UnpeelStore
    let tool: SetupTool

    private var installing: Bool { store.toolInstallsInProgress.contains(tool) }

    var body: some View {
        HStack(spacing: 10) {
            SetupToolIcon(tool: tool, size: 14)
                .opacity(0.55)

            Text(tool.displayName)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(Theme.mutedForeground)

            if let error = store.toolInstallErrors[tool] {
                Text(error)
                    .font(.system(size: 11))
                    .foregroundStyle(.red)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .help(error)
            }

            Spacer(minLength: 8)

            if installing {
                ProgressView()
                    .controlSize(.small)
                Text("Installing…")
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.mutedForeground)
            } else if let command = tool.installCommand {
                Button("Install") {
                    store.installTool(tool)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .help(command)
                if store.toolInstallErrors[tool] != nil, let url = tool.websiteURL {
                    websiteButton(url)
                }
            } else if let url = tool.websiteURL {
                websiteButton(url)
            }
        }
        .padding(.vertical, 7)
        .padding(.horizontal, 12)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Theme.activeRow.opacity(0.22))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(Theme.resizerLine.opacity(0.4), lineWidth: 1)
        )
    }

    private func websiteButton(_ url: URL) -> some View {
        Button {
            NSWorkspace.shared.open(url)
        } label: {
            Label("Website", systemImage: "arrow.up.right.square")
                .font(.system(size: 12))
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .help(url.absoluteString)
    }
}

/// Tinted rounded-rect CLI icon tile (SetupTool-keyed, unlike the bare
/// command-keyed `ToolIconView` used in preset rows).
private struct SetupToolIcon: View {
    let tool: SetupTool?
    let size: CGFloat

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill((toolColor).opacity(0.14))
            if let tool {
                ToolIconView(command: tool.defaultPresetCommand, size: size)
                    .foregroundStyle(toolColor)
            } else {
                Image(systemName: "terminal")
                    .font(.system(size: size - 2, weight: .semibold))
                    .foregroundStyle(toolColor)
            }
        }
        .frame(width: 24, height: 24)
    }

    private var toolColor: Color {
        Theme.toolColor(forCommand: tool?.defaultPresetCommand ?? "")
    }
}

// MARK: - Styling helpers

private enum EditorStyle {
    /// Runtime descriptor tint, with a neutral fallback for a plain terminal.
    static func toolColor(_ tool: QuickPresetTool?) -> Color {
        guard let tool else { return Theme.foreground.opacity(0.88) }
        return Theme.toolColor(forCommand: tool.rawValue)
    }
}

/// Badge (.badge, PresetsPanel.svelte:431-447): 10px/600, padding 2/6,
/// radius 4, tint at 16% behind tinted text.
private struct EditorBadge: View {
    let text: String
    let color: Color

    var body: some View {
        Text(text)
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(color)
            .padding(EdgeInsets(top: 2, leading: 6, bottom: 2, trailing: 6))
            .background(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(color.opacity(0.16))
            )
    }
}

/// 28×28 icon button. Stays icon-only (chrome scale unchanged) but uses
/// the native `.accessoryBar` style for system hover/press feedback
/// instead of the hand-rolled hover fill.
private struct EditorIconButton: View {
    let systemName: String
    var help: String = ""
    let action: () -> Void

    var body: some View {
        if #available(macOS 14.0, *) {
            button.buttonStyle(.accessoryBar)
        } else {
            button.buttonStyle(.borderless)
        }
    }

    private var button: some View {
        Button(action: action) {
            Image(systemName: systemName)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(Theme.mutedForeground)
                .frame(width: 24, height: 20)
        }
        .help(help)
    }
}

/// "‹ Back" (.back-button: 12px/600, muted → fg on hover).
private struct EditorBackButton: View {
    let action: () -> Void

    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Image(systemName: "arrow.left")
                    .font(.system(size: 11, weight: .semibold))
                Text("Back")
                    .font(.system(size: 12, weight: .semibold))
            }
            .foregroundStyle(hovering ? Theme.foreground : Theme.mutedForeground)
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .animation(.easeInOut(duration: 0.14), value: hovering)
    }
}

/// Native button (replaces the GlassButton stand-in; call sites unchanged).
/// macOS 26: primary = .glassProminent tinted Unpeel's neutral gray (the
/// designer's spec 2026-06-12: CTAs in the app gray, not system blue);
/// secondary/danger = .bordered — the standard push button, which keeps
/// secondary actions dimmer than the gray prominent CTA (`.glass` rendered
/// BRIGHTER than the tinted glassProminent over the dark vibrancy,
/// inverting the hierarchy). Danger is a destructive-role button (red
/// label). Shared with the other settings panels (SettingsView.swift).
struct EditorButton: View {
    enum Variant { case primary, secondary, danger }
    enum Size { case regular, small }

    let title: String
    var variant: Variant = .secondary
    var size: Size = .regular
    var disabled = false
    let action: () -> Void

    var body: some View {
        styled(
            Button(title, role: variant == .danger ? .destructive : nil, action: action)
        )
        .controlSize(size == .small ? .small : .regular)
        .disabled(disabled)
    }

    @ViewBuilder
    private func styled(_ button: some View) -> some View {
        if #available(macOS 26.0, *) {
            switch variant {
            // Neutral-gray prominent capsule: keep the native glass
            // material, tint it the app gray instead of system blue.
            // glassProminent derives the label color from the tint.
            case .primary: button.buttonStyle(.glassProminent).tint(Theme.ctaTint)
            case .secondary, .danger: button.buttonStyle(.bordered)
            }
        } else {
            switch variant {
            case .primary: button.buttonStyle(.borderedProminent).tint(Theme.ctaTint)
            case .secondary, .danger: button.buttonStyle(.bordered)
            }
        }
    }
}
