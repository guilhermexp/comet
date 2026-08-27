//
//  SessionLauncherView.swift
//  UnpeelNative
//
//  Main-area "pick a tool" launcher. This is the native port of the Svelte
//  SessionLauncherView the "+" affordance used to open (the sidebar "+" is
//  still a dropdown menu; see SidebarView.newSessionMenu). It is shown in the
//  content area for:
//    - the Finder "New Unpeel Session Here" service (UnpeelStore.openLauncher)
//    - the empty state (no session selected) for the active project
//  Picking a tile launches a session via UnpeelStore.launchSession, which
//  selects the new session and replaces this launcher with the terminal.
//

import SwiftUI

struct SessionLauncherView: View {
    @ObservedObject var store: UnpeelStore
    let project: Project

    /// Blank terminal first, then every enabled preset (same content as the
    /// sidebar "+" menu, which uses `availablePresets` as `menuPresets`).
    private var presets: [Preset] {
        [.newTerminal] + store.availablePresets
    }

    var body: some View {
        VStack(spacing: 22) {
            header

            ScrollView {
                VStack(spacing: 6) {
                    ForEach(presets) { preset in
                        LauncherRow(preset: preset) {
                            store.launchSession(projectID: project.id, command: preset.command)
                        }
                    }
                }
                .padding(.horizontal, 4)
            }
            .frame(maxWidth: 460)

            Button {
                store.openSettings(tab: .presets)
            } label: {
                Label("Manage presets…", systemImage: "slider.horizontal.3")
                    .font(.system(size: 11))
            }
            .buttonStyle(.plain)
            .foregroundStyle(Theme.mutedForeground)
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var header: some View {
        VStack(spacing: 6) {
            Text("New session")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(Theme.mutedForeground)
            Text(project.name)
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(Theme.foreground)
            Text(abbreviatedPath)
                .font(.system(size: 11))
                .foregroundStyle(Theme.foreground.opacity(0.35))
                .lineLimit(1)
                .truncationMode(.middle)
        }
    }

    private var abbreviatedPath: String {
        let home = NSHomeDirectory()
        if project.path == home { return "~" }
        if project.path.hasPrefix(home + "/") {
            return "~" + project.path.dropFirst(home.count)
        }
        return project.path
    }
}

/// A single launch choice as a full-width list row: tool icon + label on the
/// left, a chevron on the right, subtle fill that brightens on hover. Mirrors
/// the presets-settings row look (PresetsSettingsPanel) and QuickPresetButton's
/// muted→foreground hover treatment.
private struct LauncherRow: View {
    let preset: Preset
    let action: () -> Void

    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 11) {
                ToolIconView(command: preset.command, size: 18)
                    .opacity(hovering ? 1 : 0.85)
                Text(preset.label)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(hovering ? Theme.foreground : Theme.mutedForeground)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 8)
                Image(systemName: "chevron.right")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(Theme.foreground.opacity(hovering ? 0.5 : 0.25))
            }
            .padding(.horizontal, 14)
            .frame(height: 46)
            .frame(maxWidth: .infinity)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(hovering ? Theme.hoverRow : Theme.foreground.opacity(0.04))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(Theme.foreground.opacity(hovering ? 0.12 : 0.06), lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .help("Start \(preset.label)")
    }
}
