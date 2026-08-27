//
//  RemoteHosts.swift
//  UnpeelNative
//
//  Controller-side Host records. Public metadata lives in the isolated app
//  defaults suite; command-execution credentials stay in Keychain. Nothing
//  here reads or writes local Session state.
//

import AppKit
import Foundation
import Security
import UnpeelShared

/// The App→Host terminal slice is available only in explicitly branded local
/// development bundles. Customer builds stay closed until direct networking
/// uses pinned TLS/authenticated E2E, remote create/lifecycle/organization and
/// review verbs reach parity, and assembled two-process + physical two-Mac QA
/// passes. Do not turn this into an unconditional `true` for partial slices.
enum RemoteHostFeature {
    static var pickerEnabled: Bool {
        Bundle.main.object(forInfoDictionaryKey: "UnpeelDevelopmentBuild") as? Bool == true
    }
}

struct RemoteHostCredentials: Codable, Equatable, Sendable {
    let authToken: String
    let relayCredentials: RelayCredentials
}

protocol RemoteHostCredentialStoring {
    func save(_ credentials: RemoteHostCredentials, account: String) throws
    func load(account: String) -> RemoteHostCredentials?
    func delete(account: String)
}

enum RemoteHostCredentialError: LocalizedError {
    case keychain(OSStatus)
    case encoding

    var errorDescription: String? {
        switch self {
        case .keychain(let status):
            "Could not store the Host credential in Keychain (\(status))."
        case .encoding:
            "Could not encode the Host credential."
        }
    }
}

enum RemoteHostPairingError: LocalizedError {
    case invalidCode
    case incompatibleProtocol
    case candidateMismatch
    case selfPairing
    case expired
    case host(Int, String?)
    case authentication

    var errorDescription: String? {
        switch self {
        case .invalidCode:
            "That is not an Unpeel pairing code."
        case .incompatibleProtocol:
            "This Host uses an incompatible pairing protocol."
        case .candidateMismatch:
            "That code belongs to a different Host."
        case .selfPairing:
            "This is this Mac. Choose Local instead."
        case .expired:
            "That pairing code expired. Generate a new code on the Host."
        case .host(_, let message):
            message ?? "The Host rejected the pairing request."
        case .authentication:
            "The Host pairing response could not be authenticated."
        }
    }
}

struct KeychainRemoteHostCredentialStore: RemoteHostCredentialStoring {
    private static let service = "com.unpeel.native.remote-host"

    private func query(account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.service,
            kSecAttrAccount as String: account,
        ]
    }

    func save(_ credentials: RemoteHostCredentials, account: String) throws {
        guard let data = try? JSONEncoder().encode(credentials) else {
            throw RemoteHostCredentialError.encoding
        }
        let base = query(account: account)
        let updateStatus = SecItemUpdate(
            base as CFDictionary,
            [kSecValueData as String: data] as CFDictionary
        )
        if updateStatus == errSecSuccess { return }
        guard updateStatus == errSecItemNotFound else {
            throw RemoteHostCredentialError.keychain(updateStatus)
        }
        var insert = base
        insert[kSecValueData as String] = data
        insert[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let status = SecItemAdd(insert as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw RemoteHostCredentialError.keychain(status)
        }
    }

    func load(account: String) -> RemoteHostCredentials? {
        var lookup = query(account: account)
        lookup[kSecReturnData as String] = true
        lookup[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: AnyObject?
        guard SecItemCopyMatching(lookup as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data
        else { return nil }
        return try? JSONDecoder().decode(RemoteHostCredentials.self, from: data)
    }

    func delete(account: String) {
        SecItemDelete(query(account: account) as CFDictionary)
    }
}

@MainActor
final class RemoteHostStore: ObservableObject {
    private static let recordsKey = "unpeel.native.remoteHosts"
    private static let selectedHostKey = "unpeel.native.selectedRemoteHost"
    private static let controllerIDKey = "unpeel.native.remoteControllerID"

    @Published private(set) var records: [PairedHostRecord]
    /// Nil means Local. Fresh installs therefore always start in Local scope.
    @Published private(set) var selectedHostID: String?

    let controllerIdentity: RemoteDeviceIdentity

    private let defaults: UserDefaults
    private let credentialStore: any RemoteHostCredentialStoring
    private let localHostID: String?

    init(
        defaults: UserDefaults = AppDefaults.shared,
        credentialStore: any RemoteHostCredentialStoring = KeychainRemoteHostCredentialStore(),
        localHostID: String? = nil,
        deviceName: String? = nil,
        appVersion: String? = Bundle.main.object(
            forInfoDictionaryKey: "CFBundleShortVersionString"
        ) as? String
    ) {
        self.defaults = defaults
        self.credentialStore = credentialStore
        self.localHostID = localHostID
        let controllerID = Self.controllerID(in: defaults)
        controllerIdentity = RemoteDeviceIdentity(
            id: controllerID,
            name: deviceName
                ?? Host.current().localizedName
                ?? ProcessInfo.processInfo.hostName,
            platform: "macOS",
            appVersion: appVersion
        )
        let loadedRecords = Self.loadRecords(from: defaults)
        let removedLocalRecords = loadedRecords.filter {
            Self.hostIDsMatch($0.hostID, localHostID)
        }
        records = loadedRecords.filter {
            !Self.hostIDsMatch($0.hostID, localHostID)
        }
        let persistedSelection = defaults.string(forKey: Self.selectedHostKey)
        if let persistedSelection,
           records.contains(where: { $0.hostID == persistedSelection }),
           credentialStore.load(
               account: Self.credentialAccount(
                   controllerID: controllerID,
                   hostID: persistedSelection
               )
           ) != nil {
            selectedHostID = persistedSelection
        } else {
            selectedHostID = nil
        }
        if selectedHostID == nil {
            defaults.removeObject(forKey: Self.selectedHostKey)
        }
        if !removedLocalRecords.isEmpty {
            for record in removedLocalRecords {
                credentialStore.delete(
                    account: Self.credentialAccount(
                        controllerID: controllerID,
                        hostID: record.hostID
                    )
                )
            }
            persistRecords()
        }
    }

    var selectedRecord: PairedHostRecord? {
        guard let selectedHostID else { return nil }
        return records.first { $0.hostID == selectedHostID }
    }

    func credentials(for hostID: String) -> RemoteHostCredentials? {
        credentialStore.load(account: credentialAccount(hostID: hostID))
    }

    /// Complete the same sealed one-time handshake used by the iPhone. A
    /// nearby Bonjour row is only a convenience pre-filter: the code still
    /// authenticates the Host and an optional selected row must match it.
    @discardableResult
    func pair(
        code: String,
        expectedHostID: String? = nil,
        client: RemotePairingClient = RemotePairingClient()
    ) async throws -> PairedHostRecord {
        guard let payload = RemotePairingCode.decode(code) else {
            throw RemoteHostPairingError.invalidCode
        }
        guard payload.protocolVersion == RemoteControlProtocol.version else {
            throw RemoteHostPairingError.incompatibleProtocol
        }
        guard !isLocalHost(payload.macID) else {
            throw RemoteHostPairingError.selfPairing
        }
        if let expectedHostID,
           expectedHostID.caseInsensitiveCompare(payload.macID) != .orderedSame {
            throw RemoteHostPairingError.candidateMismatch
        }
        let response: RemotePairingResponse
        do {
            response = try await client.pair(
                payload: payload,
                device: controllerIdentity
            )
        } catch let error as RemotePairingClientError {
            switch error {
            case .expired:
                throw RemoteHostPairingError.expired
            case .incompatibleProtocol:
                throw RemoteHostPairingError.incompatibleProtocol
            case let .httpStatus(statusCode, serverMessage):
                throw RemoteHostPairingError.host(statusCode, serverMessage)
            case .invalidHostIdentity,
                 .invalidHTTPResponse,
                 .responseHostIdentityMismatch,
                 .responseEndpointMismatch,
                 .responseDeviceIdentityMismatch,
                 .invalidCredentials:
                throw RemoteHostPairingError.authentication
            }
        }
        try Task.checkCancellation()
        return try adopt(
            response,
            certificateFingerprint: payload.certificateFingerprint,
            select: false
        )
    }

    /// Persist a successfully authenticated pairing response. Metadata is
    /// committed only after Keychain accepts the secret, so a crash cannot
    /// leave a picker row that can never connect.
    @discardableResult
    func adopt(
        _ response: RemotePairingResponse,
        certificateFingerprint: String? = nil,
        select: Bool = true
    ) throws -> PairedHostRecord {
        guard response.protocolVersion == RemoteControlProtocol.version else {
            throw RemoteHostPairingError.incompatibleProtocol
        }
        guard !isLocalHost(response.macID) else {
            throw RemoteHostPairingError.selfPairing
        }
        guard !response.macID.isEmpty,
              response.deviceID == controllerIdentity.id,
              !response.authToken.isEmpty,
              response.relayCredentials.macID == response.macID,
              !response.relayCredentials.relayToken.isEmpty,
              response.relayCredentials.relayURL.scheme?.lowercased() == "wss",
              response.relayCredentials.e2eKey?.count == 32
        else {
            throw RemoteHostPairingError.authentication
        }
        let credentials = RemoteHostCredentials(
            authToken: response.authToken,
            relayCredentials: response.relayCredentials
        )
        try credentialStore.save(
            credentials,
            account: credentialAccount(hostID: response.macID)
        )
        let record = PairedHostRecord(
            pairing: response,
            certificateFingerprint: certificateFingerprint
        )
        records = PairedHostCollection.upserting(records, with: record)
        persistRecords()
        if select { selectHost(record.hostID) }
        return record
    }

    func selectHost(_ hostID: String?) {
        guard let hostID else {
            selectedHostID = nil
            defaults.removeObject(forKey: Self.selectedHostKey)
            return
        }
        guard records.contains(where: { $0.hostID == hostID }),
              credentials(for: hostID) != nil
        else { return }
        selectedHostID = hostID
        defaults.set(hostID, forKey: Self.selectedHostKey)
    }

    /// Scope a paired Host to Direct-only (enabled = false) or restore its
    /// Unpeel Link fallback. Same narrows-only storage convention as the
    /// inbound device flag: allowed is the nil default, so the persisted
    /// records gain no key until a Host is actually restricted.
    func setLinkEnabled(_ enabled: Bool, forHost hostID: String) {
        guard let index = records.firstIndex(where: { $0.hostID == hostID }),
              records[index].isLinkEnabled != enabled
        else { return }
        records[index].linkEnabled = enabled ? nil : false
        persistRecords()
    }

    func forget(hostID: String) {
        records = PairedHostCollection.removing(records, hostID: hostID)
        credentialStore.delete(account: credentialAccount(hostID: hostID))
        if selectedHostID == hostID { selectHost(nil) }
        persistRecords()
    }

    private func credentialAccount(hostID: String) -> String {
        Self.credentialAccount(controllerID: controllerIdentity.id, hostID: hostID)
    }

    private func isLocalHost(_ hostID: String) -> Bool {
        Self.hostIDsMatch(hostID, localHostID)
    }

    private static func hostIDsMatch(_ hostID: String, _ otherHostID: String?) -> Bool {
        guard let otherHostID, !otherHostID.isEmpty else { return false }
        return otherHostID.caseInsensitiveCompare(hostID) == .orderedSame
    }

    private static func credentialAccount(controllerID: String, hostID: String) -> String {
        "pairing.\(controllerID).\(hostID)"
    }

    private func persistRecords() {
        guard let data = try? JSONEncoder().encode(records) else { return }
        defaults.set(data, forKey: Self.recordsKey)
    }

    private static func loadRecords(from defaults: UserDefaults) -> [PairedHostRecord] {
        guard let data = defaults.data(forKey: recordsKey),
              let records = try? JSONDecoder().decode([PairedHostRecord].self, from: data)
        else { return [] }
        var seen = Set<String>()
        return records.filter { !$0.hostID.isEmpty && seen.insert($0.hostID).inserted }
    }

    private static func controllerID(in defaults: UserDefaults) -> String {
        if let value = defaults.string(forKey: controllerIDKey), !value.isEmpty {
            return value
        }
        let value = UUID().uuidString.lowercased()
        defaults.set(value, forKey: controllerIDKey)
        return value
    }
}
