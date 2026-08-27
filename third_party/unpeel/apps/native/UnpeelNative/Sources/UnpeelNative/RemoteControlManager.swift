//
//  RemoteControlManager.swift
//  UnpeelNative
//
//  Lifecycle (auto-start policy + supervision) for the remote control server.
//
//  When the server should run, the app spawns `unpeel-host __remote__` as a
//  child process (the same binary resolution as hosted sessions — see
//  `LaunchConfig.hostBinary`), captures its stdout status line, restarts it
//  with a short backoff if it exits, and terminates it on disable/app quit.
//
//  The host prints one JSON status line to stdout on successful bind:
//      { "url": "...", "port": N, "fingerprint": "...", "token": "..." }
//  We log the url + TLS fingerprint for debugging. The token is kept in
//  memory only and is NEVER logged — clients read it from the on-disk
//  remote state file the host writes under ~/.unpeel. The port + fingerprint
//  are kept as `status` so the mobile pairing/bootstrap responses can
//  advertise the server to paired phones.
//
//  Whether the server runs is decided by `shouldRun`:
//  - The hidden UserDefaults key `unpeel.native.remoteControlServer` is a
//    FORCE override when set: true = always run, false = never run.
//      defaults write com.unpeel.native unpeel.native.remoteControlServer -bool YES/NO
//      defaults delete com.unpeel.native unpeel.native.remoteControlServer
//  - When the key is absent (the normal case), the server runs exactly while
//    paired mobile devices exist (`~/.unpeel/mobile/devices.json` non-empty):
//    it auto-starts on app launch when devices exist, starts the moment a
//    device pairs, and stops when the last device is revoked.
//
//  The policy is re-evaluated on `UserDefaults.didChangeNotification` (note:
//  fires only for in-process changes; an external `defaults write` while the
//  app runs takes effect on the next launch) and on
//  `.unpeelMobileDevicesChanged`, posted by `MobilePairingStore` whenever a
//  device pairs or is revoked. Defaults read through `AppDefaults.shared` so
//  a dev/blank UNPEEL_HOME instance keeps its own isolated setting.
//

import AppKit
import Foundation

extension Notification.Name {
    /// Posted by `MobilePairingStore` whenever the paired-device set changes
    /// (pair or revoke), from any thread.
    static let unpeelMobileDevicesChanged = Notification.Name(
        "unpeel.native.mobileDevicesChanged"
    )

    /// Posted by `ViewerPresenceStore` when a phone streams a session's output
    /// (userInfo["sessionID"]). The store clears that session's unread badge,
    /// since a remote viewer counts as observing it.
    static let unpeelMobileViewerObservedSession = Notification.Name(
        "unpeel.native.mobileViewerObservedSession"
    )
}

@MainActor
final class RemoteControlManager {
    static let shared = RemoteControlManager()

    /// Hidden force-override key (see file header). Absent = automatic
    /// (run while paired devices exist).
    static let flagKey = "unpeel.native.remoteControlServer"

    /// Status parsed from the host's stdout JSON line (token intentionally
    /// omitted — see file header).
    struct Status {
        var url: String
        var port: Int
        var fingerprint: String
    }

    private(set) var status: Status?

    private var process: Process?
    /// Set while `stop()` tears the child down, so the termination handler
    /// does not treat the deliberate kill as a crash and respawn.
    private var stopping = false
    /// Consecutive rapid failures (exits shortly after launch). Reset when a
    /// run survives past `rapidExitWindow`.
    private var rapidFailures = 0
    /// Latched after `maxRapidFailures` rapid exits so unrelated
    /// UserDefaults churn does not restart a crash-looping server. Cleared
    /// when the flag is turned off (i.e. re-enabling retries fresh).
    private var gaveUp = false
    private var lastLaunchAt: Date = .distantPast
    private var pendingRestart: DispatchWorkItem?
    private var defaultsObserver: NSObjectProtocol?
    private var devicesObserver: NSObjectProtocol?
    /// Leftover stdout bytes from a read that ended mid-line.
    private var stdoutRemainder = Data()

    private let restartDelay: TimeInterval = 2
    private let maxRapidFailures = 5
    /// A child that lives at least this long is considered healthy; its exit
    /// resets the rapid-failure counter.
    private let rapidExitWindow: TimeInterval = 10

    /// The hidden default as a force override: nil when unset (automatic).
    private var forcedFlag: Bool? {
        guard AppDefaults.shared.object(forKey: Self.flagKey) != nil else { return nil }
        return AppDefaults.shared.bool(forKey: Self.flagKey)
    }

    /// Force override when set; otherwise run exactly while paired mobile
    /// devices exist.
    private var shouldRun: Bool {
        forcedFlag ?? Self.hasPairedDevices()
    }

    /// Reads the shared paired-device store (`MobilePairingStore`'s file);
    /// cheap enough to re-check on every policy sync.
    static func hasPairedDevices() -> Bool {
        let url = LaunchConfig.unpeelDir
            .appendingPathComponent("mobile")
            .appendingPathComponent("devices.json")
        guard let data = try? Data(contentsOf: url),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let devices = object["devices"] as? [[String: Any]] else {
            return false
        }
        return !devices.isEmpty
    }

    /// Call once at app startup. Starts the server if the policy says so and
    /// begins observing flag + paired-device changes.
    func startIfEnabled() {
        if defaultsObserver == nil {
            defaultsObserver = NotificationCenter.default.addObserver(
                forName: UserDefaults.didChangeNotification,
                object: AppDefaults.shared,
                queue: .main
            ) { _ in
                Task { @MainActor in
                    RemoteControlManager.shared.syncWithPolicy()
                }
            }
        }
        if devicesObserver == nil {
            devicesObserver = NotificationCenter.default.addObserver(
                forName: .unpeelMobileDevicesChanged,
                object: nil,
                queue: .main
            ) { _ in
                Task { @MainActor in
                    // A fresh pairing should retry even after a crash-loop
                    // gave up — the user is actively trying to connect.
                    RemoteControlManager.shared.gaveUp = false
                    RemoteControlManager.shared.syncWithPolicy()
                }
            }
        }
        syncWithPolicy()
    }

    /// Reconcile the running child with the current policy.
    private func syncWithPolicy() {
        if shouldRun {
            guard !gaveUp, process == nil, pendingRestart == nil else { return }
            rapidFailures = 0
            launch()
        } else {
            gaveUp = false
            if process != nil || pendingRestart != nil {
                stop()
            }
        }
    }

    /// Terminate the child (disable or app quit). Safe to call repeatedly.
    func stop() {
        pendingRestart?.cancel()
        pendingRestart = nil
        guard let process else { return }
        stopping = true
        // Detach the pipe handler before killing so a torn-down handle does
        // not fire into a half-cleared manager.
        (process.standardOutput as? Pipe)?.fileHandleForReading.readabilityHandler = nil
        (process.standardError as? Pipe)?.fileHandleForReading.readabilityHandler = nil
        if process.isRunning {
            process.terminate()
        }
        self.process = nil
        status = nil
        stopping = false
        NSLog("[UnpeelNative] remote control server stopped")
    }

    // MARK: - Spawn

    /// Kill THIS home's stray `__remote__` server before spawning our own. A
    /// hard-killed app leaves its child running (reparented to launchd); the
    /// next app instance then runs a SECOND server, and the phone can end up
    /// pinned to the stale one — its WS handshake fails and terminal
    /// streaming silently downgrades to the laggy HTTP long-poll, on the
    /// same network.
    ///
    /// Scoped, never `pkill -f`: workspace instances (other UNPEEL_HOMEs) run
    /// their own servers concurrently, and a system-wide match would have
    /// the two managers killing each other's children forever. The server
    /// writes its pid (+ `pid_started_at` since 2026-07) into this home's
    /// `remote.json`; identity is verified before signaling because pids
    /// recycle fast under agent load (same discipline as session manifests).
    private func reapOrphanedServers() {
        let stateURL = LaunchConfig.unpeelDir.appendingPathComponent("remote.json")
        guard let data = try? Data(contentsOf: stateURL),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let pid = (object["pid"] as? NSNumber)?.int32Value, pid > 1
        else { return }
        let verified: Bool
        if let recorded = (object["pid_started_at"] as? NSNumber)?.uint64Value {
            guard let actual = UnpeelStore.processStartTimeMs(pid) else { return }
            let drift = actual > recorded ? actual - recorded : recorded - actual
            verified = drift <= 10_000
        } else {
            // Legacy remote.json without pid_started_at: only a positive
            // argv match proves the pid is still our server.
            verified = Self.processCommandLine(pid)?.contains("unpeel-host __remote__") == true
        }
        guard verified else {
            NSLog("[UnpeelNative] remote control server: stale remote.json pid %d, not signaling", pid)
            return
        }
        if kill(pid, SIGTERM) == 0 {
            NSLog("[UnpeelNative] remote control server: reaped orphaned instance (pid %d)", pid)
        }
    }

    private static func processCommandLine(_ pid: Int32) -> String? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/ps")
        process.arguments = ["-p", String(pid), "-o", "command="]
        let stdout = Pipe()
        process.standardOutput = stdout
        process.standardError = Pipe()
        guard (try? process.run()) != nil else { return nil }
        let data = stdout.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { return nil }
        return String(data: data, encoding: .utf8)
    }

    private func launch() {
        let binary = LaunchConfig.hostBinary
        guard FileManager.default.isExecutableFile(atPath: binary) else {
            NSLog("[UnpeelNative] remote control server: unpeel-host not found at %@", binary)
            return
        }

        reapOrphanedServers()

        let child = Process()
        child.executableURL = URL(fileURLWithPath: binary)
        child.arguments = ["__remote__"]
        // No explicit `environment`: the child inherits the app's full env,
        // which passes UNPEEL_HOME through when set (same behavior the
        // spawned session hosts rely on for state-dir isolation).

        let stdout = Pipe()
        let stderr = Pipe()
        child.standardOutput = stdout
        child.standardError = stderr
        child.standardInput = FileHandle.nullDevice

        stdoutRemainder = Data()
        stdout.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty else { return }
            Task { @MainActor in self?.consumeStdout(data) }
        }
        stderr.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty,
                  let text = String(data: data, encoding: .utf8)?
                      .trimmingCharacters(in: .whitespacesAndNewlines),
                  !text.isEmpty else { return }
            NSLog("[UnpeelNative] remote control server: %@", text)
        }

        child.terminationHandler = { [weak self] proc in
            let code = proc.terminationStatus
            Task { @MainActor in self?.handleExit(code: code) }
        }

        do {
            try child.run()
        } catch {
            NSLog(
                "[UnpeelNative] remote control server failed to launch: %@",
                error.localizedDescription
            )
            return
        }

        process = child
        lastLaunchAt = Date()
        NSLog("[UnpeelNative] remote control server launching (pid %d)", child.processIdentifier)
    }

    private func consumeStdout(_ data: Data) {
        stdoutRemainder.append(data)
        while let newline = stdoutRemainder.firstIndex(of: UInt8(ascii: "\n")) {
            let line = stdoutRemainder[stdoutRemainder.startIndex..<newline]
            stdoutRemainder.removeSubrange(stdoutRemainder.startIndex...newline)
            parseStatusLine(Data(line))
        }
    }

    private func parseStatusLine(_ line: Data) {
        guard let object = try? JSONSerialization.jsonObject(with: line),
              let dict = object as? [String: Any],
              let url = dict["url"] as? String,
              let fingerprint = dict["fingerprint"] as? String else {
            return
        }
        let port = dict["port"] as? Int ?? 0
        status = Status(url: url, port: port, fingerprint: fingerprint)
        // Deliberately never log dict["token"].
        NSLog(
            "[UnpeelNative] remote control server ready at %@ (TLS fingerprint sha256:%@)",
            url, fingerprint
        )
    }

    // MARK: - Restart with backoff

    private func handleExit(code: Int32) {
        guard !stopping else { return }
        process = nil
        status = nil

        guard shouldRun else { return }

        let uptime = Date().timeIntervalSince(lastLaunchAt)
        if uptime >= rapidExitWindow {
            rapidFailures = 0
        } else {
            rapidFailures += 1
        }
        guard rapidFailures < maxRapidFailures else {
            gaveUp = true
            NSLog(
                "[UnpeelNative] remote control server exited (code %d) %d times in quick succession; giving up until re-enabled",
                code, rapidFailures
            )
            return
        }

        NSLog(
            "[UnpeelNative] remote control server exited (code %d); restarting in %.0fs",
            code, restartDelay
        )
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.pendingRestart = nil
            guard self.shouldRun, self.process == nil else { return }
            self.launch()
        }
        pendingRestart = work
        DispatchQueue.main.asyncAfter(deadline: .now() + restartDelay, execute: work)
    }
}
