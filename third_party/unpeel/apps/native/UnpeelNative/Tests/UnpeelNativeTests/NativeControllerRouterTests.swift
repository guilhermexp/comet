import Foundation
import XCTest
import UnpeelShared
@testable import UnpeelNative

final class NativeControllerRouterTests: XCTestCase {
    private let principal = NativeControllerPrincipal(deviceID: "phone-1", name: "Phone")

    func testMetricsValidationCrossesNativeBridge() {
        let result = NativeControllerRouter.shared.route(
            requestID: nil,
            method: "GET",
            path: "/mobile/metrics",
            query: [:],
            headers: [:],
            body: Data(),
            principal: principal,
            routeContext: nil
        )
        guard case .handled(let response) = result else {
            return XCTFail("expected Rust to own the metrics route, got \(result)")
        }
        XCTAssertEqual(response.status, 400)
        XCTAssertTrue(response.body.contains("invalid session id"))
    }

    func testMutatingRouteValidationCrossesNativeBridge() {
        for path in [
            "/mobile/write",
            "/mobile/resize",
            "/mobile/mark-read",
            "/mobile/request-screenshot",
            "/mobile/artifact-delete",
        ] {
            let result = NativeControllerRouter.shared.route(
                requestID: nil,
                method: "POST",
                path: path,
                query: [:],
                headers: ["content-type": "application/json"],
                body: Data(#"{}"#.utf8),
                principal: principal,
                routeContext: nil
            )
            guard case .handled(let response) = result else {
                return XCTFail("expected Rust to own \(path), got \(result)")
            }
            XCTAssertEqual(response.status, 400, path)
            XCTAssertTrue(response.body.contains("invalid session id"), path)
        }
    }

    func testArtifactListValidationCrossesNativeBridge() {
        let result = NativeControllerRouter.shared.route(
            requestID: nil,
            method: "GET",
            path: "/mobile/artifacts",
            query: [:],
            headers: [:],
            body: Data(),
            principal: principal,
            routeContext: nil
        )
        guard case .handled(let response) = result else {
            return XCTFail("expected Rust to own artifact listing, got \(result)")
        }
        XCTAssertEqual(response.status, 400)
        XCTAssertTrue(response.body.contains("invalid session id"))
    }

    func testOriginalArtifactReadCrossesNativeBridge() throws {
        let sessionID = "test-controller-artifact-\(UUID().uuidString.prefix(8))"
        let sessionRoot = LaunchConfig.appSessionsDir.appendingPathComponent(sessionID)
        let screenshots = sessionRoot
            .appendingPathComponent("artifacts/browser/screenshots", isDirectory: true)
        try FileManager.default.createDirectory(at: screenshots, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: sessionRoot) }
        try Data("0123456789".utf8).write(to: screenshots.appendingPathComponent("result.txt"))

        let result = NativeControllerRouter.shared.route(
            requestID: nil,
            method: "GET",
            path: "/mobile/artifact",
            query: [
                "session_id": sessionID,
                "kind": "screenshots",
                "name": "result.txt",
                "offset": "3",
                "limit": "4",
            ],
            headers: [:],
            body: Data(),
            principal: principal,
            routeContext: nil
        )
        guard case .handled(let response) = result else {
            return XCTFail("expected Rust to own original artifact reads, got \(result)")
        }
        XCTAssertEqual(response.status, 200)
        let body = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(response.body.utf8)) as? [String: Any]
        )
        XCTAssertEqual(body["contentType"] as? String, "text/plain; charset=utf-8")
        XCTAssertEqual(body["offset"] as? Int, 3)
        XCTAssertEqual(body["nextOffset"] as? Int, 7)
        XCTAssertEqual(body["totalSize"] as? Int, 10)
        XCTAssertEqual(
            Data(base64Encoded: try XCTUnwrap(body["dataBase64"] as? String)),
            Data("3456".utf8)
        )
    }

    func testArchiveContextCrossesNativeBridge() throws {
        let context = try JSONSerialization.data(withJSONObject: [
            "archivedSessionsByProject": [
                "project-1": [[
                    "id": "archived-1",
                    "projectID": "project-1",
                    "title": "Archived session",
                ]],
            ],
        ])
        let result = NativeControllerRouter.shared.route(
            requestID: "archive-request",
            method: "GET",
            path: "/mobile/archive",
            query: ["project_id": "project-1"],
            headers: [:],
            body: Data(),
            principal: principal,
            routeContext: context
        )
        guard case .handled(let response) = result else {
            return XCTFail("expected Rust to own archive listing, got \(result)")
        }
        XCTAssertEqual(response.status, 200)
        let body = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(response.body.utf8)) as? [String: Any]
        )
        XCTAssertEqual(body["projectID"] as? String, "project-1")
        let sessions = try XCTUnwrap(body["sessions"] as? [[String: Any]])
        XCTAssertEqual(sessions.first?["id"] as? String, "archived-1")
    }

    func testUnknownRouteReturnsUnhandled() {
        let result = NativeControllerRouter.shared.route(
            requestID: nil,
            method: "GET",
            path: "/mobile/not-migrated",
            query: [:],
            headers: [:],
            body: Data(),
            principal: principal,
            routeContext: nil
        )
        XCTAssertEqual(result, .unhandled)
    }

    func testBinaryBodyUsesBase64Envelope() {
        let result = NativeControllerRouter.shared.route(
            requestID: nil,
            method: "POST",
            path: "/mobile/not-migrated",
            query: [:],
            headers: ["content-type": "application/octet-stream"],
            body: Data([0xFF, 0x00, 0x80]),
            principal: principal,
            routeContext: nil
        )
        XCTAssertEqual(result, .unhandled)
    }
}

final class NativeControllerServerBoundaryTests: XCTestCase {
    private final class MemoryE2EKeyStore: MobileE2EKeyStoring {
        private var values: [String: Data] = [:]

        func load(deviceID: String) -> Data? { values[deviceID] }
        func save(_ key: Data, deviceID: String) throws { values[deviceID] = key }
        func delete(deviceID: String) { values.removeValue(forKey: deviceID) }
    }

    private final class RecordingControllerRouter: NativeControllerRouting, @unchecked Sendable {
        struct Call: Sendable {
            let requestID: String?
            let method: String
            let path: String
            let principal: NativeControllerPrincipal
        }

        private let lock = NSLock()
        private let result: NativeControllerRouteResult
        private var recordedCalls: [Call] = []

        init(result: NativeControllerRouteResult) {
            self.result = result
        }

        var calls: [Call] {
            lock.withLock { recordedCalls }
        }

        func route(
            requestID: String?,
            method: String,
            path: String,
            query _: [String: String],
            headers _: [String: String],
            body _: Data,
            principal: NativeControllerPrincipal,
            routeContext _: Data?
        ) -> NativeControllerRouteResult {
            lock.withLock {
                recordedCalls.append(Call(
                    requestID: requestID,
                    method: method,
                    path: path,
                    principal: principal
                ))
            }
            return result
        }
    }

    private final class LockedFlag: @unchecked Sendable {
        private let lock = NSLock()
        private var value = false

        func set() { lock.withLock { value = true } }
        var isSet: Bool { lock.withLock { value } }
    }

    private final class LockedString: @unchecked Sendable {
        private let lock = NSLock()
        private var value: String?

        func set(_ newValue: String) { lock.withLock { value = newValue } }
        var current: String? { lock.withLock { value } }
    }

    private final class LockedCounter: @unchecked Sendable {
        private let lock = NSLock()
        private var value = 0

        @discardableResult
        func increment() -> Int {
            lock.withLock {
                value += 1
                return value
            }
        }

        var current: Int { lock.withLock { value } }
    }

    private actor AsyncGate {
        private var isOpen = false
        private var waiters: [CheckedContinuation<Void, Never>] = []

        func wait() async {
            if isOpen { return }
            await withCheckedContinuation { continuation in
                waiters.append(continuation)
            }
        }

        func open() {
            guard !isOpen else { return }
            isOpen = true
            let pending = waiters
            waiters.removeAll()
            for waiter in pending { waiter.resume() }
        }
    }

    func testUnauthorizedRequestNeverInvokesControllerRouter() throws {
        let fixture = try makePairingFixture(pair: false)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let router = RecordingControllerRouter(result: .unhandled)
        let archiveProviderCalled = LockedFlag()
        let server = try XCTUnwrap(makeServer(
            pairingStore: fixture.store,
            router: router,
            archivedSessions: { projectID in
                archiveProviderCalled.set()
                return RemoteArchivedSessionsResponse(projectID: projectID, sessions: [])
            }
        ))
        let response = server.handle(request(
            method: "GET",
            path: "/mobile/archive",
            authToken: nil,
            query: ["project_id": "project-1"]
        ))

        XCTAssertEqual(response.status, 401)
        XCTAssertTrue(router.calls.isEmpty)
        XCTAssertFalse(archiveProviderCalled.isSet)
    }

    func testOversizedWriteIDIsRejectedBeforeEitherRouterCanRetainIt() throws {
        let fixture = try makePairingFixture(pair: true)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let router = RecordingControllerRouter(result: .unhandled)
        let server = try XCTUnwrap(makeServer(
            pairingStore: fixture.store,
            router: router
        ))
        let body = try JSONEncoder().encode(RemoteTerminalWriteRequest(
            sessionID: "session-1",
            data: "x",
            writeID: String(repeating: "a", count: MobileRemoteServer.maxWriteIDBytes + 1)
        ))

        let response = server.handle(request(
            method: "POST",
            path: "/mobile/write",
            authToken: try XCTUnwrap(fixture.authToken),
            body: body
        ))

        XCTAssertEqual(response.status, 400)
        XCTAssertTrue(response.body.contains("write id too long"))
        XCTAssertTrue(router.calls.isEmpty)
    }

    func testAuthenticatedArchiveUsesSharedRouterAndNativeSnapshot() async throws {
        let fixture = try makePairingFixture(pair: true)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let archiveProviderCalled = LockedFlag()
        let archived = RemoteSessionSummary(
            id: "archived-1",
            projectID: "project-1",
            title: "Archived session",
            command: "claude",
            createdAtUnixMs: 7,
            status: .exited,
            activity: .idle,
            archived: true
        )
        let server = try XCTUnwrap(makeServer(
            pairingStore: fixture.store,
            router: NativeControllerRouter.shared,
            archivedSessions: { projectID in
                archiveProviderCalled.set()
                guard projectID == "project-1" else { return nil }
                return RemoteArchivedSessionsResponse(
                    projectID: projectID,
                    sessions: [archived]
                )
            }
        ))
        let archiveRequest = request(
            method: "GET",
            path: "/mobile/archive",
            authToken: try XCTUnwrap(fixture.authToken),
            query: ["project_id": "project-1"]
        )
        let response = await Task.detached { server.handle(archiveRequest) }.value

        XCTAssertEqual(response.status, 200)
        XCTAssertTrue(archiveProviderCalled.isSet)
        let decoded = try JSONDecoder().decode(
            RemoteArchivedSessionsResponse.self,
            from: Data(response.body.utf8)
        )
        XCTAssertEqual(decoded.projectID, "project-1")
        XCTAssertEqual(decoded.sessions, [archived])
    }

    func testAuthenticatedThumbnailRequestRejectsArtifactSymlink() async throws {
        let fixture = try makePairingFixture(pair: true)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }

        let sessionID = "test-controller-symlink-\(UUID().uuidString.prefix(8))"
        let sessionRoot = LaunchConfig.appSessionsDir.appendingPathComponent(sessionID)
        let screenshots = sessionRoot
            .appendingPathComponent("artifacts/browser/screenshots", isDirectory: true)
        let outside = FileManager.default.temporaryDirectory
            .appendingPathComponent("unpeel-artifact-secret-\(UUID().uuidString).png")
        try FileManager.default.createDirectory(at: screenshots, withIntermediateDirectories: true)
        try Data("outside-secret".utf8).write(to: outside)
        try FileManager.default.createSymbolicLink(
            at: screenshots.appendingPathComponent("linked.png"),
            withDestinationURL: outside
        )
        defer {
            try? FileManager.default.removeItem(at: sessionRoot)
            try? FileManager.default.removeItem(at: outside)
        }

        let server = try XCTUnwrap(makeServer(
            pairingStore: fixture.store,
            router: NativeControllerRouter.shared
        ))
        let artifactRequest = request(
            method: "GET",
            path: "/mobile/artifact",
            authToken: try XCTUnwrap(fixture.authToken),
            query: [
                "session_id": sessionID,
                "kind": "screenshots",
                "name": "linked.png",
                "max_dim": "1",
            ]
        )
        let response = await Task.detached { server.handle(artifactRequest) }.value

        XCTAssertEqual(response.status, 404)
        XCTAssertFalse(response.body.contains("outside-secret"))
    }

    func testHandledMetricsAddsDesktopViewingAndAuthenticatedPrincipal() async throws {
        let fixture = try makePairingFixture(pair: true)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let router = RecordingControllerRouter(result: .handled(NativeControllerResponse(
            status: 200,
            body: #"{"sessionID":"session-1","columns":80,"rows":24,"capturedAtUnixMs":7,"desktopViewing":false}"#
        )))
        let server = try XCTUnwrap(makeServer(
            pairingStore: fixture.store,
            router: router,
            desktopViewing: true
        ))
        let request = request(
            method: "GET",
            path: "/mobile/metrics",
            authToken: try XCTUnwrap(fixture.authToken),
            query: ["session_id": "session-1"],
            requestID: "relay-42"
        )
        let response = await Task.detached { server.handle(request) }.value

        XCTAssertEqual(response.status, 200)
        let body = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(response.body.utf8)) as? [String: Any]
        )
        XCTAssertEqual(body["desktopViewing"] as? Bool, true)
        XCTAssertEqual(router.calls.count, 1)
        XCTAssertEqual(router.calls.first?.requestID, "relay-42")
        XCTAssertEqual(router.calls.first?.principal, NativeControllerPrincipal(
            deviceID: "controller-1",
            name: "Controller"
        ))
    }

    func testUnhandledRouteFallsBackToCompatibilityAdapter() async throws {
        let fixture = try makePairingFixture(pair: true)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let router = RecordingControllerRouter(result: .unhandled)
        let markedRead = LockedFlag()
        let server = try XCTUnwrap(makeServer(
            pairingStore: fixture.store,
            router: router,
            markRead: { _ in markedRead.set() }
        ))
        let body = try JSONEncoder().encode(RemoteMarkReadRequest(sessionID: "session-1"))
        let request = request(
            method: "POST",
            path: "/mobile/mark-read",
            authToken: try XCTUnwrap(fixture.authToken),
            body: body
        )
        let response = await Task.detached { server.handle(request) }.value

        XCTAssertEqual(response.status, 200)
        XCTAssertTrue(markedRead.isSet)
        XCTAssertEqual(router.calls.first?.path, "/mobile/mark-read")
    }

    func testCompatibilityRestartAgentReturnsHostRejectionReceipt() async throws {
        let fixture = try makePairingFixture(pair: true)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let providerCalled = LockedFlag()
        let server = try XCTUnwrap(makeServer(
            pairingStore: fixture.store,
            router: NativeControllerRouter.shared,
            sessionAction: { request in
                XCTAssertEqual(request.action, .restartAgent)
                providerCalled.set()
                throw MobileRemoteError(409, "Agent foreground changed")
            }
        ))
        let body = try JSONEncoder().encode(RemoteSessionActionRequest(
            sessionID: "session-1",
            action: .restartAgent
        ))
        let request = request(
            method: "POST",
            path: "/mobile/session-action",
            authToken: try XCTUnwrap(fixture.authToken),
            body: body
        )

        let response = await Task.detached { server.handle(request) }.value

        XCTAssertEqual(response.status, 409)
        XCTAssertTrue(providerCalled.isSet)
        XCTAssertTrue(response.body.contains("Agent foreground changed"))
    }

    func testCompatibilityRestartAgentReceiptDoesNotBlockMainActor() async throws {
        let fixture = try makePairingFixture(pair: true)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let commandStarted = expectation(description: "Host command started")
        let mainActorAdvanced = expectation(description: "MainActor stayed live")
        let releaseCommand = DispatchSemaphore(value: 0)
        let providerCompleted = LockedFlag()
        let commandRanOnMainThread = LockedFlag()
        let server = try XCTUnwrap(makeServer(
            pairingStore: fixture.store,
            router: RecordingControllerRouter(result: .unhandled),
            sessionAction: { request in
                XCTAssertEqual(request.action, .restartAgent)
                let failure = await UnpeelStore.runResumeAgentHostCommandOffMainActor(
                    sessionID: request.sessionID,
                    runner: { _ in
                        if Thread.isMainThread { commandRanOnMainThread.set() }
                        commandStarted.fulfill()
                        guard releaseCommand.wait(timeout: .now() + 2) == .success else {
                            return ResumeAgentHostCommandFailure(
                                status: 500,
                                message: "test Host command timed out"
                            )
                        }
                        return nil
                    }
                )
                if let failure {
                    throw MobileRemoteError(failure.status, failure.message)
                }
                providerCompleted.set()
            }
        ))
        let body = try JSONEncoder().encode(RemoteSessionActionRequest(
            sessionID: "session-1",
            action: .restartAgent
        ))
        let request = request(
            method: "POST",
            path: "/mobile/session-action",
            authToken: try XCTUnwrap(fixture.authToken),
            body: body
        )

        let responseTask = Task.detached { server.handle(request) }
        await fulfillment(of: [commandStarted], timeout: 1)
        Task { @MainActor in mainActorAdvanced.fulfill() }
        await fulfillment(of: [mainActorAdvanced], timeout: 0.25)

        XCTAssertFalse(commandRanOnMainThread.isSet)
        XCTAssertFalse(providerCompleted.isSet, "effect receipt returned before Host completion")
        releaseCommand.signal()
        let response = await responseTask.value

        XCTAssertEqual(response.status, 200)
        XCTAssertTrue(providerCompleted.isSet)
    }

    func testCompatibilityRestartAgentIsSingleFlightAndReplaysExactReceipt() async throws {
        let fixture = try makePairingFixture(pair: true)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let router = RecordingControllerRouter(result: .unhandled)
        let invocationCount = LockedCounter()
        let runtimeGeneration = LockedCounter()
        let commandStarted = expectation(description: "first Host restart started")
        let duplicateInvocation = expectation(description: "duplicate Host restart")
        duplicateInvocation.isInverted = true
        let releaseCommand = AsyncGate()
        let server = try XCTUnwrap(makeServer(
            pairingStore: fixture.store,
            router: router,
            sessionAction: { request in
                XCTAssertEqual(request.action, .restartAgent)
                if invocationCount.increment() == 1 {
                    commandStarted.fulfill()
                } else {
                    duplicateInvocation.fulfill()
                }
                await releaseCommand.wait()
                runtimeGeneration.increment()
            }
        ))
        let body = try JSONEncoder().encode(RemoteSessionActionRequest(
            sessionID: "session-1",
            action: .restartAgent
        ))
        let restartRequest = request(
            method: "POST",
            path: "/mobile/session-action",
            authToken: try XCTUnwrap(fixture.authToken),
            body: body,
            requestID: "restart-agent-once"
        )

        let leader = Task.detached { server.handle(restartRequest) }
        await fulfillment(of: [commandStarted], timeout: 1)
        let follower = Task.detached { server.handle(restartRequest) }

        // If the fallback lacks single-flight protection, the suspended
        // MainActor provider admits the second invocation immediately.
        await fulfillment(of: [duplicateInvocation], timeout: 0.1)
        XCTAssertEqual(invocationCount.current, 1)
        XCTAssertEqual(runtimeGeneration.current, 0)

        await releaseCommand.open()
        let leaderResponse = await leader.value
        let followerResponse = await follower.value
        XCTAssertEqual(leaderResponse.status, 200)
        XCTAssertEqual(followerResponse.status, leaderResponse.status)
        XCTAssertEqual(followerResponse.body, leaderResponse.body)
        XCTAssertEqual(invocationCount.current, 1)
        XCTAssertEqual(runtimeGeneration.current, 1)

        // A sequential transport retry gets the completed receipt without
        // advancing the managed runtime generation a second time.
        let replay = await Task.detached { server.handle(restartRequest) }.value
        XCTAssertEqual(replay.status, leaderResponse.status)
        XCTAssertEqual(replay.body, leaderResponse.body)
        XCTAssertEqual(invocationCount.current, 1)
        XCTAssertEqual(runtimeGeneration.current, 1)

        // The id is bound to the semantic request, even after completion.
        let differentBody = try JSONEncoder().encode(RemoteSessionActionRequest(
            sessionID: "session-2",
            action: .restartAgent
        ))
        let mismatchRequest = request(
            method: "POST",
            path: "/mobile/session-action",
            authToken: try XCTUnwrap(fixture.authToken),
            body: differentBody,
            requestID: "restart-agent-once"
        )
        let mismatch = await Task.detached { server.handle(mismatchRequest) }.value
        XCTAssertEqual(mismatch.status, 409)
        XCTAssertTrue(mismatch.body.contains("request id reused with different request"))
        XCTAssertEqual(invocationCount.current, 1)
        XCTAssertEqual(runtimeGeneration.current, 1)
    }

    func testCompatibilityLifecycleFailureReceiptIsReplayedWithoutAnotherHostCall() async throws {
        let fixture = try makePairingFixture(pair: true)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let invocationCount = LockedCounter()
        let server = try XCTUnwrap(makeServer(
            pairingStore: fixture.store,
            router: RecordingControllerRouter(result: .unhandled),
            sessionAction: { _ in
                invocationCount.increment()
                throw MobileRemoteError(409, "Agent foreground changed")
            }
        ))
        let body = try JSONEncoder().encode(RemoteSessionActionRequest(
            sessionID: "session-1",
            action: .restartAgent
        ))
        let restartRequest = request(
            method: "POST",
            path: "/mobile/session-action",
            authToken: try XCTUnwrap(fixture.authToken),
            body: body,
            requestID: "restart-agent-rejection"
        )

        let first = await Task.detached { server.handle(restartRequest) }.value
        let replay = await Task.detached { server.handle(restartRequest) }.value

        XCTAssertEqual(first.status, 409)
        XCTAssertEqual(replay.status, first.status)
        XCTAssertEqual(replay.body, first.body)
        XCTAssertEqual(invocationCount.current, 1)
    }

    func testHandledMarkReadAppliesNativePresentationEffect() async throws {
        let fixture = try makePairingFixture(pair: true)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let router = RecordingControllerRouter(result: .handled(NativeControllerResponse(
            status: 200,
            body: #"{"ok":true}"#
        )))
        let markedRead = LockedFlag()
        let server = try XCTUnwrap(makeServer(
            pairingStore: fixture.store,
            router: router,
            markRead: { _ in markedRead.set() }
        ))
        let body = try JSONEncoder().encode(RemoteMarkReadRequest(sessionID: "session-1"))
        let request = request(
            method: "POST",
            path: "/mobile/mark-read",
            authToken: try XCTUnwrap(fixture.authToken),
            body: body
        )
        let response = await Task.detached { server.handle(request) }.value

        XCTAssertEqual(response.status, 200)
        XCTAssertTrue(markedRead.isSet)
        XCTAssertEqual(router.calls.count, 1)
    }

    func testUncertainMutatingBridgeFailureIsNeverReplayed() async throws {
        let fixture = try makePairingFixture(pair: true)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let router = RecordingControllerRouter(result: .bridgeError("uncertain failure"))
        let markedRead = LockedFlag()
        let server = try XCTUnwrap(makeServer(
            pairingStore: fixture.store,
            router: router,
            markRead: { _ in markedRead.set() }
        ))
        let body = try JSONEncoder().encode(RemoteMarkReadRequest(sessionID: "session-1"))
        let request = request(
            method: "POST",
            path: "/mobile/mark-read",
            authToken: try XCTUnwrap(fixture.authToken),
            body: body
        )
        let response = await Task.detached { server.handle(request) }.value

        XCTAssertEqual(response.status, 500)
        XCTAssertFalse(markedRead.isSet)
        XCTAssertEqual(router.calls.count, 1)
    }

    func testPreCallBridgeUnavailabilityCanUseCompatibilityAdapter() async throws {
        let fixture = try makePairingFixture(pair: true)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let router = RecordingControllerRouter(result: .bridgeUnavailable("ABI mismatch"))
        let markedRead = LockedFlag()
        let server = try XCTUnwrap(makeServer(
            pairingStore: fixture.store,
            router: router,
            markRead: { _ in markedRead.set() }
        ))
        let body = try JSONEncoder().encode(RemoteMarkReadRequest(sessionID: "session-1"))
        let request = request(
            method: "POST",
            path: "/mobile/mark-read",
            authToken: try XCTUnwrap(fixture.authToken),
            body: body
        )
        let response = await Task.detached { server.handle(request) }.value

        XCTAssertEqual(response.status, 200)
        XCTAssertTrue(markedRead.isSet)
    }

    func testMutatingGETBridgeFailureIsNeverReplayed() async throws {
        let fixture = try makePairingFixture(pair: true)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let router = RecordingControllerRouter(result: .bridgeError("uncertain failure"))
        let server = try XCTUnwrap(makeServer(pairingStore: fixture.store, router: router))
        let request = request(
            method: "GET",
            path: "/mobile/relay-credentials",
            authToken: try XCTUnwrap(fixture.authToken)
        )
        let response = await Task.detached { server.handle(request) }.value

        XCTAssertEqual(response.status, 500)
        XCTAssertEqual(router.calls.count, 1)
    }

    func testSuccessfulPairingReportsTheConsumedPresentationToken() async throws {
        let fixture = try makePairingFixture(pair: false)
        defer { try? FileManager.default.removeItem(at: fixture.directory) }
        let router = RecordingControllerRouter(result: .unhandled)
        let completedToken = LockedString()
        let server = try XCTUnwrap(makeServer(
            pairingStore: fixture.store,
            router: router,
            onPairingCompleted: { completedToken.set($0) }
        ))
        let endpoint = URL(string: "http://127.0.0.1:1/mobile")!
        let payload = fixture.store.beginPairing(endpoint: endpoint)
        let pairingRequest = RemotePairingRequest(
            token: payload.token,
            device: RemoteDeviceIdentity(
                id: "controller-1",
                name: "Controller Mac",
                platform: "macOS"
            )
        )
        let plaintext = try JSONEncoder().encode(pairingRequest)
        let envelope = try RemotePairingCrypto.seal(
            plaintext,
            token: payload.token,
            macID: payload.macID,
            endpoint: payload.endpoint,
            direction: .request
        )
        let httpRequest = request(
            method: "POST",
            path: "/mobile/pair",
            authToken: nil,
            body: try JSONEncoder().encode(envelope)
        )
        let response = await Task.detached { server.handle(httpRequest) }.value

        XCTAssertEqual(response.status, 200)
        XCTAssertEqual(completedToken.current, payload.token)
        XCTAssertEqual(fixture.store.devices.map(\.id), ["controller-1"])
    }

    private func makeServer(
        pairingStore: MobilePairingStore,
        router: any NativeControllerRouting,
        desktopViewing: Bool = false,
        markRead: @escaping MobileRemoteServer.MarkReadProvider = { _ in },
        sessionAction: @escaping MobileRemoteServer.SessionActionProvider = { _ in },
        archivedSessions: @escaping MobileRemoteServer.ArchivedSessionsProvider = { _ in nil },
        onPairingCompleted: @escaping @Sendable (String) -> Void = { _ in }
    ) -> MobileRemoteServer? {
        MobileRemoteServer(
            pairingStore: pairingStore,
            bootstrapProvider: {
                RemoteBootstrapSnapshot(
                    folders: [],
                    projects: [],
                    presets: [],
                    sessions: [],
                    capturedAtUnixMs: 1
                )
            },
            createSessionProvider: { _ in RemoteCreateSessionResponse(sessionID: "created") },
            resizeDesktopProvider: { _ in },
            sessionOrganizationProvider: { _ in },
            restartSessionProvider: { _ in },
            sessionActionProvider: sessionAction,
            markReadProvider: markRead,
            approvalAnswerProvider: { _ in },
            desktopViewingProvider: { _ in desktopViewing },
            archivedSessionsProvider: archivedSessions,
            controllerRouter: router,
            onDevicesChanged: {},
            onPairingCompleted: onPairingCompleted,
            startNetworkServer: false
        )
    }

    private func makePairingFixture(
        pair: Bool
    ) throws -> (directory: URL, store: MobilePairingStore, authToken: String?) {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("unpeel-native-router-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let store = MobilePairingStore(
            storageURL: directory.appendingPathComponent("devices.json"),
            macID: "host-1",
            macName: "Host",
            e2eKeyStore: MemoryE2EKeyStore()
        )
        guard pair else { return (directory, store, nil) }
        let payload = store.beginPairing(endpoint: URL(string: "http://127.0.0.1:1/mobile")!)
        let response = try store.pair(RemotePairingRequest(
            token: payload.token,
            device: RemoteDeviceIdentity(
                id: "controller-1",
                name: "Controller",
                platform: "test"
            )
        ))
        return (directory, store, response.authToken)
    }

    private func request(
        method: String,
        path: String,
        authToken: String?,
        query: [String: String] = [:],
        body: Data = Data(),
        requestID: String? = nil
    ) -> MobileRemoteServer.HTTPRequest {
        var headers: [String: String] = [:]
        if let authToken { headers["authorization"] = "Bearer \(authToken)" }
        return MobileRemoteServer.HTTPRequest(
            requestID: requestID,
            method: method,
            rawPath: path,
            httpVersion: "HTTP/1.1",
            path: path,
            query: query,
            headers: headers,
            body: body
        )
    }
}
