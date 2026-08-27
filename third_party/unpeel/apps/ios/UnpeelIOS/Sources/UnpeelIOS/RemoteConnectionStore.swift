//
//  RemoteConnectionStore.swift
//  UnpeelIOS
//
//  Owns WHICH Mac this phone talks to. Every consumer (the bootstrap store,
//  each terminal renderer) takes its RemoteMacClient from here — the app
//  never constructs a client anywhere else, so endpoint/token can only ever
//  diverge in one place.
//
//  The phone can be paired with several Macs at once. Records live in a
//  macID-keyed collection with one ACTIVE Mac at a time; switching re-points
//  `mode`/`client` and bumps `epoch` so every consumer reloads. Credentials
//  (bearer token + relay credentials) are stored per macID in the Keychain.
//
//  Two modes:
//  - paired: the production path. The QR in Unpeel's Settings encodes a
//    RemotePairingPayload; POST /mobile/pair exchanges its one-time token
//    for a per-device bearer token, persisted in the Keychain. The same
//    token is honored by the Rust `unpeel-host __remote__` server, so
//    migrating terminal I/O there later needs no re-pair.
//  - devBridge: simulator/dev fallback — the localhost Python bridge,
//    unauthenticated. Compiled out of release device builds.
//

import Foundation
import Security
import SwiftUI
import UnpeelShared
#if os(iOS)
import UIKit
#endif

/// Persisted pairing record. Everything except the bearer token, which
/// lives in the Keychain (`RemoteKeychain`).
struct PairedMacRecord: Codable, Equatable {
    var macID: String
    var macName: String
    var endpoint: URL
    var deviceID: String
    var pairedAtUnixMs: Int64
}

enum KeychainReadResult<Value> {
    case found(Value)
    case notFound
    case temporarilyUnavailable(OSStatus)

    var value: Value? {
        guard case .found(let value) = self else { return nil }
        return value
    }

    var unavailableStatus: OSStatus? {
        guard case .temporarilyUnavailable(let status) = self else { return nil }
        return status
    }

    func map<Output>(_ transform: (Value) -> Output) -> KeychainReadResult<Output> {
        switch self {
        case .found(let value): .found(transform(value))
        case .notFound: .notFound
        case .temporarilyUnavailable(let status): .temporarilyUnavailable(status)
        }
    }
}

extension KeychainReadResult: Equatable where Value: Equatable {}

enum RemoteKeychain {
    private static let service = "com.unpeel.ios.remote"
    // Pre-multi-Mac accounts (one fixed slot each). Read once by the storage
    // migration, then deleted.
    private static let legacyTokenAccount = "mac-auth-token"
    private static let legacyRelayAccount = "mac-relay-credentials"

    private static func tokenAccount(macID: String) -> String {
        "mac-auth-token.\(macID)"
    }

    private static func relayAccount(macID: String) -> String {
        "mac-relay-credentials.\(macID)"
    }

    private static func baseQuery(account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }

    @discardableResult
    private static func saveData(_ data: Data, account: String) -> Bool {
        let query = baseQuery(account: account)
        // Device-only, unlocked: these credentials grant command execution
        // on the Mac — they must not ride iCloud Keychain onto other devices.
        // Updating in place preserves the old value if Security rejects the
        // operation. Delete-then-add used to erase the only credential before
        // callers learned that the replacement write had failed.
        let updateAttributes: [String: Any] = [kSecValueData as String: data]
        var status = SecItemUpdate(query as CFDictionary, updateAttributes as CFDictionary)
        if status == errSecItemNotFound {
            var insert = query
            insert[kSecValueData as String] = data
            insert[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
            status = SecItemAdd(insert as CFDictionary, nil)
        }
        if status != errSecSuccess {
            NSLog("[UnpeelIOS] keychain save failed: \(status)")
            return false
        }
        return true
    }

    private static func loadData(account: String) -> KeychainReadResult<Data> {
        var query = baseQuery(account: account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        switch status {
        case errSecSuccess:
            guard let data = result as? Data else {
                NSLog("[UnpeelIOS] keychain read returned unexpected data")
                return .temporarilyUnavailable(errSecDecode)
            }
            return .found(data)
        case errSecItemNotFound:
            return .notFound
        default:
            // In particular, errSecInteractionNotAllowed/errSecNotAvailable
            // are normal during a locked/background cold launch. Never turn
            // an inconclusive protected-data read into destructive unpairing.
            NSLog("[UnpeelIOS] keychain read temporarily unavailable: \(status)")
            return .temporarilyUnavailable(status)
        }
    }

    @discardableResult
    static func saveToken(_ token: String, macID: String) -> Bool {
        guard !token.isEmpty else {
            NSLog("[UnpeelIOS] refusing empty Mac bearer token")
            return false
        }
        return saveData(Data(token.utf8), account: tokenAccount(macID: macID))
    }

    static func tokenReadResult(macID: String) -> KeychainReadResult<String> {
        loadData(account: tokenAccount(macID: macID)).map {
            String(decoding: $0, as: UTF8.self)
        }
    }

    static func loadToken(macID: String) -> String? {
        guard let token = tokenReadResult(macID: macID).value, !token.isEmpty else {
            return nil
        }
        return token
    }

    static func deleteToken(macID: String) {
        SecItemDelete(baseQuery(account: tokenAccount(macID: macID)) as CFDictionary)
    }

    /// Unpeel Remote credentials (relay token + E2E key) — same protection
    /// class as the bearer token.
    enum RelayCredentialState: Equatable {
        case missing
        case invalid
        case available(RelayCredentials)
        case temporarilyUnavailable(OSStatus)

        var credentials: RelayCredentials? {
            guard case .available(let credentials) = self else { return nil }
            return credentials
        }

        var unavailableStatus: OSStatus? {
            guard case .temporarilyUnavailable(let status) = self else { return nil }
            return status
        }
    }

    static func isValid(_ credentials: RelayCredentials, expectedMacID: String) -> Bool {
        credentials.macID == expectedMacID
            && credentials.relayURL.scheme?.lowercased() == "wss"
            && credentials.relayURL.host != nil
            && !credentials.relayToken.isEmpty
            && credentials.e2eKey?.count == 32
    }

    /// Pure decoder used by the live Keychain reader and deterministic tests.
    /// A decodable blob is not necessarily usable: old/corrupt entries can
    /// carry a wrong Host id, URL, token, or E2E key length.
    static func relayCredentialState(
        data: Data?,
        expectedMacID: String
    ) -> RelayCredentialState {
        guard let data else { return .missing }
        guard let credentials = try? JSONDecoder().decode(RelayCredentials.self, from: data),
              isValid(credentials, expectedMacID: expectedMacID)
        else { return .invalid }
        return .available(credentials)
    }

    static func relayCredentialState(macID: String) -> RelayCredentialState {
        switch loadData(account: relayAccount(macID: macID)) {
        case .found(let data):
            return relayCredentialState(data: data, expectedMacID: macID)
        case .notFound:
            return .missing
        case .temporarilyUnavailable(let status):
            return .temporarilyUnavailable(status)
        }
    }

    /// Serialize and persist one validated credential set. Returning the
    /// writer's result is load-bearing: callers must never claim Relay is
    /// ready after a failed Keychain write. The injected writer keeps this
    /// failure path testable without touching the simulator Keychain.
    static func persistRelayCredentials(
        _ credentials: RelayCredentials,
        expectedMacID: String,
        writer: (Data) -> Bool
    ) -> Bool {
        guard isValid(credentials, expectedMacID: expectedMacID) else {
            NSLog("[UnpeelIOS] refusing invalid relay credentials")
            return false
        }
        guard let data = try? JSONEncoder().encode(credentials) else {
            NSLog("[UnpeelIOS] relay credential encoding failed")
            return false
        }
        guard writer(data) else {
            NSLog("[UnpeelIOS] relay credential persistence failed")
            return false
        }
        return true
    }

    @discardableResult
    static func saveRelayCredentials(_ credentials: RelayCredentials, macID: String) -> Bool {
        persistRelayCredentials(credentials, expectedMacID: macID) { data in
            saveData(data, account: relayAccount(macID: macID))
        }
    }

    static func loadRelayCredentials(macID: String) -> RelayCredentials? {
        relayCredentialState(macID: macID).credentials
    }

    static func deleteRelayCredentials(macID: String) {
        SecItemDelete(baseQuery(account: relayAccount(macID: macID)) as CFDictionary)
    }

    // MARK: - Legacy single-Mac accounts (migration only)

    static func legacyTokenReadResult() -> KeychainReadResult<String> {
        loadData(account: legacyTokenAccount).map { String(decoding: $0, as: UTF8.self) }
    }

    static func legacyRelayCredentialState() -> RelayCredentialState {
        switch loadData(account: legacyRelayAccount) {
        case .found(let data):
            guard let credentials = try? JSONDecoder().decode(
                RelayCredentials.self,
                from: data
            ) else { return .invalid }
            return .available(credentials)
        case .notFound:
            return .missing
        case .temporarilyUnavailable(let status):
            return .temporarilyUnavailable(status)
        }
    }

    static func deleteLegacyItems() {
        SecItemDelete(baseQuery(account: legacyTokenAccount) as CFDictionary)
        SecItemDelete(baseQuery(account: legacyRelayAccount) as CFDictionary)
    }
}

/// Versioned freshness marker for per-Mac Relay credentials. Build 12 and
/// earlier have no marker, so their existing pairing and stable device ids
/// stay intact while the next healthy authenticated Direct connection rotates
/// and re-saves the Relay secret once. A failed Relay handshake clears the
/// marker; returning to Direct repairs it without a re-pair.
enum RelayCredentialRefreshMarker {
    private static let currentVersion = 1
    private static let recoveryUnavailableVersion = -1
    private static let keyPrefix = "unpeel.ios.relayCredentialVersion."

    private static func key(macID: String) -> String { keyPrefix + macID }

    static func isCurrent(macID: String, defaults: UserDefaults = .standard) -> Bool {
        defaults.integer(forKey: key(macID: macID)) == currentVersion
    }

    static func markCurrent(macID: String, defaults: UserDefaults = .standard) {
        defaults.set(currentVersion, forKey: key(macID: macID))
    }

    /// The selected Host is healthy over Direct but does not implement the
    /// recovery route (current headless Hosts return 404). Keep using the
    /// structurally valid credential rather than disabling Link or retrying
    /// the mutating GET every minute. A Relay failure clears this marker.
    static func markRecoveryUnavailable(macID: String, defaults: UserDefaults = .standard) {
        defaults.set(recoveryUnavailableVersion, forKey: key(macID: macID))
    }

    static func markStale(macID: String, defaults: UserDefaults = .standard) {
        defaults.removeObject(forKey: key(macID: macID))
    }

    static func needsRefresh(
        macID: String,
        state: RemoteKeychain.RelayCredentialState,
        defaults: UserDefaults = .standard
    ) -> Bool {
        if case .temporarilyUnavailable = state { return false }
        guard case .available = state else { return true }
        let version = defaults.integer(forKey: key(macID: macID))
        return version != currentVersion && version != recoveryUnavailableVersion
    }
}

enum RelayCredentialRepairOutcome: Equatable {
    case refreshed
    case recoveryUnavailable
    case fetchFailed
    case invalidResponse
    case persistenceFailed
}

/// Identity of the exact client generation that produced a successful
/// bootstrap. The refresh loop captures this before awaiting the poll; using
/// `store.client` afterwards can accidentally attribute stale Mac A success
/// to a newly adopted Mac B client.
public struct RemoteConnectionPollProof: Sendable {
    public let client: RemoteMacClient
    public let connectionEpoch: Int
    public let hostMacID: String?

    public init(
        client: RemoteMacClient,
        connectionEpoch: Int,
        hostMacID: String?
    ) {
        self.client = client
        self.connectionEpoch = connectionEpoch
        self.hostMacID = hostMacID
    }
}

/// A bootstrap completion must distinguish a failure of the current client
/// from a reply belonging to a superseded generation. The latter is inert:
/// sticky disconnected state from Mac A must not trigger Relay fallback for
/// newly selected Mac B before B has completed its own Direct poll.
public enum RemoteConnectionPollResult: Sendable {
    case success(RemoteConnectionPollProof)
    case currentFailure
    case superseded
}

/// Exact identity of one Direct client generation. `macID` alone is not a
/// sufficient guard: re-pairing the same Mac rotates its bearer and bumps the
/// connection epoch while an older request may still be suspended.
struct RemoteDirectClientGeneration: Equatable {
    let epoch: Int
    let macID: String
    let endpoint: URL
    let authToken: String

    static func capture(
        candidate: RemoteMacClient,
        activeClient: RemoteMacClient,
        activeRecord: PairedMacRecord,
        activeToken: String,
        epoch: Int
    ) -> RemoteDirectClientGeneration? {
        guard !candidate.isRelay,
              !activeClient.isRelay,
              candidate.baseURL == activeRecord.endpoint,
              activeClient.baseURL == activeRecord.endpoint,
              candidate.authToken == activeToken,
              activeClient.authToken == activeToken
        else { return nil }
        return RemoteDirectClientGeneration(
            epoch: epoch,
            macID: activeRecord.macID,
            endpoint: activeRecord.endpoint,
            authToken: activeToken
        )
    }

    func matches(
        epoch: Int,
        activeClient: RemoteMacClient,
        activeRecord: PairedMacRecord,
        activeToken: String
    ) -> Bool {
        self.epoch == epoch
            && macID == activeRecord.macID
            && endpoint == activeRecord.endpoint
            && authToken == activeToken
            && !activeClient.isRelay
            && activeClient.baseURL == endpoint
            && activeClient.authToken == authToken
    }
}

/// Exact identity of one Relay-backed client generation. The actor identity
/// closes the same-Mac ABA case where an old Relay→Direct probe returns after
/// re-pair and after the newer generation has itself moved back onto Relay.
struct RemoteRelayClientGeneration: Equatable {
    let epoch: Int
    let macID: String
    let endpoint: URL
    let authToken: String
    let relayIdentity: ObjectIdentifier

    static func capture(
        activeClient: RemoteMacClient,
        activeRecord: PairedMacRecord,
        activeToken: String,
        epoch: Int
    ) -> RemoteRelayClientGeneration? {
        guard let relay = activeClient.relay,
              activeClient.baseURL == activeRecord.endpoint,
              activeClient.authToken == activeToken
        else { return nil }
        return RemoteRelayClientGeneration(
            epoch: epoch,
            macID: activeRecord.macID,
            endpoint: activeRecord.endpoint,
            authToken: activeToken,
            relayIdentity: ObjectIdentifier(relay)
        )
    }

    func matches(
        epoch: Int,
        activeClient: RemoteMacClient,
        activeRecord: PairedMacRecord,
        activeToken: String
    ) -> Bool {
        guard let relay = activeClient.relay else { return false }
        return self.epoch == epoch
            && macID == activeRecord.macID
            && endpoint == activeRecord.endpoint
            && authToken == activeToken
            && activeClient.baseURL == endpoint
            && activeClient.authToken == authToken
            && ObjectIdentifier(relay) == relayIdentity
    }
}

enum RemoteDirectRestore {
    /// Probe and adopt within one MainActor operation. There is no suspension
    /// between the final generation check and `adopt`, so a stale result can
    /// never overwrite the newly active Relay generation.
    @MainActor
    static func attempt(
        generation: RemoteRelayClientGeneration,
        isStillCurrent: (RemoteRelayClientGeneration) -> Bool,
        probe: () async -> RemoteBootstrapSnapshot?,
        adopt: () -> Void
    ) async -> Bool {
        guard isStillCurrent(generation), let snapshot = await probe() else {
            return false
        }
        if let probedMacID = snapshot.macID, probedMacID != generation.macID {
            NSLog("[UnpeelIOS] discarded Direct restore response from unexpected Host")
            return false
        }
        guard isStillCurrent(generation) else {
            NSLog("[UnpeelIOS] discarded Direct restore for superseded Relay client")
            return false
        }
        adopt()
        return true
    }
}

struct BoundRelayCredentialRepairResult: Equatable {
    let generation: RemoteDirectClientGeneration
    let outcome: RelayCredentialRepairOutcome
}

/// Relay failures may take longer than the cooldown itself. Derive the next
/// attempt from completion time so a slow failure cannot immediately retry.
enum RelayFallbackRetryPolicy {
    static let failureDelay: TimeInterval = 12

    static func canAttempt(now: Date, retryAfter: Date) -> Bool {
        now >= retryAfter
    }

    static func retryAfterFailure(completedAt: Date) -> Date {
        completedAt.addingTimeInterval(failureDelay)
    }
}

/// One authenticated Direct credential-repair attempt. Kept separate from
/// the connection store so missing/stale/write-failure behavior is completely
/// deterministic in the iOS unit suite.
enum RelayCredentialRepair {
    @MainActor
    private static func evaluating(
        _ result: Result<RelayCredentials, Error>,
        expectedMacID: String,
        persist: (RelayCredentials) -> Bool
    ) -> RelayCredentialRepairOutcome {
        switch result {
        case .failure(let error as RemoteMacClientError) where error.statusCode == 404:
            NSLog("[UnpeelIOS] selected Host does not support relay credential recovery")
            return .recoveryUnavailable
        case .failure(let error):
            NSLog("[UnpeelIOS] relay credential refresh failed: \(error.localizedDescription)")
            return .fetchFailed
        case .success(let credentials):
            guard RemoteKeychain.isValid(credentials, expectedMacID: expectedMacID) else {
                NSLog("[UnpeelIOS] relay credential refresh returned invalid credentials")
                return .invalidResponse
            }
            guard persist(credentials) else {
                // The Host may already have rotated. Do not keep advertising
                // the now-ambiguous old Keychain value as ready.
                NSLog("[UnpeelIOS] refreshed relay credentials could not be persisted")
                return .persistenceFailed
            }
            return .refreshed
        }
    }

    @MainActor
    private static func fetching(
        _ fetch: () async throws -> RelayCredentials
    ) async -> Result<RelayCredentials, Error> {
        do {
            return .success(try await fetch())
        } catch {
            return .failure(error)
        }
    }

    @MainActor
    static func attempt(
        expectedMacID: String,
        fetch: () async throws -> RelayCredentials,
        persist: (RelayCredentials) -> Bool
    ) async -> RelayCredentialRepairOutcome {
        evaluating(
            await fetching(fetch),
            expectedMacID: expectedMacID,
            persist: persist
        )
    }

    /// A refresh route rotates credentials, so a stale successful poll from
    /// Mac A must never enter it after the connection store switched to Mac B.
    /// Value identity here is the exact Direct endpoint + per-Host bearer;
    /// both the passed poll client and the store's current client must match
    /// the active record before `fetch` is evaluated.
    @MainActor
    static func attemptIfBoundToActiveDirectClient(
        candidate: RemoteMacClient,
        activeClient: RemoteMacClient,
        activeRecord: PairedMacRecord,
        activeToken: String,
        connectionEpoch: Int,
        isStillCurrent: (RemoteDirectClientGeneration) -> Bool,
        onSupersededAfterFetch: (RemoteDirectClientGeneration) -> Void = { _ in },
        fetch: () async throws -> RelayCredentials,
        persist: (RelayCredentials) -> Bool
    ) async -> BoundRelayCredentialRepairResult? {
        guard let generation = RemoteDirectClientGeneration.capture(
            candidate: candidate,
            activeClient: activeClient,
            activeRecord: activeRecord,
            activeToken: activeToken,
            epoch: connectionEpoch
        ), isStillCurrent(generation)
        else {
            NSLog("[UnpeelIOS] skipped relay credential refresh for stale Direct client")
            return nil
        }

        let fetchResult = await fetching(fetch)

        // The recovery route rotates the Host credential before replying.
        // Re-check after the suspension and immediately before Keychain
        // persistence so a same-Mac re-pair cannot be overwritten by the old
        // generation's response.
        guard isStillCurrent(generation) else {
            onSupersededAfterFetch(generation)
            NSLog("[UnpeelIOS] discarded relay credential response for superseded Direct client")
            return nil
        }
        return BoundRelayCredentialRepairResult(
            generation: generation,
            outcome: evaluating(
                fetchResult,
                expectedMacID: generation.macID,
                persist: persist
            )
        )
    }
}

/// Pure collection semantics for the paired-Mac list — no Keychain or
/// UserDefaults, so the invariants are unit-testable.
enum PairedMacCollection {
    /// Replace the record with the same macID in place (preserving list
    /// order), or append when it's a new Mac.
    static func upserting(
        _ records: [PairedMacRecord],
        with record: PairedMacRecord
    ) -> [PairedMacRecord] {
        var out = records
        if let index = out.firstIndex(where: { $0.macID == record.macID }) {
            out[index] = record
        } else {
            out.append(record)
        }
        return out
    }

    static func removing(_ records: [PairedMacRecord], macID: String) -> [PairedMacRecord] {
        records.filter { $0.macID != macID }
    }
}

struct PairedMacHydrationResult {
    let records: [PairedMacRecord]
    let activeRecord: PairedMacRecord?
    let activeToken: String?
    let unavailableStatuses: [OSStatus]

    var isTemporarilyUnavailable: Bool { !unavailableStatuses.isEmpty }
}

/// Resolve persisted records against typed Keychain reads. Only an explicit
/// `errSecItemNotFound` prunes a record; protected-data failures retain its
/// order, active identity, and stable device id for unlock-time hydration.
enum PairedMacHydration {
    static func resolve(
        records: [PairedMacRecord],
        preferredActiveMacID: String?,
        readToken: (String) -> KeychainReadResult<String>
    ) -> PairedMacHydrationResult {
        var retained: [PairedMacRecord] = []
        var tokens: [String: String] = [:]
        var unavailableStatuses: [OSStatus] = []
        for record in records {
            switch readToken(record.macID) {
            case .found(let token) where !token.isEmpty:
                retained.append(record)
                tokens[record.macID] = token
            case .found:
                // An empty bearer is conclusively unusable, but it is not an
                // `errSecItemNotFound` read. Keep the stable pairing record so
                // an inconclusive/corrupt Keychain value is never rewritten as
                // a destructive unpair operation.
                retained.append(record)
            case .notFound:
                break
            case .temporarilyUnavailable(let status):
                retained.append(record)
                unavailableStatuses.append(status)
            }
        }
        let active = retained.first(where: { $0.macID == preferredActiveMacID })
            ?? retained.first
        return PairedMacHydrationResult(
            records: retained,
            activeRecord: active,
            activeToken: active.flatMap { tokens[$0.macID] },
            unavailableStatuses: unavailableStatuses
        )
    }

    /// Deterministic unlock-time adoption seam. A previously unavailable
    /// bearer creates one new Direct client generation; the persisted record
    /// (including its stable device id) is reused exactly and no pairing
    /// exchange is involved.
    static func directActivation(
        from hydration: PairedMacHydrationResult,
        currentEpoch: Int
    ) -> (record: PairedMacRecord, client: RemoteMacClient, epoch: Int)? {
        guard let record = hydration.activeRecord,
              let token = hydration.activeToken,
              !token.isEmpty
        else { return nil }
        return (
            record,
            RemoteMacClient(baseURL: record.endpoint, authToken: token),
            currentEpoch &+ 1
        )
    }
}

enum LegacyPairingMigrationResult: Equatable {
    case noLegacyRecord
    case completed
    case retryNeeded
    case temporarilyUnavailable(OSStatus)

    var needsRetry: Bool {
        switch self {
        case .retryNeeded, .temporarilyUnavailable:
            return true
        case .noLegacyRecord, .completed:
            return false
        }
    }
}

enum RemotePairingPresentationPolicy {
    static func needsPairing(
        isPaired: Bool,
        keychainHydrationPending: Bool,
        devBridgeAvailable: Bool
    ) -> Bool {
        !isPaired && !keychainHydrationPending && !devBridgeAvailable
    }
}

struct PreparedRemotePairingCommit {
    let record: PairedMacRecord
    let records: [PairedMacRecord]
    let relayCredentialsSaved: Bool
}

/// Prepare the local half of pairing before any in-memory/defaults state is
/// activated. The Host commits and rotates the bearer while answering `/pair`;
/// if the replacement bearer cannot reach Keychain, claiming success would
/// leave an existing phone record pointing at access the Host has invalidated.
enum RemotePairingCommit {
    static func prepare(
        response: RemotePairingResponse,
        existingRecords: [PairedMacRecord],
        saveToken: (String, String) -> Bool = RemoteKeychain.saveToken(_:macID:),
        saveRelayCredentials: (RelayCredentials, String) -> Bool = RemoteKeychain.saveRelayCredentials(_:macID:)
    ) throws -> PreparedRemotePairingCommit {
        let record = PairedMacRecord(
            macID: response.macID,
            macName: response.macName,
            endpoint: response.endpoint,
            deviceID: response.deviceID,
            pairedAtUnixMs: response.pairedAtUnixMs
        )
        guard !response.authToken.isEmpty,
              saveToken(response.authToken, record.macID)
        else {
            throw PairingError(
                "Couldn’t save the new Mac access securely. The Mac has already replaced "
                    + "this phone’s previous access, so generate a fresh pairing code and try again."
            )
        }
        let relayCredentialsSaved = saveRelayCredentials(
            response.relayCredentials,
            record.macID
        )
        return PreparedRemotePairingCommit(
            record: record,
            records: PairedMacCollection.upserting(existingRecords, with: record),
            relayCredentialsSaved: relayCredentialsSaved
        )
    }
}

@MainActor
final class RemoteConnectionStore: ObservableObject {
    enum Mode: Equatable {
        /// Simulator/dev fallback: localhost dev bridge, unauthenticated.
        case devBridge
        /// Paired with a real Mac over the LAN.
        case paired(PairedMacRecord)
    }

    @Published private(set) var mode: Mode
    @Published private(set) var client: RemoteMacClient
    /// Bumped whenever the client identity changes; views that capture the
    /// client at creation (terminal renderers) key their identity on this.
    @Published private(set) var epoch = 0
    @Published var pairingSheetPresented = false
    @Published private(set) var lastPairingError: String?

    /// Every Mac this phone is paired with, in pairing order.
    @Published private(set) var pairedMacs: [PairedMacRecord] = []
    /// macID of the Mac `mode`/`client` currently point at.
    @Published private(set) var activeMacID: String?

    nonisolated private static let recordsKey = "unpeel.ios.pairedMacs"
    nonisolated private static let activeMacIDKey = "unpeel.ios.activeMacID"
    nonisolated private static let legacyRecordKey = "unpeel.ios.pairedMac"
    nonisolated private static let deviceIDKey = "unpeel.ios.deviceID"

    /// True while `client` tunnels through Unpeel Remote instead of the LAN.
    @Published private(set) var usingRelay = false
    /// Cheap gate so the per-poll credential upgrade never touches the
    /// Keychain unnecessarily. Per-Mac: reloaded on every active-Mac change.
    private var hasRelayCredentials = false
    private var relayCredentialsNeedRefresh = true
    private var relayCredentialFetchInFlight = false
    private var relayCredentialFetchRetryAfter = Date.distantPast
    private var relayFallbackRetryAfter = Date.distantPast
    /// A protected-data read was inconclusive, or an idempotent legacy copy
    /// could not finish. While true we preserve pairing identity, suppress the
    /// pairing sheet, and retry synchronously when iOS makes protected data
    /// available/returns the app to the foreground.
    private var keychainHydrationPending = false

    /// Credential rotation is a mutating GET. A failed response may be
    /// ambiguous, and retrying on every 2-second bootstrap poll would churn
    /// the Host record and Keychain. Keep attempts bounded while Direct is
    /// healthy; a later poll repairs automatically.
    private static let relayCredentialRetryDelay: TimeInterval = 60
    private static let relayCredentialUnavailableRetryDelay: TimeInterval = 15 * 60

    /// Whether the dev-bridge fallback is available at all. Release device
    /// builds must pair — localhost is the phone itself there.
    static var devBridgeAvailable: Bool {
        #if targetEnvironment(simulator)
        return true
        #else
        return false
        #endif
    }

    init() {
        // Initialize a harmless client before hydration; a conclusive bearer
        // read below replaces it with the exact saved Direct generation.
        mode = .devBridge
        client = RemoteMacClient(authToken: Self.devBridgeToken())
        let migration = Self.migrateLegacyStorageIfNeeded()
        hydrateStoredPairings(
            migrationResult: migration,
            bumpEpochOnAdoption: false
        )
    }

    private func hydrateStoredPairings(
        migrationResult: LegacyPairingMigrationResult,
        bumpEpochOnAdoption: Bool
    ) {
        let loaded = Self.loadRecords()
        let storedActiveID = UserDefaults.standard.string(forKey: Self.activeMacIDKey)
        let hydration = PairedMacHydration.resolve(
            records: loaded,
            preferredActiveMacID: storedActiveID,
            readToken: RemoteKeychain.tokenReadResult(macID:)
        )
        // Only explicit errSecItemNotFound records are absent from `records`.
        // Never persist pruning for interaction-not-allowed/not-available.
        if hydration.records.count != loaded.count {
            Self.saveRecords(hydration.records)
        }
        pairedMacs = hydration.records
        var remainsPending = migrationResult.needsRetry || hydration.isTemporarilyUnavailable

        guard let record = hydration.activeRecord else {
            if remainsPending {
                // A legacy-only pairing can be temporarily unavailable before
                // its scoped record exists. Preserve any previously selected
                // identity/default and wait; do not claim the user is unpaired.
                activeMacID = storedActiveID
            } else {
                let changed = activeMacID != nil || {
                    if case .paired = mode { return true }
                    return false
                }()
                activeMacID = nil
                UserDefaults.standard.removeObject(forKey: Self.activeMacIDKey)
                usingRelay = false
                hasRelayCredentials = false
                relayCredentialsNeedRefresh = true
                mode = .devBridge
                client = RemoteMacClient(authToken: Self.devBridgeToken())
                if bumpEpochOnAdoption && changed {
                    epoch &+= 1
                    RemoteServerDiscovery.shared.clear()
                }
            }
            keychainHydrationPending = remainsPending
            return
        }

        activeMacID = record.macID
        if storedActiveID != record.macID {
            UserDefaults.standard.set(record.macID, forKey: Self.activeMacIDKey)
        }
        guard let activation = PairedMacHydration.directActivation(
            from: hydration,
            currentEpoch: epoch
        ) else {
            // Empty bearer data is unusable but not equivalent to an explicit
            // missing Keychain item. Retain the record and active/device IDs;
            // protected-data failures keep the pairing sheet suppressed.
            keychainHydrationPending = remainsPending
            return
        }

        let alreadyActive: Bool = {
            guard case .paired(let currentRecord) = mode else { return false }
            return currentRecord == activation.record
                && client.baseURL == activation.client.baseURL
                && client.authToken == activation.client.authToken
        }()
        if !alreadyActive {
            mode = .paired(activation.record)
            client = activation.client
            usingRelay = false
            relayCredentialFetchRetryAfter = .distantPast
            relayFallbackRetryAfter = .distantPast
            if bumpEpochOnAdoption {
                epoch = activation.epoch
                RemoteServerDiscovery.shared.clear()
            }
        }

        let credentialState = RemoteKeychain.relayCredentialState(macID: record.macID)
        switch credentialState {
        case .temporarilyUnavailable:
            remainsPending = true
            if !alreadyActive {
                // Do not arm fallback or rotate credentials from an
                // inconclusive read. Unlock-time hydration will re-evaluate.
                hasRelayCredentials = false
                relayCredentialsNeedRefresh = false
            }
        case .available, .missing, .invalid:
            hasRelayCredentials = credentialState.credentials != nil
            relayCredentialsNeedRefresh = RelayCredentialRefreshMarker.needsRefresh(
                macID: record.macID,
                state: credentialState
            )
            if case .invalid = credentialState {
                NSLog("[UnpeelIOS] stored relay credentials are invalid; Direct will repair them")
            }
        }
        keychainHydrationPending = remainsPending
    }

    /// Re-run only the deferred reads/copies after unlock/foreground. A
    /// successful bearer hydration adopts the existing record as Direct and
    /// bumps one connection epoch; it never invokes the pairing endpoint.
    func retryKeychainHydrationIfNeeded() {
        guard keychainHydrationPending else { return }
        let migration = Self.migrateLegacyStorageIfNeeded()
        hydrateStoredPairings(
            migrationResult: migration,
            bumpEpochOnAdoption: true
        )
    }

    /// The dev bridge requires a bearer token; it writes one to
    /// `~/.unpeel/dev-bridge-token` on the Mac, and the Simulator shares the
    /// host filesystem (SIMULATOR_HOST_HOME), so the app reads it directly —
    /// no copy-paste configuration.
    private static func devBridgeToken() -> String? {
        #if targetEnvironment(simulator)
        guard let hostHome = ProcessInfo.processInfo.environment["SIMULATOR_HOST_HOME"] else {
            return nil
        }
        let path = hostHome + "/.unpeel/dev-bridge-token"
        guard let raw = try? String(contentsOfFile: path, encoding: .utf8) else { return nil }
        let token = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        return token.isEmpty ? nil : token
        #else
        return nil
        #endif
    }

    var pairedMacName: String? {
        if case .paired(let record) = mode { return record.macName }
        return nil
    }

    /// A device build with no paired Mac has nothing to talk to.
    var needsPairing: Bool {
        let isPaired: Bool
        if case .paired = mode {
            isPaired = true
        } else {
            isPaired = false
        }
        return RemotePairingPresentationPolicy.needsPairing(
            isPaired: isPaired,
            keychainHydrationPending: keychainHydrationPending,
            devBridgeAvailable: Self.devBridgeAvailable
        )
    }

    /// Exchange a scanned/pasted pairing payload for a device token. The new
    /// Mac is upserted into the collection and becomes active immediately.
    func completePairing(with payload: RemotePairingPayload) async throws {
        let response: RemotePairingResponse
        do {
            response = try await RemoteMacClient().pair(
                payload: payload,
                device: Self.deviceIdentity()
            )
        } catch RemotePairingClientError.expired {
            throw PairingError("This pairing code has expired — generate a new one on your Mac.")
        }
        let commit = try RemotePairingCommit.prepare(
            response: response,
            existingRecords: pairedMacs
        )
        let record = commit.record
        // Re-pairing the same Mac replaces its record in place; a new Mac
        // appends. Either way it becomes the active connection.
        pairedMacs = commit.records
        Self.saveRecords(pairedMacs)
        activeMacID = record.macID
        UserDefaults.standard.set(record.macID, forKey: Self.activeMacIDKey)
        hasRelayCredentials = commit.relayCredentialsSaved
        relayCredentialsNeedRefresh = !commit.relayCredentialsSaved
        if commit.relayCredentialsSaved {
            RelayCredentialRefreshMarker.markCurrent(macID: record.macID)
        } else {
            RelayCredentialRefreshMarker.markStale(macID: record.macID)
        }
        relayCredentialFetchRetryAfter = .distantPast
        relayFallbackRetryAfter = .distantPast
        mode = .paired(record)
        usingRelay = false
        client = RemoteMacClient(baseURL: record.endpoint, authToken: response.authToken)
        epoch &+= 1
        lastPairingError = nil
        // Seed the WS terminal-stream discovery from the pairing response;
        // bootstrap polls keep it fresh afterwards (the port is OS-assigned
        // per `unpeel-host __remote__` run).
        RemoteServerDiscovery.shared.update(
            port: response.remoteServerPort,
            certificateFingerprint: response.remoteServerCertificateFingerprint
        )
    }

    /// Re-point everything at another paired Mac.
    func switchTo(macID: String) {
        guard macID != activeMacID,
              let record = pairedMacs.first(where: { $0.macID == macID })
        else { return }
        let token: String
        switch RemoteKeychain.tokenReadResult(macID: macID) {
        case .found(let value) where !value.isEmpty:
            token = value
        case .temporarilyUnavailable:
            keychainHydrationPending = true
            return
        case .found, .notFound:
            return
        }
        activeMacID = macID
        UserDefaults.standard.set(macID, forKey: Self.activeMacIDKey)
        usingRelay = false
        let credentialState = RemoteKeychain.relayCredentialState(macID: macID)
        if case .temporarilyUnavailable = credentialState {
            keychainHydrationPending = true
            hasRelayCredentials = false
            relayCredentialsNeedRefresh = false
        } else {
            hasRelayCredentials = credentialState.credentials != nil
            relayCredentialsNeedRefresh = RelayCredentialRefreshMarker.needsRefresh(
                macID: macID,
                state: credentialState
            )
        }
        relayCredentialFetchRetryAfter = .distantPast
        relayFallbackRetryAfter = .distantPast
        mode = .paired(record)
        // Dropping the old client releases its RemoteRelayConnection (deinit
        // closes the socket), so relay teardown rides the reassignment.
        client = RemoteMacClient(baseURL: record.endpoint, authToken: token)
        epoch &+= 1
        // The singleton still pins the old Mac's WSS port + TLS fingerprint;
        // clear it so renderers long-poll for the beat until the new Mac's
        // first bootstrap re-seeds it (same behavior as unpair).
        RemoteServerDiscovery.shared.clear()
    }

    // MARK: - Unpeel Remote (relay) fallback

    private func isCurrentDirectGeneration(
        _ generation: RemoteDirectClientGeneration
    ) -> Bool {
        guard !usingRelay, case .paired(let record) = mode,
              let token = RemoteKeychain.loadToken(macID: record.macID)
        else { return false }
        return generation.matches(
            epoch: epoch,
            activeClient: client,
            activeRecord: record,
            activeToken: token
        )
    }

    private func isCurrentRelayGeneration(
        _ generation: RemoteRelayClientGeneration
    ) -> Bool {
        guard usingRelay, case .paired(let record) = mode,
              let token = RemoteKeychain.loadToken(macID: record.macID)
        else { return false }
        return generation.matches(
            epoch: epoch,
            activeClient: client,
            activeRecord: record,
            activeToken: token
        )
    }

    /// Last rung of the reconnection ladder: reach the Mac through the
    /// relay. Verifies with a real bootstrap before swapping the client, so
    /// a dead relay path never replaces a merely-slow LAN one.
    func activateRelayFallback() async -> Bool {
        let now = Date()
        guard RelayFallbackRetryPolicy.canAttempt(
                  now: now,
                  retryAfter: relayFallbackRetryAfter
              ),
              case .paired(let record) = mode, !usingRelay,
              hasRelayCredentials,
              let credentials = RemoteKeychain.loadRelayCredentials(macID: record.macID),
              let token = RemoteKeychain.loadToken(macID: record.macID),
              let generation = RemoteDirectClientGeneration.capture(
                  candidate: client,
                  activeClient: client,
                  activeRecord: record,
                  activeToken: token,
                  epoch: epoch
              )
        else {
            return false
        }
        let expectedMacID = record.macID
        let connection = RemoteRelayConnection(
            credentials: credentials,
            deviceID: record.deviceID
        )
        let candidate = RemoteMacClient(
            baseURL: record.endpoint,
            authToken: token,
            relay: connection
        )
        do {
            _ = try await candidate.bootstrap()
        } catch {
            guard isCurrentDirectGeneration(generation) else { return false }
            // A decodable credential can still be stale server-side. Preserve
            // it for bounded retry while away, but force authenticated Direct
            // recovery as soon as the saved endpoint is healthy again.
            relayCredentialsNeedRefresh = true
            RelayCredentialRefreshMarker.markStale(macID: expectedMacID)
            relayFallbackRetryAfter = RelayFallbackRetryPolicy.retryAfterFailure(
                completedAt: Date()
            )
            NSLog("[UnpeelIOS] relay fallback failed; credentials queued for Direct repair: \(error.localizedDescription)")
            return false
        }
        // The user may have switched Macs while the relay bootstrap was in
        // flight or re-paired the same Mac — never graft a superseded relay
        // generation onto the current Direct connection.
        guard isCurrentDirectGeneration(generation) else { return false }
        client = candidate
        usingRelay = true
        relayCredentialsNeedRefresh = false
        relayFallbackRetryAfter = .distantPast
        RelayCredentialRefreshMarker.markCurrent(macID: expectedMacID)
        epoch &+= 1
        return true
    }

    /// While on the relay, periodically probe the LAN endpoint and switch
    /// back the moment the Mac is directly reachable again (lower latency,
    /// and it re-enables the pinned WSS terminal stream).
    func restoreDirectConnection() async -> Bool {
        guard case .paired(let record) = mode, usingRelay,
              let token = RemoteKeychain.loadToken(macID: record.macID),
              let generation = RemoteRelayClientGeneration.capture(
                  activeClient: client,
                  activeRecord: record,
                  activeToken: token,
                  epoch: epoch
              )
        else { return false }
        let candidate = RemoteMacClient(baseURL: record.endpoint, authToken: token)
        return await RemoteDirectRestore.attempt(
            generation: generation,
            isStillCurrent: { self.isCurrentRelayGeneration($0) },
            probe: { try? await candidate.bootstrap() },
            adopt: {
                self.client = candidate
                self.usingRelay = false
                self.epoch &+= 1
            }
        )
    }

    /// Upgrade/recovery path for existing pairings: missing, undecodable,
    /// pre-marker (build 12), and Relay-failed credentials are rotated over
    /// the healthy bearer-authenticated Direct channel. No pairing identity
    /// or saved Host record changes.
    @discardableResult
    func ensureRelayCredentials(
        after poll: RemoteConnectionPollProof
    ) async -> RelayCredentialRepairOutcome? {
        let polledClient = poll.client
        guard case .paired(let record) = mode, !usingRelay, !polledClient.isRelay,
              !relayCredentialFetchInFlight,
              poll.hostMacID == record.macID,
              poll.connectionEpoch == epoch,
              let activeToken = RemoteKeychain.loadToken(macID: record.macID)
        else { return nil }
        let expectedMacID = record.macID
        let expectedEpoch = poll.connectionEpoch
        guard let initialGeneration = RemoteDirectClientGeneration.capture(
            candidate: polledClient,
            activeClient: self.client,
            activeRecord: record,
            activeToken: activeToken,
            epoch: expectedEpoch
        ), isCurrentDirectGeneration(initialGeneration)
        else {
            NSLog("[UnpeelIOS] skipped relay credential refresh for stale Direct client")
            return nil
        }
        let storedState = RemoteKeychain.relayCredentialState(macID: expectedMacID)
        if case .temporarilyUnavailable = storedState {
            keychainHydrationPending = true
            // A healthy Direct poll does not authorize rotating a secret that
            // may merely be hidden behind protected-data lock state.
            return nil
        }
        hasRelayCredentials = storedState.credentials != nil
        relayCredentialsNeedRefresh = relayCredentialsNeedRefresh
            || RelayCredentialRefreshMarker.needsRefresh(
                macID: expectedMacID,
                state: storedState
            )
        guard relayCredentialsNeedRefresh, Date() >= relayCredentialFetchRetryAfter else {
            return nil
        }
        relayCredentialFetchInFlight = true
        defer { relayCredentialFetchInFlight = false }

        guard let result = await RelayCredentialRepair.attemptIfBoundToActiveDirectClient(
            candidate: polledClient,
            activeClient: self.client,
            activeRecord: record,
            activeToken: activeToken,
            connectionEpoch: expectedEpoch,
            isStillCurrent: { self.isCurrentDirectGeneration($0) },
            onSupersededAfterFetch: {
                // The mutating request was sent, so the Host may have rotated
                // even though this generation lost authority before the
                // reply. Preserve no readiness claim; switching back (or the
                // new same-Mac generation's next poll) must repair again.
                RelayCredentialRefreshMarker.markStale(macID: $0.macID)
            },
            fetch: { try await polledClient.relayCredentials() },
            persist: {
                return RemoteKeychain.saveRelayCredentials($0, macID: expectedMacID)
            }
        ) else { return nil }
        // Actor continuations may interleave between the helper returning and
        // this caller resuming. Keep the same exact guard for all state/marker
        // mutations after the await.
        guard isCurrentDirectGeneration(result.generation) else {
            NSLog("[UnpeelIOS] discarded relay credential state for superseded Direct client")
            return nil
        }
        let outcome = result.outcome
        switch outcome {
        case .refreshed:
            hasRelayCredentials = true
            relayCredentialsNeedRefresh = false
            relayCredentialFetchRetryAfter = .distantPast
            relayFallbackRetryAfter = .distantPast
            RelayCredentialRefreshMarker.markCurrent(macID: expectedMacID)
        case .recoveryUnavailable:
            // Headless Hosts do not yet expose credential recovery. A valid
            // build-12 credential remains the only Link path and must stay
            // armed; suppress repeat GETs until an actual Relay failure marks
            // it stale again.
            hasRelayCredentials = storedState.credentials != nil
            relayCredentialsNeedRefresh = false
            relayCredentialFetchRetryAfter = storedState.credentials == nil
                ? Date().addingTimeInterval(Self.relayCredentialUnavailableRetryDelay)
                : .distantPast
            RelayCredentialRefreshMarker.markRecoveryUnavailable(macID: expectedMacID)
        case .fetchFailed, .invalidResponse, .persistenceFailed:
            // The credential route rotates before replying. For an invalid,
            // lost, or unpersisted response the prior set may no longer be
            // accepted, so don't claim readiness; retry later over Direct.
            hasRelayCredentials = false
            relayCredentialsNeedRefresh = true
            relayCredentialFetchRetryAfter = Date().addingTimeInterval(
                Self.relayCredentialRetryDelay
            )
            RelayCredentialRefreshMarker.markStale(macID: expectedMacID)
        }
        return outcome
    }

    /// Forget one paired Mac locally. (Server-side revocation lives in the
    /// Mac's Settings ▸ devices list.) Forgetting the active Mac switches to
    /// the next paired one; forgetting the last falls back to dev bridge /
    /// needs-pairing.
    func unpair(macID: String) {
        RemoteKeychain.deleteToken(macID: macID)
        RemoteKeychain.deleteRelayCredentials(macID: macID)
        RelayCredentialRefreshMarker.markStale(macID: macID)
        pairedMacs = PairedMacCollection.removing(pairedMacs, macID: macID)
        Self.saveRecords(pairedMacs)
        guard macID == activeMacID else { return }
        if let next = pairedMacs.first?.macID {
            activeMacID = nil
            switchTo(macID: next)
            if activeMacID != nil { return }
            // switchTo refused (missing token) — fall through to unpaired.
        }
        activeMacID = nil
        UserDefaults.standard.removeObject(forKey: Self.activeMacIDKey)
        usingRelay = false
        hasRelayCredentials = false
        relayCredentialsNeedRefresh = true
        relayCredentialFetchRetryAfter = .distantPast
        relayFallbackRetryAfter = .distantPast
        mode = .devBridge
        client = RemoteMacClient(authToken: Self.devBridgeToken())
        epoch &+= 1
        // The dev bridge has no WS stream; drop the stale Mac endpoint so
        // renderers stop attempting it.
        RemoteServerDiscovery.shared.clear()
    }

    /// Best-effort APNs token registration with EVERY paired Mac — not just
    /// the active one — so "needs input" notifications arrive from all of
    /// them. Unreachable Macs are retried on every epoch bump and app launch
    /// (the root view re-uploads the cached token on both), so no persisted
    /// retry queue is needed; the POST is idempotent per deviceID.
    func registerPushTokenEverywhere(apnsToken: String, environment: String) {
        for record in pairedMacs {
            guard let token = RemoteKeychain.loadToken(macID: record.macID) else { continue }
            let relayCredentials = RemoteKeychain.loadRelayCredentials(macID: record.macID)
            Task {
                let direct = RemoteMacClient(baseURL: record.endpoint, authToken: token)
                if (try? await direct.registerPushToken(
                    apnsToken: apnsToken, environment: environment
                )) != nil { return }
                // LAN failed — try the relay, so an off-network Mac still
                // learns the fresh token. The transient connection closes on
                // task end (deinit).
                guard let relayCredentials else { return }
                let relayClient = RemoteMacClient(
                    baseURL: record.endpoint,
                    authToken: token,
                    relay: RemoteRelayConnection(
                        credentials: relayCredentials,
                        deviceID: record.deviceID
                    )
                )
                _ = try? await relayClient.registerPushToken(
                    apnsToken: apnsToken, environment: environment
                )
            }
        }
    }

    func recordPairingError(_ message: String) {
        lastPairingError = message
    }

    // MARK: - Persistence

    nonisolated private static func loadRecords(
        defaults: UserDefaults = .standard
    ) -> [PairedMacRecord] {
        guard let data = defaults.data(forKey: recordsKey) else { return [] }
        return (try? JSONDecoder().decode([PairedMacRecord].self, from: data)) ?? []
    }

    nonisolated private static func saveRecords(
        _ records: [PairedMacRecord],
        defaults: UserDefaults = .standard
    ) {
        if let data = try? JSONEncoder().encode(records) {
            defaults.set(data, forKey: recordsKey)
        }
    }

    /// One-time upgrade from the single-Mac storage scheme (one record blob,
    /// fixed Keychain accounts) to the macID-keyed collection. Idempotent and
    /// self-healing: new keys are written first, add-if-absent, and the
    /// legacy keys are deleted last — a crash at any point re-runs the whole
    /// thing harmlessly on next launch. Injection points exist so tests can
    /// run this against a scratch UserDefaults suite and a fake keychain.
    @discardableResult
    nonisolated static func migrateLegacyStorageIfNeeded(
        defaults: UserDefaults = .standard,
        loadScopedToken: (String) -> KeychainReadResult<String> = {
            RemoteKeychain.tokenReadResult(macID: $0)
        },
        loadScopedRelayCredentials: (String) -> RemoteKeychain.RelayCredentialState = {
            RemoteKeychain.relayCredentialState(macID: $0)
        },
        loadLegacyToken: () -> KeychainReadResult<String> = RemoteKeychain.legacyTokenReadResult,
        loadLegacyRelayCredentials: () -> RemoteKeychain.RelayCredentialState = RemoteKeychain.legacyRelayCredentialState,
        saveToken: (String, String) -> Bool = RemoteKeychain.saveToken(_:macID:),
        saveRelayCredentials: (RelayCredentials, String) -> Bool = RemoteKeychain.saveRelayCredentials(_:macID:),
        deleteLegacyKeychainItems: () -> Void = RemoteKeychain.deleteLegacyItems
    ) -> LegacyPairingMigrationResult {
        guard let data = defaults.data(forKey: legacyRecordKey) else {
            return .noLegacyRecord
        }
        guard let record = try? JSONDecoder().decode(PairedMacRecord.self, from: data) else {
            // A corrupt legacy blob cannot identify a real pairing. This is a
            // conclusive decode failure, not a protected-data read failure.
            defaults.removeObject(forKey: legacyRecordKey)
            deleteLegacyKeychainItems()
            return .completed
        }

        // Resolve every protected Keychain read before the first write or
        // cleanup. A locked/background launch can make either legacy slot (or
        // either scoped slot from a partial run) temporarily inaccessible; in
        // that case the migration must be a true zero-mutation retry.
        let scopedTokenState = loadScopedToken(record.macID)
        let scopedRelayState = loadScopedRelayCredentials(record.macID)
        let legacyTokenState = loadLegacyToken()
        let legacyRelayState = loadLegacyRelayCredentials()
        let unavailableStatuses = [
            scopedTokenState.unavailableStatus,
            scopedRelayState.unavailableStatus,
            legacyTokenState.unavailableStatus,
            legacyRelayState.unavailableStatus,
        ].compactMap { $0 }
        if let status = unavailableStatuses.first {
            NSLog("[UnpeelIOS] legacy pairing migration deferred until protected data is available: \(status)")
            return .temporarilyUnavailable(status)
        }

        // 1. New Keychain items first. A retained migration sentinel can
        // coexist with a later recovery/re-pair, so any nonempty scoped bearer
        // and any structurally valid scoped Relay set are authoritative.
        let scopedToken: String?
        if case .found(let token) = scopedTokenState, !token.isEmpty {
            scopedToken = token
        } else {
            scopedToken = nil
        }
        if scopedToken == nil,
           case .found(let legacyToken) = legacyTokenState,
           !legacyToken.isEmpty,
           !saveToken(legacyToken, record.macID) {
            NSLog("[UnpeelIOS] legacy pairing migration deferred after token write failure")
            return .retryNeeded
        }

        var relayWriteSucceeded = false
        var retainLegacyCredentialsForRetry = false
        if case .available(let scopedRelay) = scopedRelayState,
           RemoteKeychain.isValid(scopedRelay, expectedMacID: record.macID) {
            relayWriteSucceeded = true
        } else if case .available(let legacyRelay) = legacyRelayState {
            if RemoteKeychain.isValid(legacyRelay, expectedMacID: record.macID) {
                relayWriteSucceeded = saveRelayCredentials(legacyRelay, record.macID)
                retainLegacyCredentialsForRetry = !relayWriteSucceeded
            } else {
                NSLog("[UnpeelIOS] dropping invalid legacy relay credentials")
            }
        } else if case .invalid = legacyRelayState {
            NSLog("[UnpeelIOS] dropping undecodable legacy relay credentials")
        }

        // 2. Add the stable pairing record once, then claim it as active only
        // if no later pairing/switch has already chosen another Mac.
        var records = loadRecords(defaults: defaults)
        if !records.contains(where: { $0.macID == record.macID }) {
            records.append(record)
            saveRecords(records, defaults: defaults)
        }
        if defaults.string(forKey: activeMacIDKey) == nil {
            defaults.set(record.macID, forKey: activeMacIDKey)
        }
        if relayWriteSucceeded {
            RelayCredentialRefreshMarker.markCurrent(
                macID: record.macID,
                defaults: defaults
            )
        } else {
            // Direct remains usable through the migrated bearer. Its first
            // healthy poll repairs a missing/invalid/failed Relay write.
            RelayCredentialRefreshMarker.markStale(
                macID: record.macID,
                defaults: defaults
            )
        }
        if retainLegacyCredentialsForRetry {
            // Keep the only known-good legacy Relay set and sentinel. A later
            // Direct recovery may install a newer scoped set; scoped-valid
            // precedence above ensures this retry never overwrites it.
            NSLog("[UnpeelIOS] legacy relay credential migration deferred after scoped write failure")
            return .retryNeeded
        }

        // 3. Drop legacy state last, only after every Keychain read was
        // conclusive and every required copy succeeded.
        defaults.removeObject(forKey: legacyRecordKey)
        deleteLegacyKeychainItems()
        return .completed
    }

    /// Stable per-install identity; the Mac keys its device list on this,
    /// so re-pairing the same phone replaces rather than duplicates.
    private static func deviceIdentity() -> RemoteDeviceIdentity {
        let defaults = UserDefaults.standard
        let id: String
        if let existing = defaults.string(forKey: deviceIDKey), !existing.isEmpty {
            id = existing
        } else {
            id = UUID().uuidString.lowercased()
            defaults.set(id, forKey: deviceIDKey)
        }
        #if os(iOS)
        let name = UIDevice.current.name
        let platform = UIDevice.current.systemName
        #else
        let name = "iOS Device"
        let platform = "iOS"
        #endif
        let version = Bundle.main
            .object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
        return RemoteDeviceIdentity(
            id: id,
            name: name,
            platform: platform,
            appVersion: version
        )
    }
}

struct PairingError: LocalizedError {
    let message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }
}
