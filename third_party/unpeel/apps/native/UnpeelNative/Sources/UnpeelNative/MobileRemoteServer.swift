import CryptoKit
import Darwin
import dnssd
import Foundation
import ImageIO
import Security
import UniformTypeIdentifiers
import UnpeelShared

/// The peer closed the connection (or the idle timeout fired) between
/// keep-alive requests — a normal end of connection, not an error.
private struct MobileConnectionClosed: Error {}

struct MobileRemoteError: Error {
    let status: Int
    let message: String

    init(_ status: Int, _ message: String) {
        self.status = status
        self.message = message
    }
}

private struct MobilePairedDeviceFile: Codable {
    var version: Int = 1
    var devices: [MobilePairedDeviceRecord] = []
}

struct MobilePairedDeviceRecord: Codable, Equatable {
    var id: String
    var name: String
    var platform: String
    var appVersion: String?
    var tokenHash: String
    var pairedAtUnixMs: Int64
    var lastSeenAtUnixMs: Int64?
    /// The E2E key lives in the login Keychain and the locked, 0600 shared
    /// Host store used by the standalone TUI. The relay token is stored
    /// hash-only: the raw value lives solely on the phone.
    var relayTokenHash: String
    /// APNs device token (hex) + environment (`sandbox`/`production`) for push
    /// notifications, registered post-pairing via `/mobile/push-token`. Nil
    /// until the phone reports one (permission granted + remote registration).
    var apnsToken: String?
    var apnsEnvironment: String?
    /// Per-device Unpeel Link scope: false keeps this device Direct/LAN-only —
    /// its relay token is never registered with the uplink. Nil means allowed,
    /// so pre-flag records and every fresh pairing keep today's behavior.
    var relayAllowed: Bool?

    var isRelayAllowed: Bool { relayAllowed ?? true }

    var summary: RemotePairedDeviceSummary {
        RemotePairedDeviceSummary(
            id: id,
            name: name,
            platform: platform,
            appVersion: appVersion,
            pairedAtUnixMs: pairedAtUnixMs,
            lastSeenAtUnixMs: lastSeenAtUnixMs,
            relayAllowed: relayAllowed
        )
    }
}

private struct ActiveMobilePairing {
    var token: String
    var endpoint: URL
    var expiresAtUnixMs: Int64
}

protocol MobileE2EKeyStoring: AnyObject {
    func load(deviceID: String) -> Data?
    func save(_ key: Data, deviceID: String) throws
    func delete(deviceID: String)
}

final class MobileE2EKeychainStore: MobileE2EKeyStoring {
    private let service = "com.unpeel.mobile.e2e"
    /// Keychain items are bundle-scoped, not UNPEEL_HOME-scoped, so the
    /// account must carry this instance's macID: the phone reuses ONE
    /// deviceID across every Mac/workspace it pairs with, and a bare-deviceID
    /// account would let a second workspace's pairing overwrite the first
    /// workspace's per-device static relay key.
    private let macID: String

    init(macID: String) {
        self.macID = macID
    }

    func load(deviceID: String) -> Data? {
        if let data = loadData(account: scopedAccount(deviceID: deviceID)) {
            return data
        }
        // Pairings created before workspace support used the bare deviceID:
        // copy forward under the scoped account (keep the legacy item —
        // rollback safety).
        // No cross-workspace bleed: this only runs for devices already in this
        // instance's own devices.json, and new pairings write scoped.
        guard let legacy = loadData(account: deviceID) else { return nil }
        try? save(legacy, deviceID: deviceID)
        return legacy
    }

    func save(_ key: Data, deviceID: String) throws {
        let query = baseQuery(account: scopedAccount(deviceID: deviceID))
        let update = SecItemUpdate(
            query as CFDictionary,
            [kSecValueData as String: key] as CFDictionary
        )
        if update == errSecSuccess { return }
        guard update == errSecItemNotFound else { throw MobileRemoteError(500, "keychain update failed") }
        var add = query
        add[kSecValueData as String] = key
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        guard SecItemAdd(add as CFDictionary, nil) == errSecSuccess else {
            throw MobileRemoteError(500, "keychain save failed")
        }
    }

    func delete(deviceID: String) {
        SecItemDelete(baseQuery(account: scopedAccount(deviceID: deviceID)) as CFDictionary)
        // Also drop the legacy bare-deviceID item; deleting a missing item
        // is a no-op, and revocation should leave no copy behind.
        SecItemDelete(baseQuery(account: deviceID) as CFDictionary)
    }

    private func scopedAccount(deviceID: String) -> String {
        "\(macID).\(deviceID)"
    }

    private func loadData(account: String) -> Data? {
        var query = baseQuery(account: account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess else { return nil }
        return result as? Data
    }

    private func baseQuery(account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }
}

final class MobilePairingStore: @unchecked Sendable {
    let macID: String
    let macName: String

    private let storageURL: URL
    private let e2eKeyStore: MobileE2EKeyStoring
    /// `flock` coordinates native and TUI processes. This process lock also
    /// serializes multiple MobilePairingStore instances because BSD `flock`
    /// semantics alone are not a portable same-process mutex.
    private static let processStorageLock = NSLock()
    private let lock = NSLock()
    private var activePairing: ActiveMobilePairing?
    private var records: [MobilePairedDeviceRecord] = []

    init(
        storageURL: URL = LaunchConfig.unpeelDir
            .appendingPathComponent("mobile")
            .appendingPathComponent("devices.json"),
        macID: String = MobilePairingStore.defaultMacID(),
        macName: String = UnpeelWorkspaceContext.advertisedHostName,
        e2eKeyStore: MobileE2EKeyStoring? = nil
    ) {
        self.storageURL = storageURL
        self.macID = macID
        self.macName = macName
        // Default resolved here, not in the parameter list, because the
        // keychain account is scoped by this store's macID.
        self.e2eKeyStore = e2eKeyStore ?? MobileE2EKeychainStore(macID: macID)
        // Reconcile while native still has Keychain access. This makes an
        // existing app pairing usable by the standalone TUI without asking
        // the phone to pair again. The locked authority file is reloaded
        // before reconciliation; a valid shared revision wins so a key that
        // the TUI rotated is copied back into the scoped Keychain account.
        do {
            try withFreshRecords { directoryDescriptor in
                try reconcileSharedE2EKeysLocked(
                    directoryDescriptor: directoryDescriptor
                )
            }
        } catch {
            // Construction cannot throw. Link fails closed, but devices.json
            // remains the independent authority for Direct/LAN access and an
            // explicit revoke must still be able to remove that authority.
            NSLog("[UnpeelNative] failed to reconcile mobile E2E keys: \(error)")
        }
    }

    var devices: [RemotePairedDeviceSummary] {
        (try? withFreshRecords { _ in records.map(\.summary) }) ?? []
    }

    func beginPairing(
        endpoint: URL,
        now: Date = Date(),
        ttlSeconds: TimeInterval = 5 * 60
    ) -> RemotePairingPayload {
        // Uppercase base32 keeps the compact pairing code (RemotePairingCode)
        // inside the QR alphanumeric charset — a visibly coarser code than
        // base64url forced. 16 one-time bytes with a 5-minute TTL is ample;
        // the durable per-device tokens stay 32-byte base64url.
        let token = Self.randomBase32Token(byteCount: 16)
        let expiresAt = Self.unixMs(now.addingTimeInterval(ttlSeconds))
        lock.withLock {
            activePairing = ActiveMobilePairing(
                token: token,
                endpoint: endpoint,
                expiresAtUnixMs: expiresAt
            )
        }
        return RemotePairingPayload(
            macID: macID,
            macName: macName,
            endpoint: endpoint,
            token: token,
            certificateFingerprint: nil,
            expiresAtUnixMs: expiresAt
        )
    }

    func cancelPairing() {
        lock.withLock {
            activePairing = nil
        }
    }

    func pair(_ request: RemotePairingRequest, now: Date = Date()) throws -> RemotePairingResponse {
        let response = try pairLocked(request, now: now)
        // Drives the remote control server's auto-start policy
        // (RemoteControlManager runs it while paired devices exist).
        NotificationCenter.default.post(name: .unpeelMobileDevicesChanged, object: nil)
        return response
    }

    /// Open the LAN pairing request with the currently displayed QR secret.
    /// Pairing deliberately stays on the tiny HTTP bootstrap server, but no
    /// credential-bearing byte is plaintext or unauthenticated on the LAN.
    func decryptPairingRequest(_ envelope: RemotePairingEnvelope) throws -> RemotePairingRequest {
        let context = try lock.withLock { () throws -> ActiveMobilePairing in
            guard let activePairing else { throw MobileRemoteError(401, "pairing is not active") }
            guard activePairing.expiresAtUnixMs > Self.unixMs(Date()) else {
                self.activePairing = nil
                throw MobileRemoteError(401, "pairing token expired")
            }
            return activePairing
        }
        do {
            let plaintext = try RemotePairingCrypto.open(
                envelope,
                token: context.token,
                macID: macID,
                endpoint: context.endpoint,
                direction: .request
            )
            return try JSONDecoder().decode(RemotePairingRequest.self, from: plaintext)
        } catch let error as MobileRemoteError {
            throw error
        } catch {
            throw MobileRemoteError(401, "invalid encrypted pairing request")
        }
    }

    private func pairLocked(
        _ request: RemotePairingRequest,
        now: Date
    ) throws -> RemotePairingResponse {
        try withFreshRecords { directoryDescriptor in
            try reconcileSharedE2EKeysLocked(
                directoryDescriptor: directoryDescriptor
            )
            guard let activePairing else {
                throw MobileRemoteError(401, "pairing is not active")
            }
            let nowMs = Self.unixMs(now)
            guard activePairing.expiresAtUnixMs > nowMs else {
                self.activePairing = nil
                throw MobileRemoteError(401, "pairing token expired")
            }
            guard request.token == activePairing.token else {
                throw MobileRemoteError(401, "invalid pairing token")
            }

            let authToken = Self.randomToken(byteCount: 32)
            let deviceID = request.device.id
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .isEmpty ? UUID().uuidString.lowercased() : request.device.id
            // Unpeel Remote credentials ride the same pairing exchange: the
            // E2E key and relay token reach the phone over the LAN channel
            // the scanned one-time token just authenticated.
            let previousKey = e2eKeyStore.load(deviceID: deviceID)
            let previousSharedValue = try sharedE2EKeyValue(
                deviceID: deviceID,
                directoryDescriptor: directoryDescriptor
            )
            let e2eKey = Self.randomBytes(32)
            let relayToken = Self.randomToken(byteCount: 32)
            try e2eKeyStore.save(e2eKey, deviceID: deviceID)
            do {
                try saveSharedE2EKey(
                    e2eKey,
                    deviceID: deviceID,
                    directoryDescriptor: directoryDescriptor
                )
            } catch {
                restoreKeychain(previousKey, deviceID: deviceID)
                throw error
            }
            let record = MobilePairedDeviceRecord(
                id: deviceID,
                name: request.device.name,
                platform: request.device.platform,
                appVersion: request.device.appVersion,
                tokenHash: Self.sha256(authToken),
                pairedAtUnixMs: nowMs,
                lastSeenAtUnixMs: nowMs,
                relayTokenHash: Self.sha256(relayToken)
            )

            records.removeAll { $0.id == record.id }
            records.append(record)
            do {
                try persistLocked(directoryDescriptor: directoryDescriptor)
            } catch {
                // Keep the Keychain and devices.json one logical credential
                // revision. In particular, a failed re-pair must not replace
                // the old device's E2E key while its old authority record is
                // still the durable one.
                restoreKeychain(previousKey, deviceID: deviceID)
                restoreSharedE2EKeyValue(
                    previousSharedValue,
                    deviceID: deviceID,
                    directoryDescriptor: directoryDescriptor
                )
                throw error
            }
            self.activePairing = nil

            return RemotePairingResponse(
                macID: macID,
                macName: macName,
                endpoint: activePairing.endpoint,
                deviceID: record.id,
                authToken: authToken,
                pairedAtUnixMs: nowMs,
                relayCredentials: RelayCredentials(
                    relayURL: RelayConfig.relayURL,
                    macID: macID,
                    relayToken: relayToken,
                    e2eKey: e2eKey
                )
            )
        }
    }

    /// Hashes the relay Worker validates client connects against; the host
    /// uplink registers them in its hello frame. Devices with relay disallowed
    /// are simply never registered — the relay refuses their token and they
    /// fail closed to Direct.
    func relayTokenRegistrations() -> [RelayDeviceTokenRegistration] {
        (try? withFreshRecords { _ in
            records.filter(\.isRelayAllowed).map { record in
                RelayDeviceTokenRegistration(deviceID: record.id, tokenHash: record.relayTokenHash)
            }
        }) ?? []
    }

    func relayAllowed(forDeviceID deviceID: String) -> Bool {
        relayTokenHash(forDeviceID: deviceID) != nil
    }

    /// The exact Link authorization revision for one device. Binding an
    /// established crypto session to this value (not merely the stable device
    /// id) makes credential rotation/re-pair revoke the old session even when
    /// another frontend changed devices.json and an in-process notification
    /// was missed.
    func relayTokenHash(forDeviceID deviceID: String) -> String? {
        try? withFreshRecords { _ in
            guard let record = records.first(where: { $0.id == deviceID }),
                  record.isRelayAllowed else { return nil }
            return record.relayTokenHash
        }
    }

    /// Scope a paired device to Direct-only (or back). Posting the devices
    /// change replaces the uplink, so the relay's registered token set and any
    /// in-flight relay connection for the device drop immediately.
    func setRelayAllowed(deviceID: String, allowed: Bool) {
        let changed: Bool
        do {
            changed = try withFreshRecords { directoryDescriptor in
                guard let index = records.firstIndex(where: { $0.id == deviceID }),
                      records[index].isRelayAllowed != allowed else { return false }
                // Allowed is the nil default: store only the narrowing value
                // so the file gains no key until a device is restricted.
                records[index].relayAllowed = allowed ? nil : false
                try persistLocked(directoryDescriptor: directoryDescriptor)
                return true
            }
        } catch {
            NSLog("[UnpeelNative] failed to change device relay scope: \(error)")
            changed = false
        }
        if changed {
            NotificationCenter.default.post(name: .unpeelMobileDevicesChanged, object: nil)
        }
    }

    func e2eKey(forDeviceID deviceID: String) -> Data? {
        try? withFreshRecords { directoryDescriptor in
            guard records.contains(where: { $0.id == deviceID }) else { return nil }
            try reconcileSharedE2EKeysLocked(
                directoryDescriptor: directoryDescriptor
            )
            return try loadSharedE2EKey(
                deviceID: deviceID,
                directoryDescriptor: directoryDescriptor
            )
        }
    }

    /// Mint fresh relay credentials after credential loss. Reported write
    /// failures restore the previous key/token revision. The flat shared-store
    /// compatibility ABI cannot make a process/power-loss window spanning
    /// Keychain, shared keys, and devices.json crash-atomic.
    func rotateRelayCredentials(deviceID: String) -> RelayCredentials? {
        let credentials: RelayCredentials?
        do {
            credentials = try withFreshRecords { directoryDescriptor in
                try reconcileSharedE2EKeysLocked(
                    directoryDescriptor: directoryDescriptor
                )
                guard let index = records.firstIndex(where: { $0.id == deviceID }) else {
                    return nil
                }
                let previousKey = e2eKeyStore.load(deviceID: deviceID)
                let previousSharedValue = try sharedE2EKeyValue(
                    deviceID: deviceID,
                    directoryDescriptor: directoryDescriptor
                )
                let e2eKey = Self.randomBytes(32)
                let relayToken = Self.randomToken(byteCount: 32)
                try e2eKeyStore.save(e2eKey, deviceID: deviceID)
                do {
                    try saveSharedE2EKey(
                        e2eKey,
                        deviceID: deviceID,
                        directoryDescriptor: directoryDescriptor
                    )
                } catch {
                    restoreKeychain(previousKey, deviceID: deviceID)
                    throw error
                }
                records[index].relayTokenHash = Self.sha256(relayToken)
                do {
                    try persistLocked(directoryDescriptor: directoryDescriptor)
                } catch {
                    restoreKeychain(previousKey, deviceID: deviceID)
                    restoreSharedE2EKeyValue(
                        previousSharedValue,
                        deviceID: deviceID,
                        directoryDescriptor: directoryDescriptor
                    )
                    throw error
                }
                return RelayCredentials(
                    relayURL: RelayConfig.relayURL,
                    macID: macID,
                    relayToken: relayToken,
                    e2eKey: e2eKey
                )
            }
        } catch {
            NSLog("[UnpeelNative] failed to rotate relay credentials: \(error)")
            credentials = nil
        }
        if credentials != nil {
            // The uplink re-sends its hello (fresh hash set) on this signal.
            NotificationCenter.default.post(name: .unpeelMobileDevicesChanged, object: nil)
        }
        return credentials
    }

    /// Register (or refresh) a device's APNs token so the Mac can push to it.
    /// Called from the authenticated `/mobile/push-token` route.
    func setPushToken(deviceID: String, token: String, environment: String) {
        do {
            try withFreshRecords { directoryDescriptor in
                guard let index = records.firstIndex(where: { $0.id == deviceID }) else { return }
                guard records[index].apnsToken != token
                    || records[index].apnsEnvironment != environment else { return }
                records[index].apnsToken = token
                records[index].apnsEnvironment = environment
                try persistLocked(directoryDescriptor: directoryDescriptor)
            }
        } catch {
            NSLog("[UnpeelNative] failed to persist push token: \(error)")
        }
    }

    /// Drop a device's APNs token after APNs reports it dead
    /// (BadDeviceToken/Unregistered), so the Mac stops pushing to it.
    func clearPushToken(deviceID: String) {
        do {
            try withFreshRecords { directoryDescriptor in
                guard let index = records.firstIndex(where: { $0.id == deviceID }),
                      records[index].apnsToken != nil else { return }
                records[index].apnsToken = nil
                records[index].apnsEnvironment = nil
                try persistLocked(directoryDescriptor: directoryDescriptor)
            }
        } catch {
            NSLog("[UnpeelNative] failed to clear push token: \(error)")
        }
    }

    /// Every paired device with a registered push token, for fan-out.
    func pushTargets() -> [(deviceID: String, token: String, environment: String)] {
        (try? withFreshRecords { _ in
            records.compactMap { record in
                // Pushes travel through the Unpeel Link service, so a device
                // scoped Direct-only is not a push target either — "Direct
                // only" means nothing about this device rides the relay.
                guard record.isRelayAllowed else { return nil }
                guard let token = record.apnsToken, !token.isEmpty else { return nil }
                return (record.id, token, record.apnsEnvironment ?? "production")
            }
        }) ?? []
    }

    func verifyAuthorizationHeader(_ header: String?, now: Date = Date()) -> String? {
        guard let token = Self.bearerToken(from: header) else { return nil }
        let tokenHash = Self.sha256(token)
        do {
            return try withFreshRecords { directoryDescriptor in
                guard let index = records.firstIndex(where: { $0.tokenHash == tokenHash }) else {
                    return nil
                }
                let id = records[index].id
                let nowMs = Self.unixMs(now)
                let previous = records[index].lastSeenAtUnixMs ?? 0
                if nowMs - previous > 60_000 {
                    records[index].lastSeenAtUnixMs = nowMs
                    do {
                        try persistLocked(directoryDescriptor: directoryDescriptor)
                    } catch {
                        // lastSeen is diagnostic only. The credential was read
                        // from the locked authority file and remains valid;
                        // do not turn a metadata write failure into auth churn.
                        records = try Self.loadRecordsStrict(
                            directoryDescriptor: directoryDescriptor,
                            fileName: storageURL.lastPathComponent
                        )
                        NSLog("[UnpeelNative] failed to persist device lastSeen: \(error)")
                    }
                }
                return id
            }
        } catch {
            // A missing, locked, or malformed authority file fails closed.
            return nil
        }
    }

    @discardableResult
    func revokeDevice(id: String) -> Bool {
        let changed: Bool
        do {
            changed = try withFreshRecords { directoryDescriptor in
                guard records.contains(where: { $0.id == id }) else { return false }
                records.removeAll { $0.id == id }
                // Commit the authorization removal first. Key cleanup is
                // intentionally after the durable authority change, while the
                // same cross-process lock still excludes a re-pair of this id.
                try persistLocked(directoryDescriptor: directoryDescriptor)
                do {
                    try removeSharedE2EKey(
                        deviceID: id,
                        directoryDescriptor: directoryDescriptor
                    )
                } catch {
                    // Authorization is already durably gone. A leftover key
                    // is an unusable orphan, so cleanup failure must not make
                    // the caller believe revocation failed.
                    NSLog("[UnpeelNative] failed to clean revoked shared E2E key: \(error)")
                }
                e2eKeyStore.delete(deviceID: id)
                return true
            }
        } catch {
            NSLog("[UnpeelNative] failed to revoke mobile device: \(error)")
            changed = false
        }
        if changed {
            // Lets the server/uplink close active access or stop entirely.
            NotificationCenter.default.post(name: .unpeelMobileDevicesChanged, object: nil)
        }
        return changed
    }

    /// Serialize every native/TUI read-modify-write through the stable
    /// `devices.lock` inode, and always reload the authority after acquiring
    /// it. Atomic rename alone protects readers from torn JSON but does not
    /// prevent two frontends from overwriting each other's newer snapshot.
    private func withFreshRecords<T>(_ operation: (Int32) throws -> T) throws -> T {
        try lock.withLock {
            Self.processStorageLock.lock()
            defer { Self.processStorageLock.unlock() }

            let directory = storageURL.deletingLastPathComponent()
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true
            )
            let directoryDescriptor = open(
                directory.path,
                O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK
            )
            guard directoryDescriptor >= 0 else {
                throw MobileRemoteError(500, "device store directory open failed")
            }
            defer { _ = close(directoryDescriptor) }
            try Self.requireFileKind(
                descriptor: directoryDescriptor,
                kind: mode_t(S_IFDIR),
                message: "device store directory is unsafe"
            )

            let descriptor = "devices.lock".withCString { name in
                openat(
                    directoryDescriptor,
                    name,
                    O_CREAT | O_RDWR | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK,
                    mode_t(0o600)
                )
            }
            guard descriptor >= 0 else {
                throw MobileRemoteError(500, "device store lock open failed")
            }
            defer { _ = close(descriptor) }
            try Self.requirePrivateRegularFile(
                descriptor: descriptor,
                message: "device store lock is unsafe"
            )
            guard fchmod(descriptor, mode_t(0o600)) == 0 else {
                throw MobileRemoteError(500, "device store lock permission failed")
            }
            guard flock(descriptor, LOCK_EX) == 0 else {
                throw MobileRemoteError(500, "device store lock failed")
            }
            defer { _ = flock(descriptor, LOCK_UN) }

            records = try Self.loadRecordsStrict(
                directoryDescriptor: directoryDescriptor,
                fileName: storageURL.lastPathComponent
            )
            do {
                return try operation(directoryDescriptor)
            } catch {
                // A failed mutation must not remain authoritative in memory.
                records = (try? Self.loadRecordsStrict(
                    directoryDescriptor: directoryDescriptor,
                    fileName: storageURL.lastPathComponent
                )) ?? []
                throw error
            }
        }
    }

    private func persistLocked(directoryDescriptor: Int32) throws {
        let data = try JSONEncoder().encode(MobilePairedDeviceFile(devices: records))
        try Self.writePrivateAtomically(
            data,
            directoryDescriptor: directoryDescriptor,
            fileName: storageURL.lastPathComponent,
            description: "device store"
        )
    }

    /// Reconcile the Keychain and the CLI-readable flat map while holding the
    /// same `devices.lock` used for the authority file. Only authorized device
    /// records participate. A valid shared revision wins (TUI rotation), and
    /// a missing shared revision is copied from scoped-or-legacy Keychain.
    private func reconcileSharedE2EKeysLocked(directoryDescriptor: Int32) throws {
        var values = try Self.loadSharedE2EKeyValues(
            directoryDescriptor: directoryDescriptor
        )
        var changed = false
        for record in records {
            let name = sharedE2EKeyName(deviceID: record.id)
            if let encoded = values[name] {
                let shared = try Self.decodeSharedE2EKey(encoded)
                if e2eKeyStore.load(deviceID: record.id) != shared {
                    try e2eKeyStore.save(shared, deviceID: record.id)
                }
            } else if let keychain = e2eKeyStore.load(deviceID: record.id) {
                guard keychain.count == 32 else {
                    throw MobileRemoteError(500, "Keychain E2E key is invalid")
                }
                values[name] = keychain.base64EncodedString()
                changed = true
            }
        }
        if changed {
            try Self.saveSharedE2EKeyValues(
                values,
                directoryDescriptor: directoryDescriptor
            )
        }
    }

    private func sharedE2EKeyName(deviceID: String) -> String {
        "\(macID).\(deviceID)"
    }

    private func sharedE2EKeyValue(
        deviceID: String,
        directoryDescriptor: Int32
    ) throws -> String? {
        let value = try Self.loadSharedE2EKeyValues(
            directoryDescriptor: directoryDescriptor
        )[sharedE2EKeyName(deviceID: deviceID)]
        if let value {
            _ = try Self.decodeSharedE2EKey(value)
        }
        return value
    }

    private func loadSharedE2EKey(
        deviceID: String,
        directoryDescriptor: Int32
    ) throws -> Data? {
        guard let value = try sharedE2EKeyValue(
            deviceID: deviceID,
            directoryDescriptor: directoryDescriptor
        ) else { return nil }
        return try Self.decodeSharedE2EKey(value)
    }

    private func saveSharedE2EKey(
        _ key: Data,
        deviceID: String,
        directoryDescriptor: Int32
    ) throws {
        guard key.count == 32 else {
            throw MobileRemoteError(500, "E2E key is invalid")
        }
        var values = try Self.loadSharedE2EKeyValues(
            directoryDescriptor: directoryDescriptor
        )
        values[sharedE2EKeyName(deviceID: deviceID)] = key.base64EncodedString()
        try Self.saveSharedE2EKeyValues(values, directoryDescriptor: directoryDescriptor)
    }

    private func removeSharedE2EKey(
        deviceID: String,
        directoryDescriptor: Int32
    ) throws {
        var values = try Self.loadSharedE2EKeyValues(
            directoryDescriptor: directoryDescriptor
        )
        guard values.removeValue(forKey: sharedE2EKeyName(deviceID: deviceID)) != nil else {
            return
        }
        try Self.saveSharedE2EKeyValues(values, directoryDescriptor: directoryDescriptor)
    }

    private func restoreKeychain(_ previousKey: Data?, deviceID: String) {
        if let previousKey {
            do {
                try e2eKeyStore.save(previousKey, deviceID: deviceID)
            } catch {
                // The restored shared revision remains canonical and will be
                // copied back on the next native locked read.
                NSLog("[UnpeelNative] failed to roll back Keychain E2E key: \(error)")
            }
        } else {
            e2eKeyStore.delete(deviceID: deviceID)
        }
    }

    private func restoreSharedE2EKeyValue(
        _ previousValue: String?,
        deviceID: String,
        directoryDescriptor: Int32
    ) {
        do {
            var values = try Self.loadSharedE2EKeyValues(
                directoryDescriptor: directoryDescriptor
            )
            let name = sharedE2EKeyName(deviceID: deviceID)
            if let previousValue {
                values[name] = previousValue
            } else {
                values.removeValue(forKey: name)
            }
            try Self.saveSharedE2EKeyValues(
                values,
                directoryDescriptor: directoryDescriptor
            )
        } catch {
            NSLog("[UnpeelNative] failed to roll back shared E2E key: \(error)")
        }
    }

    private static func loadSharedE2EKeyValues(
        directoryDescriptor: Int32
    ) throws -> [String: String] {
        guard let data = try readPrivateRegularFile(
            directoryDescriptor: directoryDescriptor,
            fileName: "e2e-keys.json",
            description: "shared E2E key store"
        ) else { return [:] }
        do {
            return try JSONDecoder().decode([String: String].self, from: data)
        } catch {
            throw MobileRemoteError(500, "shared E2E key store is malformed")
        }
    }

    private static func saveSharedE2EKeyValues(
        _ values: [String: String],
        directoryDescriptor: Int32
    ) throws {
        do {
            var data = try JSONSerialization.data(
                withJSONObject: values,
                options: [.prettyPrinted, .sortedKeys]
            )
            data.append(0x0A)
            try writePrivateAtomically(
                data,
                directoryDescriptor: directoryDescriptor,
                fileName: "e2e-keys.json",
                description: "shared E2E key store"
            )
        } catch {
            if error is MobileRemoteError { throw error }
            throw MobileRemoteError(500, "shared E2E key store encoding failed")
        }
    }

    private static func decodeSharedE2EKey(_ encoded: String) throws -> Data {
        guard let key = Data(base64Encoded: encoded),
              key.count == 32,
              key.base64EncodedString() == encoded
        else {
            throw MobileRemoteError(500, "shared E2E key is invalid")
        }
        return key
    }

    private static func loadRecordsStrict(
        directoryDescriptor: Int32,
        fileName: String
    ) throws -> [MobilePairedDeviceRecord] {
        guard let data = try readPrivateRegularFile(
            directoryDescriptor: directoryDescriptor,
            fileName: fileName,
            description: "device store"
        ) else { return [] }
        do {
            return try JSONDecoder().decode(MobilePairedDeviceFile.self, from: data).devices
        } catch {
            throw MobileRemoteError(500, "device store is unreadable")
        }
    }

    private static func readPrivateRegularFile(
        directoryDescriptor: Int32,
        fileName: String,
        description: String
    ) throws -> Data? {
        try requireSafeFileName(fileName)
        let descriptor = fileName.withCString { name in
            openat(
                directoryDescriptor,
                name,
                O_RDONLY | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK
            )
        }
        guard descriptor >= 0 else {
            if errno == ENOENT { return nil }
            throw MobileRemoteError(500, "\(description) open failed")
        }
        defer { _ = close(descriptor) }
        try requirePrivateRegularFile(
            descriptor: descriptor,
            message: "\(description) is unsafe"
        )
        guard fchmod(descriptor, mode_t(0o600)) == 0 else {
            throw MobileRemoteError(500, "\(description) permission failed")
        }

        var metadata = stat()
        guard fstat(descriptor, &metadata) == 0,
              metadata.st_size >= 0,
              metadata.st_size <= 4 * 1024 * 1024
        else {
            throw MobileRemoteError(500, "\(description) is too large")
        }

        var data = Data()
        data.reserveCapacity(Int(metadata.st_size))
        var buffer = [UInt8](repeating: 0, count: 16 * 1024)
        while true {
            let count = buffer.withUnsafeMutableBytes { bytes in
                Darwin.read(descriptor, bytes.baseAddress, bytes.count)
            }
            if count < 0 {
                if errno == EINTR { continue }
                throw MobileRemoteError(500, "\(description) read failed")
            }
            if count == 0 { break }
            guard data.count + count <= 4 * 1024 * 1024 else {
                throw MobileRemoteError(500, "\(description) is too large")
            }
            data.append(buffer, count: count)
        }
        return data
    }

    private static func writePrivateAtomically(
        _ data: Data,
        directoryDescriptor: Int32,
        fileName: String,
        description: String
    ) throws {
        try requireSafeFileName(fileName)
        try requireRegularFileOrMissing(
            directoryDescriptor: directoryDescriptor,
            fileName: fileName,
            description: description
        )

        let temporaryName = ".\(fileName).\(getpid()).\(UUID().uuidString).tmp"
        let descriptor = temporaryName.withCString { name in
            openat(
                directoryDescriptor,
                name,
                O_CREAT | O_EXCL | O_WRONLY | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK,
                mode_t(0o600)
            )
        }
        guard descriptor >= 0 else {
            throw MobileRemoteError(500, "\(description) temporary file open failed")
        }

        var needsClose = true
        var committed = false
        defer {
            if needsClose { _ = close(descriptor) }
            if !committed {
                temporaryName.withCString { name in
                    _ = unlinkat(directoryDescriptor, name, 0)
                }
            }
        }

        try requirePrivateRegularFile(
            descriptor: descriptor,
            message: "\(description) temporary file is unsafe"
        )
        guard fchmod(descriptor, mode_t(0o600)) == 0 else {
            throw MobileRemoteError(500, "\(description) permission failed")
        }
        try data.withUnsafeBytes { bytes in
            guard let base = bytes.baseAddress else { return }
            var offset = 0
            while offset < bytes.count {
                let written = Darwin.write(
                    descriptor,
                    base.advanced(by: offset),
                    bytes.count - offset
                )
                if written < 0 {
                    if errno == EINTR { continue }
                    throw MobileRemoteError(500, "\(description) write failed")
                }
                guard written > 0 else {
                    throw MobileRemoteError(500, "\(description) short write")
                }
                offset += written
            }
        }
        guard fsync(descriptor) == 0 else {
            throw MobileRemoteError(500, "\(description) sync failed")
        }
        let closeResult = close(descriptor)
        needsClose = false
        guard closeResult == 0 else {
            throw MobileRemoteError(500, "\(description) close failed")
        }

        let renameResult = temporaryName.withCString { temporary in
            fileName.withCString { final in
                renameat(directoryDescriptor, temporary, directoryDescriptor, final)
            }
        }
        guard renameResult == 0 else {
            throw MobileRemoteError(500, "\(description) commit failed")
        }
        committed = true
        // The file was synced before rename. Directory fsync makes that
        // rename durable; after the commit point, never claim failure and
        // trigger a rollback that could disagree with the live revision.
        _ = fsync(directoryDescriptor)
    }

    private static func requireRegularFileOrMissing(
        directoryDescriptor: Int32,
        fileName: String,
        description: String
    ) throws {
        let descriptor = fileName.withCString { name in
            openat(
                directoryDescriptor,
                name,
                O_RDONLY | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK
            )
        }
        guard descriptor >= 0 else {
            if errno == ENOENT { return }
            throw MobileRemoteError(500, "\(description) inspection failed")
        }
        defer { _ = close(descriptor) }
        try requirePrivateRegularFile(
            descriptor: descriptor,
            message: "\(description) is unsafe"
        )
    }

    private static func requirePrivateRegularFile(
        descriptor: Int32,
        message: String
    ) throws {
        var metadata = stat()
        guard fstat(descriptor, &metadata) == 0,
              metadata.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG),
              metadata.st_nlink == 1
        else {
            throw MobileRemoteError(500, message)
        }
    }

    private static func requireFileKind(
        descriptor: Int32,
        kind: mode_t,
        message: String
    ) throws {
        var metadata = stat()
        guard fstat(descriptor, &metadata) == 0,
              metadata.st_mode & mode_t(S_IFMT) == kind
        else {
            throw MobileRemoteError(500, message)
        }
    }

    private static func requireSafeFileName(_ fileName: String) throws {
        guard !fileName.isEmpty,
              fileName != ".",
              fileName != "..",
              !fileName.contains("/")
        else {
            throw MobileRemoteError(500, "device store filename is unsafe")
        }
    }

    /// Stable identity for this logical Host instance. The `macID` spelling
    /// is retained on the shipped wire, but the identity belongs to the Host,
    /// not specifically to mobile clients.
    static func defaultMacID() -> String {
        let url = LaunchConfig.unpeelDir
            .appendingPathComponent("mobile")
            .appendingPathComponent("mac-id")
        return stableHostID(at: url)
    }

    /// Return one durable Host identity even when multiple app processes
    /// sharing an UNPEEL_HOME launch for the first time together. The lock is
    /// cross-process; the second reader checks the file again only after the
    /// first writer has committed it.
    static func stableHostID(at url: URL) -> String {
        if let existing = persistedHostID(at: url) { return existing }

        do {
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
        } catch {
            NSLog("[UnpeelNative] failed to create Host identity directory: \(error)")
            return UUID().uuidString.lowercased()
        }

        let lockURL = url.appendingPathExtension("lock")
        let fd = open(lockURL.path, O_CREAT | O_WRONLY, 0o600)
        guard fd >= 0 else {
            if let existing = persistedHostID(at: url) { return existing }
            let fallback = UUID().uuidString.lowercased()
            NSLog("[UnpeelNative] failed to lock Host identity file")
            return fallback
        }
        defer { close(fd) }
        guard flock(fd, LOCK_EX) == 0 else {
            if let existing = persistedHostID(at: url) { return existing }
            let fallback = UUID().uuidString.lowercased()
            NSLog("[UnpeelNative] failed to acquire Host identity lock")
            return fallback
        }
        defer { flock(fd, LOCK_UN) }

        if let existing = persistedHostID(at: url) { return existing }
        let id = UUID().uuidString.lowercased()
        do {
            try (id + "\n").write(to: url, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: url.path
            )
        } catch {
            NSLog("[UnpeelNative] failed to persist mobile mac id: \(error)")
        }
        return id
    }

    private static func persistedHostID(at url: URL) -> String? {
        guard let raw = try? String(contentsOf: url, encoding: .utf8) else { return nil }
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    static func bearerToken(from header: String?) -> String? {
        guard let header else { return nil }
        let trimmed = header.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.lowercased().hasPrefix("bearer ") else { return nil }
        let token = trimmed.dropFirst("bearer ".count)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return token.isEmpty ? nil : token
    }

    static func sha256(_ value: String) -> String {
        SHA256.hash(data: Data(value.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
    }

    static func randomToken(byteCount: Int) -> String {
        var bytes = [UInt8](repeating: 0, count: byteCount)
        let status = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        if status != errSecSuccess {
            return (UUID().uuidString + UUID().uuidString)
                .replacingOccurrences(of: "-", with: "")
                .lowercased()
        }
        return Data(bytes).unpeelBase64URLString()
    }

    static func randomBytes(_ count: Int) -> Data {
        var bytes = [UInt8](repeating: 0, count: count)
        let status = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        precondition(status == errSecSuccess, "SecRandomCopyBytes failed")
        return Data(bytes)
    }

    static func randomBase32Token(byteCount: Int) -> String {
        var bytes = [UInt8](repeating: 0, count: byteCount)
        let status = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        if status != errSecSuccess {
            bytes = Array((UUID().uuidString + UUID().uuidString).utf8.prefix(byteCount))
        }
        let alphabet = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZ234567")
        var output = ""
        var buffer = 0
        var bitsInBuffer = 0
        for byte in bytes {
            buffer = (buffer << 8) | Int(byte)
            bitsInBuffer += 8
            while bitsInBuffer >= 5 {
                bitsInBuffer -= 5
                output.append(alphabet[(buffer >> bitsInBuffer) & 0x1F])
            }
        }
        if bitsInBuffer > 0 {
            output.append(alphabet[(buffer << (5 - bitsInBuffer)) & 0x1F])
        }
        return output
    }

    static func unixMs(_ date: Date) -> Int64 {
        Int64(date.timeIntervalSince1970 * 1000)
    }
}

final class MobileRemoteServer: @unchecked Sendable {
    /// Native Host capabilities in the canonical ledger's stable order.
    /// `HostProtocolConformanceTests` fails if this drifts from
    /// `protocol/host-capabilities-v1.json`.
    static let hostProtocol = RemoteHostProtocolDescriptor(capabilities: [
        "approval.answer",
        "approval.list",
        "artifact.delete",
        "artifact.list",
        "artifact.read",
        "artifact.request_screenshot",
        "artifact.upload",
        "artifact.upload.resumable",
        "host.bootstrap",
        "pairing.create",
        "project.organization.set",
        "push.register",
        "relay.credentials.recover",
        "session.archive",
        "session.archive.list",
        "session.create",
        "session.input.write",
        "session.mark_read",
        "session.metrics.read",
        "session.notify_when_done.set",
        "session.order.set",
        "session.output.read",
        "session.output.subscribe",
        "session.pin.set",
        "session.remove",
        "session.resize",
        "session.resize_desktop",
        "session.restart",
        "session.restore",
        "session.runtime.resume",
        "session.stop",
        "session.title.set",
        "session.transcript.markdown",
    ])

    typealias BootstrapProvider = @MainActor @Sendable () -> RemoteBootstrapSnapshot
    typealias CreateSessionProvider = @MainActor @Sendable (RemoteCreateSessionRequest) throws -> RemoteCreateSessionResponse
    typealias ResizeDesktopProvider = @MainActor @Sendable (RemoteDesktopResizeRequest) throws -> Void
    typealias SessionOrganizationProvider = @MainActor @Sendable (RemoteSessionOrganizationPatch) throws -> Void
    typealias ProjectOrganizationProvider = @MainActor @Sendable (RemoteProjectOrganizationPatch) throws -> Void
    typealias SessionOrderProvider = @MainActor @Sendable (RemoteSessionOrderRequest) throws -> Void
    typealias RestartSessionProvider = @MainActor @Sendable (RemoteRestartSessionRequest) throws -> Void
    typealias SessionActionProvider = @MainActor @Sendable (
        RemoteSessionActionRequest
    ) async throws -> Void
    typealias MarkReadProvider = @MainActor @Sendable (RemoteMarkReadRequest) throws -> Void
    typealias RequestScreenshotProvider = @Sendable (RemoteScreenshotRequest) throws -> RemoteScreenshotRequestResponse
    /// Answer a pending MCP approval prompt (409 when no longer pending).
    typealias ApprovalAnswerProvider = @MainActor @Sendable (RemoteApprovalAnswerRequest) throws -> Void
    /// Whether the desktop is actively viewing the given session (selected +
    /// app frontmost). Rides the metrics response so the phone's fit policy
    /// knows when re-asserting the letterbox can't fight a human at the Mac.
    typealias DesktopViewingProvider = @MainActor @Sendable (String) -> Bool

    /// One project's archived sessions for the phone's archive library
    /// (GET /mobile/archive). Nil = unknown project id.
    typealias ArchivedSessionsProvider = @MainActor @Sendable (String) -> RemoteArchivedSessionsResponse?

    let pairingStore: MobilePairingStore
    private(set) var endpoint: URL

    private let bootstrapProvider: BootstrapProvider
    private let createSessionProvider: CreateSessionProvider
    private let resizeDesktopProvider: ResizeDesktopProvider
    private let sessionOrganizationProvider: SessionOrganizationProvider
    private let projectOrganizationProvider: ProjectOrganizationProvider
    private let sessionOrderProvider: SessionOrderProvider
    private let restartSessionProvider: RestartSessionProvider
    private let sessionActionProvider: SessionActionProvider
    private let markReadProvider: MarkReadProvider
    private let requestScreenshotProvider: RequestScreenshotProvider
    private let approvalAnswerProvider: ApprovalAnswerProvider
    private let desktopViewingProvider: DesktopViewingProvider
    private let archivedSessionsProvider: ArchivedSessionsProvider
    private let controllerRouter: any NativeControllerRouting
    private let lifecycleReplayCache = LifecycleReplayCache()
    private let onDevicesChanged: @Sendable () -> Void
    private let onPairingCompleted: @Sendable (String) -> Void
    private let decoder = JSONDecoder()
    private let encoder = JSONEncoder()
    private var listenFD: Int32 = -1
    private var acceptThread: Thread?
    private var bonjourAdvertiser: MobileServerBonjourAdvertiser?

    init?(
        pairingStore: MobilePairingStore = MobilePairingStore(),
        bootstrapProvider: @escaping BootstrapProvider,
        createSessionProvider: @escaping CreateSessionProvider,
        resizeDesktopProvider: @escaping ResizeDesktopProvider,
        sessionOrganizationProvider: @escaping SessionOrganizationProvider,
        projectOrganizationProvider: @escaping ProjectOrganizationProvider = { _ in
            throw MobileRemoteError(501, "project organization is not supported")
        },
        sessionOrderProvider: @escaping SessionOrderProvider = { _ in
            throw MobileRemoteError(501, "session order is not supported")
        },
        restartSessionProvider: @escaping RestartSessionProvider,
        sessionActionProvider: @escaping SessionActionProvider,
        markReadProvider: @escaping MarkReadProvider,
        requestScreenshotProvider: @escaping RequestScreenshotProvider = { request in
            try MobileSessionControl.requestScreenshot(sessionID: request.sessionID)
        },
        approvalAnswerProvider: @escaping ApprovalAnswerProvider,
        desktopViewingProvider: @escaping DesktopViewingProvider,
        archivedSessionsProvider: @escaping ArchivedSessionsProvider = { _ in nil },
        controllerRouter: any NativeControllerRouting = NativeControllerRouter.shared,
        onDevicesChanged: @escaping @Sendable () -> Void,
        onPairingCompleted: @escaping @Sendable (String) -> Void = { _ in },
        startNetworkServer: Bool = true
    ) {
        self.pairingStore = pairingStore
        self.bootstrapProvider = bootstrapProvider
        self.createSessionProvider = createSessionProvider
        self.resizeDesktopProvider = resizeDesktopProvider
        self.sessionOrganizationProvider = sessionOrganizationProvider
        self.projectOrganizationProvider = projectOrganizationProvider
        self.sessionOrderProvider = sessionOrderProvider
        self.restartSessionProvider = restartSessionProvider
        self.sessionActionProvider = sessionActionProvider
        self.markReadProvider = markReadProvider
        self.requestScreenshotProvider = requestScreenshotProvider
        self.approvalAnswerProvider = approvalAnswerProvider
        self.desktopViewingProvider = desktopViewingProvider
        self.archivedSessionsProvider = archivedSessionsProvider
        self.controllerRouter = controllerRouter
        self.onDevicesChanged = onDevicesChanged
        self.onPairingCompleted = onPairingCompleted
        endpoint = URL(string: "http://127.0.0.1:0/mobile")!

        // The conformance harness exercises the real authenticated route
        // adapter without binding a port or starting Bonjour threads.
        if !startNetworkServer { return }

        guard let claimed = Self.claimMobileListener(portFileURL: Self.portFileURL) else {
            return nil
        }
        let fd = claimed.descriptor
        let port = claimed.port
        endpoint = URL(string: "http://\(Self.preferredLANAddress()):\(port)/mobile")!
        listenFD = fd

        let thread = Thread { [weak self] in
            self?.acceptLoop(fd: fd)
        }
        thread.name = "unpeel.mobile-remote-server"
        thread.start()
        acceptThread = thread

        // Keep advertising as an unauthenticated nearby/pairing hint. Paired
        // Controllers must not send a saved bearer to this discovered HTTP
        // address; TXT identity alone is not proof of Host possession.
        bonjourAdvertiser = MobileServerBonjourAdvertiser(
            macID: pairingStore.macID,
            macName: pairingStore.macName,
            port: port
        )

        NSLog("[UnpeelNative] mobile remote server listening on \(endpoint.absoluteString)")
    }

    deinit {
        stop()
    }

    func stop() {
        if listenFD >= 0 {
            close(listenFD)
            listenFD = -1
        }
        bonjourAdvertiser?.stop()
        bonjourAdvertiser = nil
    }

    private static var portFileURL: URL {
        LaunchConfig.unpeelDir
            .appendingPathComponent("mobile")
            .appendingPathComponent("server-port")
    }

    private static func persistedPort(at url: URL) -> UInt16? {
        guard let raw = try? String(contentsOf: url, encoding: .utf8),
              let port = UInt16(raw.trimmingCharacters(in: .whitespacesAndNewlines)),
              port > 0
        else { return nil }
        return port
    }

    @discardableResult
    private static func persistPort(_ port: UInt16, at url: URL) -> Bool {
        do {
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try "\(port)\n".write(to: url, atomically: true, encoding: .utf8)
            return true
        } catch {
            NSLog("[UnpeelNative] failed to persist mobile server port: \(error)")
            return false
        }
    }

    /// Shared native/TUI listener claim. The stable lock serializes the
    /// absent/corrupt-port case so two frontends cannot each bind port zero,
    /// publish different winners, and both start Link. A known endpoint is
    /// exact-or-nothing; the caller retries after the current owner releases.
    static func claimMobileListener(
        portFileURL: URL
    ) -> (descriptor: Int32, port: UInt16)? {
        let fd = socket(AF_INET, SOCK_STREAM, 0)
        guard fd >= 0 else { return nil }

        var reuse: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &reuse, socklen_t(MemoryLayout<Int32>.size))

        let directory = portFileURL.deletingLastPathComponent()
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        } catch {
            close(fd)
            return nil
        }
        let lockURL = directory.appendingPathComponent("server-port.lock")
        let lockFD = open(lockURL.path, O_CREAT | O_RDWR | O_CLOEXEC, mode_t(0o600))
        guard lockFD >= 0 else {
            close(fd)
            return nil
        }
        defer { close(lockFD) }
        guard fchmod(lockFD, mode_t(0o600)) == 0, flock(lockFD, LOCK_EX) == 0 else {
            close(fd)
            return nil
        }
        defer { _ = flock(lockFD, LOCK_UN) }

        // The phone persists this server's endpoint at pairing time. Binding
        // a replacement for a known port would orphan Direct because paired
        // Controllers deliberately do not trust Bonjour endpoint adoption.
        let canonicalPort = persistedPort(at: portFileURL)
        let headlessPort = persistedPort(
            at: directory.appendingPathComponent("headless-server-port")
        )
        let savedPort = canonicalPort ?? headlessPort
        var address = sockaddr_in()
        address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = savedPort?.bigEndian ?? 0
        address.sin_addr.s_addr = inet_addr("0.0.0.0")
        let bound = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bound == 0, listen(fd, 32) == 0 else {
            close(fd)
            return nil
        }

        var assigned = sockaddr_in()
        var length = socklen_t(MemoryLayout<sockaddr_in>.size)
        let got = withUnsafeMutablePointer(to: &assigned) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                getsockname(fd, $0, &length)
            }
        }
        guard got == 0 else {
            close(fd)
            return nil
        }
        let port = UInt16(bigEndian: assigned.sin_port)
        // Repair a missing/corrupt canonical file from the legacy headless
        // endpoint under the same lock. Native and Rust therefore resolve
        // exactly the same port and cannot become simultaneous owners.
        guard canonicalPort != nil || persistPort(port, at: portFileURL) else {
            close(fd)
            return nil
        }
        return (fd, port)
    }

    func beginPairing() -> RemotePairingPayload {
        pairingStore.beginPairing(endpoint: endpoint)
    }

    func cancelPairing() {
        pairingStore.cancelPairing()
    }

    func revokeDevice(id: String) {
        if pairingStore.revokeDevice(id: id) {
            onDevicesChanged()
        }
    }

    func setDeviceRelayAllowed(id: String, allowed: Bool) {
        pairingStore.setRelayAllowed(deviceID: id, allowed: allowed)
        onDevicesChanged()
    }

    var pairedDevices: [RemotePairedDeviceSummary] {
        pairingStore.devices
    }

    private func acceptLoop(fd: Int32) {
        while true {
            let client = accept(fd, nil, nil)
            guard client >= 0 else {
                if listenFD < 0 || errno == EBADF { return }
                continue
            }
            let thread = Thread { [weak self] in
                self?.handleConnection(client)
            }
            thread.start()
        }
    }

    /// Defensive cap on requests served over one keep-alive connection.
    private static let maxRequestsPerConnection = 1000

    /// Keep the native compatibility adapter aligned with both Rust ingress
    /// paths. These ids are retained in a bounded per-session history, so the
    /// entry count alone is not a memory bound unless each key is bounded too.
    static let maxWriteIDBytes = 128

    static func validatedWriteID(_ writeID: String?) throws -> String? {
        guard let writeID, !writeID.isEmpty else { return nil }
        guard writeID.utf8.count <= maxWriteIDBytes else {
            throw MobileRemoteError(400, "write id too long")
        }
        return writeID
    }

    private func handleConnection(_ fd: Int32) {
        defer { close(fd) }

        // Keep-alive: the phone long-polls roughly continuously, so serve
        // many requests per TCP connection instead of one (no handshake +
        // thread churn per poll). The read timeout doubles as the idle
        // timeout between requests so dead connections don't pin threads.
        var readTimeout = timeval(tv_sec: 30, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &readTimeout, socklen_t(MemoryLayout<timeval>.size))
        var sendTimeout = timeval(tv_sec: 10, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &sendTimeout, socklen_t(MemoryLayout<timeval>.size))

        var pending = Data()
        for served in 0..<Self.maxRequestsPerConnection {
            let request: HTTPRequest
            do {
                request = try readRequest(fd, pending: &pending)
            } catch is MobileConnectionClosed {
                return
            } catch let error as MobileRemoteError {
                respond(fd, status: error.status, body: errorJSON(error.message), keepAlive: false)
                return
            } catch {
                respond(fd, status: 400, body: errorJSON("bad request"), keepAlive: false)
                return
            }
            let response = handle(request)
            let connectionHeader = request.headers["connection"]?.lowercased()
            let keepAlive = connectionHeader != "close"
                && (request.httpVersion != "HTTP/1.0" || connectionHeader == "keep-alive")
                && served < Self.maxRequestsPerConnection - 1
            guard respond(fd, status: response.status, body: response.body, keepAlive: keepAlive),
                  keepAlive
            else { return }
        }
    }

    // Internal (not private): the relay uplink builds HTTPRequests from
    // decrypted tunnel frames and runs them through the same `handle`
    // pipeline as LAN traffic — same auth, same routes, zero divergence.
    struct HTTPRequest {
        var requestID: String?
        var method: String
        var rawPath: String
        var httpVersion: String
        var path: String
        var query: [String: String]
        var headers: [String: String]
        var body: Data
    }

    struct HTTPResponse {
        var status: Int
        var body: String
    }

    /// Swift still owns native lifecycle cleanup, so the shared Rust router
    /// deliberately returns these requests as unhandled. Preserve the shared
    /// at-most-once contract across that compatibility seam: one authenticated
    /// request id has one semantic request and one exact receipt.
    private final class LifecycleReplayCache: @unchecked Sendable {
        private struct Key: Hashable {
            let deviceID: String
            let requestID: String
        }

        private final class Entry {
            let fingerprint: Data
            let insertedAt: TimeInterval
            let completion = DispatchGroup()
            var response: HTTPResponse?

            init(fingerprint: Data, insertedAt: TimeInterval) {
                self.fingerprint = fingerprint
                self.insertedAt = insertedAt
                completion.enter()
            }
        }

        private static let ttl: TimeInterval = 5 * 60
        private static let maxEntries = 512

        private let lock = NSLock()
        private var entries: [Key: Entry] = [:]

        func execute(
            deviceID: String,
            request: HTTPRequest,
            operation: () -> HTTPResponse
        ) -> HTTPResponse {
            guard let requestID = request.requestID else {
                // Older Direct clients did not attach semantic ids. Keep
                // those clients working, but do not claim replay safety.
                return operation()
            }
            guard !requestID.isEmpty,
                  requestID.utf8.count <= MobileRemoteServer.maxWriteIDBytes
            else {
                return HTTPResponse(
                    status: 400,
                    body: #"{"error":"invalid request id"}"#
                )
            }

            let key = Key(deviceID: deviceID, requestID: requestID)
            let fingerprint = Self.fingerprint(request)
            let now = ProcessInfo.processInfo.systemUptime
            let entry: Entry
            let isLeader: Bool

            lock.lock()
            pruneCompleted(now: now, reservingSpace: true)
            if let existing = entries[key] {
                guard existing.fingerprint == fingerprint else {
                    lock.unlock()
                    return HTTPResponse(
                        status: 409,
                        body: #"{"error":"request id reused with different request"}"#
                    )
                }
                entry = existing
                isLeader = false
            } else {
                let created = Entry(fingerprint: fingerprint, insertedAt: now)
                entries[key] = created
                entry = created
                isLeader = true
            }
            lock.unlock()

            if !isLeader {
                entry.completion.wait()
                return lock.withLock {
                    entry.response ?? HTTPResponse(
                        status: 500,
                        body: #"{"error":"request processing aborted"}"#
                    )
                }
            }

            // `operation` converts every thrown provider failure to its exact
            // HTTP response before returning. Publish that receipt before
            // waking followers, then retain it for sequential Link/Direct
            // retries without invoking the Host again.
            let response = operation()
            lock.withLock {
                entry.response = response
                pruneCompleted(
                    now: ProcessInfo.processInfo.systemUptime,
                    reservingSpace: false
                )
            }
            entry.completion.leave()
            return response
        }

        /// In-flight entries are never evicted. A burst may temporarily
        /// exceed the cap and is trimmed as leaders publish their receipts.
        private func pruneCompleted(now: TimeInterval, reservingSpace: Bool) {
            entries = entries.filter { _, entry in
                entry.response == nil || now - entry.insertedAt <= Self.ttl
            }
            let targetCount = reservingSpace ? Self.maxEntries - 1 : Self.maxEntries
            while entries.count > targetCount,
                  let victim = entries
                    .filter({ $0.value.response != nil })
                    .min(by: { $0.value.insertedAt < $1.value.insertedAt })?.key {
                entries.removeValue(forKey: victim)
            }
        }

        private static func fingerprint(_ request: HTTPRequest) -> Data {
            var canonical = Data()
            func append(_ bytes: Data) {
                var length = UInt64(bytes.count).bigEndian
                withUnsafeBytes(of: &length) { canonical.append(contentsOf: $0) }
                canonical.append(bytes)
            }
            func append(_ value: String) {
                append(Data(value.utf8))
            }

            append(request.method)
            append(request.path)
            for (key, value) in request.query.sorted(by: {
                $0.key == $1.key ? $0.value < $1.value : $0.key < $1.key
            }) {
                append(key)
                append(value)
            }
            append(request.headers["content-type"] ?? "")
            append(request.body)
            return Data(SHA256.hash(data: canonical))
        }
    }

    /// Serve one `/mobile/*` request tunneled through the Unpeel Remote
    /// relay. Blocking (long-polls sleep); callers dispatch off the WS
    /// receive path.
    func handleTunneled(
        _ request: RelayTunnelRequest,
        connectionID: String? = nil
    ) -> RelayTunnelResponse {
        var headers: [String: String] = [:]
        if let auth = request.auth { headers["authorization"] = auth }
        if let contentType = request.contentType { headers["content-type"] = contentType }
        let httpRequest = HTTPRequest(
            requestID: connectionID.map { "relay:\($0):\(request.id)" }
                ?? "relay:\(request.id)",
            method: request.method,
            rawPath: request.path,
            httpVersion: "HTTP/1.1",
            path: request.path,
            query: request.query,
            headers: headers,
            body: request.body
        )
        let response = handle(httpRequest)
        return RelayTunnelResponse(
            id: request.id,
            status: response.status,
            body: Data(response.body.utf8)
        )
    }

    /// Reads one request from the connection, keeping any over-read bytes
    /// (the start of a pipelined next request) in `pending` for the next
    /// call. Throws `MobileConnectionClosed` on a clean close/idle timeout
    /// between requests.
    private func readRequest(_ fd: Int32, pending buffer: inout Data) throws -> HTTPRequest {
        var chunk = [UInt8](repeating: 0, count: 64 * 1024)
        var headerEndRange: Range<Data.Index>?
        var contentLength = 0
        var contentLengthHeader: String?
        var hasTransferEncoding = false
        var requestLine = ""
        var headers: [String: String] = [:]

        while true {
            if headerEndRange == nil,
               let range = buffer.range(of: Data("\r\n\r\n".utf8)) {
                headerEndRange = range
                guard let header = String(
                    data: buffer[buffer.startIndex..<range.lowerBound],
                    encoding: .utf8
                ) else {
                    throw MobileRemoteError(400, "bad request")
                }
                let lines = header.components(separatedBy: "\r\n")
                requestLine = lines.first ?? ""
                for line in lines.dropFirst() {
                    guard let split = line.firstIndex(of: ":") else { continue }
                    let name = line[..<split]
                        .trimmingCharacters(in: .whitespacesAndNewlines)
                        .lowercased()
                    let value = line[line.index(after: split)...]
                        .trimmingCharacters(in: .whitespacesAndNewlines)
                    headers[name] = value
                    if name == "content-length" {
                        guard contentLengthHeader == nil else {
                            throw MobileRemoteError(400, "duplicate content-length")
                        }
                        contentLengthHeader = value
                    } else if name == "transfer-encoding" {
                        hasTransferEncoding = true
                    }
                }
                if hasTransferEncoding {
                    throw MobileRemoteError(400, "unsupported transfer-encoding")
                }
                do {
                    contentLength = try StrictHTTPContentLength.parse(contentLengthHeader)
                } catch StrictHTTPContentLengthError.tooLarge {
                    throw MobileRemoteError(400, "body too large")
                } catch {
                    throw MobileRemoteError(400, "invalid content-length")
                }
            }

            if let headerEnd = headerEndRange {
                let bodyBytes = buffer.distance(from: headerEnd.upperBound, to: buffer.endIndex)
                if bodyBytes >= contentLength {
                    let bodyEnd = buffer.index(headerEnd.upperBound, offsetBy: contentLength)
                    let body = buffer.subdata(in: headerEnd.upperBound..<bodyEnd)
                    buffer.removeSubrange(buffer.startIndex..<bodyEnd)
                    return try makeRequest(
                        requestLine: requestLine,
                        headers: headers,
                        body: body
                    )
                }
            }

            if buffer.count > 8 * 1024 * 1024 {
                throw MobileRemoteError(400, "request too large")
            }
            let read = recv(fd, &chunk, chunk.count, 0)
            guard read > 0 else {
                if buffer.isEmpty { throw MobileConnectionClosed() }
                throw MobileRemoteError(400, "bad request")
            }
            buffer.append(contentsOf: chunk[0..<read])
        }
    }

    private func makeRequest(
        requestLine: String,
        headers: [String: String],
        body: Data
    ) throws -> HTTPRequest {
        let parts = requestLine.split(separator: " ")
        guard parts.count >= 2 else {
            throw MobileRemoteError(400, "bad request")
        }
        let method = String(parts[0])
        let rawPath = String(parts[1])
        guard let components = URLComponents(string: "http://unpeel.local\(rawPath)") else {
            throw MobileRemoteError(400, "bad path")
        }
        var query: [String: String] = [:]
        for item in components.queryItems ?? [] {
            query[item.name] = item.value
        }
        return HTTPRequest(
            requestID: headers["x-unpeel-request-id"],
            method: method,
            rawPath: rawPath,
            httpVersion: parts.count >= 3 ? String(parts[2]) : "HTTP/1.1",
            path: components.path,
            query: query,
            headers: headers,
            body: body
        )
    }

    func handle(_ request: HTTPRequest) -> HTTPResponse {
        do {
            if request.path == "/mobile/pair" {
                guard request.method == "POST" else {
                    throw MobileRemoteError(405, "method not allowed")
                }
                let envelope = try decoder.decode(RemotePairingEnvelope.self, from: request.body)
                let pairRequest = try pairingStore.decryptPairingRequest(envelope)
                let response = try pairingStore.pair(pairRequest)
                onDevicesChanged()
                onPairingCompleted(pairRequest.token)
                // Advertise the remote control (WSS terminal) server when it
                // is already up. It usually auto-starts *because of* this
                // pairing, so a nil here is normal — the phone learns the
                // port + fingerprint from the next /mobile/bootstrap.
                let advertised = remoteServerStatus()
                let enriched = RemotePairingResponse(
                    protocolVersion: response.protocolVersion,
                    macID: response.macID,
                    macName: response.macName,
                    endpoint: response.endpoint,
                    deviceID: response.deviceID,
                    authToken: response.authToken,
                    pairedAtUnixMs: response.pairedAtUnixMs,
                    remoteServerPort: advertised?.port,
                    remoteServerCertificateFingerprint: advertised?.fingerprint,
                    relayCredentials: response.relayCredentials
                )
                let plaintext = try encoder.encode(enriched)
                let sealed = try RemotePairingCrypto.seal(
                    plaintext,
                    token: pairRequest.token,
                    macID: enriched.macID,
                    endpoint: enriched.endpoint,
                    direction: .response
                )
                return try jsonResponse(sealed)
            }

            guard request.path.hasPrefix("/mobile/") else {
                throw MobileRemoteError(404, "not found")
            }
            guard request.method == "GET" || request.method == "POST" else {
                throw MobileRemoteError(405, "method not allowed")
            }
            let authHeader = request.headers["authorization"]
            guard let deviceID = pairingStore.verifyAuthorizationHeader(authHeader) else {
                throw MobileRemoteError(401, "unauthorized")
            }

            // Validate the idempotency key before either the shared router or
            // the Swift compatibility adapter can forward it to session.sock.
            // The request is decoded once here and reused by the fallback.
            let terminalWrite: RemoteTerminalWriteRequest?
            if request.method == "POST", request.path == "/mobile/write" {
                let decoded = try decoder.decode(
                    RemoteTerminalWriteRequest.self,
                    from: request.body
                )
                terminalWrite = RemoteTerminalWriteRequest(
                    sessionID: decoded.sessionID,
                    data: decoded.data,
                    writeID: try Self.validatedWriteID(decoded.writeID)
                )
            } else {
                terminalWrite = nil
            }

            // Authentication and pairing stay in the transport adapter. Only
            // after they resolve a durable principal does the request cross
            // the JSON-only Rust boundary. Routes not yet owned by the shared
            // router deliberately fall through to the shipped Swift adapter.
            let principal = NativeControllerPrincipal(
                deviceID: deviceID,
                name: pairingStore.devices.first { $0.id == deviceID }?.name ?? deviceID
            )
            // Viewer presence is transport-adapter state, not a Controller
            // protocol verb. Record it before the shared Rust router gets a
            // chance to return so Direct HTTP viewers participate in the same
            // per-device push suppression as WSS viewers.
            if request.method == "GET", request.path == "/mobile/output" {
                ViewerPresenceStore.noteMobileViewerAsync(
                    sessionID: request.query["session_id"] ?? request.query["sessionID"],
                    deviceID: deviceID,
                    deviceName: pairingStore.devices.first { $0.id == deviceID }?.name
                )
            }
            var compatibilityBootstrap: RemoteBootstrapSnapshot?
            var compatibilityArchive: RemoteArchivedSessionsResponse?
            var archiveProviderWasCalled = false
            var archivedSessionsByProject: [String: RemoteArchivedSessionsResponse]?
            if request.method == "GET", request.path == "/mobile/bootstrap" {
                let snapshot = try currentBootstrapSnapshot()
                compatibilityBootstrap = snapshot
            } else if request.method == "GET", request.path == "/mobile/archive" {
                // An empty map is meaningful: the route context exists, but
                // no known project matched. The shared router owns the
                // missing/unknown/known-empty distinction.
                archivedSessionsByProject = [:]
                if let projectID = normalizedProjectID(request.query["project_id"]) {
                    archiveProviderWasCalled = true
                    compatibilityArchive = try mainActorValue {
                        self.archivedSessionsProvider(projectID)
                    }
                    if let compatibilityArchive {
                        archivedSessionsByProject?[projectID] = compatibilityArchive
                    }
                }
            }
            let routeContext = try nativeRouteContext(
                bootstrap: compatibilityBootstrap,
                archivedSessionsByProject: archivedSessionsByProject
            )
            // `max_dim` is an optional native ImageIO enrichment. Keep that
            // request in Swift so it can derive an in-memory thumbnail from
            // bytes supplied by the shared Rust no-follow range reader.
            let requestsNativeThumbnail = request.method == "GET"
                && request.path == "/mobile/artifact"
                && request.query["max_dim"].flatMap(Int.init).map { $0 > 0 } == true
            if !requestsNativeThumbnail {
                switch controllerRouter.route(
                    requestID: request.requestID,
                    method: request.method,
                    path: request.path,
                    query: request.query,
                    headers: request.headers,
                    body: request.body,
                    principal: principal,
                    routeContext: routeContext
                ) {
                case .handled(let response):
                    return try adaptControllerResponse(response, for: request)
                case .unhandled:
                    break
                case .bridgeUnavailable(let message):
                    NSLog("[UnpeelNative] native controller bridge unavailable; using compatibility adapter: \(message)")
                case .bridgeError(let message):
                    // Replay safety is per operation, not per HTTP method:
                    // relay-credential recovery is a GET that rotates secrets.
                    // A mutation may already have applied before an FFI/response
                    // failure, so never duplicate it through compatibility code.
                    NSLog("[UnpeelNative] native controller bridge error: \(message)")
                    if !Self.canReplayAfterBridgeError(request) {
                        return HTTPResponse(
                            status: 500,
                            body: errorJSON("native controller bridge failed")
                        )
                    }
                }
            }

            switch (request.method, request.path) {
            case ("GET", "/mobile/bootstrap"):
                // The bootstrap is the phone's steady-state poll, so it also
                // carries the current remote control server port +
                // certificate fingerprint — already-paired phones discover
                // (and re-discover, the port changes per run) the WSS
                // endpoint here without re-pairing.
                return try jsonResponse(compatibilityBootstrap ?? currentBootstrapSnapshot())
            case ("GET", "/mobile/output"):
                return try jsonResponse(MobileSessionControl.outputChunk(query: request.query))
            case ("GET", "/mobile/metrics"):
                let base = try MobileSessionControl.metrics(query: request.query)
                let viewing = try mainActorValue {
                    self.desktopViewingProvider(base.sessionID)
                }
                return try jsonResponse(RemoteTerminalMetrics(
                    sessionID: base.sessionID,
                    columns: base.columns,
                    rows: base.rows,
                    capturedAtUnixMs: base.capturedAtUnixMs,
                    desktopViewing: viewing
                ))
            case ("GET", "/mobile/archive"):
                guard let projectID = normalizedProjectID(request.query["project_id"]) else {
                    throw MobileRemoteError(400, "project_id required")
                }
                let archived: RemoteArchivedSessionsResponse?
                if archiveProviderWasCalled {
                    archived = compatibilityArchive
                } else {
                    archived = try mainActorValue {
                        self.archivedSessionsProvider(projectID)
                    }
                }
                guard let archived else { throw MobileRemoteError(404, "unknown project") }
                return try jsonResponse(archived)
            case ("GET", "/mobile/transcript-markdown"):
                return try jsonResponse(try MobileSessionControl.transcriptMarkdown(query: request.query))
            case ("POST", "/mobile/write"):
                guard let body = terminalWrite else {
                    throw MobileRemoteError(400, "invalid write request")
                }
                try MobileSessionControl.write(
                    sessionID: body.sessionID, data: body.data, writeID: body.writeID
                )
                return HTTPResponse(status: 200, body: #"{"ok":true}"#)
            case ("POST", "/mobile/resize"):
                let body = try decoder.decode(RemoteTerminalResizeRequest.self, from: request.body)
                try MobileSessionControl.resize(
                    sessionID: body.sessionID,
                    columns: body.columns,
                    rows: body.rows
                )
                return HTTPResponse(status: 200, body: #"{"ok":true}"#)
            case ("POST", "/mobile/upload"):
                return try jsonResponse(try MobileSessionControl.saveUploadedImage(
                    sessionID: request.query["session_id"] ?? request.query["sessionID"],
                    contentType: request.headers["content-type"],
                    body: request.body
                ))
            case ("GET", "/mobile/artifacts"):
                return try jsonResponse(try MobileSessionControl.browserArtifacts(query: request.query))
            case ("POST", "/mobile/artifact-delete"):
                return try jsonResponse(try MobileSessionControl.deleteArtifact(query: request.query))
            case ("GET", "/mobile/artifact"):
                return try jsonResponse(try MobileSessionControl.browserArtifactChunk(query: request.query))
            case ("POST", "/mobile/resize-desktop"):
                let body = try decoder.decode(RemoteDesktopResizeRequest.self, from: request.body)
                try mainActorValue { try self.resizeDesktopProvider(body) }
                return HTTPResponse(status: 200, body: #"{"ok":true}"#)
            case ("POST", "/mobile/session-organization"):
                let body = try decoder.decode(RemoteSessionOrganizationPatch.self, from: request.body)
                try mainActorValue { try self.sessionOrganizationProvider(body) }
                return HTTPResponse(status: 200, body: #"{"ok":true}"#)
            case ("POST", "/mobile/project-organization"):
                let body = try decoder.decode(RemoteProjectOrganizationPatch.self, from: request.body)
                try mainActorValue { try self.projectOrganizationProvider(body) }
                return HTTPResponse(status: 200, body: #"{"ok":true}"#)
            case ("POST", "/mobile/session-order"):
                let body = try decoder.decode(RemoteSessionOrderRequest.self, from: request.body)
                try mainActorValue { try self.sessionOrderProvider(body) }
                return HTTPResponse(status: 200, body: #"{"ok":true}"#)
            case ("POST", "/mobile/restart-session"):
                return compatibilityLifecycleResponse(
                    deviceID: deviceID,
                    request: request
                ) {
                    let body = try decoder.decode(
                        RemoteRestartSessionRequest.self,
                        from: request.body
                    )
                    try mainActorValue { try self.restartSessionProvider(body) }
                    return HTTPResponse(status: 200, body: #"{"ok":true}"#)
                }
            case ("POST", "/mobile/session-action"):
                return compatibilityLifecycleResponse(
                    deviceID: deviceID,
                    request: request
                ) {
                    let body = try decoder.decode(
                        RemoteSessionActionRequest.self,
                        from: request.body
                    )
                    try mainActorAsyncValue { try await self.sessionActionProvider(body) }
                    return HTTPResponse(status: 200, body: #"{"ok":true}"#)
                }
            case ("POST", "/mobile/mark-read"):
                let body = try decoder.decode(RemoteMarkReadRequest.self, from: request.body)
                try mainActorValue { try self.markReadProvider(body) }
                return HTTPResponse(status: 200, body: #"{"ok":true}"#)
            case ("POST", "/mobile/request-screenshot"):
                let body = try decoder.decode(RemoteScreenshotRequest.self, from: request.body)
                return try jsonResponse(try requestScreenshotProvider(body))
            case ("POST", "/mobile/approvals/answer"):
                let body = try decoder.decode(RemoteApprovalAnswerRequest.self, from: request.body)
                try mainActorValue { try self.approvalAnswerProvider(body) }
                return HTTPResponse(status: 200, body: #"{"ok":true}"#)
            case ("POST", "/mobile/push-token"):
                let body = try decoder.decode(RemotePushTokenRegistration.self, from: request.body)
                let token = body.apnsToken.trimmingCharacters(in: .whitespaces)
                let env = body.environment == "production" ? "production" : "sandbox"
                guard token.range(of: "^[0-9a-fA-F]{16,200}$", options: .regularExpression) != nil
                else { throw MobileRemoteError(400, "invalid apns token") }
                pairingStore.setPushToken(deviceID: deviceID, token: token, environment: env)
                onDevicesChanged()
                return HTTPResponse(status: 200, body: #"{"ok":true}"#)
            case ("POST", "/mobile/sessions"):
                let body = try decoder.decode(RemoteCreateSessionRequest.self, from: request.body)
                return try jsonResponse(try mainActorValue {
                    try self.createSessionProvider(body)
                })
            case ("GET", "/mobile/relay-credentials"):
                // Upgrade/recovery path for Unpeel Remote: rotates the
                // calling device's relay token + E2E key and returns the
                // fresh set. Rides the same bearer-authed LAN channel that
                // delivered the device token at pairing.
                guard let credentials = pairingStore.rotateRelayCredentials(deviceID: deviceID) else {
                    throw MobileRemoteError(404, "unknown device")
                }
                return try jsonResponse(credentials)
            default:
                throw MobileRemoteError(404, "not found")
            }
        } catch let error as MobileRemoteError {
            return HTTPResponse(status: error.status, body: errorJSON(error.message))
        } catch {
            return HTTPResponse(status: 400, body: errorJSON("request failed"))
        }
    }

    /// Live status of the `unpeel-host __remote__` server (port + TLS
    /// fingerprint), nil when it is not running. Read on the main actor,
    /// best-effort — advertisement fields simply stay nil on failure.
    private func remoteServerStatus() -> RemoteControlManager.Status? {
        try? mainActorValue { RemoteControlManager.shared.status }
    }

    private func currentBootstrapSnapshot() throws -> RemoteBootstrapSnapshot {
        try mainActorValue {
            let snapshot = self.bootstrapProvider()
            guard let status = RemoteControlManager.shared.status else {
                return snapshot
            }
            return RemoteBootstrapSnapshot(
                protocolVersion: snapshot.protocolVersion,
                hostProtocol: snapshot.hostProtocol,
                macID: snapshot.macID,
                macName: snapshot.macName,
                folders: snapshot.folders,
                projects: snapshot.projects,
                presets: snapshot.presets,
                sessions: snapshot.sessions,
                capturedAtUnixMs: snapshot.capturedAtUnixMs,
                remoteServerPort: status.port,
                remoteServerCertificateFingerprint: status.fingerprint,
                experimentalWorktreesEnabled: snapshot.experimentalWorktreesEnabled,
                proEntitled: snapshot.proEntitled,
                pendingApprovals: snapshot.pendingApprovals
            )
        }
    }

    /// Adapter-resolved state crosses the stable JSON boundary as route
    /// context. Rust owns validation and response envelopes; Swift retains
    /// native sidebar/activity derivation until that state is shared too.
    private func nativeRouteContext(
        bootstrap: RemoteBootstrapSnapshot?,
        archivedSessionsByProject: [String: RemoteArchivedSessionsResponse]?
    ) throws -> Data? {
        var context: [String: Any] = [:]
        if let bootstrap {
            let snapshotData = try encoder.encode(bootstrap)
            guard let snapshotObject = try JSONSerialization.jsonObject(with: snapshotData)
                as? [String: Any]
            else {
                throw MobileRemoteError(500, "bootstrap context encoding failed")
            }
            context["bootstrap"] = [
                "snapshot": snapshotObject,
                "hostID": pairingStore.macID,
            ]
        }
        if let archivedSessionsByProject {
            var archives: [String: Any] = [:]
            for (projectID, response) in archivedSessionsByProject {
                let responseData = try encoder.encode(response)
                guard let responseObject = try JSONSerialization.jsonObject(with: responseData)
                    as? [String: Any],
                      let sessions = responseObject["sessions"] as? [Any]
                else {
                    throw MobileRemoteError(500, "archive context encoding failed")
                }
                archives[projectID] = sessions
            }
            context["archivedSessionsByProject"] = archives
        }
        guard !context.isEmpty else { return nil }
        return try JSONSerialization.data(withJSONObject: context)
    }

    private func normalizedProjectID(_ value: String?) -> String? {
        let projectID = (value ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        return projectID.isEmpty ? nil : projectID
    }

    private func adaptControllerResponse(
        _ response: NativeControllerResponse,
        for request: HTTPRequest
    ) throws -> HTTPResponse {
        if request.method == "POST",
           request.path == "/mobile/mark-read",
           response.status == 200 {
            let body = try decoder.decode(RemoteMarkReadRequest.self, from: request.body)
            try mainActorValue { try self.markReadProvider(body) }
            return HTTPResponse(status: response.status, body: response.body)
        }
        guard request.method == "GET",
              request.path == "/mobile/metrics",
              response.status == 200
        else {
            return HTTPResponse(status: response.status, body: response.body)
        }
        guard let data = response.body.data(using: .utf8),
              var object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let sessionID = object["sessionID"] as? String
        else {
            throw MobileRemoteError(500, "invalid native controller metrics response")
        }
        object["desktopViewing"] = try mainActorValue {
            self.desktopViewingProvider(sessionID)
        }
        let enriched = try JSONSerialization.data(withJSONObject: object)
        guard let body = String(data: enriched, encoding: .utf8) else {
            throw MobileRemoteError(500, "response encoding failed")
        }
        return HTTPResponse(status: response.status, body: body)
    }

    private func compatibilityLifecycleResponse(
        deviceID: String,
        request: HTTPRequest,
        operation: () throws -> HTTPResponse
    ) -> HTTPResponse {
        lifecycleReplayCache.execute(deviceID: deviceID, request: request) {
            do {
                return try operation()
            } catch let error as MobileRemoteError {
                return HTTPResponse(status: error.status, body: errorJSON(error.message))
            } catch {
                return HTTPResponse(status: 400, body: errorJSON("request failed"))
            }
        }
    }

    private static func canReplayAfterBridgeError(_ request: HTTPRequest) -> Bool {
        guard request.method == "GET" else { return false }
        return [
            "/mobile/bootstrap",
            "/mobile/output",
            "/mobile/metrics",
            "/mobile/archive",
            "/mobile/transcript-markdown",
            "/mobile/artifacts",
            "/mobile/artifact",
        ].contains(request.path)
    }

    private func mainActorValue<T>(
        _ operation: @escaping @MainActor @Sendable () throws -> T
    ) throws -> T {
        let semaphore = DispatchSemaphore(value: 0)
        let box = MobileResultBox<Result<T, Error>>()
        Task { @MainActor in
            do {
                box.set(.success(try operation()))
            } catch {
                box.set(.failure(error))
            }
            semaphore.signal()
        }
        if semaphore.wait(timeout: .now() + 20) == .timedOut {
            throw MobileRemoteError(504, "mobile bridge timed out")
        }
        switch box.get() {
        case .success(let value): return value
        case .failure(let error): throw error
        case nil: throw MobileRemoteError(504, "mobile bridge timed out")
        }
    }

    /// Keep a synchronous HTTP effect receipt while allowing an async
    /// MainActor provider to suspend around blocking Host work. The request
    /// thread still waits for the provider's exact success/failure, but the
    /// MainActor is free to render and process unrelated actions meanwhile.
    private func mainActorAsyncValue<T>(
        _ operation: @escaping @MainActor @Sendable () async throws -> T
    ) throws -> T {
        let semaphore = DispatchSemaphore(value: 0)
        let box = MobileResultBox<Result<T, Error>>()
        Task { @MainActor in
            do {
                box.set(.success(try await operation()))
            } catch {
                box.set(.failure(error))
            }
            semaphore.signal()
        }
        if semaphore.wait(timeout: .now() + 20) == .timedOut {
            throw MobileRemoteError(504, "mobile bridge timed out")
        }
        switch box.get() {
        case .success(let value): return value
        case .failure(let error): throw error
        case nil: throw MobileRemoteError(504, "mobile bridge timed out")
        }
    }

    private func jsonResponse<T: Encodable>(_ value: T) throws -> HTTPResponse {
        let data = try encoder.encode(value)
        guard let body = String(data: data, encoding: .utf8) else {
            throw MobileRemoteError(500, "response encoding failed")
        }
        return HTTPResponse(status: 200, body: body)
    }

    private func errorJSON(_ message: String) -> String {
        let data = (try? JSONSerialization.data(withJSONObject: ["error": message]))
            ?? Data(#"{"error":"request failed"}"#.utf8)
        return String(data: data, encoding: .utf8) ?? #"{"error":"request failed"}"#
    }

    @discardableResult
    private func respond(_ fd: Int32, status: Int, body: String, keepAlive: Bool) -> Bool {
        let reason: String
        switch status {
        case 200: reason = "OK"
        case 400: reason = "Bad Request"
        case 401: reason = "Unauthorized"
        case 404: reason = "Not Found"
        case 405: reason = "Method Not Allowed"
        case 409: reason = "Conflict"
        case 413: reason = "Payload Too Large"
        case 415: reason = "Unsupported Media Type"
        case 422: reason = "Unprocessable Content"
        case 429: reason = "Too Many Requests"
        case 500: reason = "Internal Server Error"
        case 504: reason = "Gateway Timeout"
        case 507: reason = "Insufficient Storage"
        default: reason = "Error"
        }
        let payload =
            "HTTP/1.1 \(status) \(reason)\r\n"
            + "Content-Type: application/json\r\n"
            + "Cache-Control: no-store\r\n"
            + "Content-Length: \(body.utf8.count)\r\n"
            + "Connection: \(keepAlive ? "keep-alive" : "close")\r\n\r\n"
            + body
        let bytes = Array(payload.utf8)
        var sent = 0
        while sent < bytes.count {
            let n = bytes.withUnsafeBufferPointer { pointer in
                send(fd, pointer.baseAddress! + sent, bytes.count - sent, 0)
            }
            guard n > 0 else { return false }
            sent += n
        }
        return true
    }

    private static func preferredLANAddress() -> String {
        var ifaddr: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&ifaddr) == 0, let first = ifaddr else {
            return "127.0.0.1"
        }
        defer { freeifaddrs(ifaddr) }

        let skippedPrefixes = ["lo", "utun", "awdl", "llw", "bridge"]
        for pointer in sequence(first: first, next: { $0.pointee.ifa_next }) {
            let interface = pointer.pointee
            guard let addr = interface.ifa_addr,
                  addr.pointee.sa_family == UInt8(AF_INET)
            else { continue }
            let name = String(cString: interface.ifa_name)
            if skippedPrefixes.contains(where: { name.hasPrefix($0) }) { continue }
            let flags = Int32(interface.ifa_flags)
            guard flags & IFF_UP != 0, flags & IFF_RUNNING != 0 else { continue }

            var address = addr.withMemoryRebound(to: sockaddr_in.self, capacity: 1) {
                $0.pointee.sin_addr
            }
            var buffer = [CChar](repeating: 0, count: Int(INET_ADDRSTRLEN))
            guard inet_ntop(AF_INET, &address, &buffer, socklen_t(INET_ADDRSTRLEN)) != nil else {
                continue
            }
            let bytes = buffer.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }
            let ip = String(decoding: bytes, as: UTF8.self)
            if !ip.hasPrefix("127.") { return ip }
        }
        return "127.0.0.1"
    }
}

enum MobileSessionControl {
    /// Truncated tail replays must not start mid-escape-sequence or
    /// mid-UTF-8 rune (garbage first rows); scan back this far for a safe
    /// boundary. Same constant as the dev bridge / Rust host.
    private static let replayAlignmentLookbackBytes = 16 * 1024

    static func outputChunk(query: [String: String]) throws -> RemoteTerminalOutputChunk {
        try outputChunk(query: query, retentionRetries: 3)
    }

    private static func outputChunk(
        query: [String: String],
        retentionRetries: Int
    ) throws -> RemoteTerminalOutputChunk {
        let sessionID = try requiredSessionID(query["session_id"] ?? query["sessionID"])
        // Honor the client's replay window (iOS asks for up to 8MB); the old
        // 1MB clamp silently shrank phone scrollback vs the dev bridge.
        let limit = max(1, min(Int(query["limit"] ?? "") ?? 512 * 1024, 8 * 1024 * 1024))
        let outputURL = sessionDir(sessionID).appendingPathComponent("output.bin")
        var size = (try? outputURL.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
        let requestedOffset = query["offset"].flatMap(UInt64.init)

        // Long-poll: a caught-up client holds until new bytes land (or the
        // wait expires) — output reaches the phone ~20ms after the TUI draws
        // instead of a poll interval later. Runs on the connection thread.
        let waitMs = min(Int(query["wait_ms"] ?? "") ?? 0, 25_000)
        if waitMs > 0, let requestedOffset, requestedOffset == UInt64(size) {
            let deadline = Date().addingTimeInterval(TimeInterval(waitMs) / 1000)
            while UInt64(size) <= requestedOffset, Date() < deadline {
                Thread.sleep(forTimeInterval: 0.02)
                // Fresh stat each pass: URL.resourceValues caches per
                // instance, which would keep this loop asleep forever.
                let attributes = try? FileManager.default.attributesOfItem(
                    atPath: outputURL.path
                )
                size = (attributes?[.size] as? Int) ?? size
            }
        }
        let logicalEnd = UInt64(size)
        let retainedFrom = outputRetainedFrom(sessionID: sessionID, logicalEnd: logicalEnd)
        let explicitOffsetIsRetained = requestedOffset.map {
            $0 >= retainedFrom && $0 <= logicalEnd
        } ?? false
        let replayTail = requestedOffset == nil || !explicitOffsetIsRetained
        let desiredTailStart = logicalEnd > UInt64(limit) ? logicalEnd - UInt64(limit) : 0
        var start = explicitOffsetIsRetained
            ? (requestedOffset ?? retainedFrom)
            : max(retainedFrom, desiredTailStart)

        var data: Data
        if FileManager.default.fileExists(atPath: outputURL.path),
           let handle = try? FileHandle(forReadingFrom: outputURL) {
            defer { try? handle.close() }
            let desiredReplayStart = start
            if replayTail {
                start = alignedReplayStart(
                    handle: handle,
                    desiredStart: start,
                    retainedFrom: retainedFrom
                )
            }
            try handle.seek(toOffset: start)
            let alignmentAllowance = replayTail ? Int(desiredReplayStart - start) : 0
            data = try handle.read(
                upToCount: min(
                    limit + alignmentAllowance,
                    max(0, size - Int(start))
                )
            ) ?? Data()
        } else {
            data = Data()
        }
        let newestSize = UInt64(
            (try? outputURL.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? size
        )
        let newestRetainedFrom = outputRetainedFrom(
            sessionID: sessionID,
            logicalEnd: newestSize
        )
        if start < newestRetainedFrom {
            // Marker publication precedes hole punching. A floor advance
            // during this lock-free read may have returned sparse NULs; retry
            // against the new generation and never feed that sample.
            if retentionRetries > 0 {
                return try outputChunk(query: query, retentionRetries: retentionRetries - 1)
            }
            return RemoteTerminalOutputChunk(
                sessionID: sessionID,
                offset: newestRetainedFrom,
                nextOffset: newestRetainedFrom,
                dataBase64: "",
                truncated: true,
                capturedAtUnixMs: MobilePairingStore.unixMs(Date())
            )
        }
        let truncated = requestedOffset.map { $0 != start } ?? (start > 0)

        // Live chunks must not END mid-sequence either: a chunk cut inside
        // a repaint bisects erase+redraw across two polls (visible flash).
        // Withhold the trailing partial bytes; nextOffset points at the
        // boundary so the client re-requests them next poll — no data loss.
        if !truncated, !data.isEmpty {
            let alignedEnd = alignedLiveChunkEnd(data)
            if alignedEnd < data.count {
                data = data.prefix(alignedEnd)
            }
        }

        return RemoteTerminalOutputChunk(
            sessionID: sessionID,
            offset: start,
            nextOffset: start + UInt64(data.count),
            dataBase64: data.base64EncodedString(),
            truncated: truncated,
            capturedAtUnixMs: MobilePairingStore.unixMs(Date())
        )
    }

    /// Latest safe end (a complete escape sequence / UTF-8 rune boundary)
    /// for a live chunk, so the client can withhold a partial tail rather
    /// than cut a chunk mid-sequence.
    ///
    /// Scans the ENTIRE chunk from its start — which is always a ground-state
    /// boundary (the previous chunk's aligned end, or the aligned replay
    /// start) — so the escape/UTF-8 state machine has correct context. A
    /// bounded 128-byte tail window (the previous approach) was wrong on two
    /// counts: it missed boundaries inside sequences longer than the window,
    /// and — by starting the state machine mid-sequence — it could return a
    /// "boundary" that is actually *inside* a CSI/OSC/DCS sequence. Either
    /// way the chunk got cut mid-sequence, and the client's injected
    /// synchronized-output bracket bytes (ESC[?2026h/l, spliced between
    /// chunks) then landed inside that unfinished sequence and corrupted it
    /// into random on-screen characters. The scan is a single linear pass
    /// over bytes already in memory — cheap even at the 200KB–512KB cap.
    ///
    /// Returns `data.count` when the end is already safe or no boundary was
    /// found (withholding would empty the chunk) — never stall.
    static func alignedLiveChunkEnd(_ data: Data) -> Int {
        guard data.count > 1 else { return data.count }
        let boundary = alignTailStart(
            in: data,
            scanStart: 0,
            desiredStart: UInt64(data.count)
        )
        guard boundary > 0 else { return data.count }
        return Int(boundary)
    }

    private static func alignedReplayStart(
        handle: FileHandle,
        desiredStart: UInt64,
        retainedFrom: UInt64
    ) -> UInt64 {
        guard desiredStart > retainedFrom else { return retainedFrom }
        let scanStart = desiredStart > UInt64(replayAlignmentLookbackBytes)
            ? max(retainedFrom, desiredStart - UInt64(replayAlignmentLookbackBytes))
            : retainedFrom
        guard (try? handle.seek(toOffset: scanStart)) != nil,
              let window = try? handle.read(upToCount: Int(desiredStart - scanStart)),
              !window.isEmpty
        else { return desiredStart }
        return alignTailStart(in: window, scanStart: scanStart, desiredStart: desiredStart)
    }

    private static func outputRetainedFrom(sessionID: String, logicalEnd: UInt64) -> UInt64 {
        let url = sessionDir(sessionID).appendingPathComponent("output-retention.json")
        guard let data = try? Data(contentsOf: url),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              (object["version"] as? NSNumber)?.uint32Value == 1,
              let retained = (object["retained_from"] as? NSNumber)?.uint64Value
        else { return 0 }
        return min(retained, logicalEnd)
    }

    /// Port of the dev bridge's `align_tail_start_in_window`: latest byte
    /// position ≤ `desiredStart` that is not inside a UTF-8 rune or an
    /// ANSI escape/CSI/OSC/DCS/SOS-PM-APC sequence.
    static func alignTailStart(in window: Data, scanStart: UInt64, desiredStart: UInt64) -> UInt64 {
        enum ScanState {
            case ground, escape, escapeIntermediate, csi, osc, oscEscape
            case dcs, dcsEscape, sosPmApc, sosPmApcEscape
        }

        let bytes = [UInt8](window)
        var initialIndex = 0
        while initialIndex < bytes.count, bytes[initialIndex] & 0b1100_0000 == 0b1000_0000 {
            initialIndex += 1
        }

        var lastBoundary = scanStart + UInt64(initialIndex)
        var state = ScanState.ground
        for index in initialIndex..<bytes.count {
            let value = bytes[index]
            let absolute = scanStart + UInt64(index)
            if value == 0x18 || value == 0x1A {
                lastBoundary = absolute + 1
                state = .ground
                continue
            }
            if value == 0x1B {
                switch state {
                case .escape, .escapeIntermediate, .csi:
                    lastBoundary = absolute
                    state = .escape
                    continue
                default:
                    break
                }
            }
            switch state {
            case .ground:
                if value == 0x1B {
                    lastBoundary = absolute
                    state = .escape
                } else if value == 0x0A || value == 0x0D {
                    lastBoundary = absolute + 1
                }
            case .escape:
                switch value {
                case UInt8(ascii: "["): state = .csi
                case UInt8(ascii: "]"): state = .osc
                case UInt8(ascii: "P"): state = .dcs
                case UInt8(ascii: "X"), UInt8(ascii: "^"), UInt8(ascii: "_"): state = .sosPmApc
                case 0x20...0x2F: state = .escapeIntermediate
                default:
                    lastBoundary = absolute + 1
                    state = .ground
                }
            case .escapeIntermediate:
                if (0x30...0x7E).contains(value) {
                    lastBoundary = absolute + 1
                    state = .ground
                }
            case .csi:
                if (0x40...0x7E).contains(value) {
                    lastBoundary = absolute + 1
                    state = .ground
                }
            case .osc:
                if value == 0x07 {
                    lastBoundary = absolute + 1
                    state = .ground
                } else if value == 0x1B {
                    state = .oscEscape
                }
            case .oscEscape:
                if value == UInt8(ascii: "\\") {
                    lastBoundary = absolute + 1
                    state = .ground
                } else {
                    state = .osc
                }
            case .dcs:
                if value == 0x1B { state = .dcsEscape }
            case .dcsEscape:
                if value == UInt8(ascii: "\\") {
                    lastBoundary = absolute + 1
                    state = .ground
                } else {
                    state = .dcs
                }
            case .sosPmApc:
                if value == 0x1B { state = .sosPmApcEscape }
            case .sosPmApcEscape:
                if value == UInt8(ascii: "\\") {
                    lastBoundary = absolute + 1
                    state = .ground
                } else {
                    state = .sosPmApc
                }
            }
        }
        return min(lastBoundary, desiredStart)
    }

    static func metrics(query: [String: String]) throws -> RemoteTerminalMetrics {
        let sessionID = try requiredSessionID(query["session_id"] ?? query["sessionID"])
        let response = try sendControl(
            sessionID: sessionID,
            payload: [
                "type": "viewport_snapshot",
                "cols": 0,
                "rows": 0,
                "scroll_offset_rows": 0,
                "viewport_rows": 1,
            ]
        )
        let viewport = response["viewport"] as? [String: Any]
        let columns = viewport?["cols"] as? Int ?? 83
        let rows = viewport?["rows"] as? Int ?? 31
        return RemoteTerminalMetrics(
            sessionID: sessionID,
            columns: max(2, min(columns, 300)),
            rows: max(2, min(rows, 120)),
            capturedAtUnixMs: MobilePairingStore.unixMs(Date())
        )
    }

    /// The session's conversation rendered as Markdown, for the phone's
    /// "Copy transcript". Same renderer as the desktop action
    /// (`unpeel-host __transcript__ markdown`, filtered by the shared
    /// Settings ▸ Transcripts options), so both copy identical content.
    static func transcriptMarkdown(query: [String: String]) throws -> RemoteTranscriptMarkdown {
        let sessionID = try requiredSessionID(query["session_id"] ?? query["sessionID"])
        // Optional range override from the phone's flyout (entry count, 0 =
        // whole conversation); absent = the Mac's Settings ▸ Transcripts range.
        let entries = query["entries"].flatMap(Int.init)
        let outcome = UnpeelStore.runTranscriptMarkdown(sessionID: sessionID, entries: entries)
        if let error = outcome.error {
            throw MobileRemoteError(502, error)
        }
        return RemoteTranscriptMarkdown(
            sessionID: sessionID,
            markdown: outcome.markdown.trimmingCharacters(in: .whitespacesAndNewlines)
        )
    }

    static func write(sessionID: String, data: String, writeID: String? = nil) throws {
        var payload: [String: Any] = ["type": "write", "data": data]
        if let writeID, !writeID.isEmpty {
            payload["write_id"] = writeID
        }
        _ = try sendControl(
            sessionID: try requiredSessionID(sessionID),
            payload: payload
        )
    }

    static func requestScreenshot(sessionID: String) throws -> RemoteScreenshotRequestResponse {
        let sessionID = try requiredSessionID(sessionID)
        let process = Process()
        process.executableURL = URL(fileURLWithPath: LaunchConfig.hostBinary)
        process.arguments = ["__request_screenshot__", sessionID]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        let stderr = Pipe()
        process.standardError = stderr
        do {
            try process.run()
        } catch {
            throw MobileRemoteError(502, "could not start the screenshot request")
        }
        let errData = stderr.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            let detail = String(data: errData, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            throw MobileRemoteError(
                404,
                detail.flatMap { $0.isEmpty ? nil : $0 } ?? "session host unavailable"
            )
        }
        return RemoteScreenshotRequestResponse(
            requestedAtUnixMs: MobilePairingStore.unixMs(Date())
        )
    }

    static func resize(sessionID: String, columns: Int, rows: Int) throws {
        _ = try sendControl(
            sessionID: try requiredSessionID(sessionID),
            payload: [
                "type": "resize",
                "cols": max(2, min(columns, 300)),
                "rows": max(2, min(rows, 120)),
            ]
        )
    }

    private static func sendControl(sessionID: String, payload: [String: Any]) throws -> [String: Any] {
        let socketURL = sessionDir(sessionID).appendingPathComponent("session.sock")
        let data = try JSONSerialization.data(withJSONObject: payload)
        guard var command = String(data: data, encoding: .utf8) else {
            throw MobileRemoteError(400, "invalid command")
        }
        command.append("\n")

        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw MobileRemoteError(500, "socket unavailable") }
        defer { close(fd) }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let pathBytes = Array(socketURL.path.utf8)
        let maxLen = MemoryLayout.size(ofValue: addr.sun_path) - 1
        guard pathBytes.count <= maxLen else {
            throw MobileRemoteError(400, "session socket path too long")
        }
        withUnsafeMutableBytes(of: &addr.sun_path) { dst in
            dst.copyBytes(from: pathBytes)
        }

        var tv = timeval(tv_sec: 2, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &tv, socklen_t(MemoryLayout<timeval>.size))
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, socklen_t(MemoryLayout<timeval>.size))

        let connected = withUnsafePointer(to: &addr) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard connected == 0 else {
            throw MobileRemoteError(404, "session host unavailable")
        }

        let sent = command.withCString { send(fd, $0, strlen($0), 0) }
        guard sent > 0 else {
            throw MobileRemoteError(500, "failed to write session command")
        }

        // One command → one newline-terminated reply per connection, so
        // read in chunks (multi-KB JSON replies would cost thousands of
        // 1-byte recv syscalls) and discard anything past the newline.
        var response = Data()
        var chunk = [UInt8](repeating: 0, count: 4096)
        while true {
            let count = recv(fd, &chunk, chunk.count, 0)
            guard count > 0 else { break }
            if let newlineIndex = chunk[0..<count].firstIndex(of: 0x0A) {
                response.append(contentsOf: chunk[0..<newlineIndex])
                break
            }
            response.append(contentsOf: chunk[0..<count])
            if response.count > 4 * 1024 * 1024 {
                throw MobileRemoteError(500, "session response too large")
            }
        }
        guard !response.isEmpty,
              let json = try JSONSerialization.jsonObject(with: response) as? [String: Any]
        else {
            throw MobileRemoteError(500, "invalid session response")
        }
        if (json["ok"] as? Bool) != true {
            throw MobileRemoteError(400, (json["error"] as? String) ?? "session command failed")
        }
        return json
    }

    /// POST /mobile/upload?session_id=…: raw image bytes from the phone. Saved
    /// into the session's `artifacts/uploads/` dir so it shows in that
    /// session's gallery (and stays attributed to it); falls back to the shared
    /// `dropped-images` dir only when no session context is supplied. Returns
    /// the Mac-side path the phone pastes into the agent's composer — an image
    /// from the phone is indistinguishable from a desktop drop by the time the
    /// agent sees it. Filename is server-generated; the client supplies none.
    static func saveUploadedImage(
        sessionID: String?,
        contentType: String?,
        body: Data
    ) throws -> [String: String] {
        guard !body.isEmpty else {
            throw MobileRemoteError(400, "empty upload")
        }
        let ext = contentType?.lowercased().contains("png") == true ? "png" : "jpg"
        let dir: URL
        if let sessionID = try? requiredSessionID(sessionID),
           let uploads = SessionArtifactStore.kindDir(sessionID, kind: "uploads") {
            dir = uploads
        } else {
            dir = LaunchConfig.unpeelDir.appendingPathComponent(
                "dropped-images",
                isDirectory: true
            )
        }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let timestamp = UInt64(Date().timeIntervalSince1970 * 1000)
        let name = "phone-\(timestamp)-\(UUID().uuidString.prefix(8)).\(ext)"
        let url = dir.appendingPathComponent(name)
        try body.write(to: url, options: .atomic)
        return ["path": url.path]
    }

    private static func requiredSessionID(_ value: String?) throws -> String {
        let id = (value ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        guard !id.isEmpty, !id.contains("/"), !id.contains("..") else {
            throw MobileRemoteError(400, "invalid session id")
        }
        return id
    }

    // MARK: - Browser MCP artifacts

    // Kind list + dir mapping live in the shared SessionArtifactStore
    // (SessionArtifacts.swift), which the desktop gallery reads too.

    /// Raw bytes per artifact chunk: sized so one chunk, base64'd and sealed,
    /// clears the relay's 512KB frame cap.
    private static let singleChunkBytes = 200 * 1024

    /// Avoid assembling pathological multi-gigabyte images merely because a
    /// Controller requested a gallery thumbnail. Larger sources fall back to
    /// the shared original-byte range response.
    private static let thumbnailSourceMaxBytes = 64 * 1024 * 1024

    /// ImageIO output is derived only from bytes already read through the
    /// Rust no-follow reader. NSCache is thread-safe and keeps repeated gallery
    /// polls from decoding the same source without creating another on-disk
    /// path that would need its own traversal/TOCTOU contract.
    private final class ThumbnailCache: @unchecked Sendable {
        private let values: NSCache<NSString, NSData> = {
            let cache = NSCache<NSString, NSData>()
            cache.countLimit = 64
            cache.totalCostLimit = 32 * 1024 * 1024
            return cache
        }()

        func value(for key: NSString) -> Data? {
            values.object(forKey: key) as Data?
        }

        func insert(_ value: Data, for key: NSString) {
            values.setObject(value as NSData, forKey: key, cost: value.count)
        }
    }

    private static let thumbnailCache = ThumbnailCache()

    private static func thumbnailData(source: Data, maxDim: Int) -> Data? {
        let dim = min(max(maxDim, 32), 1024)
        let digest = SHA256.hash(data: source)
            .map { String(format: "%02x", $0) }
            .joined()
        let cacheKey = "\(digest)-\(dim)" as NSString
        if let cached = thumbnailCache.value(for: cacheKey) {
            return cached
        }

        guard let imageSource = CGImageSourceCreateWithData(
            source as CFData,
            [kCGImageSourceShouldCache: false] as CFDictionary
        ) else { return nil }
        let options = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: dim,
        ] as CFDictionary
        guard let cgImage = CGImageSourceCreateThumbnailAtIndex(imageSource, 0, options) else {
            return nil
        }

        let output = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(
            output,
            UTType.jpeg.identifier as CFString,
            1,
            nil
        ) else { return nil }
        CGImageDestinationAddImage(
            destination,
            cgImage,
            [kCGImageDestinationLossyCompressionQuality: 0.75] as CFDictionary
        )
        guard CGImageDestinationFinalize(destination) else { return nil }
        let encoded = output as Data
        thumbnailCache.insert(encoded, for: cacheKey)
        return encoded
    }

    /// A single path segment with no traversal — the same rule the desktop
    /// applies to session ids, applied to `kind`/`name` so a crafted request
    /// can't escape the artifacts dir.
    private static func safeArtifactSegment(_ value: String?) throws -> String {
        let segment = (value ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        guard !segment.isEmpty,
              !segment.contains("/"),
              !segment.contains("\\"),
              !segment.contains("..")
        else {
            throw MobileRemoteError(400, "invalid artifact path")
        }
        return segment
    }

    /// GET /mobile/artifacts?session_id=… — the session's browser-MCP gallery
    /// (screenshots + downloads), metadata only, newest-first.
    static func browserArtifacts(query: [String: String]) throws -> RemoteBrowserArtifactList {
        let sessionID = try requiredSessionID(query["session_id"] ?? query["sessionID"])
        let artifacts = SessionArtifactStore.list(sessionID).map { artifact in
            RemoteBrowserArtifact(
                kind: artifact.kind,
                name: artifact.name,
                size: artifact.size,
                modifiedAtUnixMs: artifact.modifiedAt == .distantPast
                    ? 0 : MobilePairingStore.unixMs(artifact.modifiedAt)
            )
        }
        return RemoteBrowserArtifactList(
            sessionID: sessionID,
            artifacts: artifacts,
            capturedAtUnixMs: MobilePairingStore.unixMs(Date())
        )
    }

    /// GET /mobile/artifact?session_id=…&kind=…&name=…&offset=N&limit=M — one
    /// offset-addressed slice of an artifact's bytes. Ranged because a single
    /// screenshot far exceeds `RelayProtocol.maxFrameBytes` (512KB) once
    /// base64'd through the tunnel; the client reassembles across chunks.
    /// `max_dim=N` asks for a downscaled JPEG variant of an image artifact
    /// instead of the original bytes — the gallery grid path, so tiles don't
    /// pull multi-megabyte screenshots over the relay. The original file is
    /// never modified.
    static func browserArtifactChunk(query: [String: String]) throws -> RemoteBrowserArtifactChunk {
        guard let maxDim = Int(query["max_dim"] ?? ""), maxDim > 0 else {
            return try sharedOriginalArtifactChunk(query: query)
        }

        var metadataQuery = query
        metadataQuery.removeValue(forKey: "max_dim")
        metadataQuery["offset"] = "0"
        metadataQuery["limit"] = "1"
        let first = try sharedOriginalArtifactChunk(query: metadataQuery)

        // A file at or under one chunk is already one round trip. Non-images,
        // pathological source sizes, decode failures, and encode failures all
        // fall back through the same shared reader; Swift never reopens the
        // Controller-selected Host path.
        guard first.contentType.hasPrefix("image/"),
              first.totalSize > UInt64(singleChunkBytes),
              first.totalSize <= UInt64(thumbnailSourceMaxBytes),
              let source = try originalArtifactData(query: metadataQuery, first: first),
              let thumbnail = thumbnailData(source: source, maxDim: maxDim)
        else {
            return try sharedOriginalArtifactChunk(query: query)
        }

        let requestedLimit = query["limit"]
            .flatMap(Int.init)
            .flatMap { $0 >= 0 ? $0 : nil }
            ?? singleChunkBytes
        let limit = max(1, min(requestedLimit, singleChunkBytes))
        let start = min(
            query["offset"].flatMap(UInt64.init) ?? 0,
            UInt64(thumbnail.count)
        )
        let end = min(start + UInt64(limit), UInt64(thumbnail.count))
        let data = thumbnail.subdata(in: Int(start) ..< Int(end))
        return RemoteBrowserArtifactChunk(
            sessionID: first.sessionID,
            kind: first.kind,
            name: first.name,
            contentType: "image/jpeg",
            offset: start,
            nextOffset: end,
            totalSize: UInt64(thumbnail.count),
            dataBase64: data.base64EncodedString(),
            capturedAtUnixMs: MobilePairingStore.unixMs(Date())
        )
    }

    /// Route an original-byte request through the same Rust bridge used by
    /// authenticated HTTP/Link traffic. This helper is also the thumbnail
    /// adapter's only source of bytes; bridge failure is a hard failure, never
    /// permission to fall back to a lexical Swift path.
    private static func sharedOriginalArtifactChunk(
        query: [String: String]
    ) throws -> RemoteBrowserArtifactChunk {
        var originalQuery = query
        originalQuery.removeValue(forKey: "max_dim")
        let result = NativeControllerRouter.shared.route(
            requestID: nil,
            method: "GET",
            path: "/mobile/artifact",
            query: originalQuery,
            headers: [:],
            body: Data(),
            principal: NativeControllerPrincipal(
                deviceID: "native-thumbnail-adapter",
                name: "Native thumbnail adapter"
            ),
            routeContext: nil
        )
        switch result {
        case .handled(let response):
            guard response.status == 200 else {
                let object = response.body.data(using: .utf8).flatMap { data in
                    (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
                }
                let message = object?["error"] as? String
                throw MobileRemoteError(response.status, message ?? "artifact read failed")
            }
            guard let data = response.body.data(using: .utf8) else {
                throw MobileRemoteError(500, "invalid artifact response")
            }
            return try JSONDecoder().decode(RemoteBrowserArtifactChunk.self, from: data)
        case .unhandled:
            throw MobileRemoteError(500, "shared artifact reader unavailable")
        case .bridgeUnavailable, .bridgeError:
            throw MobileRemoteError(500, "shared artifact reader failed")
        }
    }

    /// Reassemble one immutable source through bounded shared-reader chunks.
    /// `nil` means it changed or could not make progress; the caller then
    /// serves the requested original range through Rust.
    private static func originalArtifactData(
        query: [String: String],
        first: RemoteBrowserArtifactChunk
    ) throws -> Data? {
        guard let firstBytes = Data(base64Encoded: first.dataBase64),
              first.offset == 0,
              first.nextOffset == UInt64(firstBytes.count)
        else { return nil }
        var assembled = firstBytes
        let expectedTotal = first.totalSize
        for _ in 0 ..< 4096 {
            if UInt64(assembled.count) == expectedTotal { return assembled }
            if UInt64(assembled.count) > expectedTotal { return nil }
            var chunkQuery = query
            chunkQuery["offset"] = "\(assembled.count)"
            chunkQuery["limit"] = "\(singleChunkBytes)"
            let chunk = try sharedOriginalArtifactChunk(query: chunkQuery)
            guard chunk.sessionID == first.sessionID,
                  chunk.kind == first.kind,
                  chunk.name == first.name,
                  chunk.contentType == first.contentType,
                  chunk.totalSize == expectedTotal,
                  chunk.offset == UInt64(assembled.count),
                  let bytes = Data(base64Encoded: chunk.dataBase64),
                  !bytes.isEmpty,
                  chunk.nextOffset == chunk.offset + UInt64(bytes.count)
            else { return nil }
            assembled.append(bytes)
        }
        return nil
    }

    /// POST /mobile/artifact-delete?session_id=…&kind=…&name=… — remove one
    /// gallery artifact from disk (screenshot/download/upload). Idempotent: a
    /// missing file is a no-op success. Path segments are traversal-checked.
    static func deleteArtifact(query: [String: String]) throws -> [String: String] {
        let sessionID = try requiredSessionID(query["session_id"] ?? query["sessionID"])
        let kind = try safeArtifactSegment(query["kind"])
        guard SessionArtifactStore.kindDir(sessionID, kind: kind) != nil else {
            throw MobileRemoteError(404, "unknown artifact kind")
        }
        let name = try safeArtifactSegment(query["name"])
        try SessionArtifactStore.delete(sessionID, kind: kind, name: name)
        return ["ok": "true"]
    }

    private static func sessionDir(_ sessionID: String) -> URL {
        LaunchConfig.appSessionsDir.appendingPathComponent(sessionID)
    }
}

/// Publishes the mobile remote server as `_unpeel-remote._tcp` via the
/// dnssd C API (NetService is deprecated; NWListener would have to own the
/// socket, which stays a raw BSD listener). The registration lives as long
/// as the DNSServiceRef; name conflicts auto-rename daemon-side.
final class MobileServerBonjourAdvertiser: @unchecked Sendable {
    static let serviceType = "_unpeel-remote._tcp"

    private var serviceRef: DNSServiceRef?
    private let lock = NSLock()

    init?(macID: String, macName: String, port: UInt16) {
        var txt = TXTRecordRef()
        TXTRecordCreate(&txt, 0, nil)
        defer { TXTRecordDeallocate(&txt) }
        let idBytes = Array(macID.utf8)
        guard idBytes.count <= 255 else { return nil }
        let txtStatus = idBytes.withUnsafeBufferPointer { buffer in
            TXTRecordSetValue(&txt, "macid", UInt8(buffer.count), buffer.baseAddress)
        }
        guard txtStatus == kDNSServiceErr_NoError else { return nil }

        var ref: DNSServiceRef?
        let status = DNSServiceRegister(
            &ref,
            0,
            0,
            macName,
            Self.serviceType,
            nil,
            nil,
            port.bigEndian,
            TXTRecordGetLength(&txt),
            TXTRecordGetBytesPtr(&txt),
            nil,
            nil
        )
        guard status == kDNSServiceErr_NoError, let ref else {
            NSLog("[UnpeelNative] bonjour advertise failed: \(status)")
            return nil
        }
        DNSServiceSetDispatchQueue(ref, DispatchQueue.global(qos: .utility))
        serviceRef = ref
    }

    deinit {
        stop()
    }

    func stop() {
        lock.lock()
        defer { lock.unlock() }
        if let ref = serviceRef {
            DNSServiceRefDeallocate(ref)
            serviceRef = nil
        }
    }
}

private final class MobileResultBox<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private var value: Value?

    func set(_ value: Value) {
        lock.lock()
        self.value = value
        lock.unlock()
    }

    func get() -> Value? {
        lock.lock()
        defer { lock.unlock() }
        return value
    }
}

private extension NSLock {
    func withLock<T>(_ operation: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try operation()
    }
}

private extension Data {
    func unpeelBase64URLString() -> String {
        base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}
