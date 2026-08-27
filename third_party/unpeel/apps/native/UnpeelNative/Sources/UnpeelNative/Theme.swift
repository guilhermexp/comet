//
//  Theme.swift
//  UnpeelNative
//
//  Resolved design tokens from DESIGN.md (extracted from the Svelte app,
//  light + dark themes, default color scheme). No Ghostty imports here.
//
//  Light/dark switching is appearance-driven: tokens are dynamic NSColors
//  (resolved per the view's effective appearance), the window follows
//  NSApp.appearance, and the Ghostty surface flips its own light/dark
//  config via the wrapper's viewDidChangeEffectiveAppearance hook. The
//  user preference lives in ThemePreference (Light/Dark/System).
//

import AppKit
import SwiftUI
import UnpeelShared

extension NSColor {
    /// 0xRRGGBB hex literal (sRGB).
    convenience init(hex: UInt32, opacity: Double = 1) {
        self.init(
            srgbRed: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            alpha: opacity
        )
    }
}

extension Color {
    /// 0xRRGGBB hex literal.
    init(hex: UInt32, opacity: Double = 1) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: opacity
        )
    }

    /// Appearance-dynamic color, the `body[data-theme]` CSS-variable
    /// equivalent: resolves per the view's effective appearance, so the
    /// whole token set flips when the window appearance changes.
    init(light: NSColor, dark: NSColor) {
        self.init(nsColor: Theme.dynamicNSColor(light: light, dark: dark))
    }
}

// MARK: - Theme preference (the Tauri Appearance tab's `theme` value)

/// Mirrors the Tauri app's theme setting ("light" / "dark" / "system",
/// state.rs default "system"). The native value is a UserDefaults overlay
/// over the (read-only) app-state.json `theme` field, same merge rule as
/// pins and presets: until the user picks a mode natively, Unpeel follows
/// whatever the Tauri app last saved.
enum ThemePreference: String, CaseIterable, Identifiable {
    case system
    case light
    case dark

    var id: String { rawValue }

    var title: String {
        switch self {
        case .system: return "System"
        case .light: return "Light"
        case .dark: return "Dark"
        }
    }

    /// The NSAppearance override for NSApp; nil = follow macOS.
    var nsAppearance: NSAppearance? {
        switch self {
        case .system: return nil
        case .light: return NSAppearance(named: .aqua)
        case .dark: return NSAppearance(named: .darkAqua)
        }
    }
}

/// Native-only sidebar folder palette. Stored by raw value in UserDefaults;
/// app-state.json stays owned by the shared backend.
enum ProjectFolderColor: String, CaseIterable, Identifiable {
    case sky
    case blue
    case violet
    case rose
    case amber
    case moss
    case teal
    case graphite

    var id: String { rawValue }

    var title: String {
        switch self {
        case .sky: return "Sky"
        case .blue: return "Blue"
        case .violet: return "Violet"
        case .rose: return "Rose"
        case .amber: return "Amber"
        case .moss: return "Moss"
        case .teal: return "Teal"
        case .graphite: return "Graphite"
        }
    }

    var nsColor: NSColor {
        switch self {
        case .sky:
            return Theme.dynamicNSColor(
                light: NSColor(hex: 0x2095C9), dark: NSColor(hex: 0x7DD3FC)
            )
        case .blue:
            return Theme.dynamicNSColor(
                light: NSColor(hex: 0x4F73E6), dark: NSColor(hex: 0x7EA6FF)
            )
        case .violet:
            return Theme.dynamicNSColor(
                light: NSColor(hex: 0x7B5BDA), dark: NSColor(hex: 0xB79CFF)
            )
        case .rose:
            return Theme.dynamicNSColor(
                light: NSColor(hex: 0xD75F8F), dark: NSColor(hex: 0xF79AC0)
            )
        case .amber:
            return Theme.dynamicNSColor(
                light: NSColor(hex: 0xB87511), dark: NSColor(hex: 0xF8C86A)
            )
        case .moss:
            return Theme.dynamicNSColor(
                light: NSColor(hex: 0x5F9A3D), dark: NSColor(hex: 0x9DD67A)
            )
        case .teal:
            return Theme.dynamicNSColor(
                light: NSColor(hex: 0x159B91), dark: NSColor(hex: 0x64DCCB)
            )
        case .graphite:
            return Theme.dynamicNSColor(
                light: NSColor(hex: 0x687083), dark: NSColor(hex: 0xB8BCC8)
            )
        }
    }

    var tint: Color { Color(nsColor: nsColor) }
}

enum Theme {
    // MARK: Chrome / layout

    static let titlebarHeight: CGFloat = 38
    static let sidebarDefaultWidth: CGFloat = 300
    static let sidebarMinWidth: CGFloat = 220
    static let sidebarMaxWidth: CGFloat = 520

    /// The stock macOS window corner radius (16pt on macOS 26). The
    /// terminal pane's leading corners are rounded with this so its left
    /// edge mirrors the window's own rounding on the right. AppDelegate
    /// overwrites the default with the radius read off the real window
    /// frame at launch, so it tracks whatever the running OS uses.
    @MainActor static var windowCornerRadius: CGFloat = 16

    /// Shared factory so SwiftUI tokens and layer-backed AppKit consumers
    /// resolve from the same dynamic provider.
    static func dynamicNSColor(light: NSColor, dark: NSColor) -> NSColor {
        NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.aqua, .darkAqua]) == .aqua ? light : dark
        }
    }

    // MARK: Colors (DESIGN.md §2; light values from glass.css [data-theme="light"])

    /// Primary text: light #111217, dark #F3F5FB
    static let foreground = Color(
        light: NSColor(hex: 0x111217), dark: NSColor(hex: 0xF3F5FB)
    )
    /// Muted/secondary text: foreground @ 60% light / 66% dark
    static let mutedForeground = Color(
        light: NSColor(hex: 0x111217, opacity: 0.60),
        dark: NSColor(hex: 0xF3F5FB, opacity: 0.66)
    )
    /// Terminal surface (opaque): light #ffffff, dark #1A1A1F. NSColor twin
    /// for layer-backed views (TerminalHostView.SwapContainer). Darkened
    /// from #222228 (2026-08-04) so the floating cards sit deeper against
    /// the glass; the canvas board inherits it as its surface.
    static let terminalBackgroundNSColor = dynamicNSColor(
        light: NSColor(hex: 0xFFFFFF), dark: NSColor(hex: 0x1A1A1F)
    )
    static let terminalBackground = Color(nsColor: terminalBackgroundNSColor)
    /// Opaque app fallback: light #ffffff, dark #2B2E37
    static let appBackground = Color(
        light: NSColor(hex: 0xFFFFFF), dark: NSColor(hex: 0x2B2E37)
    )
    /// Hover row bg: foreground @ 10% in both modes
    static let hoverRow = Color(
        light: NSColor(hex: 0x111217, opacity: 0.10),
        dark: NSColor(hex: 0xF3F5FB, opacity: 0.10)
    )
    /// Active/selected row bg: light solid white (--glass-active-tint),
    /// dark rgba(255,255,255,0.16)
    static let activeRow = Color(
        light: NSColor(hex: 0xFFFFFF),
        dark: NSColor(hex: 0xFFFFFF, opacity: 0.16)
    )
    /// Attention dot (session) #f59e0b
    static let attention = Color(hex: 0xF59E0B)
    /// Unread badge #60a5fa
    static let unread = Color(hex: 0x60A5FA)
    /// Danger #ef4444
    static let danger = Color(hex: 0xEF4444)
    /// Control accent for native form controls (switch ON state) and the
    /// Quick badge — the Svelte quick-launch green (PresetsPanel.svelte
    /// badge #34C759, same hue as the system switch green).
    static let accent = Color(hex: 0x34C759)
    /// Neutral CTA tint for prominent glass buttons and segmented selection
    /// (designer's spec 2026-06-12: CTAs in the app gray, not system blue).
    /// Light gray reads as "primary" against the dark cards — the old
    /// hand-rolled Save was white-20%; glassProminent needs a near-white
    /// fill to get the same emphasis with an auto-dark label. (A/B'd
    /// 2026-06-12 against the darker app gray #555C6F: that one rendered
    /// nearly identical to the .bordered secondary next to it, killing the
    /// primary/secondary hierarchy.) Light mode inverts to near-black
    /// (--primary light #111217) for the same emphasis with an auto-light
    /// label.
    static let ctaTint = Color(
        light: NSColor(hex: 0x111217), dark: NSColor(white: 0.85, alpha: 1)
    )
    /// Sidebar resizer hairline: dark ≈ rgba(255,255,255,0.055), light a
    /// subtle dark hairline
    static let resizerLine = Color(
        light: NSColor(hex: 0x000000, opacity: 0.08),
        dark: NSColor(hex: 0xFFFFFF, opacity: 0.055)
    )
    /// Glass catch-light along the content pane's leading edge: peaks in the
    /// vertical center and fades into `resizerLine` at the top and bottom, so
    /// the rounded edge reads as a raised glass surface rather than a flat seam.
    static let paneEdgeHighlight = Color(
        light: NSColor(hex: 0xFFFFFF, opacity: 0.40),
        dark: NSColor(hex: 0xFFFFFF, opacity: 0.14)
    )
    /// Generic busy spinner fg/muted mix: dark ≈ #B9BDC9, light ≈ #4A4D55
    static let genericSpinner = Color(
        light: NSColor(hex: 0x4A4D55), dark: NSColor(hex: 0xB9BDC9)
    )
    /// Settings shell dim over the content tint (.settings-main-shell):
    /// dark black @ 24%, light white @ 36% (SettingsView.svelte).
    static let settingsShellDim = Color(
        light: NSColor(hex: 0xFFFFFF, opacity: 0.36),
        dark: NSColor(hex: 0x000000, opacity: 0.24)
    )

    // MARK: Tint overlays painted over vibrancy (DESIGN.md §1)

    // Dark: tune the raw sidebar material toward the content pane so the
    // rounded sidebar boundary does not read as a dark full-height slab. The
    // sidebar list fade is handled by `SidebarListFadeMask`, not by these
    // tint overlays.
    //
    // Each wash below is a historical STACK of uniform tints flattened into
    // the single equivalent color: alpha-compositing two flat colors yields
    // one flat color, and each dropped layer was a full-surface blend pass
    // per frame. The component tints are kept inline so the DESIGN.md tokens
    // stay legible.

    /// Alpha-composite `top` over `bottom` (straight alpha, sRGB — the space
    /// CA blends layers in), so the merged wash renders identically to the
    /// old two-layer stack.
    private static func flattened(_ bottom: NSColor, _ top: NSColor) -> NSColor {
        guard let b = bottom.usingColorSpace(.sRGB),
              let t = top.usingColorSpace(.sRGB) else { return top }
        let ab = b.alphaComponent, at = t.alphaComponent
        let outA = at + ab * (1 - at)
        guard outA > 0 else { return .clear }
        func mix(_ tc: CGFloat, _ bc: CGFloat) -> CGFloat {
            (tc * at + bc * ab * (1 - at)) / outA
        }
        return NSColor(
            srgbRed: mix(t.redComponent, b.redComponent),
            green: mix(t.greenComponent, b.greenComponent),
            blue: mix(t.blueComponent, b.blueComponent),
            alpha: outA
        )
    }

    /// Sidebar: subdued neutral wash; light mode keeps a soft white glass
    /// tint. Bottom #FFFFFF@26% / #1E2A31@10%, top #FFFFFF@18% / #151B20@12%.
    static let sidebarTint = Color(
        light: flattened(
            NSColor(hex: 0xFFFFFF, opacity: 0.26),
            NSColor(hex: 0xFFFFFF, opacity: 0.18)
        ),
        dark: flattened(
            NSColor(hex: 0x1E2A31, opacity: 0.10),
            NSColor(hex: 0x151B20, opacity: 0.12)
        )
    )
    /// Main content: dark #222228 @ 12% over #2B2E37 @ 20%; light white 12%/32%
    static let contentTint = Color(
        light: flattened(
            NSColor(hex: 0xFFFFFF, opacity: 0.32),
            NSColor(hex: 0xFFFFFF, opacity: 0.12)
        ),
        dark: flattened(
            NSColor(hex: 0x2B2E37, opacity: 0.20),
            NSColor(hex: 0x222228, opacity: 0.12)
        )
    )
    /// Settings shell (.settings-main-shell): the content tint pair over
    /// `settingsShellDim` (white@36% / black@24%), flattened the same way.
    static let settingsShellTint = Color(
        light: flattened(
            flattened(
                NSColor(hex: 0xFFFFFF, opacity: 0.36),
                NSColor(hex: 0xFFFFFF, opacity: 0.32)
            ),
            NSColor(hex: 0xFFFFFF, opacity: 0.12)
        ),
        dark: flattened(
            flattened(
                NSColor(hex: 0x000000, opacity: 0.24),
                NSColor(hex: 0x2B2E37, opacity: 0.20)
            ),
            NSColor(hex: 0x222228, opacity: 0.12)
        )
    )

    // MARK: Tool brand colors (DESIGN.md §1/§5)

    /// The CLI's brand tint as a raw 0xRRGGBB, or nil when there is no
    /// per-tool brand color (plain terminals, unknown commands). This is the
    /// single color table: the local Color accessors below AND the phone wire
    /// (`RemoteSessionSummary.spinnerColorHex`) both read it, so a new CLI's
    /// color reaches every surface — including paired phones, with no phone
    /// update — by adding one line here.
    static func toolColorHex(forCommand command: String) -> Int? {
        UnpeelRuntimeCatalog.runtime(command: command)?.tintColorHex
    }

    /// Provider-specific spinner treatments can differ from their brand marks.
    static func toolSpinnerColorHex(forCommand command: String) -> Int? {
        guard let runtime = UnpeelRuntimeCatalog.runtime(command: command) else { return nil }
        return runtime.spinnerTintColorHex ?? runtime.tintColorHex
    }

    static func toolColor(forCommand command: String) -> Color {
        if let hex = toolColorHex(forCommand: command) { return Color(hex: UInt32(hex)) }
        if command.trimmingCharacters(in: .whitespaces).isEmpty {
            // Plain terminal: near-fg gray per mode.
            return Color(light: NSColor(hex: 0x4A4F5A), dark: NSColor(hex: 0xD6D9E1))
        }
        return genericSpinner
    }

    static func toolSpinnerColor(forCommand command: String) -> Color {
        if let hex = toolSpinnerColorHex(forCommand: command) { return Color(hex: UInt32(hex)) }
        return toolColor(forCommand: command)
    }

    // MARK: Sidebar row typography

    /// Session/folder row labels. The Svelte app's 12px/600 renders heavier
    /// in native SF Pro than in the web sidebar, so the native rows use
    /// 13pt medium instead (designer's call, 2026-06-12).
    static let rowLabelFont = Font.system(size: 13, weight: .medium)
    /// Session titles render at full foreground opacity (folders sit at
    /// 0.6), so medium reads optically bolder there — sessions drop one
    /// more weight to match (designer's call, 2026-06-12).
    static let sessionLabelFont = Font.system(size: 13, weight: .regular)
    /// NSFont twin for CALayer-based renderers (shimmer overlay) that must
    /// match rowLabelFont metrics exactly.
    @MainActor static let rowLabelNSFont = NSFont.systemFont(ofSize: 13, weight: .medium)

    // MARK: Spinner (DESIGN.md §5)

    static let spinnerFrames: [String] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
    static let spinnerInterval: TimeInterval = 0.12
}

/// Plain description of the terminal surface theme, one variant per
/// appearance. Consumed by GhosttyBridge (which translates it into a
/// Ghostty `TerminalTheme`, so the surface flips with the window
/// appearance); defined here so the values stay next to the rest of the
/// design tokens.
/// Values: DESIGN.md §3 (terminal/theme.ts light + dark, default scheme).
struct TerminalPaneStyle {
    struct Variant {
        var background: String
        var foreground: String
        var selectionBackground: String
        var cursorColor: String
        /// ANSI 0–15 (default-scheme surface overrides applied).
        var palette: [String]
    }

    /// Dark (default-scheme overrides: brightBlack #6e6e76).
    /// Background follows Theme.terminalBackgroundNSColor (#1A1A1F): the
    /// Ghostty surface paints this string directly, so it must match the
    /// chrome's surface color or the pane shows as a lighter inset patch.
    var dark = Variant(
        background: "#1A1A1F",
        foreground: "#fafafa",
        selectionBackground: "#3a3a40",
        cursorColor: "#fafafa",
        palette: [
            "#1c1c22", "#ef4444", "#22c55e", "#eab308",
            "#3b82f6", "#a855f7", "#06b6d4", "#a1a1aa",
            "#6e6e76", "#f87171", "#4ade80", "#facc15",
            "#60a5fa", "#c084fc", "#22d3ee", "#fafafa",
        ]
    )

    /// Light (terminal/theme.ts light, default scheme).
    var light = Variant(
        background: "#ffffff",
        foreground: "#09090b",
        selectionBackground: "#d4d4d8",
        cursorColor: "#09090b",
        palette: [
            "#09090b", "#dc2626", "#16a34a", "#ca8a04",
            "#2563eb", "#9333ea", "#0891b2", "#e4e4e7",
            "#71717a", "#ef4444", "#22c55e", "#eab308",
            "#3b82f6", "#a855f7", "#06b6d4", "#fafafa",
        ]
    )

    var fontSize: Float = 13
    var windowPaddingX: Int = 10
    var windowPaddingY: Int = 6
    /// Ghostty `window-padding-balance`. Keep this FALSE (ghostty's own
    /// default): balanced padding re-splits the leftover pixels (view size
    /// mod cell size) around the grid on every resize, so during a window
    /// drag the whole text block shifts by a few pixels per frame — visible
    /// as the terminal "shaking". Unbalanced, the grid is pinned at the
    /// fixed top-left padding and the remainder accrues bottom/right
    /// (invisible: window-padding-color=extend paints it as canvas).
    var windowPaddingBalanced = false
    /// Ghostty `mouse-scroll-multiplier`, discrete (wheel-tick) field only —
    /// 3 is ghostty tip's own discrete default. Trackpad (precision) scroll
    /// is pinned to 1 in GhosttyBridge to match the Ghostty app's feel.
    var mouseScrollMultiplier: Int = 3
    /// nil = leave Ghostty's bundled default (JetBrains Mono).
    var fontFamily: String?

    /// DESIGN.md font stack: JetBrains Mono bundled-first, SF Mono fallback.
    /// Ghostty itself bundles JetBrains Mono as its default face, so when
    /// neither is installed system-wide we leave fontFamily nil and still
    /// get JetBrains Mono.
    static func resolved() -> TerminalPaneStyle {
        var style = TerminalPaneStyle()
        for (psName, family) in [("JetBrainsMono-Regular", "JetBrains Mono"),
                                 ("SFMono-Regular", "SF Mono")] {
            if NSFont(name: psName, size: 13) != nil {
                style.fontFamily = family
                break
            }
        }
        return style
    }
}
