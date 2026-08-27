//
//  RelayUplinkManager.swift
//  UnpeelNative
//
//  The Mac side of Unpeel Remote: one outbound WebSocket to the Cloudflare
//  relay (apps/relay) that lets paired phones reach this Mac away from the
//  LAN. Decrypted tunnel requests run through the exact same
//  MobileRemoteServer.handle pipeline as LAN traffic — the relay adds no
//  new authorization surface. All payloads are end-to-end encrypted with
//  per-device keys (RelayProtocol.swift in UnpeelShared); the relay sees
//  ciphertext only.
//
//  Gates, in order: the mobile feature flag, ≥1 paired device enrolled on
//  Unpeel Link (the per-device `relayAllowed` flag — there is no global
//  toggle anymore), and a signed entitlement from unpeel.com (the
//  paid-service lever — fetched with the license key and cached in
//  ~/.unpeel/mobile/relay-entitlement.json). See
//  docs/feature/unpeel-remote.md.
//

import Darwin
import Foundation
import UnpeelShared

enum RelayConfig {
    /// The retired global "Access away from home" toggle's key. New builds
    /// gate the uplink purely on per-device enrollment, but the key stays
    /// alive for downgrade compatibility: `migrateLegacyRelayPreference`
    /// folds a stored `false` into the enrollment list once, and enrolling a
    /// device writes `true` back so an older build (which still reads this
    /// key) doesn't silently cut relay access for enrolled phones.
    static let enabledDefaultsKey = "unpeel.native.remoteAccessEnabled"
    /// One-shot marker for `migrateLegacyRelayPreference`.
    static let enrollmentMigratedDefaultsKey = "unpeel.native.linkEnrollmentMigrated"
    /// Hidden override for dev (`ws://127.0.0.1:8787` against `wrangler dev`).
    static let urlOverrideDefaultsKey = "unpeel.native.relayURL"
    private static let productionURL = URL(string: "wss://relay.unpeel.com")!

    static var relayURL: URL {
        if let raw = AppDefaults.shared.string(forKey: urlOverrideDefaultsKey),
           let url = URL(string: raw.trimmingCharacters(in: .whitespacesAndNewlines)),
           url.scheme == "ws" || url.scheme == "wss" {
            return url
        }
        return productionURL
    }
}

/// Durable Link authority shared by the native Host and the headless Rust
/// Host. A cached entitlement is a 30-day bearer, so deleting the cache alone
/// is not a sufficient revocation primitive: an unlink can fail, or another
/// frontend can have already read it. The marker lives outside `mobile/`, is
/// written before cache removal, and is serialized with Rust through the same
/// `link-license.lock` flock.
enum LinkSuppressionReason: String, Codable {
    case userDisabled = "user_disabled"
    case authorizationRejected = "authorization_rejected"
    /// `/api/activate` and the local key commit succeeded, but a fresh relay
    /// entitlement has not committed yet. Cached authority stays blocked;
    /// automatic refresh is safe across process restart.
    case activationPending = "activation_pending"
}

struct LinkSuppressionRecord: Codable, Equatable {
    let version: Int
    let generation: String
    let reason: LinkSuppressionReason
    let disabledAt: Int64

    enum CodingKeys: String, CodingKey {
        case version, generation, reason
        case disabledAt = "disabled_at"
    }
}

struct LinkCachedEntitlement: Codable, Equatable {
    let entitlement: String
    let expiresAt: Int64
    let macID: String
}

enum LinkAuthorityStore {
    struct LocalState: Equatable {
        let suppression: LinkSuppressionRecord?
        let cached: LinkCachedEntitlement?
    }

    struct SuppressionOutcome: Equatable {
        let record: LinkSuppressionRecord
        /// A committed marker already fails closed. Keep an unlink diagnostic
        /// for logs/tests without turning a safe deactivation into failure.
        let cacheRemovalError: String?
    }

    private static func suppressionURL(home: URL) -> URL {
        home.appendingPathComponent("link-disabled.json")
    }

    private static func lockURL(home: URL) -> URL {
        home.appendingPathComponent("link-license.lock")
    }

    private static func cacheURL(home: URL) -> URL {
        home.appendingPathComponent("mobile")
            .appendingPathComponent("relay-entitlement.json")
    }

    private static func posixError(_ operation: String) -> NSError {
        let code = errno
        return NSError(
            domain: NSPOSIXErrorDomain,
            code: Int(code),
            userInfo: [NSLocalizedDescriptionKey: "\(operation): \(String(cString: strerror(code)))"]
        )
    }

    private static func withLock<T>(home: URL, _ body: () throws -> T) throws -> T {
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        let descriptor = open(lockURL(home: home).path, O_CREAT | O_RDWR | O_CLOEXEC, 0o600)
        guard descriptor >= 0 else { throw posixError("could not open Link authority lock") }
        defer { _ = close(descriptor) }
        guard fchmod(descriptor, 0o600) == 0 else {
            throw posixError("could not secure Link authority lock")
        }
        guard flock(descriptor, LOCK_EX) == 0 else {
            throw posixError("could not acquire Link authority lock")
        }
        defer { _ = flock(descriptor, LOCK_UN) }
        return try body()
    }

    private static func fileKind(at url: URL) throws -> mode_t? {
        var value = stat()
        guard lstat(url.path, &value) == 0 else {
            if errno == ENOENT { return nil }
            throw posixError("could not inspect \(url.lastPathComponent)")
        }
        return value.st_mode & mode_t(S_IFMT)
    }

    private static func readSuppressionUnlocked(home: URL) throws -> LinkSuppressionRecord? {
        let url = suppressionURL(home: home)
        guard let kind = try fileKind(at: url) else { return nil }
        guard kind == mode_t(S_IFREG) else {
            throw NSError(
                domain: "UnpeelLinkAuthority",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Link disable marker is not a regular file"]
            )
        }
        let record = try JSONDecoder().decode(LinkSuppressionRecord.self, from: Data(contentsOf: url))
        guard record.version == 1,
              !record.generation.isEmpty
        else {
            throw NSError(
                domain: "UnpeelLinkAuthority",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "Link disable marker is invalid"]
            )
        }
        return record
    }

    private static func writePrivateAtomically(_ data: Data, to url: URL) throws {
        let directory = url.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let temporary = directory.appendingPathComponent(".\(url.lastPathComponent).\(UUID().uuidString).tmp")
        defer { try? FileManager.default.removeItem(at: temporary) }
        guard FileManager.default.createFile(
            atPath: temporary.path,
            contents: data,
            attributes: [.posixPermissions: 0o600]
        ) else {
            throw NSError(
                domain: "UnpeelLinkAuthority",
                code: 3,
                userInfo: [NSLocalizedDescriptionKey: "could not create \(url.lastPathComponent)"]
            )
        }
        let handle = try FileHandle(forWritingTo: temporary)
        try handle.synchronize()
        try handle.close()
        guard rename(temporary.path, url.path) == 0 else {
            throw posixError("could not publish \(url.lastPathComponent)")
        }
    }

    private static func removeCacheUnlocked(home: URL) -> String? {
        let url = cacheURL(home: home)
        guard unlink(url.path) != 0 else { return nil }
        if errno == ENOENT { return nil }
        return posixError("could not remove cached Link entitlement").localizedDescription
    }

    static func localState(home: URL, macID: String) throws -> LocalState {
        try withLock(home: home) {
            let suppression = try readSuppressionUnlocked(home: home)
            var cached: LinkCachedEntitlement?
            if suppression == nil,
               try fileKind(at: cacheURL(home: home)) == mode_t(S_IFREG),
               let data = try? Data(contentsOf: cacheURL(home: home)) {
                cached = try? JSONDecoder().decode(LinkCachedEntitlement.self, from: data)
                if cached?.macID != macID { cached = nil }
            }
            return LocalState(suppression: suppression, cached: cached)
        }
    }

    static func suppression(home: URL) throws -> LinkSuppressionRecord? {
        try withLock(home: home) { try readSuppressionUnlocked(home: home) }
    }

    static func suppress(home: URL, reason: LinkSuppressionReason) throws -> SuppressionOutcome {
        try withLock(home: home) {
            // A late transport/service rejection may strengthen an active or
            // pending state, but it must never weaken an explicit user off.
            if reason == .authorizationRejected,
               let current = try readSuppressionUnlocked(home: home),
               current.reason == .userDisabled {
                return SuppressionOutcome(
                    record: current,
                    cacheRemovalError: removeCacheUnlocked(home: home)
                )
            }
            let record = LinkSuppressionRecord(
                version: 1,
                generation: UUID().uuidString.lowercased(),
                reason: reason,
                disabledAt: Int64(Date().timeIntervalSince1970)
            )
            try writePrivateAtomically(try JSONEncoder().encode(record), to: suppressionURL(home: home))
            return SuppressionOutcome(
                record: record,
                cacheRemovalError: removeCacheUnlocked(home: home)
            )
        }
    }

    /// Convert only the authority generation observed before `/api/activate`
    /// began. A deactivation that happened while the request was in flight
    /// writes a different generation and therefore wins. Even a fresh
    /// activation gets a pending marker: a legacy/pre-marker cache must never
    /// authorize a newly activated (possibly different) key.
    static func markActivationPending(
        home: URL,
        expectedSuppressionGeneration: String?
    ) throws -> String? {
        try withLock(home: home) {
            let current = try readSuppressionUnlocked(home: home)
            guard current?.generation == expectedSuppressionGeneration else {
                throw NSError(
                    domain: "UnpeelLinkAuthority",
                    code: 5,
                    userInfo: [NSLocalizedDescriptionKey: "Link was disabled while activating"]
                )
            }
            let pending = LinkSuppressionRecord(
                version: current?.version ?? 1,
                generation: current?.generation ?? UUID().uuidString.lowercased(),
                reason: .activationPending,
                disabledAt: current?.disabledAt ?? Int64(Date().timeIntervalSince1970)
            )
            try writePrivateAtomically(
                try JSONEncoder().encode(pending),
                to: suppressionURL(home: home)
            )
            if let warning = removeCacheUnlocked(home: home) {
                // The marker already makes the retained bearer unusable. A
                // later fresh commit will replace it; keep the diagnostic for
                // operators without weakening the durable deny.
                NSLog("[relay-uplink] activation cache invalidation failed: \(warning)")
            }
            return pending.generation
        }
    }

    /// Commit a fresh entitlement only if no newer deactivation/rejection won
    /// while the network request was in flight. Publishing the cache before
    /// clearing the exact marker keeps every crash point fail-closed.
    static func commit(
        _ entitlement: LinkCachedEntitlement,
        expectedSuppressionGeneration: String?,
        home: URL
    ) throws {
        try withLock(home: home) {
            let current = try readSuppressionUnlocked(home: home)
            guard current?.generation == expectedSuppressionGeneration else {
                throw NSError(
                    domain: "UnpeelLinkAuthority",
                    code: 4,
                    userInfo: [NSLocalizedDescriptionKey: "Link authority changed while authorizing"]
                )
            }
            try writePrivateAtomically(
                try JSONEncoder().encode(entitlement),
                to: cacheURL(home: home)
            )
            if current != nil {
                guard unlink(suppressionURL(home: home).path) == 0 else {
                    throw posixError("could not clear Link disable marker")
                }
            }
        }
    }
}

private struct ActiveRelayCryptoSession {
    let deviceID: String
    /// Exact relay credential revision accepted when this E2E session was
    /// established. A stable device id is intentionally insufficient:
    /// re-pairing rotates the relay token while retaining that id.
    let relayTokenHash: String
    var crypto: RelayCryptoSession
}

@MainActor
final class RelayUplinkManager: NSObject, ObservableObject {
    static let shared = RelayUplinkManager()

    /// Maximum client-data envelope the host may receive from the relay.
    ///
    /// The canonical envelope is `[type: 1][connID: 4][idLength: 1]`
    /// followed by a device id of at most 128 bytes and an opaque payload of
    /// at most `RelayProtocol.maxFrameBytes`: 512 KiB + 134 bytes. Older
    /// Worker revisions admitted five extra payload bytes (the host-data
    /// envelope allowance) before wrapping them, so retain that small margin
    /// for rolling-deploy compatibility.
    static let maximumInboundMessageBytes = RelayProtocol.maxFrameBytes + 6 + 128 + 5

    enum Status: Equatable {
        case off
        case needsLicense
        case connecting
        case connected
        case error(String)

        var label: String {
            switch self {
            case .off: return "Off"
            case .needsLicense: return "Requires an active license"
            case .connecting: return "Connecting…"
            case .connected: return "Connected to the relay"
            case .error(let message): return message
            }
        }
    }

    @Published private(set) var status: Status = .off

    enum PushDiagnostic: Equatable {
        case neverAttempted
        case delivered
        case failed(String)

        var label: String {
            switch self {
            case .neverAttempted: return "No push attempted yet"
            case .delivered: return "Delivered to APNs"
            case .failed(let message): return message
            }
        }
    }

    @Published private(set) var lastPushDiagnostic: PushDiagnostic = .neverAttempted
    @Published private(set) var lastPushAttemptAt: Date?

    private weak var server: MobileRemoteServer?
    private var socket: URLSessionWebSocketTask?
    private lazy var session = URLSession(
        configuration: .ephemeral,
        delegate: RelaySocketDelegate(manager: self),
        delegateQueue: nil
    )
    /// Invalidates stale receive loops / reconnect timers after a restart.
    private var generation = 0
    private var reconnectDelay: TimeInterval = 2
    private var cryptoSessions: [UInt32: ActiveRelayCryptoSession] = [:]
    private var announcedRegistrations: [RelayDeviceTokenRegistration] = []
    /// Tunneled requests block (output long-polls sleep); they run here,
    /// never on the receive path.
    private let handlerQueue = DispatchQueue(
        label: "unpeel.relay-uplink.handlers",
        qos: .userInitiated,
        attributes: .concurrent
    )
    private var devicesObserver: NSObjectProtocol?
    /// Covers the current process even when persisting the durable marker
    /// itself failed. A failed local deactivation is reported to the user,
    /// but it must still close every live socket immediately.
    private var authoritySuppressedInMemory = false

    // MARK: - Lifecycle

    /// One-shot migration from the retired global "Access away from home"
    /// toggle to the enrollment-list model. A user who had explicitly turned
    /// the relay OFF gets every existing paired device narrowed to
    /// Direct-only once, so their prior intent survives as an empty
    /// enrollment list; a stored `true` (or no stored choice) leaves every
    /// device flag untouched, so no shipped phone loses relay access.
    static func migrateLegacyRelayPreference(
        store: MobilePairingStore,
        defaults: UserDefaults = AppDefaults.shared
    ) {
        guard !defaults.bool(forKey: RelayConfig.enrollmentMigratedDefaultsKey) else { return }
        if defaults.object(forKey: RelayConfig.enabledDefaultsKey) as? Bool == false {
            for device in store.devices {
                store.setRelayAllowed(deviceID: device.id, allowed: false)
            }
        }
        defaults.set(true, forKey: RelayConfig.enrollmentMigratedDefaultsKey)
    }

    func attach(server: MobileRemoteServer) {
        Self.migrateLegacyRelayPreference(store: server.pairingStore)
        self.server = server
        if devicesObserver == nil {
            devicesObserver = NotificationCenter.default.addObserver(
                forName: .unpeelMobileDevicesChanged,
                object: nil,
                queue: .main
            ) { _ in
                Task { @MainActor in
                    // Pair/unpair/credential rotation changes both the relay
                    // token set and E2E keys. Replace the uplink so the DO
                    // drops every in-flight client and no old crypto session
                    // survives revocation.
                    RelayUplinkManager.shared.disconnect()
                    RelayUplinkManager.shared.refresh()
                }
            }
        }
        refresh()
    }

    func detach() {
        server = nil
        refresh()
    }

    func refresh() {
        let flag = UnpeelFeatureFlags.mobileRemoteControlEnabled
        let haveServer = server != nil
        // Enrollment IS the switch: only relay-allowed devices count, and
        // with every device scoped Direct-only there is nothing to register,
        // so don't hold a connected-looking uplink open for nobody.
        let haveEnrolled = !(server?.pairingStore.relayTokenRegistrations().isEmpty ?? true)
        let shouldRun = flag && haveServer && haveEnrolled && !authoritySuppressedInMemory
        NSLog("[relay-uplink] refresh shouldRun=\(shouldRun) flag=\(flag) server=\(haveServer) enrolled=\(haveEnrolled) socket=\(socket != nil)")
        guard shouldRun else {
            disconnect()
            status = .off
            return
        }
        guard socket == nil else { return }
        connect()
    }

    /// Immediate, restart-safe local shutdown. The network seat release runs
    /// later; it is never allowed to keep an established Relay socket alive.
    func deactivateLocalAuthority() -> String? {
        disconnect()
        authoritySuppressedInMemory = true
        do {
            let outcome = try LinkAuthorityStore.suppress(
                home: LaunchConfig.unpeelDir,
                reason: .userDisabled
            )
            if let warning = outcome.cacheRemovalError {
                NSLog("[relay-uplink] cache removal after durable deactivation failed: \(warning)")
            }
            status = .needsLicense
            return nil
        } catch {
            status = .error("Could not persist Link deactivation")
            return error.localizedDescription
        }
    }

    /// A server revocation is also durable authority. Unlike a user disable,
    /// it may recover automatically after the stored key becomes valid again,
    /// but never by reusing the rejected cached bearer.
    func rejectLocalAuthority() -> String? {
        persistAuthorizationRejection(message: "Unpeel Link authorization rejected")
    }

    /// Every authoritative service rejection takes the same local-first path:
    /// close the live socket, publish the shared deny marker, then invalidate
    /// the cached bearer. A late rejection must not weaken a user-disabled
    /// marker written while its request was in flight.
    private func persistAuthorizationRejection(message: String) -> String? {
        disconnect()
        do {
            let outcome = try LinkAuthorityStore.suppress(
                home: LaunchConfig.unpeelDir,
                reason: .authorizationRejected
            )
            if let warning = outcome.cacheRemovalError {
                NSLog("[relay-uplink] cache removal after authorization rejection failed: \(warning)")
            }
            authoritySuppressedInMemory = outcome.record.reason == .userDisabled
            status = authoritySuppressedInMemory ? .needsLicense : .error(message)
            return nil
        } catch {
            authoritySuppressedInMemory = true
            status = .error("Could not persist Link authorization rejection")
            return error.localizedDescription
        }
    }

    /// Snapshot the durable generation before `/api/activate` starts. The
    /// finishing commit must still see this exact generation so a concurrent
    /// deactivation (native or TUI) always wins.
    func activationSuppressionGeneration() throws -> String? {
        try LinkAuthorityStore.suppression(home: LaunchConfig.unpeelDir)?.generation
    }

    /// Called only after `/api/activate` accepted a key and the Keychain
    /// commit succeeded. Make that recovery permission durable before any
    /// entitlement request; the marker itself still blocks cached access.
    func resumeAfterExplicitActivation(
        expectedSuppressionGeneration: String?
    ) -> String? {
        disconnect()
        do {
            _ = try LinkAuthorityStore.markActivationPending(
                home: LaunchConfig.unpeelDir,
                expectedSuppressionGeneration: expectedSuppressionGeneration
            )
            authoritySuppressedInMemory = false
            return nil
        } catch {
            authoritySuppressedInMemory = true
            status = .error("Link changed while activation was finishing")
            return error.localizedDescription
        }
    }

    private func disconnect() {
        generation += 1
        socket?.cancel(with: .goingAway, reason: nil)
        socket = nil
        announcedRegistrations = []
        cryptoSessions.removeAll()
        for connID in outputStreams.keys { cancelOutputStream(connID: connID) }
    }

    // MARK: - Connection

    private func connect() {
        guard let macID = server?.pairingStore.macID else { return }
        generation += 1
        let gen = generation
        status = .connecting
        Task { [weak self] in
            guard let self, self.generation == gen else { return }
            guard let entitlement = await self.currentEntitlement(macID: macID) else {
                NSLog("[relay-uplink] connect aborted: no entitlement (status=\(self.status.label))")
                // currentEntitlement set the status (needsLicense / error);
                // retry on the normal backoff — licenses appear, servers heal.
                if !self.authoritySuppressedInMemory {
                    self.scheduleReconnect(generation: gen)
                }
                return
            }
            guard self.generation == gen else { return }
            let base = RelayConfig.relayURL.absoluteString
                .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            guard let url = URL(string: "\(base)/v1/host/\(macID)") else {
                NSLog("[relay-uplink] connect aborted: bad URL from base=\(base)")
                return
            }
            NSLog("[relay-uplink] opening host socket to \(url.absoluteString) (entitlement len=\(entitlement.count))")
            var request = URLRequest(url: url)
            request.timeoutInterval = 15
            request.setValue("Bearer \(entitlement)", forHTTPHeaderField: "Authorization")
            let socket = self.session.webSocketTask(with: request)
            socket.maximumMessageSize = Self.maximumInboundMessageBytes
            self.socket = socket
            socket.resume()
            self.receiveLoop(socket: socket, generation: gen)
            self.pingLoop(socket: socket, generation: gen)
        }
    }

    /// Called by the socket delegate when the WS handshake completes.
    func socketDidOpen(_ task: URLSessionWebSocketTask) {
        NSLog("[relay-uplink] socketDidOpen (current=\(task === socket))")
        guard task === socket else { return }
        // The authority may have changed in another frontend while DNS/TLS/
        // WebSocket setup was in flight. Recheck before registering devices.
        guard !stopForSharedSuppressionIfNeeded() else { return }
        reconnectDelay = 2
        status = .connected
        sendHello()
        monitorDeviceAuthorizations(socket: task, generation: generation)
    }

    /// Called by the socket delegate on close/error.
    func socketDidClose(_ task: URLSessionWebSocketTask) {
        let code = (task.response as? HTTPURLResponse)?.statusCode
        NSLog("[relay-uplink] socketDidClose current=\(task === socket) httpStatus=\(code.map(String.init) ?? "nil") error=\(task.error?.localizedDescription ?? "nil")")
        guard task === socket else { return }
        if Self.isAuthorizationRejection(code) {
            guard persistAuthorizationRejection(
                message: "Remote access entitlement rejected"
            ) == nil else { return }
        }
        scheduleAuthorizationRecoveryIfAllowed()
    }

    nonisolated static func isAuthorizationRejection(_ statusCode: Int?) -> Bool {
        statusCode == 401 || statusCode == 403
    }

    private func scheduleReconnect(generation gen: Int) {
        guard generation == gen else { return }
        socket?.cancel(with: .goingAway, reason: nil)
        socket = nil
        cryptoSessions.removeAll()
        if status == .connected { status = .connecting }
        let delay = reconnectDelay
        reconnectDelay = min(reconnectDelay * 2, 60)
        Task { [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            guard let self, self.generation == gen else { return }
            self.generation += 1
            self.refresh()
        }
    }

    private func scheduleAuthorizationRecoveryIfAllowed() {
        guard !authoritySuppressedInMemory else { return }
        scheduleReconnect(generation: generation)
    }

    private func receiveLoop(socket: URLSessionWebSocketTask, generation gen: Int) {
        socket.receive { [weak self] result in
            Task { @MainActor in
                guard let self, self.generation == gen, socket === self.socket else { return }
                switch result {
                case .success(let message):
                    if case .data(let data) = message {
                        self.handleFrame(data)
                    }
                    self.receiveLoop(socket: socket, generation: gen)
                case .failure:
                    self.scheduleReconnect(generation: gen)
                }
            }
        }
    }

    private func pingLoop(socket: URLSessionWebSocketTask, generation gen: Int) {
        Task { [weak self] in
            while let self, self.isCurrent(socket: socket, generation: gen) {
                try? await Task.sleep(nanoseconds: 25_000_000_000)
                socket.sendPing { _ in }
            }
        }
    }

    private func isCurrent(socket: URLSessionWebSocketTask, generation gen: Int) -> Bool {
        generation == gen && socket === self.socket
    }

    /// Return true after stopping for a durable suppression or an unreadable
    /// authority file. Rejection/activation-pending may immediately seek a
    /// fresh entitlement; an explicit user disable remains off.
    private func stopForSharedSuppressionIfNeeded() -> Bool {
        do {
            guard let suppression = try LinkAuthorityStore.suppression(
                home: LaunchConfig.unpeelDir
            ) else { return false }
            disconnect()
            authoritySuppressedInMemory = suppression.reason == .userDisabled
            if authoritySuppressedInMemory {
                status = .needsLicense
            } else {
                refresh()
            }
            return true
        } catch {
            disconnect()
            authoritySuppressedInMemory = true
            status = .error("Link authority state is unreadable")
            return true
        }
    }

    /// Filesystem state is the authority and may be changed by the TUI, so an
    /// in-process NotificationCenter observer is only the fast path. Poll the
    /// small registration set while connected; any membership/scope/token
    /// change tears down the uplink generation, which makes the Relay close
    /// every old client and prevents quiet output subscriptions surviving a
    /// cross-process revocation.
    private func monitorDeviceAuthorizations(
        socket: URLSessionWebSocketTask,
        generation gen: Int
    ) {
        Task { [weak self] in
            while let self, self.isCurrent(socket: socket, generation: gen) {
                try? await Task.sleep(nanoseconds: 500_000_000)
                guard self.isCurrent(socket: socket, generation: gen) else { return }
                if self.stopForSharedSuppressionIfNeeded() { return }
                let current = self.currentRelayRegistrations()
                guard current == self.announcedRegistrations else {
                    self.disconnect()
                    self.refresh()
                    return
                }
            }
        }
    }

    // MARK: - Frames

    func sendHello() {
        guard let socket else { return }
        let registrations = currentRelayRegistrations()
        guard let hello = RelayHostFrame.encodeHello(devices: registrations)
        else { return }
        announcedRegistrations = registrations
        socket.send(.data(hello)) { _ in }
    }

    private func currentRelayRegistrations() -> [RelayDeviceTokenRegistration] {
        (server?.pairingStore.relayTokenRegistrations() ?? []).sorted {
            if $0.deviceID != $1.deviceID { return $0.deviceID < $1.deviceID }
            return $0.tokenHash < $1.tokenHash
        }
    }

    private func handleFrame(_ data: Data) {
        guard let frame = RelayHostFrame.decode(data) else { return }
        switch frame {
        case .clientClosed(let connID):
            cryptoSessions[connID] = nil
            cancelOutputStream(connID: connID)
        case .clientData(let connID, let deviceID, let payload):
            handleClientPayload(
                connID: connID,
                authenticatedDeviceID: deviceID,
                payload: payload
            )
        }
    }

    private func handleClientPayload(
        connID: UInt32,
        authenticatedDeviceID: String,
        payload: Data
    ) {
        guard cryptoSessions[connID] != nil else {
            handleClientHello(
                connID: connID,
                authenticatedDeviceID: authenticatedDeviceID,
                payload: payload
            )
            return
        }
        guard var active = cryptoSessions[connID],
              active.deviceID == authenticatedDeviceID,
              server?.pairingStore.relayTokenHash(forDeviceID: active.deviceID)
                == active.relayTokenHash,
              let plaintext = try? active.crypto.open(payload) else {
            cryptoSessions[connID] = nil
            cancelOutputStream(connID: connID)
            return
        }
        cryptoSessions[connID] = active
        dispatchTunneledRequest(plaintext, connID: connID)
    }

    private func handleClientHello(connID: UInt32, authenticatedDeviceID: String, payload: Data) {
        guard let hello = try? JSONDecoder().decode(RelayClientHello.self, from: payload),
              hello.v == RelayProtocol.version,
              authenticatedDeviceID == hello.deviceID,
              // Direct-only devices are never registered in the hello, so the
              // relay shouldn't route them here at all — this closes the
              // window between a scope change and the uplink's reconnect.
              server?.pairingStore.relayAllowed(forDeviceID: hello.deviceID) == true,
              let relayTokenHash = server?.pairingStore.relayTokenHash(
                forDeviceID: hello.deviceID
              ),
              let clientSalt = hello.salt, clientSalt.count == 16,
              let clientEphemeral = hello.ephemeralPublicKey,
              let e2eKey = server?.pairingStore.e2eKey(forDeviceID: hello.deviceID)
        else {
            // Unknown device / malformed hello: say nothing (the phone
            // times out). The relayToken gate should have stopped this
            // earlier; don't leak which device ids exist.
            return
        }
        // Forward-secret handshake: fresh ephemeral X25519 keypair per
        // connection. The derived session keys depend on this ephemeral
        // secret, so a later theft of the static e2eKey cannot decrypt this
        // recorded session.
        let hostEphemeral = RelayHandshake.EphemeralKeyPair()
        let hostSalt = MobilePairingStore.randomBytes(16)
        guard let sharedSecret = try? RelayHandshake.sharedSecret(
            privateKey: hostEphemeral.privateKey,
            peerPublicKey: clientEphemeral
        ), let session = try? RelayCryptoSession(
            e2eKey: e2eKey,
            sharedSecret: sharedSecret,
            clientSalt: clientSalt,
            hostSalt: hostSalt,
            isHost: true
        ) else { return }
        // MAC keyed by the device key proves to the phone that this host
        // holds it (and pins both ephemeral keys against a relay swap).
        let mac = RelayHandshake.transcriptMAC(
            e2eKey: e2eKey,
            deviceID: hello.deviceID,
            clientSalt: clientSalt,
            hostSalt: hostSalt,
            clientEphemeralPublicKey: clientEphemeral,
            hostEphemeralPublicKey: hostEphemeral.publicKey
        )
        guard let reply = try? JSONEncoder().encode(RelayHostHello(
            salt: hostSalt,
            ephemeralPublicKey: hostEphemeral.publicKey,
            mac: mac
        )) else { return }
        cryptoSessions[connID] = ActiveRelayCryptoSession(
            deviceID: hello.deviceID,
            relayTokenHash: relayTokenHash,
            crypto: session
        )
        socket?.send(.data(RelayHostFrame.encodeData(connID: connID, payload: reply))) { _ in }
    }

    private func dispatchTunneledRequest(_ plaintext: Data, connID: UInt32) {
        guard let request = try? JSONDecoder().decode(RelayTunnelRequest.self, from: plaintext),
              let server
        else { return }
        // The relay-only output stream (subscribe/credit/stop) is handled
        // here, not in the `/mobile/*` pipeline: it needs this connection's
        // identity (connID) to know where to push frames. Auth is the same
        // per-device token check the tunnel pipeline uses.
        if request.path.hasPrefix("/relay/") {
            let response = handleStreamControl(request, connID: connID)
            sendSealedResponse(response, connID: connID)
            return
        }
        let replayConnectionID = "\(generation):\(connID)"
        handlerQueue.async {
            let response = server.handleTunneled(
                request,
                connectionID: replayConnectionID
            )
            Task { @MainActor [weak self] in
                self?.sendSealedResponse(response, connID: connID)
            }
        }
    }

    private func sendSealedResponse(_ response: RelayTunnelResponse, connID: UInt32) {
        guard var active = cryptoSessions[connID],
              server?.pairingStore.relayTokenHash(forDeviceID: active.deviceID)
                == active.relayTokenHash,
              let json = Self.boundedResponsePlaintext(response),
              let sealed = try? active.crypto.seal(json)
        else {
            cryptoSessions[connID] = nil
            cancelOutputStream(connID: connID)
            return
        }
        cryptoSessions[connID] = active
        socket?.send(.data(RelayHostFrame.encodeData(connID: connID, payload: sealed))) { _ in }
    }

    /// Keep an oversized route response from reaching the Worker, which would
    /// close the shared Host uplink. The small replacement remains correlated
    /// to the original request and is measured before `seal` advances a nonce.
    static func boundedResponsePlaintext(_ response: RelayTunnelResponse) -> Data? {
        guard let json = try? JSONEncoder().encode(response) else { return nil }
        if json.count <= RelayProtocol.maxPlaintextBytes { return json }

        let replacement = RelayTunnelResponse(
            id: response.id,
            status: 413,
            body: Data(#"{"error":"response too large"}"#.utf8)
        )
        guard let replacementJSON = try? JSONEncoder().encode(replacement),
              replacementJSON.count <= RelayProtocol.maxPlaintextBytes
        else { return nil }
        return replacementJSON
    }

    // MARK: - Output push stream (relay-only)

    /// One live push stream per relay connection (the phone views one
    /// session at a time; a new subscribe replaces the old stream).
    private final class OutputStreamState {
        let sessionID: String
        /// Unconsumed send budget in payload bytes; the phone tops it up as
        /// it feeds. Keeps a fast Mac from flooding the relay socket and a
        /// slow phone (the long-poll's implicit backpressure, made explicit).
        var credit: Int
        var creditWaiter: CheckedContinuation<Void, Never>?
        var task: Task<Void, Never>?
        var lastCols: Int?
        var lastRows: Int?

        init(sessionID: String, credit: Int) {
            self.sessionID = sessionID
            self.credit = credit
        }
    }

    private var outputStreams: [UInt32: OutputStreamState] = [:]
    /// Payload bytes per pushed frame. Base64 (~1.33x) + JSON + AEAD must
    /// stay under the relay's 512KB frame cap with lots of margin; smaller
    /// frames also pace the stream more smoothly on cellular.
    private static let pushChunkBytes = 96 * 1024
    /// Match the normal iOS initial replay. The complete window is compressed
    /// once and normally crosses Cloudflare in a single message.
    private static let bootstrapReplayBytes = 768 * 1024
    /// Raw compressed bytes per encrypted relay message. Base64 JSON expansion
    /// keeps this comfortably below the Worker's 512 KB frame ceiling.
    private static let bootstrapWireChunkBytes = 320 * 1024
    private static let defaultInitialCredit = 384 * 1024
    private static let maxStreamCredit = 4 * 1024 * 1024

    static func relayFrameIsRebased(
        chunkOffset: UInt64,
        cursor: UInt64?,
        truncated: Bool
    ) -> Bool {
        truncated || (cursor != nil && chunkOffset != cursor)
    }

    static func relayFrameShouldPush(
        payloadIsEmpty: Bool,
        first: Bool,
        rebased: Bool
    ) -> Bool {
        first || !payloadIsEmpty || rebased
    }

    private func handleStreamControl(
        _ request: RelayTunnelRequest,
        connID: UInt32
    ) -> RelayTunnelResponse {
        func reply(_ status: Int, _ message: String = "") -> RelayTunnelResponse {
            RelayTunnelResponse(
                id: request.id,
                status: status,
                body: Data("{\"ok\":\(status == 200)}\(message)".utf8)
            )
        }
        // Same trust boundary as the tunneled /mobile/* pipeline.
        guard let server,
              server.pairingStore.verifyAuthorizationHeader(request.auth) != nil else {
            return reply(401)
        }
        switch request.path {
        case RelayStreamPaths.subscribe:
            guard let sessionID = request.query["session_id"],
                  !sessionID.isEmpty, sessionID.utf8.count <= 128 else {
                return reply(400)
            }
            cancelOutputStream(connID: connID)
            let credit = Int(request.query["credit"] ?? "") ?? Self.defaultInitialCredit
            let state = OutputStreamState(
                sessionID: sessionID,
                credit: min(Self.maxStreamCredit, max(Self.pushChunkBytes, credit))
            )
            let offset = request.query["offset"].flatMap(UInt64.init)
            outputStreams[connID] = state
            state.task = Task { @MainActor [weak self] in
                await self?.runOutputStream(connID: connID, state: state, from: offset)
            }
            return reply(200)
        case RelayStreamPaths.credit:
            guard let state = outputStreams[connID],
                  state.sessionID == request.query["session_id"],
                  let bytes = Int(request.query["bytes"] ?? ""),
                  bytes > 0, bytes <= Self.maxStreamCredit else {
                return reply(404)
            }
            state.credit = min(state.credit, Self.maxStreamCredit)
            state.credit += min(bytes, Self.maxStreamCredit - state.credit)
            state.creditWaiter?.resume()
            state.creditWaiter = nil
            return reply(200)
        case RelayStreamPaths.unsubscribe:
            cancelOutputStream(connID: connID)
            return reply(200)
        default:
            return reply(404)
        }
    }

    private func cancelOutputStream(connID: UInt32) {
        guard let state = outputStreams.removeValue(forKey: connID) else { return }
        state.task?.cancel()
        state.creditWaiter?.resume()
        state.creditWaiter = nil
    }

    /// The long-poll loop, moved to the Mac's side of the relay: read from
    /// `output.bin` (blocking waits off the main actor), push each chunk as
    /// a sealed frame the moment it exists. The phone's per-update cost
    /// drops from a full relay round-trip to one-way delivery.
    private func runOutputStream(
        connID: UInt32,
        state: OutputStreamState,
        from initialOffset: UInt64?
    ) async {
        var cursor = initialOffset
        var first = true
        var lastGridCheck = ContinuousClock.now - .seconds(10)

        if initialOffset == nil {
            let bootstrapSessionID = state.sessionID
            let bootstrapLimit = Self.bootstrapReplayBytes
            let bootstrap: (chunk: RemoteTerminalOutputChunk, cols: Int?, rows: Int?)? =
                await withCheckedContinuation { continuation in
                    handlerQueue.async {
                        let query = [
                            "session_id": bootstrapSessionID,
                            "limit": "\(bootstrapLimit)",
                        ]
                        guard let chunk = try? MobileSessionControl.outputChunk(query: query)
                        else { return continuation.resume(returning: nil) }
                        let metrics = try? MobileSessionControl.metrics(
                            query: ["session_id": bootstrapSessionID]
                        )
                        continuation.resume(
                            returning: (chunk, metrics?.columns, metrics?.rows)
                        )
                    }
                }
            guard !Task.isCancelled, outputStreams[connID] === state else { return }
            guard let bootstrap else {
                cancelOutputStream(connID: connID)
                return
            }
            let replay = Data(base64Encoded: bootstrap.chunk.dataBase64) ?? Data()
            let (encoding, wireBytes) = RelayBootstrapCodec.encode(replay)
            let partCount = max(
                1,
                (wireBytes.count + Self.bootstrapWireChunkBytes - 1)
                    / Self.bootstrapWireChunkBytes
            )
            for index in 0 ..< partCount {
                let start = index * Self.bootstrapWireChunkBytes
                let end = min(start + Self.bootstrapWireChunkBytes, wireBytes.count)
                let bytes = start < end ? Data(wireBytes[start ..< end]) : Data()
                sendSealedPush(
                    RelayStreamPush(
                        stream: state.sessionID,
                        offset: bootstrap.chunk.offset,
                        data: bytes,
                        rebased: true,
                        cols: index == 0 ? bootstrap.cols : nil,
                        rows: index == 0 ? bootstrap.rows : nil,
                        bootstrap: RelayStreamBootstrapPart(
                            index: index,
                            final: index == partCount - 1,
                            encoding: encoding,
                            uncompressedBytes: replay.count,
                            endOffset: bootstrap.chunk.nextOffset
                        )
                    ),
                    connID: connID
                )
            }
            if let cols = bootstrap.cols { state.lastCols = cols }
            if let rows = bootstrap.rows { state.lastRows = rows }
            cursor = bootstrap.chunk.nextOffset
            first = false
            lastGridCheck = .now
        }

        while !Task.isCancelled, outputStreams[connID] === state {
            if state.credit <= 0 {
                await withCheckedContinuation { continuation in
                    state.creditWaiter = continuation
                }
                if Task.isCancelled || outputStreams[connID] !== state { break }
                continue
            }
            let want = min(state.credit, Self.pushChunkBytes)
            var query = ["session_id": state.sessionID, "limit": "\(want)"]
            if let cursor {
                query["offset"] = "\(cursor)"
                // Short server-side wait: keeps cancellation responsive and
                // a handler thread is only parked while this stream is live.
                query["wait_ms"] = "3000"
            }
            let wantGrid = first || ContinuousClock.now - lastGridCheck > .seconds(1)
            let outputQuery = query
            let sessionID = state.sessionID
            let result: (chunk: RemoteTerminalOutputChunk, cols: Int?, rows: Int?)? =
                await withCheckedContinuation { continuation in
                    handlerQueue.async {
                        guard let chunk = try? MobileSessionControl.outputChunk(query: outputQuery)
                        else { return continuation.resume(returning: nil) }
                        var cols: Int?
                        var rows: Int?
                        if wantGrid,
                           let metrics = try? MobileSessionControl.metrics(
                               query: ["session_id": sessionID]
                           ) {
                            cols = metrics.columns
                            rows = metrics.rows
                        }
                        continuation.resume(returning: (chunk, cols, rows))
                    }
                }
            guard !Task.isCancelled, outputStreams[connID] === state else { break }
            guard let result else {
                // Session gone/exited: end the stream; the phone's receive
                // loop notices the silence on its next poll fallback.
                cancelOutputStream(connID: connID)
                break
            }
            if wantGrid { lastGridCheck = ContinuousClock.now }
            let chunk = result.chunk
            let payload = Data(base64Encoded: chunk.dataBase64) ?? Data()
            let rebased = Self.relayFrameIsRebased(
                chunkOffset: chunk.offset,
                cursor: cursor,
                truncated: chunk.truncated
            )
            // Suppress only an ordinary idle long-poll expiry. A retained
            // journal can rebase to an empty replacement floor before the new
            // Host writes its first byte; that empty frame must reach the phone
            // so it resets the old terminal before later output is appended.
            if !Self.relayFrameShouldPush(
                payloadIsEmpty: payload.isEmpty,
                first: first,
                rebased: rebased
            ) {
                cursor = chunk.nextOffset
                continue
            }
            let gridChanged = result.cols != state.lastCols || result.rows != state.lastRows
            let push = RelayStreamPush(
                stream: state.sessionID,
                offset: chunk.offset,
                data: payload,
                rebased: rebased,
                cols: (first || gridChanged) ? result.cols : nil,
                rows: (first || gridChanged) ? result.rows : nil
            )
            if let cols = result.cols { state.lastCols = cols }
            if let rows = result.rows { state.lastRows = rows }
            sendSealedPush(push, connID: connID)
            state.credit -= payload.count
            cursor = chunk.nextOffset
            first = false
        }
    }

    private func sendSealedPush(_ push: RelayStreamPush, connID: UInt32) {
        guard var active = cryptoSessions[connID],
              server?.pairingStore.relayTokenHash(forDeviceID: active.deviceID)
                == active.relayTokenHash,
              let json = try? JSONEncoder().encode(push),
              json.count <= RelayProtocol.maxPlaintextBytes,
              let sealed = try? active.crypto.seal(json)
        else {
            cryptoSessions[connID] = nil
            cancelOutputStream(connID: connID)
            return
        }
        cryptoSessions[connID] = active
        socket?.send(.data(RelayHostFrame.encodeData(connID: connID, payload: sealed))) { _ in }
    }

    // MARK: - Entitlement

    private struct EntitlementResponse: Decodable {
        let entitlement: String
        let expiresAt: Int64

        enum CodingKeys: String, CodingKey {
            case entitlement
            case expiresAt = "expires_at"
        }
    }

    /// Cached entitlement while >7 days of validity remain; otherwise a
    /// fresh one from unpeel.com using the stored license key. Nil (with
    /// status set) when there's no license or the server refuses.
    private func currentEntitlement(macID: String) async -> String? {
        guard !authoritySuppressedInMemory else { return nil }
        let now = Int64(Date().timeIntervalSince1970)
        let local: LinkAuthorityStore.LocalState
        do {
            local = try LinkAuthorityStore.localState(
                home: LaunchConfig.unpeelDir,
                macID: macID
            )
        } catch {
            // Unreadable/malformed durable deny is deny, never absence.
            authoritySuppressedInMemory = true
            status = .error("Link authority state is unreadable")
            return nil
        }
        if let suppression = local.suppression,
           suppression.reason == .userDisabled {
            authoritySuppressedInMemory = true
            status = .needsLicense
            return nil
        }

        // LOCAL-DEV ONLY: a dev token (default `unpeel.native.relayDevToken`)
        // is presented verbatim as the entitlement, skipping the unpeel.com
        // fetch — pairs with the relay Worker's DEV_ENTITLEMENT_BYPASS so a
        // dev Mac with a dev-signed license can run the relay locally. Unset
        // in real builds, so production still fetches a signed entitlement.
        if LicenseConfig.developmentBuildLicenseBypassEnabled,
           local.suppression == nil,
           let devToken = AppDefaults.shared.string(forKey: "unpeel.native.relayDevToken"),
           !devToken.trimmingCharacters(in: .whitespaces).isEmpty {
            return devToken
        }
        guard let licenseKey = LicenseManager.shared.currentLicenseKey else {
            status = .needsLicense
            return nil
        }
        // A cached bearer is never authority on its own. Keychain deletion,
        // revocation, or an unlicensed restart must deny it even if durable
        // cache invalidation encountered a filesystem failure.
        if let cached = local.cached,
           cached.expiresAt > now + 7 * 24 * 3600 {
            return cached.entitlement
        }
        var request = URLRequest(
            url: LicenseConfig.apiBaseURL.appendingPathComponent("api/remote/entitlement")
        )
        request.httpMethod = "POST"
        request.timeoutInterval = 15
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONSerialization.data(withJSONObject: [
            "key": licenseKey,
            "mac_id": macID,
            "device_id": LicenseManager.deviceID,
        ])
        do {
            let (data, response) = try await URLSession.shared.data(for: request)
            guard let http = response as? HTTPURLResponse else { throw URLError(.badServerResponse) }
            guard http.statusCode == 200 else {
                if [400, 401, 402, 403, 404, 409, 410, 422].contains(http.statusCode) {
                    _ = persistAuthorizationRejection(
                        message: http.statusCode == 402
                            ? "Remote access requires Unpeel Link"
                            : "Could not authorize remote access (HTTP \(http.statusCode))"
                    )
                    scheduleAuthorizationRecoveryIfAllowed()
                }
                return nil
            }
            let issued = try JSONDecoder().decode(EntitlementResponse.self, from: data)
            guard issued.expiresAt > now else { throw URLError(.badServerResponse) }
            // Deactivation/key replacement may have won while URLSession was
            // suspended. Never let that late response publish authority.
            guard LicenseManager.shared.currentLicenseKey == licenseKey else { return nil }
            let cached = LinkCachedEntitlement(
                entitlement: issued.entitlement,
                expiresAt: issued.expiresAt,
                macID: macID
            )
            try LinkAuthorityStore.commit(
                cached,
                expectedSuppressionGeneration: local.suppression?.generation,
                home: LaunchConfig.unpeelDir
            )
            authoritySuppressedInMemory = false
            return issued.entitlement
        } catch {
            status = .error("Could not reach unpeel.com for remote access")
            return nil
        }
    }

    struct PushResult {
        let ok: Bool
        /// APNs `reason` on failure (e.g. `BadDeviceToken`, `Unregistered`),
        /// so the caller can prune a dead token.
        let reason: String?
    }

    static func pushFailureLabel(reason: String?) -> String {
        switch reason {
        case "remote-disabled": return "Unpeel Link is turned off"
        case "no-mac": return "The Host has no Link identity"
        case "no-entitlement": return "Link entitlement unavailable"
        case "forbidden": return "Link entitlement rejected"
        case "bad-url": return "Invalid Link service URL"
        case "network": return "Could not reach Unpeel Link"
        case "apns-not-configured": return "Link push is not configured"
        case "BadDeviceToken", "Unregistered": return "APNs rejected the device token"
        case "too many pushes": return "Link push rate limit reached"
        case "bad-token", "bad-message", "bad-metadata", "message-too-large":
            return "Push request was rejected"
        case .some(let reason): return "Push failed: \(reason)"
        case .none: return "Link returned an invalid push response"
        }
    }

    private func recordPushResult(_ result: PushResult) -> PushResult {
        lastPushAttemptAt = Date()
        lastPushDiagnostic = result.ok
            ? .delivered
            : .failed(Self.pushFailureLabel(reason: result.reason))
        return result
    }

    /// Forward one alert to APNs through the relay's `/v1/push/<macID>`
    /// (entitlement-gated — the same paid boundary as the streaming uplink).
    /// Independent of the WS socket, so it works even when streaming is idle.
    /// The relay owns the APNs key and signs the provider JWT; the Mac only
    /// supplies the device token + text.
    func sendPush(
        apnsToken: String,
        environment: String,
        title: String,
        body: String,
        sessionID: String,
        kind: String
    ) async -> PushResult {
        // No global relay toggle anymore: enrollment is per-device, enforced
        // where push targets are collected (`MobilePairingStore.pushTargets`
        // skips Direct-only devices — pushes ride the Link service too).
        guard UnpeelFeatureFlags.mobileRemoteControlEnabled else {
            return recordPushResult(PushResult(ok: false, reason: "remote-disabled"))
        }
        guard let macID = server?.pairingStore.macID else {
            return recordPushResult(PushResult(ok: false, reason: "no-mac"))
        }
        guard let entitlement = await currentEntitlement(macID: macID) else {
            return recordPushResult(PushResult(ok: false, reason: "no-entitlement"))
        }
        // A TUI/native deactivation may have landed while entitlement lookup
        // awaited the network. Recheck before presenting the bearer to Push.
        guard !stopForSharedSuppressionIfNeeded() else {
            return recordPushResult(PushResult(ok: false, reason: "no-entitlement"))
        }
        var components = URLComponents(url: RelayConfig.relayURL, resolvingAgainstBaseURL: false)
        components?.scheme = RelayConfig.relayURL.scheme == "wss" ? "https" : "http"
        guard let httpBase = components?.url else {
            return recordPushResult(PushResult(ok: false, reason: "bad-url"))
        }
        let base = httpBase.absoluteString.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        guard let url = URL(string: "\(base)/v1/push/\(macID)") else {
            return recordPushResult(PushResult(ok: false, reason: "bad-url"))
        }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 15
        request.setValue("Bearer \(entitlement)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONSerialization.data(withJSONObject: [
            "apnsToken": apnsToken,
            "environment": environment,
            "title": title,
            "body": body,
            "sessionId": sessionID,
            "kind": kind,
        ])
        do {
            let (data, response) = try await URLSession.shared.data(for: request)
            let http = response as? HTTPURLResponse
            let parsed = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
            let ok = http?.statusCode == 200 && (parsed?["ok"] as? Bool == true)
            if Self.isAuthorizationRejection(http?.statusCode) {
                _ = persistAuthorizationRejection(message: "Link entitlement rejected")
                scheduleAuthorizationRecoveryIfAllowed()
                return recordPushResult(PushResult(ok: false, reason: "forbidden"))
            }
            let reason = parsed?["reason"] as? String
                ?? parsed?["error"] as? String
                ?? http.map { "http-\($0.statusCode)" }
            return recordPushResult(PushResult(ok: ok, reason: reason))
        } catch {
            return recordPushResult(PushResult(ok: false, reason: "network"))
        }
    }
}

/// WS open/close callbacks arrive off-main; hop them onto the manager.
private final class RelaySocketDelegate: NSObject, URLSessionWebSocketDelegate, @unchecked Sendable {
    private weak var manager: RelayUplinkManager?

    init(manager: RelayUplinkManager) {
        self.manager = manager
    }

    func urlSession(
        _ session: URLSession,
        webSocketTask: URLSessionWebSocketTask,
        didOpenWithProtocol protocol: String?
    ) {
        Task { @MainActor [weak manager] in
            manager?.socketDidOpen(webSocketTask)
        }
    }

    func urlSession(
        _ session: URLSession,
        webSocketTask: URLSessionWebSocketTask,
        didCloseWith closeCode: URLSessionWebSocketTask.CloseCode,
        reason: Data?
    ) {
        Task { @MainActor [weak manager] in
            manager?.socketDidClose(webSocketTask)
        }
    }

    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didCompleteWithError error: Error?
    ) {
        guard let webSocketTask = task as? URLSessionWebSocketTask else { return }
        Task { @MainActor [weak manager] in
            manager?.socketDidClose(webSocketTask)
        }
    }
}
