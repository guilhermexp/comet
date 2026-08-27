//
//  Chrome.swift
//  UnpeelNative
//
//  Window-chrome building blocks: vibrancy backgrounds, drag regions,
//  the custom 38px titlebar (DESIGN.md §1).
//

import AppKit
import SwiftUI

// MARK: - Vibrancy

/// NSVisualEffectView wrapper. `.sidebar` behind the sidebar,
/// `.underWindowBackground` behind the content area (DESIGN.md §1).
struct VisualEffectBackground: NSViewRepresentable {
    let material: NSVisualEffectView.Material
    /// `.behindWindow` for window chrome; `.withinWindow` for in-window
    /// overlays (⌘K palette, ⌃Tab switcher) that should frost the app
    /// content beneath them rather than the desktop.
    var blendingMode: NSVisualEffectView.BlendingMode = .behindWindow

    func makeNSView(context _: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = material
        view.blendingMode = blendingMode
        view.state = .followsWindowActiveState
        return view
    }

    func updateNSView(_ view: NSVisualEffectView, context _: Context) {
        view.material = material
        view.blendingMode = blendingMode
    }
}

/// Sidebar background: native sidebar vibrancy plus a subdued neutral wash.
/// The top/bottom row fade is owned by `SidebarListFadeMask`.
struct SidebarBackground: View {
    var body: some View {
        ZStack {
            VisualEffectBackground(material: .sidebar)
            Theme.sidebarTint
        }
        .ignoresSafeArea()
    }
}

/// Content background: vibrancy + #2B2E37@20% + #222228@12%.
struct ContentBackground: View {
    var body: some View {
        ZStack {
            VisualEffectBackground(material: .underWindowBackground)
            Theme.contentTint
        }
        .ignoresSafeArea()
    }
}

// MARK: - Content leading-edge hairline

/// The sidebar/content boundary hairline. Instead of a straight full-height
/// line, it traces the content pane's rounded leading contour: along the top
/// edge into the top-left corner arc, down the left edge, and out through the
/// bottom-left arc — so the border follows ContentArea's clipped shape.
struct LeadingEdgeContour: Shape {
    let radius: CGFloat

    func path(in rect: CGRect) -> Path {
        var p = Path()
        p.move(to: CGPoint(x: rect.minX + radius, y: rect.minY))
        // Angles are in SwiftUI's flipped space (0° = right, 90° = down).
        // Top-left corner: pointing up (-90°) sweeping to pointing left (180°).
        p.addRelativeArc(
            center: CGPoint(x: rect.minX + radius, y: rect.minY + radius),
            radius: radius,
            startAngle: .degrees(-90),
            delta: .degrees(-90)
        )
        p.addLine(to: CGPoint(x: rect.minX, y: rect.maxY - radius))
        // Bottom-left corner: pointing left (180°) to pointing down (90°).
        p.addRelativeArc(
            center: CGPoint(x: rect.minX + radius, y: rect.maxY - radius),
            radius: radius,
            startAngle: .degrees(180),
            delta: .degrees(-90)
        )
        return p
    }
}

// MARK: - Drag region

/// Transparent strip that moves the window when dragged and toggles
/// maximize on double-click (the `-webkit-app-region: drag` equivalent).
struct WindowDragArea: NSViewRepresentable {
    final class DragView: NSView {
        override func mouseDown(with event: NSEvent) {
            if event.clickCount == 2 {
                window?.zoom(nil)
            } else {
                window?.performDrag(with: event)
            }
        }
    }

    func makeNSView(context _: Context) -> DragView { DragView() }
    func updateNSView(_: DragView, context _: Context) {}
}

// MARK: - Titlebar

/// Custom 38px titlebar over the content pane. Title 13px/600 muted,
/// `parent / worktree` separators at 0.55 opacity, workspace segment
/// weight 500. Background = terminal background while a terminal shows.
struct TitleBarView: View {
    let segments: [String]
    var branch: String? = nil
    var branchIsWorktree = false
    let showsTerminalBackground: Bool
    var terminalBackgroundColor = Theme.terminalBackgroundNSColor

    var body: some View {
        ZStack {
            // Always paint an opaque strip when a themed terminal is up so the
            // semi-transparent content dim / vibrancy never peeks through the
            // titlebar band (the usual "wrong color above the TUI" stripe).
            if showsTerminalBackground {
                Color(nsColor: terminalBackgroundColor)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            WindowDragArea()
            titleText
        }
        .frame(maxWidth: .infinity)
        .frame(height: Theme.titlebarHeight)
        // Solid fill also on the container so SwiftUI layout doesn't leave
        // a transparent hit-region band under the text.
        .background(showsTerminalBackground ? Color(nsColor: terminalBackgroundColor) : Color.clear)
    }

    private var titleText: some View {
        HStack(spacing: 5) {
            ForEach(Array(segments.enumerated()), id: \.offset) { index, segment in
                if index > 0 {
                    titleSeparator
                }
                Text(segment)
                    .font(.system(size: 13, weight: index > 0 ? .medium : .semibold))
                    .foregroundStyle(Theme.mutedForeground)
                    .lineLimit(1)
            }
            // Muted current-branch suffix for git projects: a small branch
            // glyph + branch name at reduced opacity, after the title.
            if let branch, !branch.isEmpty {
                HStack(spacing: 3) {
                    ChromeIconView(icon: branchIsWorktree ? .branch : .gitBranch, size: 12)
                    Text(branch)
                        .font(.system(size: 12, weight: .medium))
                        .lineLimit(1)
                }
                .foregroundStyle(Theme.mutedForeground)
                .opacity(0.55)
                .padding(.leading, 2)
            }
        }
        .allowsHitTesting(false)
    }

    private var titleSeparator: some View {
        Text("/")
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(Theme.mutedForeground.opacity(0.55))
    }
}
