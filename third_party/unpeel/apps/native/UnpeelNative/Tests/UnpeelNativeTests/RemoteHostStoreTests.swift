import XCTest
import UnpeelShared
@testable import UnpeelNative

@MainActor
final class RemoteHostStoreTests: XCTestCase {
    func testFreshStoreStartsLocalAndPersistsPairingWithoutSecretMetadata() throws {
        let fixture = makeFixture()
        let store = RemoteHostStore(
            defaults: fixture.defaults,
            credentialStore: fixture.credentials,
            deviceName: "Controller Mac",
            appVersion: "1"
        )
        XCTAssertNil(store.selectedHostID)

        let response = try pairingResponse(
            hostID: "host-1",
            token: "top-secret",
            deviceID: store.controllerIdentity.id
        )
        let record = try store.adopt(response, certificateFingerprint: "pair-pin")

        XCTAssertEqual(record.hostID, "host-1")
        XCTAssertEqual(store.selectedHostID, "host-1")
        XCTAssertEqual(store.credentials(for: "host-1")?.authToken, "top-secret")
        let persistedRecords = try XCTUnwrap(
            fixture.defaults.dictionaryRepresentation().values
                .compactMap { $0 as? Data }
                .compactMap { try? JSONDecoder().decode([PairedHostRecord].self, from: $0) }
                .first
        )
        XCTAssertEqual(persistedRecords, [record])
        let persistedJSON = try JSONEncoder().encode(persistedRecords)
        XCTAssertFalse(String(decoding: persistedJSON, as: UTF8.self).contains("top-secret"))

        let restored = RemoteHostStore(
            defaults: fixture.defaults,
            credentialStore: fixture.credentials,
            deviceName: "Controller Mac"
        )
        XCTAssertEqual(restored.records, store.records)
        XCTAssertEqual(restored.selectedHostID, "host-1")
    }

    func testForgetDeletesOnlyThatHostsCredentialAndReturnsToLocal() throws {
        let fixture = makeFixture()
        let store = RemoteHostStore(
            defaults: fixture.defaults,
            credentialStore: fixture.credentials,
            deviceName: "Controller"
        )
        try store.adopt(pairingResponse(
            hostID: "a",
            token: "a-token",
            deviceID: store.controllerIdentity.id
        ), select: false)
        try store.adopt(pairingResponse(
            hostID: "b",
            token: "b-token",
            deviceID: store.controllerIdentity.id
        ))

        store.forget(hostID: "b")

        XCTAssertNil(store.selectedHostID)
        XCTAssertEqual(store.records.map(\.hostID), ["a"])
        XCTAssertNotNil(store.credentials(for: "a"))
        XCTAssertNil(store.credentials(for: "b"))
    }

    func testWorkspaceControllerIDsScopeKeychainAccounts() throws {
        let credentials = MemoryRemoteHostCredentials()
        let firstDefaults = makeDefaults()
        let secondDefaults = makeDefaults()
        defer {
            firstDefaults.removePersistentDomain(forName: firstDefaultsSuite(firstDefaults))
            secondDefaults.removePersistentDomain(forName: firstDefaultsSuite(secondDefaults))
        }
        let first = RemoteHostStore(
            defaults: firstDefaults,
            credentialStore: credentials,
            deviceName: "First"
        )
        let second = RemoteHostStore(
            defaults: secondDefaults,
            credentialStore: credentials,
            deviceName: "Second"
        )
        try first.adopt(pairingResponse(
            hostID: "same-host",
            token: "first",
            deviceID: first.controllerIdentity.id
        ))
        try second.adopt(pairingResponse(
            hostID: "same-host",
            token: "second",
            deviceID: second.controllerIdentity.id
        ))

        XCTAssertNotEqual(first.controllerIdentity.id, second.controllerIdentity.id)
        XCTAssertEqual(first.credentials(for: "same-host")?.authToken, "first")
        XCTAssertEqual(second.credentials(for: "same-host")?.authToken, "second")
        XCTAssertEqual(credentials.values.count, 2)
    }

    /// The per-Host Link scope narrows only (nil = allowed, false stored),
    /// persists across store reloads, and restoring enrollment removes the
    /// key again instead of writing `true`.
    func testSetLinkEnabledNarrowsPersistsAndRestores() throws {
        let fixture = makeFixture()
        let store = RemoteHostStore(
            defaults: fixture.defaults,
            credentialStore: fixture.credentials,
            deviceName: "Controller"
        )
        try store.adopt(pairingResponse(
            hostID: "host-1",
            token: "secret",
            deviceID: store.controllerIdentity.id
        ))
        XCTAssertEqual(store.records.first?.linkEnabled, nil)

        store.setLinkEnabled(false, forHost: "host-1")
        XCTAssertEqual(store.records.first?.linkEnabled, false)

        let reloaded = RemoteHostStore(
            defaults: fixture.defaults,
            credentialStore: fixture.credentials,
            deviceName: "Controller"
        )
        XCTAssertEqual(reloaded.records.first?.linkEnabled, false)

        reloaded.setLinkEnabled(true, forHost: "host-1")
        XCTAssertNil(reloaded.records.first?.linkEnabled)
        let restored = RemoteHostStore(
            defaults: fixture.defaults,
            credentialStore: fixture.credentials,
            deviceName: "Controller"
        )
        XCTAssertNil(restored.records.first?.linkEnabled)
        XCTAssertTrue(restored.records.first?.isLinkEnabled ?? false)
    }

    func testMissingCredentialKeepsHostRecordButRestoresLocalScope() throws {
        let fixture = makeFixture()
        let original = RemoteHostStore(
            defaults: fixture.defaults,
            credentialStore: fixture.credentials,
            deviceName: "Controller"
        )
        try original.adopt(pairingResponse(
            hostID: "host-1",
            token: "secret",
            deviceID: original.controllerIdentity.id
        ))
        XCTAssertEqual(original.selectedHostID, "host-1")

        let missingCredentials = MemoryRemoteHostCredentials()
        let restored = RemoteHostStore(
            defaults: fixture.defaults,
            credentialStore: missingCredentials,
            deviceName: "Controller"
        )

        XCTAssertEqual(restored.records.map(\.hostID), ["host-1"])
        XCTAssertNil(restored.selectedHostID)
        restored.selectHost("host-1")
        XCTAssertNil(restored.selectedHostID)
    }

    func testNearbySelectionMustMatchAuthenticatedPairingCode() async throws {
        let fixture = makeFixture()
        let store = RemoteHostStore(
            defaults: fixture.defaults,
            credentialStore: fixture.credentials,
            deviceName: "Controller"
        )
        let payload = RemotePairingPayload(
            macID: "real-host",
            macName: "",
            endpoint: try XCTUnwrap(URL(string: "http://127.0.0.1:17661/mobile")),
            token: "ONE-TIME",
            expiresAtUnixMs: Int64(Date().timeIntervalSince1970 * 1_000) + 60_000
        )
        let code = try XCTUnwrap(RemotePairingCode.encode(payload))

        do {
            _ = try await store.pair(code: code, expectedHostID: "different-host")
            XCTFail("pairing should stop before contacting a mismatched Host")
        } catch let error as RemoteHostPairingError {
            guard case .candidateMismatch = error else {
                return XCTFail("unexpected error: \(error)")
            }
        }
        XCTAssertTrue(store.records.isEmpty)
        XCTAssertTrue(fixture.credentials.values.isEmpty)
    }

    func testSelfPairingIsRejectedBeforeNetworkOrPersistence() async throws {
        let fixture = makeFixture()
        let store = RemoteHostStore(
            defaults: fixture.defaults,
            credentialStore: fixture.credentials,
            localHostID: "this-host",
            deviceName: "Controller"
        )
        let payload = RemotePairingPayload(
            macID: "THIS-HOST",
            macName: "This Mac",
            endpoint: try XCTUnwrap(URL(string: "http://127.0.0.1:17661/mobile")),
            token: "ONE-TIME",
            expiresAtUnixMs: Int64(Date().timeIntervalSince1970 * 1_000) + 60_000
        )
        let code = try XCTUnwrap(RemotePairingCode.encode(payload))

        do {
            _ = try await store.pair(code: code)
            XCTFail("self-pairing should stop before contacting the local Host")
        } catch let error as RemoteHostPairingError {
            guard case .selfPairing = error else {
                return XCTFail("unexpected error: \(error)")
            }
        }

        let response = try pairingResponse(
            hostID: "this-host",
            token: "self-token",
            deviceID: store.controllerIdentity.id
        )
        XCTAssertThrowsError(try store.adopt(response)) { error in
            guard let pairingError = error as? RemoteHostPairingError,
                  case .selfPairing = pairingError
            else {
                return XCTFail("unexpected error: \(error)")
            }
        }
        XCTAssertTrue(store.records.isEmpty)
        XCTAssertTrue(fixture.credentials.values.isEmpty)
    }

    func testRestoringAnOldSelfPairRemovesItsRecordAndCredential() throws {
        let fixture = makeFixture()
        let original = RemoteHostStore(
            defaults: fixture.defaults,
            credentialStore: fixture.credentials,
            deviceName: "Controller"
        )
        try original.adopt(pairingResponse(
            hostID: "this-host",
            token: "old-self-token",
            deviceID: original.controllerIdentity.id
        ))
        XCTAssertEqual(original.selectedHostID, "this-host")

        let restored = RemoteHostStore(
            defaults: fixture.defaults,
            credentialStore: fixture.credentials,
            localHostID: "THIS-HOST",
            deviceName: "Controller"
        )

        XCTAssertTrue(restored.records.isEmpty)
        XCTAssertNil(restored.selectedHostID)
        XCTAssertNil(restored.credentials(for: "this-host"))
    }

    private func pairingResponse(
        hostID: String,
        token: String,
        deviceID: String
    ) throws -> RemotePairingResponse {
        RemotePairingResponse(
            macID: hostID,
            macName: "Host \(hostID)",
            endpoint: try XCTUnwrap(URL(string: "http://127.0.0.1:17661/mobile")),
            deviceID: deviceID,
            authToken: token,
            pairedAtUnixMs: 42,
            relayCredentials: RelayCredentials(
                relayURL: try XCTUnwrap(URL(string: "wss://relay.example")),
                macID: hostID,
                relayToken: "relay-\(hostID)",
                e2eKey: Data(repeating: 3, count: 32)
            )
        )
    }

    private func makeFixture() -> (
        defaults: UserDefaults,
        credentials: MemoryRemoteHostCredentials
    ) {
        (makeDefaults(), MemoryRemoteHostCredentials())
    }

    private func makeDefaults() -> UserDefaults {
        let suite = "RemoteHostStoreTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defaults.removePersistentDomain(forName: suite)
        defaults.set(suite, forKey: "test-suite-name")
        return defaults
    }

    private func firstDefaultsSuite(_ defaults: UserDefaults) -> String {
        defaults.string(forKey: "test-suite-name")!
    }
}

private final class MemoryRemoteHostCredentials: RemoteHostCredentialStoring {
    var values: [String: RemoteHostCredentials] = [:]

    func save(_ credentials: RemoteHostCredentials, account: String) throws {
        values[account] = credentials
    }

    func load(account: String) -> RemoteHostCredentials? { values[account] }

    func delete(account: String) { values.removeValue(forKey: account) }
}
