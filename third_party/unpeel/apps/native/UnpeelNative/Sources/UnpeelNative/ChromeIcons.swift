//
//  ChromeIcons.swift
//  UnpeelNative
//
//  App-chrome icons (sidebar, titlebar, footer), carried over verbatim from
//  the Svelte app so the native build uses the exact same artwork:
//  - Phosphor-style 256-viewBox fill icons from lib/icons.ts
//  - the Lucide "split" branch icon inlined in ProjectItem.svelte:307-308
//  - the two inline titlebar SVGs in App.svelte:1440/1449
//
//  Same pattern as ToolIcons.swift: `currentColor` is rewritten to #FFFFFF
//  and the decoded NSImage is marked as a template, so SwiftUI's
//  foregroundStyle tints it (hover swaps muted → foreground exactly like
//  the Svelte `color` transitions).
//

import AppKit
import SwiftUI

enum ChromeIcon: String, CaseIterable {
    case folderClosed
    case folderOpen
    case folderSimple
    case branch
    case gitBranch
    case pin
    case pinSlash
    case pushPin
    case settings
    case addProjectPlus
    case plus
    case plusBold
    case collapseAll
    case mobile
    case sidebarToggle
    case back
    case bell
    case orchestrator
    case broadcast
    case dragHandle

    fileprivate func svg(inverted: Bool) -> String {
        switch self {
        // Project row folder mark — collapsed.
        case .folderClosed:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" fill="none" viewBox="0 0 256 256"><defs><linearGradient id="folderClosedGlass" x1="36" y1="38" x2="222" y2="218" gradientUnits="userSpaceOnUse">"## + Self.glassStops(inverted: inverted) + ##"</linearGradient></defs><path d="M216,72H131.31L104,44.69A15.88,15.88,0,0,0,92.69,40H40A16,16,0,0,0,24,56V200.62A15.41,15.41,0,0,0,39.39,216h177.5A15.13,15.13,0,0,0,232,200.89V88A16,16,0,0,0,216,72ZM40,56H92.69l16,16H40Z" fill="url(#folderClosedGlass)"></path><path d="M216,72H131.31L104,44.69A15.88,15.88,0,0,0,92.69,40H40A16,16,0,0,0,24,56V200.62A15.41,15.41,0,0,0,39.39,216h177.5A15.13,15.13,0,0,0,232,200.89V88A16,16,0,0,0,216,72ZM40,56H92.69l16,16H40Z" fill="none" stroke="#FFFFFF" stroke-opacity="0.30" stroke-width="8" stroke-linejoin="round"></path></svg>"##
        // Project row folder mark — expanded.
        case .folderOpen:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" fill="none" viewBox="0 0 256 256"><defs><linearGradient id="folderOpenGlass" x1="34" y1="46" x2="224" y2="216" gradientUnits="userSpaceOnUse">"## + Self.glassStops(inverted: inverted) + ##"</linearGradient></defs><path d="M245,110.64A16,16,0,0,0,232,104H216V88a16,16,0,0,0-16-16H130.67L102.94,51.2a16.14,16.14,0,0,0-9.6-3.2H40A16,16,0,0,0,24,64V208h0a8,8,0,0,0,8,8H211.1a8,8,0,0,0,7.59-5.47l28.49-85.47A16.05,16.05,0,0,0,245,110.64ZM93.34,64,123.2,86.4A8,8,0,0,0,128,88h72v16H69.77a16,16,0,0,0-15.18,10.94L40,158.7V64Z" fill="url(#folderOpenGlass)"></path><path d="M245,110.64A16,16,0,0,0,232,104H216V88a16,16,0,0,0-16-16H130.67L102.94,51.2a16.14,16.14,0,0,0-9.6-3.2H40A16,16,0,0,0,24,64V208h0a8,8,0,0,0,8,8H211.1a8,8,0,0,0,7.59-5.47l28.49-85.47A16.05,16.05,0,0,0,245,110.64ZM93.34,64,123.2,86.4A8,8,0,0,0,128,88h72v16H69.77a16,16,0,0,0-15.18,10.94L40,158.7V64Z" fill="none" stroke="#FFFFFF" stroke-opacity="0.30" stroke-width="8" stroke-linejoin="round"></path></svg>"##
        // Phosphor "folder" outline — inline worktree child folder rows in
        // the sidebar (lighter than the glass-filled project folder marks).
        case .folderSimple:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" fill="none" viewBox="0 0 256 256"><rect width="256" height="256" fill="none"/><path d="M224,88V200.89a7.11,7.11,0,0,1-7.11,7.11H40a8,8,0,0,1-8-8V64a8,8,0,0,1,8-8H93.33a8,8,0,0,1,4.8,1.6L128,80h88A8,8,0,0,1,224,88Z" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/></svg>"##
        // ProjectItem.svelte:307-308 (workspaceBranchIcon): Lucide "split",
        // stroke-width 2. The Svelte string rotates it 90° via an inline
        // style; ChromeIconView applies the same rotation in SwiftUI since
        // NSImage's SVG decoder ignores `style` transforms.
        case .branch:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M16 3h5v5"/><path d="M8 3H3v5"/><path d="M12 22v-8.3a4 4 0 0 0-1.172-2.872L3 3"/><path d="m15 9 6-6"/></svg>"##
        // Lucide "git-branch": the standard current-branch mark for a plain
        // (non-worktree) git project's titlebar suffix.
        case .gitBranch:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="6" x2="6" y1="3" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/></svg>"##
        // icons.ts:29 (pinIcon) — session-row pin action
        case .pin:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M233.91,82.79,173.22,22.1a14,14,0,0,0-19.81,0L98.93,76.77c-9.52-3.25-34-8.34-59.71,12.41A14,14,0,0,0,38.1,110l49.71,49.71-44.05,44a6,6,0,1,0,8.48,8.48l44.05-44.05L146,217.89a14,14,0,0,0,9.9,4.11q.49,0,1,0a14,14,0,0,0,10.19-5.54c19.72-26.21,17.15-47.23,12.46-59.3l54.37-54.55A14,14,0,0,0,233.91,82.79ZM225.42,94.1h0l-57.27,57.46a6,6,0,0,0-1.11,6.92c9.94,19.88-1.71,40.32-9.54,50.72a2,2,0,0,1-3,.2L46.58,101.51a2,2,0,0,1,.18-3c12.5-10.09,24.5-12.76,33.7-12.76a42.13,42.13,0,0,1,17.25,3.41A6,6,0,0,0,104.64,88L161.9,30.59a2,2,0,0,1,2.83,0l60.69,60.68A2,2,0,0,1,225.42,94.1Z"></path></svg>"##
        // icons.ts:30 (unpinIcon, slashed pin) — pinned session rows
        case .pinSlash:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M52.44,36A6,6,0,0,0,43.56,44L71.27,74.51C61.78,76,50.6,80,39.22,89.18A14,14,0,0,0,38.1,110l49.71,49.71-44.05,44a6,6,0,1,0,8.48,8.48l44.05-44.05L146,217.89a14,14,0,0,0,9.9,4.11q.49,0,1,0a14,14,0,0,0,10.19-5.54,85.51,85.51,0,0,0,12.44-22.84l24,26.45a6,6,0,1,0,8.87-8.08ZM157.49,209.21a2,2,0,0,1-3,.2L46.58,101.51a2,2,0,0,1,.18-3c13.18-10.64,25.84-12.9,34.79-12.7L170,183.11C167.83,193.74,162.11,203.07,157.49,209.21Zm76.42-106.62-44.65,44.78a6,6,0,1,1-8.5-8.47l44.65-44.79a2,2,0,0,0,0-2.84L164.73,30.59a2,2,0,0,0-2.83,0L120.68,71.94a6,6,0,0,1-8.5-8.47l41.23-41.36a14,14,0,0,1,19.81,0l60.69,60.69A14,14,0,0,1,233.91,102.59Z"></path></svg>"##
        // Phosphor "push-pin"-style thumbtack — session-row pin action while
        // the row is NOT yet pinned (the "click to pin" affordance).
        case .pushPin:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M216,168h-9.29L185.54,48H192a8,8,0,0,0,0-16H64a8,8,0,0,0,0,16h6.46L49.29,168H40a8,8,0,0,0,0,16h80v56a8,8,0,0,0,16,0V184h80a8,8,0,0,0,0-16ZM86.71,48h82.58l21.17,120H65.54Z"></path></svg>"##
        // Footer settings gear.
        case .settings:
            return Self.glassSVG(
                path: ##"M237.94,107.21a8,8,0,0,0-3.89-5.4l-29.83-17-.12-33.62a8,8,0,0,0-2.83-6.08,111.91,111.91,0,0,0-36.72-20.67,8,8,0,0,0-6.46.59L128,41.85,97.88,25a8,8,0,0,0-6.47-.6A111.92,111.92,0,0,0,54.73,45.15a8,8,0,0,0-2.83,6.07l-.15,33.65-29.83,17a8,8,0,0,0-3.89,5.4,106.47,106.47,0,0,0,0,41.56,8,8,0,0,0,3.89,5.4l29.83,17,.12,33.63a8,8,0,0,0,2.83,6.08,111.91,111.91,0,0,0,36.72,20.67,8,8,0,0,0,6.46-.59L128,214.15,158.12,231a7.91,7.91,0,0,0,3.9,1,8.09,8.09,0,0,0,2.57-.42,112.1,112.1,0,0,0,36.68-20.73,8,8,0,0,0,2.83-6.07l.15-33.65,29.83-17a8,8,0,0,0,3.89-5.4A106.47,106.47,0,0,0,237.94,107.21ZM128,168a40,40,0,1,1,40-40A40,40,0,0,1,128,168Z"##,
                gradientID: "settingsGlass",
                inverted: inverted
            )
        // Footer add-project button.
        case .addProjectPlus:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M228,128a12,12,0,0,1-12,12H140v76a12,12,0,0,1-24,0V140H40a12,12,0,0,1,0-24h76V40a12,12,0,0,1,24,0v76h76A12,12,0,0,1,228,128Z"></path></svg>"##
        // icons.ts:21 (plusIcon) — footer add-project + quick-strip "+"
        case .plus:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M222,128a6,6,0,0,1-6,6H134v82a6,6,0,0,1-12,0V134H40a6,6,0,0,1,0-12h82V40a6,6,0,0,1,12,0v82h82A6,6,0,0,1,222,128Z"></path></svg>"##
        // App.svelte:1449 (inline) — collapsed-sidebar "new session" plus
        // (slightly heavier 8-unit strokes than icons.ts plusIcon)
        case .plusBold:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M224,128a8,8,0,0,1-8,8H136v80a8,8,0,0,1-16,0V136H40a8,8,0,0,1,0-16h80V40a8,8,0,0,1,16,0v80h80A8,8,0,0,1,224,128Z"></path></svg>"##
        // icons.ts:23 (collapseAllIcon) — footer collapse-all
        case .collapseAll:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M222,128a6,6,0,0,1-6,6H40a6,6,0,0,1,0-12H216A6,6,0,0,1,222,128Zm-98.24-27.76a6,6,0,0,0,8.48,0l32-32a6,6,0,0,0-8.48-8.48L134,81.51V16a6,6,0,0,0-12,0V81.51L100.24,59.76a6,6,0,0,0-8.48,8.48Zm8.48,55.52a6,6,0,0,0-8.48,0l-32,32a6,6,0,0,0,8.48,8.48L122,174.49V240a6,6,0,0,0,12,0V174.49l21.76,21.75a6,6,0,0,0,8.48-8.48Z"></path></svg>"##
        // Phosphor "device-mobile" — retained for phone-specific surfaces.
        case .mobile:
            return Self.glassSVG(
                path: ##"M176,16H80A24,24,0,0,0,56,40V216a24,24,0,0,0,24,24h96a24,24,0,0,0,24-24V40A24,24,0,0,0,176,16Zm8,200a8,8,0,0,1-8,8H80a8,8,0,0,1-8-8V40a8,8,0,0,1,8-8h96a8,8,0,0,1,8,8ZM144,56a8,8,0,0,1-8,8H120a8,8,0,0,1,0-16h16A8,8,0,0,1,144,56Zm-8,136a8,8,0,1,1-8-8A8,8,0,0,1,136,192Z"##,
                gradientID: "mobileGlass",
                inverted: inverted
            )
        // titlebar sidebar toggle.
        case .sidebarToggle:
            return Self.glassSVG(
                path: ##"M216,40H40A16,16,0,0,0,24,56V200a16,16,0,0,0,16,16H216a16,16,0,0,0,16-16V56A16,16,0,0,0,216,40Zm0,160H88V56H216V200Z"##,
                gradientID: "sidebarToggleGlass",
                inverted: inverted
            )
        // icons.ts:20 (backIcon) — settings sidebar "Back" row
        case .back:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M222,128a6,6,0,0,1-6,6H54.49l61.75,61.76a6,6,0,1,1-8.48,8.48l-72-72a6,6,0,0,1,0-8.48l72-72a6,6,0,0,1,8.48,8.48L54.49,122H216A6,6,0,0,1,222,128Z"></path></svg>"##
        // Phosphor "bell" (fill) — titlebar activity glyph when nothing is
        // spinning but there are recently finished (unread) sessions.
        case .bell:
            return Self.glassSVG(
                path: ##"M221.8,175.94C216.25,166.38,208,139.33,208,104a80,80,0,1,0-160,0c0,35.34-8.26,62.38-13.81,71.94A16,16,0,0,0,48,200H88.81a40,40,0,0,0,78.38,0H208a16,16,0,0,0,13.8-24.06ZM128,216a24,24,0,0,1-22.62-16h45.24A24,24,0,0,1,128,216Z"##,
                gradientID: "bellGlass",
                inverted: inverted
            )
        // Phosphor "tree-structure" (fill) — marks a write-capable session in
        // the sidebar.
        case .orchestrator:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M144,96V80H128a8,8,0,0,0-8,8v80a8,8,0,0,0,8,8h16V160a16,16,0,0,1,16-16h48a16,16,0,0,1,16,16v48a16,16,0,0,1-16,16H160a16,16,0,0,1-16-16V192H128a24,24,0,0,1-24-24V136H72v8a16,16,0,0,1-16,16H24A16,16,0,0,1,8,144V112A16,16,0,0,1,24,96H56a16,16,0,0,1,16,16v8h32V88a24,24,0,0,1,24-24h16V48a16,16,0,0,1,16-16h48a16,16,0,0,1,16,16V96a16,16,0,0,1-16,16H160A16,16,0,0,1,144,96Z"></path></svg>"##
        // Phosphor "broadcast" (fill) — radiating signal: the sidebar footer
        // remote button (hosts and controllers).
        case .broadcast:
            return Self.glassSVG(
                path: ##"M168,128a40,40,0,1,1-40-40A40,40,0,0,1,168,128Zm40,0a79.74,79.74,0,0,0-20.37-53.33,8,8,0,1,0-11.92,10.67,64,64,0,0,1,0,85.33,8,8,0,0,0,11.92,10.67A79.79,79.79,0,0,0,208,128ZM80.29,85.34A8,8,0,1,0,68.37,74.67a79.94,79.94,0,0,0,0,106.67,8,8,0,0,0,11.92-10.67,63.95,63.95,0,0,1,0-85.33Zm158.28-4A119.48,119.48,0,0,0,213.71,44a8,8,0,1,0-11.42,11.2,103.9,103.9,0,0,1,0,145.56A8,8,0,1,0,213.71,212,120.12,120.12,0,0,0,238.57,81.29ZM32.17,168.48A103.9,103.9,0,0,1,53.71,55.22,8,8,0,1,0,42.29,44a119.87,119.87,0,0,0,0,168,8,8,0,1,0,11.42-11.2A103.61,103.61,0,0,1,32.17,168.48Z"##,
                gradientID: "broadcastGlass",
                inverted: inverted
            )
        // Phosphor "dots-six-vertical" — sidebar drag-to-reorder grip, shown
        // on hover in the row's leading slot (folder / status indicator).
        case .dragHandle:
            return ##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><circle cx="92" cy="60" r="12"/><circle cx="164" cy="60" r="12"/><circle cx="92" cy="128" r="12"/><circle cx="164" cy="128" r="12"/><circle cx="92" cy="196" r="12"/><circle cx="164" cy="196" r="12"/></svg>"##
        }
    }

    private static func glassSVG(path: String, gradientID: String, inverted: Bool) -> String {
        """
        <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" fill="none" viewBox="0 0 256 256"><defs><linearGradient id="\(gradientID)" x1="34" y1="40" x2="224" y2="218" gradientUnits="userSpaceOnUse">\(glassStops(inverted: inverted))</linearGradient></defs><path d="\(path)" fill="url(#\(gradientID))"></path><path d="\(path)" fill="none" stroke="#FFFFFF" stroke-opacity="0.30" stroke-width="8" stroke-linejoin="round"></path></svg>
        """
    }

    /// The glass-fill gradient stops. Default ramps bright→faint along the
    /// top-left→bottom-right diagonal (reads well on dark chrome). `inverted`
    /// reverses the opacity ramp so the faint end leads — used in light mode so
    /// the icon's tint doesn't pool solid in the top-left corner.
    private static func glassStops(inverted: Bool) -> String {
        if inverted {
            return ##"<stop offset="0" stop-color="#FFFFFF" stop-opacity="0.52"/><stop offset="0.55" stop-color="#FFFFFF" stop-opacity="0.80"/><stop offset="1" stop-color="#FFFFFF" stop-opacity="0.98"/>"##
        }
        return ##"<stop offset="0" stop-color="#FFFFFF" stop-opacity="0.98"/><stop offset="0.45" stop-color="#FFFFFF" stop-opacity="0.80"/><stop offset="1" stop-color="#FFFFFF" stop-opacity="0.52"/>"##
    }

    /// The Svelte branch icon string carries `style="transform: rotate(90deg)"`.
    fileprivate var rotationDegrees: Double { self == .branch ? 90 : 0 }
}

@MainActor
enum ChromeIconStore {
    private struct Key: Hashable {
        let icon: ChromeIcon
        let inverted: Bool
    }

    private static var cache: [Key: NSImage?] = [:]

    static func image(for icon: ChromeIcon, inverted: Bool = false) -> NSImage? {
        let key = Key(icon: icon, inverted: inverted)
        if let cached = cache[key] { return cached }
        let image = NSImage(data: Data(icon.svg(inverted: inverted).utf8))
        image?.isTemplate = true
        cache[key] = image
        return image
    }
}

/// Renders one Svelte chrome icon at the pixel size the web app uses,
/// tinted by the surrounding foregroundStyle. Falls back to a near-enough
/// SF Symbol if SVG decoding ever fails.
struct ChromeIconView: View {
    let icon: ChromeIcon
    var size: CGFloat = 16

    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        if let image = ChromeIconStore.image(for: icon, inverted: colorScheme == .light) {
            Image(nsImage: image)
                .resizable()
                .interpolation(.high)
                .aspectRatio(contentMode: .fit)
                .rotationEffect(.degrees(icon.rotationDegrees))
                .frame(width: size, height: size)
        } else {
            Image(systemName: fallbackSymbol)
                .font(.system(size: size * 0.85, weight: .medium))
                .frame(width: size, height: size)
        }
    }

    private var fallbackSymbol: String {
        switch icon {
        case .folderClosed, .folderOpen, .folderSimple: return "folder"
        case .branch, .gitBranch: return "arrow.triangle.branch"
        case .pin: return "pin"
        case .pinSlash: return "pin.slash"
        case .pushPin: return "pin"
        case .settings: return "gearshape"
        case .addProjectPlus, .plus, .plusBold: return "plus"
        case .collapseAll: return "chevron.up.chevron.down"
        case .mobile: return "iphone"
        case .sidebarToggle: return "sidebar.left"
        case .back: return "arrow.left"
        case .bell: return "bell"
        case .orchestrator: return "point.3.connected.trianglepath.dotted"
        case .broadcast: return "dot.radiowaves.left.and.right"
        case .dragHandle: return "line.3.horizontal"
        }
    }
}
