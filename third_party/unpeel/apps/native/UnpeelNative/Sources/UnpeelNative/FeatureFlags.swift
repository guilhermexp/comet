import Foundation

/// A user-facing experimental feature, toggleable in Settings ▸ Experimental.
///
/// Adding a new experiment is a single entry in `all` below: it automatically
/// gets a toggle row in the Experimental tab and an `isEnabled` check you can
/// gate UI on. Persistence is a native UserDefaults overlay (never
/// app-state.json), keyed by `defaultsKey`; optional environment overrides are
/// dev escape hatches that force-enable the feature when an env var == "1".
struct ExperimentalFeature: Identifiable, Hashable {
    /// Stable id; also the UserDefaults key suffix. Never rename once shipped.
    let key: String
    let title: String
    let summary: String
    let defaultsKey: String
    let envOverride: String?
    let legacyEnvOverrides: [String]
    let defaultOn: Bool

    var id: String { key }

    init(
        key: String,
        title: String,
        summary: String,
        envOverride: String? = nil,
        legacyEnvOverrides: [String] = [],
        defaultOn: Bool = false
    ) {
        self.key = key
        self.title = title
        self.summary = summary
        self.defaultsKey = "unpeel.experimental.\(key)"
        self.envOverride = envOverride
        self.legacyEnvOverrides = legacyEnvOverrides
        self.defaultOn = defaultOn
    }

    var envOverrides: [String] {
        [envOverride].compactMap { $0 } + legacyEnvOverrides
    }
}

extension ExperimentalFeature {
    /// Run sessions in isolated git worktrees so multiple agents can work the
    /// same repo in parallel. Gates every worktree UI entry point (project
    /// context menu, "In a new worktree" submenus, the inline worktree
    /// folder rows).
    static let worktrees = ExperimentalFeature(
        key: "worktrees",
        title: "Git worktrees",
        summary: "Run sessions in an isolated git worktree of a project so multiple "
            + "agents can work the same repo in parallel without touching each other's "
            + "files. Adds worktree options to the project and new-session menus.",
        envOverride: "UNPEEL_DEV_WORKTREES",
        defaultOn: true
    )

    /// Sessions MCP: agent sessions can read other sessions and coordinate
    /// freely inside their sidebar group. Gates the Settings ▸ Sessions MCP
    /// tab and whether new sessions launch with the MCP client injected.
    static let sessionsMcp = ExperimentalFeature(
        key: "sessionsMcp",
        title: "Sessions use",
        summary: "Let an agent session see your other sessions: it can read them all, "
            + "coordinate freely with sessions in its sidebar group, and ask before "
            + "writing to another group. These are cooperation controls, not a sandbox "
            + "against commands running as your macOS user. Adds the Sessions settings "
            + "tab. Applies when a session starts, so already-running sessions pick it "
            + "up after a restart.",
        envOverride: "UNPEEL_DEV_SESSIONS_MCP",
        defaultOn: true
    )

    /// Workspaces: run additional, fully isolated instances of Unpeel on this
    /// Mac (own sessions, projects, settings, and phone pairing identity).
    /// Gates the Settings ▸ Workspaces tab. The persisted key is deliberately
    /// still `profiles`: shipped experimental-feature keys are immutable.
    static let workspaces = ExperimentalFeature(
        key: "profiles",
        title: "Workspaces",
        summary: "Run extra, fully separate copies of Unpeel on this Mac — each "
            + "workspace has its own sessions, projects, presets, settings, and "
            + "pairs with your phone as its own Mac. Adds the Workspaces settings "
            + "tab.",
        envOverride: "UNPEEL_DEV_WORKSPACES",
        legacyEnvOverrides: ["UNPEEL_DEV_PROFILES"],
        defaultOn: true
    )

    /// Computer Use MCP: agent sessions can read app windows and drive them
    /// in the background through the embedded cua-driver engine. Gates the
    /// Settings ▸ Computer tab, the engine daemon, and whether new sessions
    /// launch with the `computer` domain advertised.
    static let computerUse = ExperimentalFeature(
        key: "computerUse",
        title: "Computer use",
        summary: "Development only. Let agent sessions control this Mac's apps in the "
            + "background: read a window's UI elements, take screenshots, click, and type — "
            + "without moving your cursor or stealing focus. By default each "
            + "session asks you once before its first action. That prompt coordinates "
            + "agents; it is not isolation from same-user shell code. Adds the Computer "
            + "settings tab in development builds only.",
        envOverride: "UNPEEL_DEV_COMPUTER_USE",
        defaultOn: false
    )

    /// Browser MCP: agent sessions get an isolated real browser. Gates the
    /// Settings ▸ Browser tab and whether new sessions launch with the
    /// `browser` domain advertised.
    static let browserMcp = ExperimentalFeature(
        key: "browserMcp",
        title: "Browser use",
        summary: "Let agent sessions drive a real browser — open pages, click, "
            + "fill forms, and take screenshots. Each session gets its own "
            + "isolated browser with no access to your normal browser profile. Browser "
            + "access prompts are cooperation controls, not a sandbox against commands "
            + "running as your macOS user. Adds the Browser settings tab.",
        envOverride: "UNPEEL_DEV_BROWSER_MCP",
        defaultOn: true
    )

    /// Everything shown in Settings ▸ Experimental, in display order.
    static let all: [ExperimentalFeature] = [
        .worktrees, .sessionsMcp, .browserMcp, .computerUse, .workspaces,
    ]
}

enum UnpeelFeatureFlags {
    /// Computer Use currently relies on an unrestricted same-UID daemon that
    /// inherits the app's TCC grants. Until hosted sessions have a kernel-
    /// enforced broker boundary, it is a development-build facility only.
    static var computerUseAvailable: Bool {
        computerUseAvailable(infoDictionary: Bundle.main.infoDictionary)
    }

    /// Pure form used by containment tests. The marker is baked into dev
    /// bundles by build-app.sh; missing, false, or a wrong type fails closed.
    static func computerUseAvailable(infoDictionary: [String: Any]?) -> Bool {
        infoDictionary?["UnpeelDevelopmentBuild"] as? Bool == true
    }

    static func isAvailable(_ feature: ExperimentalFeature) -> Bool {
        isAvailable(feature, developmentBuild: computerUseAvailable)
    }

    /// Availability independent of Bundle.main, so tests cover the production
    /// boundary without relying on the Swift test runner's Info.plist.
    static func isAvailable(
        _ feature: ExperimentalFeature, developmentBuild: Bool
    ) -> Bool {
        feature != .computerUse || developmentBuild
    }

    static var availableExperimentalFeatures: [ExperimentalFeature] {
        ExperimentalFeature.all.filter(isAvailable)
    }

    /// Whether an experimental feature is currently enabled — env override
    /// first (dev escape hatch), then the user's stored preference, then the
    /// feature's built-in default.
    static func isEnabled(_ feature: ExperimentalFeature) -> Bool {
        guard isAvailable(feature) else { return false }
        if feature.envOverrides.contains(where: {
            ProcessInfo.processInfo.environment[$0] == "1"
        }) {
            return true
        }
        if AppDefaults.shared.object(forKey: feature.defaultsKey) == nil {
            return feature.defaultOn
        }
        return AppDefaults.shared.bool(forKey: feature.defaultsKey)
    }

    /// Persist a user preference for an experimental feature.
    static func setEnabled(_ enabled: Bool, for feature: ExperimentalFeature) {
        guard isAvailable(feature) else { return }
        AppDefaults.shared.set(enabled, forKey: feature.defaultsKey)
    }

    static var mobileRemoteControlEnabled: Bool {
        if ProcessInfo.processInfo.environment["UNPEEL_DEV_MOBILE_REMOTE"] == "1" {
            return true
        }
        let key = "unpeel.dev.mobileRemoteControl"
        guard AppDefaults.shared.object(forKey: key) != nil else {
            return true
        }
        return AppDefaults.shared.bool(forKey: key)
    }

    /// Mac-as-client: connect this Unpeel to another Unpeel's remote server
    /// and attach to its sessions. Experimental; pairs with the Rust-side
    /// UNPEEL_REMOTE_ATTACH=1 gate on the attach CLI.
    static var remoteUnpeelClientEnabled: Bool {
        if ProcessInfo.processInfo.environment["UNPEEL_REMOTE_ATTACH"] == "1" {
            return true
        }
        return AppDefaults.shared.bool(forKey: "unpeel.dev.remoteUnpeelClient")
    }
}
