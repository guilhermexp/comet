//
//  AppDelegate.swift
//  UnpeelNative
//
//  AppKit shell: a single chromeless window (hidden title, full-size
//  content view, repositioned traffic lights) hosting the SwiftUI shell.
//  Knows nothing about Ghostty beyond the GhosttyTerminalPane bridge.
//

import AppKit
import Sparkle
import SwiftUI

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate, SPUUpdaterDelegate,
    NSMenuItemValidation {
    private var window: NSWindow?
    private var store: UnpeelStore?
    private var hookServer: HookServer?
    private let surfaceCache = SurfaceCache()
    private var updaterController: SPUStandardUpdaterController?
    private var licenseLoadObserver: NSObjectProtocol?
    private var menuBar: MenuBarController?

    func applicationDidFinishLaunching(_: Notification) {
        if ProcessInfo.processInfo.environment["UNPEEL_DEBUG"] == "1" {
            GhosttyTerminalPane.enableDebugLogging()
        }

        // One instance per UNPEEL_HOME: two processes on the same state dir
        // would fight over sessions, ports, and pairing. Identity-verified
        // pidfile, so a stale file never blocks a launch.
        if UnpeelWorkspaceLauncher.otherInstanceOwnsCurrentHome() {
            let alert = NSAlert()
            alert.messageText = "This workspace is already running"
            alert.informativeText =
                "Another Unpeel instance is already using this workspace's data. Switch to it instead."
            alert.runModal()
            NSApp.terminate(nil)
            return
        }
        UnpeelWorkspaceLauncher.writeOwnPidFile()

        let store = UnpeelStore()
        self.store = store

        // Finder "New Unpeel Session Here" service (NSServices in Info.plist):
        // macOS delivers the right-clicked folder(s) to `newUnpeelSession`.
        NSApp.servicesProvider = self

        // Hook server: receives provider hook events for sessions this
        // instance can see and registers its port in ~/.unpeel/app-ports
        // (the hook scripts broadcast to every registered port).
        // Capture the store weakly: it also holds the server (attachHookServer),
        // so a strong capture here would form a store ⇄ server retain cycle.
        if let server = HookServer(onEvent: { [weak store] event in
            Task { @MainActor in store?.handleHookEvent(event) }
        }) {
            hookServer = server
            store.attachHookServer(server)
        } else {
            NSLog("[UnpeelNative] hook server failed to start; sessions launch without hook env")
        }
        if UnpeelFeatureFlags.mobileRemoteControlEnabled {
            store.startHostRemoteServer()
        }
        // macOS banners for "needs input" / "notify when done"; tapping one
        // selects the session. The phone push is the away-from-desk counterpart.
        DesktopNotifier.shared.onSelectSession = { [weak store] sessionID in
            store?.revealSessionInSidebar(sessionID)
        }
        DesktopNotifier.shared.requestAuthorizationIfNeeded()
        // Hidden flag (unpeel.native.remoteControlServer): spawns
        // `unpeel-host __remote__` as a managed child. No-op when off (default).
        RemoteControlManager.shared.startIfEnabled()
        // Computer-use engine daemon (cua-driver, embedded): must be spawned
        // by THIS app so TCC attributes to Unpeel.app. No-op unless the
        // Computer use flag is on and access isn't Off.
        ComputerEngineManager.shared.startIfEnabled()

        // ⌘1–9 switches between the active project's sessions (held ⌘ shows
        // the hints). Installed here, not in UnpeelStore.init, so the
        // self-tests' throwaway stores never register event monitors.
        store.installSessionShortcutMonitors()

        // Menu-bar presence: keeps the activity dropdown (and live spinner) one
        // click away while the window is closed, and reopens to the session.
        menuBar = MenuBarController(
            store: store,
            onSelect: { [weak self] sessionID in
                self?.openSession(sessionID)
            },
            onManageWorkspaces: { [weak self] in
                self?.showMainWindow()
                self?.store?.openSettings(tab: .workspaces)
            },
            onShowAllRecent: { [weak self] in
                self?.showMainWindow()
                self?.store?.openRecentActivity()
            }
        )

        installMainMenu()
        showMainWindow()

        if let window {
            Snapshot.armIfRequested(window: window, store: store, cache: surfaceCache)
        }
        Snapshot.runPresetSelfTestIfRequested(store: store)
        Snapshot.runReorderSelfTestIfRequested(store: store)
        Snapshot.runRenameSelfTestIfRequested(store: store)

        DispatchQueue.main.async { [weak self] in
            self?.startSparkleUpdaterIfNeeded()
        }
    }

    private func startSparkleUpdaterIfNeeded() {
        guard updaterController == nil, Self.sparkleCanStart else { return }

        // Wait for the Keychain license load: starting earlier would run the
        // first scheduled check against a still-`unlicensed` state and
        // silently drop it until the next 24h interval.
        guard LicenseManager.shared.initialLoadComplete else {
            if licenseLoadObserver == nil {
                licenseLoadObserver = NotificationCenter.default.addObserver(
                    forName: LicenseManager.initialLoadNotification,
                    object: nil,
                    queue: .main
                ) { [weak self] _ in
                    Task { @MainActor in
                        if let self, let observer = self.licenseLoadObserver {
                            NotificationCenter.default.removeObserver(observer)
                            self.licenseLoadObserver = nil
                        }
                        self?.startSparkleUpdaterIfNeeded()
                    }
                }
            }
            return
        }

        let controller = SPUStandardUpdaterController(
            startingUpdater: false,
            updaterDelegate: self,
            userDriverDelegate: nil
        )
        updaterController = controller
        controller.startUpdater()
        // Sparkle resolves the feed as delegate → UserDefaults → Info.plist.
        // Any stray SUFeedURL default would silently override the baked feed
        // (see feedURLString(for:) below, which pins it); clear it too so the
        // defaults never carry one.
        controller.updater.clearFeedURLFromUserDefaults()
        installMainMenu()
    }

    /// Creates the main window if it does not exist, otherwise brings the
    /// existing one to the front. Building lives here (not inline in launch) so
    /// Dock re-open (`applicationShouldHandleReopen`) and menu-bar session
    /// clicks can resurrect the window after the user closed it — the hosted
    /// PTYs keep running while the app lives on as a menu-bar agent.
    func showMainWindow() {
        if let window {
            window.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            positionTrafficLights()
            return
        }
        guard let store else { return }

        // Route @AppStorage (e.g. sidebar width) through the same defaults
        // store as the rest of the app, so a dev/blank UNPEEL_HOME instance is
        // isolated here too.
        let root = RootView(store: store, cache: surfaceCache)
            .defaultAppStorage(AppDefaults.shared)

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1200, height: 800),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = ""
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isMovableByWindowBackground = false
        window.contentMinSize = NSSize(width: 800, height: 600)
        window.isOpaque = false
        window.backgroundColor = .clear
        // We nil out `self.window` in windowWillClose and rebuild on demand, so
        // don't let AppKit free it from under us on close.
        window.isReleasedWhenClosed = false
        // Appearance is app-driven: UnpeelStore.init already applied the
        // saved ThemePreference to NSApp.appearance (nil = follow macOS),
        // and the window inherits it.
        window.delegate = self

        // The terminal pane rounds its leading corners to match the window
        // radius (Theme.windowCornerRadius). The system radius is not public
        // API, so read it off the frame view (NSThemeFrame.cornerRadius) and
        // keep the hardcoded fallback if the key ever disappears. Done before
        // the hosting view is attached so the first SwiftUI render sees it.
        if let frameView = window.contentView?.superview,
           frameView.responds(to: Selector(("cornerRadius"))),
           let radius = frameView.value(forKey: "cornerRadius") as? CGFloat,
           radius > 0 {
            Theme.windowCornerRadius = radius
        }

        let hosting = NSHostingView(rootView: root)
        // Never let SwiftUI's reported min/max size drive the window frame:
        // the default `.standardBounds` propagates content minimums into
        // window autolayout, and the Settings → Advanced grouped Form
        // reports its FULL row stack (~3800pt for ~60 running terminals) as
        // a minimum — opening that tab yanked the window to screen height.
        // The window's own contentMinSize (800×600) is the real floor;
        // panels scroll internally.
        hosting.sizingOptions = []
        window.contentView = hosting

        window.center()
        window.makeKeyAndOrderFront(nil)
        self.window = window

        positionTrafficLights()
        NSApp.activate(ignoringOtherApps: true)
    }

    /// Standard macOS main menu (HIG order): About / Check for Updates…,
    /// Settings… (⌘,) + Agent CLI Tools…, Services, Hide/Show, Quit — plus
    /// Edit (so cut/copy/paste/undo work in text fields), Window, and Help.
    private func installMainMenu() {
        let mainMenu = NSMenu()

        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        // Explicit enabled-state management: with autoenablesItems (the
        // default), AppKit re-enables the targetless "Check for Updates…"
        // item via responder-chain action resolution, making the disable
        // below a silent no-op button.
        appMenu.autoenablesItems = false
        let aboutItem = NSMenuItem(
            title: "About Unpeel",
            action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)),
            keyEquivalent: ""
        )
        aboutItem.target = NSApp
        appMenu.addItem(aboutItem)
        let updatesItem = NSMenuItem(
            title: "Check for Updates…",
            action: #selector(checkForUpdates(_:)),
            keyEquivalent: ""
        )
        if updaterController != nil {
            updatesItem.target = self
        } else {
            updatesItem.isEnabled = false
        }
        appMenu.addItem(updatesItem)
        appMenu.addItem(.separator())
        let settingsItem = NSMenuItem(
            title: "Settings…",
            action: #selector(openSettingsFromMenu),
            keyEquivalent: ","
        )
        settingsItem.target = self
        appMenu.addItem(settingsItem)
        appMenu.addItem(.separator())
        let servicesItem = NSMenuItem(title: "Services", action: nil, keyEquivalent: "")
        let servicesMenu = NSMenu(title: "Services")
        servicesItem.submenu = servicesMenu
        appMenu.addItem(servicesItem)
        NSApp.servicesMenu = servicesMenu
        appMenu.addItem(.separator())
        let hideItem = NSMenuItem(
            title: "Hide Unpeel",
            action: #selector(NSApplication.hide(_:)),
            keyEquivalent: "h"
        )
        hideItem.target = NSApp
        appMenu.addItem(hideItem)
        let hideOthersItem = NSMenuItem(
            title: "Hide Others",
            action: #selector(NSApplication.hideOtherApplications(_:)),
            keyEquivalent: "h"
        )
        hideOthersItem.keyEquivalentModifierMask = [.command, .option]
        hideOthersItem.target = NSApp
        appMenu.addItem(hideOthersItem)
        let showAllItem = NSMenuItem(
            title: "Show All",
            action: #selector(NSApplication.unhideAllApplications(_:)),
            keyEquivalent: ""
        )
        showAllItem.target = NSApp
        appMenu.addItem(showAllItem)
        appMenu.addItem(.separator())
        appMenu.addItem(NSMenuItem(
            title: "Quit Unpeel",
            action: #selector(NSApplication.terminate(_:)),
            keyEquivalent: "q"
        ))
        appItem.submenu = appMenu
        mainMenu.addItem(appItem)

        // Session menu: ⌘N launches the leading favorite preset in the
        // current project (UnpeelStore.launchDefaultSession). Deliberately no
        // tabs/panes items — the sidebar (one row per session) is the model.
        let sessionItem = NSMenuItem()
        let sessionMenu = NSMenu(title: "Session")
        let newSessionItem = NSMenuItem(
            title: "New Session",
            action: #selector(newSessionFromMenu),
            keyEquivalent: "n"
        )
        newSessionItem.target = self
        sessionMenu.addItem(newSessionItem)
        // ⌘T is terminal muscle memory: a plain shell, where ⌘N is the
        // leading agent favorite.
        let newTerminalItem = NSMenuItem(
            title: "New Terminal",
            action: #selector(newTerminalFromMenu),
            keyEquivalent: "t"
        )
        newTerminalItem.target = self
        sessionMenu.addItem(newTerminalItem)
        sessionMenu.addItem(.separator())
        // The palette is the discoverable home for "jump to anything";
        // the key equivalent works app-wide, including over the terminal.
        let paletteItem = NSMenuItem(
            title: "Command Palette",
            action: #selector(toggleCommandPaletteFromMenu),
            keyEquivalent: "k"
        )
        paletteItem.target = self
        sessionMenu.addItem(paletteItem)
        sessionMenu.addItem(.separator())
        // ⌘⇧S (the Firefox screenshot binding; the system's ⌘⇧3/4/5 are
        // intercepted before apps see them). Routed via notification to the
        // displayed session's gallery chip, which owns the capture flow.
        let screenshotItem = NSMenuItem(
            title: "Take Screenshot…",
            action: #selector(takeScreenshotFromMenu),
            keyEquivalent: "s"
        )
        screenshotItem.keyEquivalentModifierMask = [.command, .shift]
        screenshotItem.target = self
        sessionMenu.addItem(screenshotItem)
        sessionItem.submenu = sessionMenu
        mainMenu.addItem(sessionItem)

        let editItem = NSMenuItem()
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(NSMenuItem(
            title: "Undo", action: Selector(("undo:")), keyEquivalent: "z"
        ))
        editMenu.addItem(NSMenuItem(
            title: "Redo", action: Selector(("redo:")), keyEquivalent: "Z"
        ))
        editMenu.addItem(.separator())
        editMenu.addItem(NSMenuItem(
            title: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x"
        ))
        editMenu.addItem(NSMenuItem(
            title: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c"
        ))
        editMenu.addItem(NSMenuItem(
            title: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v"
        ))
        editMenu.addItem(NSMenuItem(
            title: "Select All",
            action: #selector(NSText.selectAll(_:)),
            keyEquivalent: "a"
        ))
        editMenu.addItem(.separator())
        // Terminal search rides libghostty's own engine; the notification
        // reaches the displayed pane's find bar (GhosttyBridge). The key
        // equivalents only arrive here because the surface's default
        // keybinds are cleared (applySurfaceKeybinds) — a focused surface
        // consumes any chord it has a binding for before NSMenu runs.
        let findItem = NSMenuItem(
            title: "Find…",
            action: #selector(findInTerminalFromMenu),
            keyEquivalent: "f"
        )
        findItem.target = self
        editMenu.addItem(findItem)
        let findNextItem = NSMenuItem(
            title: "Find Next",
            action: #selector(findNextInTerminalFromMenu),
            keyEquivalent: "g"
        )
        findNextItem.target = self
        editMenu.addItem(findNextItem)
        let findPreviousItem = NSMenuItem(
            title: "Find Previous",
            action: #selector(findPreviousInTerminalFromMenu),
            keyEquivalent: "G"
        )
        findPreviousItem.target = self
        editMenu.addItem(findPreviousItem)
        editItem.submenu = editMenu
        mainMenu.addItem(editItem)

        // Window menu: closing the window keeps the app alive as a menu-bar
        // agent (applicationShouldTerminateAfterLastWindowClosed), so ⌘W
        // lives here rather than in a File menu we don't otherwise need.
        let windowItem = NSMenuItem()
        let windowMenu = NSMenu(title: "Window")
        windowMenu.addItem(NSMenuItem(
            title: "Close Window",
            action: #selector(NSWindow.performClose(_:)),
            keyEquivalent: "w"
        ))
        windowMenu.addItem(NSMenuItem(
            title: "Minimize",
            action: #selector(NSWindow.performMiniaturize(_:)),
            keyEquivalent: "m"
        ))
        windowMenu.addItem(NSMenuItem(
            title: "Zoom",
            action: #selector(NSWindow.performZoom(_:)),
            keyEquivalent: ""
        ))
        windowMenu.addItem(.separator())
        windowMenu.addItem(NSMenuItem(
            title: "Bring All to Front",
            action: #selector(NSApplication.arrangeInFront(_:)),
            keyEquivalent: ""
        ))
        windowItem.submenu = windowMenu
        mainMenu.addItem(windowItem)
        NSApp.windowsMenu = windowMenu

        let helpItem = NSMenuItem()
        let helpMenu = NSMenu(title: "Help")
        let helpDocsItem = NSMenuItem(
            title: "Unpeel Help",
            action: #selector(openHelpFromMenu),
            keyEquivalent: ""
        )
        helpDocsItem.target = self
        helpMenu.addItem(helpDocsItem)
        helpItem.submenu = helpMenu
        mainMenu.addItem(helpItem)
        NSApp.helpMenu = helpMenu

        NSApp.mainMenu = mainMenu
    }

    // MARK: - Finder service

    /// `NSMessage` target for the "New Unpeel Session Here" Finder service
    /// (declared in Info.plist, see build-app.sh). AppKit delivers the
    /// right-clicked folder(s) on `pboard`; we reuse-or-add the first one as
    /// a project and show the main-screen session launcher for it.
    @objc func newUnpeelSession(
        _ pboard: NSPasteboard,
        userData _: String?,
        error _: AutoreleasingUnsafeMutablePointer<NSString>?
    ) {
        guard store?.selectedHostScope == .local else { return }
        let urls = pboard.readObjects(
            forClasses: [NSURL.self],
            options: [.urlReadingFileURLsOnly: true]
        ) as? [URL] ?? []

        // Folders only (NSSendFileTypes already filters to public.folder, but
        // be defensive in case a file slips through).
        let folder = urls.first { url in
            var isDir: ObjCBool = false
            return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir)
                && isDir.boolValue
        }
        guard let folder else { return }

        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
        store?.openLauncher(forFolder: folder.path)
    }

    @objc private func openSettingsFromMenu() {
        store?.openSettings()
    }

    @objc private func newSessionFromMenu() {
        store?.launchDefaultSession()
    }

    @objc private func newTerminalFromMenu() {
        store?.launchDefaultTerminal()
    }

    @objc private func toggleCommandPaletteFromMenu() {
        store?.toggleCommandPalette()
    }

    @objc private func takeScreenshotFromMenu() {
        NotificationCenter.default.post(name: .unpeelTakeSessionScreenshot, object: nil)
    }

    @objc private func findInTerminalFromMenu() {
        NotificationCenter.default.post(name: .unpeelTerminalFind, object: nil)
    }

    @objc private func findNextInTerminalFromMenu() {
        NotificationCenter.default.post(name: .unpeelTerminalFindNext, object: nil)
    }

    @objc private func findPreviousInTerminalFromMenu() {
        NotificationCenter.default.post(name: .unpeelTerminalFindPrevious, object: nil)
    }

    /// Session ▸ Take Screenshot… greys out while the session gallery is
    /// disabled (Appearance ▸ "Session gallery") — the gallery chip owns the
    /// capture flow, so with no chip mounted the notification goes nowhere.
    func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
        if menuItem.action == #selector(takeScreenshotFromMenu) {
            return store?.selectedHostScope == .local
                && (store?.showSessionGallery ?? false)
        }
        if menuItem.action == #selector(newSessionFromMenu)
            || menuItem.action == #selector(newTerminalFromMenu)
            || menuItem.action == #selector(toggleCommandPaletteFromMenu)
        {
            return store?.selectedHostScope == .local
        }
        // Find drives the displayed Local pane's find bar; remote panes
        // don't listen (yet).
        if menuItem.action == #selector(findInTerminalFromMenu)
            || menuItem.action == #selector(findNextInTerminalFromMenu)
            || menuItem.action == #selector(findPreviousInTerminalFromMenu)
        {
            return store?.selectedHostScope == .local
        }
        return true
    }

    @objc private func openHelpFromMenu() {
        NSWorkspace.shared.open(URL(string: "https://unpeel.com/docs")!)
    }

    @objc private func checkForUpdates(_ sender: Any?) {
        refreshSparkleLicenseHeaders()
        updaterController?.checkForUpdates(sender)
    }

    private func refreshSparkleLicenseHeaders() {
        updaterController?.updater.httpHeaders = Self.licenseHeadersForUpdateFeed
    }

    /// The baked-in feed URL. Handing this to Sparkle via the delegate (top
    /// priority in its feed resolution) means a `SUFeedURL` UserDefaults entry
    /// — writable by any local process — can never redirect update checks
    /// (and the license headers that ride on them) to another host.
    private static var infoPlistFeedURL: String? {
        Bundle.main.object(forInfoDictionaryKey: "SUFeedURL") as? String
    }

    /// Update-check headers, but only for our own update host: never attach
    /// them to a request that could be going anywhere else. Always carries the
    /// anonymous install id (active-install/MAU counting server-side); license
    /// headers ride along only while a Pro license is active.
    private static var licenseHeadersForUpdateFeed: [String: String]? {
        guard let feed = infoPlistFeedURL,
              Self.isTrustedUpdateHost(URL(string: feed)?.host)
        else { return nil }
        var headers = LicenseManager.shared.updateAuthorizationHeaders ?? [:]
        headers["X-Unpeel-Install-ID"] = installID
        if let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String {
            headers["X-Unpeel-App-Version"] = version
        }
        return headers
    }

    /// Anonymous install identity for update checks: a random UUID minted once
    /// per install. Deliberately NOT `LicenseManager.deviceID` — that one is
    /// hardware-derived and bound to license seats, and the server must not be
    /// able to join usage rows to licensing rows.
    private static let installIDKey = "unpeel.native.installID"
    private static var installID: String {
        let defaults = AppDefaults.shared
        if let existing = defaults.string(forKey: installIDKey), !existing.isEmpty {
            return existing
        }
        let minted = UUID().uuidString
        defaults.set(minted, forKey: installIDKey)
        return minted
    }

    private static func isTrustedUpdateHost(_ host: String?) -> Bool {
        guard let host else { return false }
        return host == "unpeel.com" || host.hasSuffix(".unpeel.com")
    }

    func feedURLString(for _: SPUUpdater) -> String? {
        Self.infoPlistFeedURL
    }

    func updater(_ updater: SPUUpdater, mayPerform updateCheck: SPUUpdateCheck) throws {
        // Updates are never license-gated: the app is free and the same bytes
        // are publicly downloadable as the install DMG. Pro headers still ride
        // along (trusted hosts only) so the server could gate entitlements.
        updater.httpHeaders = Self.licenseHeadersForUpdateFeed
    }

    func updater(_ updater: SPUUpdater, willDownloadUpdate item: SUAppcastItem, with request: NSMutableURLRequest) {
        guard let headers = Self.licenseHeadersForUpdateFeed,
              Self.isTrustedUpdateHost(request.url?.host)
        else { return }
        updater.httpHeaders = headers
        for (field, value) in headers {
            request.setValue(value, forHTTPHeaderField: field)
        }
    }

    private static var sparkleCanStart: Bool {
        // Only the default instance updates: two updaters double-install,
        // Sparkle's relaunch goes through `open` (dropping UNPEEL_HOME, so a
        // workspace would come back as the default workspace), and
        // clearFeedURLFromUserDefaults writes the shared .standard domain.
        // Workspace instances pick the new binary up on their next relaunch.
        guard UnpeelWorkspaceContext.isDefaultInstance else { return false }
        guard Bundle.main.bundlePath.hasSuffix(".app") else { return false }
        let feed = Bundle.main.object(forInfoDictionaryKey: "SUFeedURL") as? String
        let key = Bundle.main.object(forInfoDictionaryKey: "SUPublicEDKey") as? String
        return feed?.isEmpty == false && key?.isEmpty == false
    }

    /// Traffic lights at (12, 17) per DESIGN.md §1 — centers land in the
    /// middle of the 38px custom titlebar strip.
    private func positionTrafficLights() {
        guard let window else { return }
        let types: [NSWindow.ButtonType] = [.closeButton, .miniaturizeButton, .zoomButton]
        guard let first = window.standardWindowButton(.closeButton),
              let container = first.superview else { return }

        let xStart: CGFloat = 12
        let spacing: CGFloat = 20
        for (index, type) in types.enumerated() {
            guard let button = window.standardWindowButton(type) else { continue }
            var frame = button.frame
            frame.origin.x = xStart + CGFloat(index) * spacing
            // Center vertically in the 38px titlebar (container sits at the
            // very top of the window), clamped inside the container.
            let centerFromTop = Theme.titlebarHeight / 2
            frame.origin.y = container.bounds.height - centerFromTop - (frame.height / 2)
            frame.origin.y = max(0, min(frame.origin.y, container.bounds.height - frame.height))
            button.setFrameOrigin(frame.origin)
        }
    }

    // Re-apply after events that make AppKit reset button positions.
    func windowDidResize(_: Notification) { positionTrafficLights() }
    func windowDidBecomeKey(_: Notification) { positionTrafficLights() }
    func windowDidEnterFullScreen(_: Notification) { store?.windowIsFullScreen = true }
    func windowDidExitFullScreen(_: Notification) {
        store?.windowIsFullScreen = false
        positionTrafficLights()
    }

    // Closing the window does NOT quit: the app lives on as a menu-bar agent
    // so hosted sessions keep their live spinners and stay one click away.
    // ⌘Q is the explicit teardown (sessions still survive as hosted PTYs).
    func applicationShouldTerminateAfterLastWindowClosed(_: NSApplication) -> Bool {
        false
    }

    // Dock-icon click (or other re-open) with no window on screen rebuilds it.
    func applicationShouldHandleReopen(_: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if !flag { showMainWindow() }
        return true
    }

    /// Menu-bar session click: bring the app forward, (re)build the window if it
    /// was closed, then reveal + smooth-scroll to the session. The reveal is
    /// dispatched one runloop tick later so a freshly-mounted sidebar is laid
    /// out before `revealSessionInSidebar` fires its scroll request (which it
    /// holds live for ~1s, so the new sidebar still catches it).
    func openSession(_ sessionID: String) {
        showMainWindow()
        DispatchQueue.main.async { [weak self] in
            self?.store?.revealSessionInSidebar(sessionID)
        }
    }

    // Drop our reference so the next showMainWindow() rebuilds rather than
    // re-fronting a torn-down window (paired with isReleasedWhenClosed = false).
    func windowWillClose(_ notification: Notification) {
        if (notification.object as? NSWindow) === window {
            window = nil
        }
    }

    /// Clean shutdown: drop our port from ~/.unpeel/app-ports so hook
    /// scripts stop broadcasting to a dead listener (HookServer Drop parity,
    /// hook_server.rs:595-603).
    func applicationWillTerminate(_: Notification) {
        RemoteControlManager.shared.stop()
        ComputerEngineManager.shared.stop()
        store?.stopHostRemoteServer()
        hookServer?.stop()
        hookServer = nil
        // Best-effort: the identity check in runningPid makes a leftover
        // pidfile harmless after a crash.
        UnpeelWorkspaceLauncher.removeOwnPidFile()
    }
}
