import Foundation
import XCTest
import UnpeelShared
@testable import UnpeelNative

final class HostProtocolConformanceTests: XCTestCase {
    private final class MemoryE2EKeyStore: MobileE2EKeyStoring {
        private var values: [String: Data] = [:]

        func load(deviceID: String) -> Data? { values[deviceID] }
        func save(_ key: Data, deviceID: String) throws { values[deviceID] = key }
        func delete(deviceID: String) { values.removeValue(forKey: deviceID) }
    }

    private final class EffectRecorder: @unchecked Sendable {
        private let lock = NSLock()
        private var restartSessionIDs: [String] = []
        private var sessionActions: [RemoteSessionActionRequest] = []

        func recordRestart(_ sessionID: String) {
            lock.lock()
            restartSessionIDs.append(sessionID)
            lock.unlock()
        }

        func recordAction(_ request: RemoteSessionActionRequest) {
            lock.lock()
            sessionActions.append(request)
            lock.unlock()
        }

        func snapshot() -> (
            restarts: [String],
            actions: [RemoteSessionActionRequest]
        ) {
            lock.lock()
            defer { lock.unlock() }
            return (restartSessionIDs, sessionActions)
        }
    }

    func testNativeCapabilitiesMatchCanonicalLedger() throws {
        let root = try fixtureJSON("host-capabilities-v1.json")
        let protocolInfo = try XCTUnwrap(root["protocol"] as? [String: Any])
        XCTAssertEqual(protocolInfo["major"] as? Int, RemoteControlProtocol.hostMajorVersion)
        XCTAssertEqual(protocolInfo["minor"] as? Int, RemoteControlProtocol.hostMinorVersion)

        let entries = try XCTUnwrap(root["capabilities"] as? [[String: Any]])
        let expected = entries.compactMap { entry -> String? in
            guard entry["native"] as? Bool == true else { return nil }
            return entry["id"] as? String
        }
        XCTAssertEqual(MobileRemoteServer.hostProtocol.capabilities, expected)
    }

    func testNativeAdapterRunsSharedConformanceFixture() async throws {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("unpeel-host-conformance-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let pairingStore = MobilePairingStore(
            storageURL: tempDir.appendingPathComponent("devices.json"),
            macID: "conformance-host",
            macName: "Conformance Host",
            e2eKeyStore: MemoryE2EKeyStore()
        )
        let pairing = pairingStore.beginPairing(
            endpoint: URL(string: "http://127.0.0.1:1/mobile")!
        )
        let paired = try pairingStore.pair(RemotePairingRequest(
            token: pairing.token,
            device: RemoteDeviceIdentity(
                id: "conformance-controller",
                name: "Conformance Controller",
                platform: "test"
            )
        ))

        let snapshot = RemoteBootstrapSnapshot(
            hostProtocol: MobileRemoteServer.hostProtocol,
            macID: "conformance-host",
            macName: "Conformance Host",
            folders: [],
            projects: [],
            presets: [],
            sessions: [],
            capturedAtUnixMs: 1
        )
        let effects = EffectRecorder()
        let server = try XCTUnwrap(MobileRemoteServer(
            pairingStore: pairingStore,
            bootstrapProvider: { snapshot },
            createSessionProvider: { _ in RemoteCreateSessionResponse(sessionID: "created") },
            resizeDesktopProvider: { _ in },
            sessionOrganizationProvider: { patch in
                guard patch.sessionID == "conformance-session" else {
                    throw MobileRemoteError(404, "Unknown session")
                }
            },
            // Mirrors UnpeelStore.applyRemoteProjectOrganization: resolve the
            // project first, then reject the unimplemented folder move, then
            // treat a single-sibling sortOrder move as a successful no-op.
            projectOrganizationProvider: { patch in
                guard patch.projectID == "conformance-project" else {
                    throw MobileRemoteError(404, "Unknown project")
                }
                if patch.folderID != nil {
                    throw MobileRemoteError(
                        501, "Moving a project between folders is not supported"
                    )
                }
            },
            restartSessionProvider: { request in
                guard request.sessionID == "conformance-restart" else {
                    throw MobileRemoteError(404, "Unknown session")
                }
                effects.recordRestart(request.sessionID)
            },
            sessionActionProvider: { request in
                if request.sessionID == "conformance-unknown" {
                    throw MobileRemoteError(404, "Unknown session")
                }
                if request.sessionID == "conformance-broken" {
                    throw MobileRemoteError(500, "Lifecycle effect failed")
                }
                if request.sessionID == "conformance-stop-exited",
                   request.action == .stop {
                    throw MobileRemoteError(409, "Session is not running")
                }
                if request.sessionID == "conformance-restart-agent-exited",
                   request.action == .restartAgent {
                    throw MobileRemoteError(409, "Session is not running")
                }
                if request.sessionID == "conformance-resume-agent-exited",
                   request.action == .resumeAgent {
                    throw MobileRemoteError(409, "Session is not running")
                }
                let isKnownEffect = switch request.action {
                case .stop:
                    request.sessionID == "conformance-stop-live"
                case .restart:
                    request.sessionID == "conformance-action-restart"
                case .restartAgent:
                    request.sessionID == "conformance-action-restart-agent"
                case .resumeAgent:
                    request.sessionID == "conformance-action-resume-agent"
                case .remove:
                    request.sessionID == "conformance-remove"
                }
                guard isKnownEffect else {
                    throw MobileRemoteError(404, "Unknown session")
                }
                effects.recordAction(request)
            },
            markReadProvider: { _ in },
            approvalAnswerProvider: { _ in
                throw MobileRemoteError(409, "approval no longer pending")
            },
            desktopViewingProvider: { _ in false },
            archivedSessionsProvider: { projectID in
                guard projectID == "conformance-project" else { return nil }
                return RemoteArchivedSessionsResponse(projectID: projectID, sessions: [])
            },
            onDevicesChanged: {},
            startNetworkServer: false
        ))

        let fixture = try fixtureJSON("host-conformance-v1.json")
        XCTAssertEqual(fixture["schemaVersion"] as? Int, 1)
        let cases = try XCTUnwrap(fixture["cases"] as? [[String: Any]])

        for item in cases {
            let id = try XCTUnwrap(item["id"] as? String)
            let method = try XCTUnwrap(item["method"] as? String)
            let path = try XCTUnwrap(item["path"] as? String)
            let expected = try XCTUnwrap(item["expected"] as? [String: Any])
            let expectedStatus = try XCTUnwrap(expected["native"] as? Int)
            let body: Data
            if let object = item["body"] {
                body = try JSONSerialization.data(withJSONObject: object)
            } else {
                body = Data()
            }
            let request = MobileRemoteServer.HTTPRequest(
                requestID: "conformance-\(id)",
                method: method,
                rawPath: path,
                httpVersion: "HTTP/1.1",
                path: path,
                query: item["query"] as? [String: String] ?? [:],
                headers: ["authorization": "Bearer \(paired.authToken)"],
                body: body
            )
            let response = await Task.detached { server.handle(request) }.value
            XCTAssertEqual(response.status, expectedStatus, "conformance case \(id)")

            if [
                "restart.success",
                "session-action.stop-success",
                "session-action.restart-success",
                "session-action.restart-agent-success",
                "session-action.resume-agent-success",
                "session-action.remove-success",
            ].contains(id), response.status == 200 {
                let receipt = try XCTUnwrap(
                    JSONSerialization.jsonObject(with: Data(response.body.utf8))
                        as? [String: Any]
                )
                XCTAssertEqual(receipt["ok"] as? Bool, true, "conformance case \(id)")
            }

            if id == "bootstrap.valid", response.status == 200 {
                let decoded = try JSONDecoder().decode(
                    RemoteBootstrapSnapshot.self,
                    from: Data(response.body.utf8)
                )
                XCTAssertEqual(decoded.hostProtocol, MobileRemoteServer.hostProtocol)
                XCTAssertEqual(decoded.macID, "conformance-host")
                // The fixture snapshot carries `1`; Rust owns and refreshes
                // this metadata after the authenticated adapter boundary.
                XCTAssertGreaterThan(decoded.capturedAtUnixMs, 1)
            }
            if id == "archive.known-empty", response.status == 200 {
                let decoded = try JSONDecoder().decode(
                    RemoteArchivedSessionsResponse.self,
                    from: Data(response.body.utf8)
                )
                XCTAssertEqual(decoded.projectID, "conformance-project")
                XCTAssertTrue(decoded.sessions.isEmpty)
            }
        }

        let recorded = effects.snapshot()
        XCTAssertEqual(recorded.restarts, ["conformance-restart"])
        XCTAssertEqual(recorded.actions, [
            RemoteSessionActionRequest(sessionID: "conformance-stop-live", action: .stop),
            RemoteSessionActionRequest(sessionID: "conformance-action-restart", action: .restart),
            RemoteSessionActionRequest(
                sessionID: "conformance-action-restart-agent", action: .restartAgent
            ),
            RemoteSessionActionRequest(
                sessionID: "conformance-action-resume-agent", action: .resumeAgent
            ),
            RemoteSessionActionRequest(sessionID: "conformance-remove", action: .remove),
        ])
    }

    private func fixtureJSON(_ name: String) throws -> [String: Any] {
        var directory = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        for _ in 0..<10 {
            let candidate = directory
                .appendingPathComponent("protocol")
                .appendingPathComponent(name)
            if FileManager.default.fileExists(atPath: candidate.path) {
                let data = try Data(contentsOf: candidate)
                return try XCTUnwrap(
                    JSONSerialization.jsonObject(with: data) as? [String: Any]
                )
            }
            directory.deleteLastPathComponent()
        }
        XCTFail("could not locate protocol/\(name) from \(#filePath)")
        return [:]
    }
}
