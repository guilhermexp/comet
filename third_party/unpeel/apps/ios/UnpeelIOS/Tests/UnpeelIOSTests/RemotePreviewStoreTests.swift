import XCTest
import UnpeelShared
@testable import UnpeelIOS

@MainActor
private final class BootstrapResponseGate {
    private var continuation: CheckedContinuation<RemoteBootstrapSnapshot, Error>?

    func response() async throws -> RemoteBootstrapSnapshot {
        try await withCheckedThrowingContinuation { continuation = $0 }
    }

    func waitUntilStarted() async {
        while continuation == nil { await Task.yield() }
    }

    func succeed(with snapshot: RemoteBootstrapSnapshot) {
        continuation?.resume(returning: snapshot)
        continuation = nil
    }
}

@MainActor
final class RemotePreviewStoreTests: XCTestCase {
    func testInitialSelectionPrefersAttentionSession() {
        let store = RemotePreviewStore.preview

        XCTAssertEqual(store.selectedSession?.activity, .blocked)
    }

    func testInitialSelectionIncludesEveryProviderTerminal() {
        let base = RemoteBootstrapSnapshot.mock
        let amp = RemoteSessionSummary(
            id: "amp-session",
            projectID: "project-unpeel",
            providerID: "amp",
            title: "Amp terminal",
            command: "amp",
            createdAtUnixMs: 1,
            status: .running,
            activity: .blocked
        )
        let codex = RemoteSessionSummary(
            id: "codex-session",
            projectID: "project-unpeel",
            providerID: "codex",
            title: "Codex terminal",
            command: "codex",
            createdAtUnixMs: 2,
            status: .running,
            activity: .idle
        )
        let snapshot = RemoteBootstrapSnapshot(
            protocolVersion: base.protocolVersion,
            macID: base.macID,
            macName: base.macName,
            folders: base.folders,
            projects: base.projects,
            presets: base.presets,
            sessions: [amp, codex],
            capturedAtUnixMs: base.capturedAtUnixMs
        )
        let store = RemotePreviewStore(snapshot: snapshot)

        XCTAssertTrue(amp.supportsIOSSessionAPI)
        XCTAssertTrue(codex.supportsIOSSessionAPI)
        XCTAssertEqual(store.selectedSessionID, amp.id)

        store.select(codex)
        XCTAssertEqual(store.selectedSessionID, codex.id)
    }

    func testSelectDismissesSessionsDrawer() {
        let store = RemotePreviewStore.preview
        let target = store.snapshot.sessions.last!
        store.sessionsDrawerPresented = true

        store.select(target)

        XCTAssertEqual(store.selectedSessionID, target.id)
        XCTAssertFalse(store.sessionsDrawerPresented)
    }

    // MARK: - Activity sheet

    func testBellActivityBucketsKeepSelectedBlockersFirstAndDeduplicated() {
        let blockedZ = activitySession(
            id: "blocked-z", activity: .blocked, updatedAtUnixMs: 300
        )
        let blockedA = activitySession(
            id: "blocked-a", activity: .blocked, updatedAtUnixMs: 300
        )
        let blockedOld = activitySession(
            id: "blocked-old", activity: .blocked, updatedAtUnixMs: 100
        )
        let working = activitySession(
            id: "working", activity: .working, updatedAtUnixMs: 200
        )
        let blockedAliasWorking = activitySession(
            id: "blocked-z", activity: .working, updatedAtUnixMs: 400
        )
        let finished = activitySession(
            id: "finished", activity: .done, updatedAtUnixMs: 500, unread: true
        )
        let snapshot = sessionCreationSnapshot(
            hostProtocol: nil,
            sessions: [
                blockedOld,
                working,
                blockedZ,
                finished,
                blockedA,
                blockedZ,
                blockedAliasWorking,
            ]
        )
        let store = RemotePreviewStore(snapshot: snapshot)
        // The currently open blocker must still be visible in the bell sheet.
        store.selectedSessionID = blockedZ.id

        XCTAssertEqual(
            store.bellBlockedSessions.map(\.id),
            ["blocked-a", "blocked-z", "blocked-old"]
        )
        XCTAssertEqual(store.bellActiveSessions.map(\.id), ["working"])
        XCTAssertEqual(store.bellRecentSessions.map(\.id), ["finished"])

        let blockerIDs = Set(store.bellBlockedSessions.map(\.id))
        XCTAssertTrue(blockerIDs.isDisjoint(with: store.bellActiveSessions.map(\.id)))
        XCTAssertTrue(blockerIDs.isDisjoint(with: store.bellRecentSessions.map(\.id)))
    }

    // MARK: - Disconnected transport recovery

    func testDisconnectedRecoveryGoesDirectlyToRelay() async {
        var relayAttempts = 0
        let store = RemotePreviewStore(
            snapshot: .empty,
            client: RemoteMacClient(),
            createSessionOverride: nil,
            bootstrapOverride: {
                throw URLError(.cannotConnectToHost)
            }
        )
        store.attemptRelayFallback = {
            relayAttempts += 1
            return true
        }

        await store.loadFromBridge()
        XCTAssertTrue(store.isDisconnected)

        let recovered = await store.recoverDisconnectedConnection()

        XCTAssertTrue(recovered)
        XCTAssertEqual(relayAttempts, 1)
    }

    func testConnectedStateNeverStartsRelayRecovery() async {
        var relayAttempts = 0
        let store = RemotePreviewStore(snapshot: .empty)
        store.attemptRelayFallback = {
            relayAttempts += 1
            return true
        }

        let recovered = await store.recoverDisconnectedConnection()

        XCTAssertFalse(recovered)
        XCTAssertEqual(relayAttempts, 0)
    }

    func testSuccessfulPollProofCapturesExactClientAndConnectionEpoch() async {
        let client = RemoteMacClient(
            baseURL: URL(string: "http://10.0.0.1:4485")!,
            authToken: "bearer-a"
        )
        let store = RemotePreviewStore(
            snapshot: .empty,
            client: client,
            createSessionOverride: nil,
            bootstrapOverride: { .mock }
        )
        store.adoptClient(client, connectionEpoch: 17)

        let pollResult = await store.loadFromBridge()
        guard case .success(let proof) = pollResult else {
            return XCTFail("expected a successful poll proof")
        }

        XCTAssertEqual(proof.connectionEpoch, 17)
        XCTAssertEqual(proof.hostMacID, RemoteBootstrapSnapshot.mock.macID)
        XCTAssertEqual(proof.client.baseURL, client.baseURL)
        XCTAssertEqual(proof.client.authToken, "bearer-a")
        XCTAssertFalse(proof.client.isRelay)
    }

    func testPollResponseIsDiscardedAfterCrossMacClientGenerationChange() async {
        let gate = BootstrapResponseGate()
        let clientA = RemoteMacClient(
            baseURL: URL(string: "http://10.0.0.1:4485")!,
            authToken: "bearer-a"
        )
        let clientB = RemoteMacClient(
            baseURL: URL(string: "http://10.0.0.2:4485")!,
            authToken: "bearer-b"
        )
        let store = RemotePreviewStore(
            snapshot: .empty,
            client: clientA,
            createSessionOverride: nil,
            bootstrapOverride: { try await gate.response() }
        )
        store.adoptClient(clientA, connectionEpoch: 1)

        let poll = Task { await store.loadFromBridge() }
        await gate.waitUntilStarted()
        store.adoptClient(clientB, connectionEpoch: 2)
        gate.succeed(with: .mock)

        let pollResult = await poll.value
        guard case .superseded = pollResult else {
            return XCTFail("Mac A success must not be attributed to current Mac B")
        }
        XCTAssertNil(store.snapshot.macID, "stale Mac A snapshot must not be applied")
        XCTAssertEqual(store.client.baseURL, clientB.baseURL)
        XCTAssertEqual(store.client.authToken, "bearer-b")
    }

    func testSupersededPollNeverUsesStickyDisconnectedStateForNewMacFallback() async {
        let gate = BootstrapResponseGate()
        var bootstrapCalls = 0
        let clientA = RemoteMacClient(
            baseURL: URL(string: "http://10.0.0.1:4485")!,
            authToken: "bearer-a"
        )
        let clientB = RemoteMacClient(
            baseURL: URL(string: "http://10.0.0.2:4485")!,
            authToken: "bearer-b"
        )
        let store = RemotePreviewStore(
            snapshot: .empty,
            client: clientA,
            createSessionOverride: nil,
            bootstrapOverride: {
                bootstrapCalls += 1
                if bootstrapCalls == 1 { throw URLError(.cannotConnectToHost) }
                return try await gate.response()
            }
        )
        store.adoptClient(clientA, connectionEpoch: 1)
        var relayAttempts = 0
        store.attemptRelayFallback = {
            relayAttempts += 1
            return true
        }

        let failedPoll = await store.loadFromBridge()
        guard case .currentFailure = failedPoll else {
            return XCTFail("first Mac A poll should be a current failure")
        }
        XCTAssertTrue(store.isDisconnected)

        let stalePoll = Task { await store.loadFromBridge() }
        await gate.waitUntilStarted()
        store.adoptClient(clientB, connectionEpoch: 2)
        gate.succeed(with: .mock)
        let staleResult = await stalePoll.value
        guard case .superseded = staleResult else {
            return XCTFail("in-flight Mac A completion should be superseded")
        }

        let recovered = await store.recoverDisconnectedConnection(after: staleResult)
        XCTAssertFalse(recovered)
        XCTAssertEqual(relayAttempts, 0, "Mac B needs its own failed Direct poll first")
    }

    // MARK: - Host session-create capability

    func testSessionCreationCapabilityKeepsLegacyPermissiveAndRequiresAdvertisedOperation() {
        let legacy = RemotePreviewStore(snapshot: sessionCreationSnapshot(hostProtocol: nil))
        XCTAssertTrue(legacy.supportsSessionCreation)
        legacy.showPresetDrawer(for: "project-unpeel")
        XCTAssertEqual(legacy.presetDrawerProjectID, "project-unpeel")

        let advertised = RemotePreviewStore(
            snapshot: sessionCreationSnapshot(
                hostProtocol: .init(capabilities: ["session.create"])
            )
        )
        XCTAssertTrue(advertised.supportsSessionCreation)

        let omitted = RemotePreviewStore(
            snapshot: sessionCreationSnapshot(
                hostProtocol: .init(capabilities: ["host.bootstrap"])
            )
        )
        XCTAssertFalse(omitted.supportsSessionCreation)
        omitted.showPresetDrawer(for: "project-unpeel")
        XCTAssertNil(omitted.presetDrawerProjectID)
        XCTAssertNil(
            omitted.startSession(
                projectID: "project-unpeel",
                preset: omitted.snapshot.presets[0]
            )
        )

        let incompatible = RemotePreviewStore(
            snapshot: sessionCreationSnapshot(
                hostProtocol: .init(
                    majorVersion: RemoteControlProtocol.hostMajorVersion + 1,
                    capabilities: ["session.create"]
                )
            )
        )
        XCTAssertFalse(incompatible.supportsSessionCreation)
    }

    func testResumeAgentRequiresReturnedShellAndBothCapabilities() async {
        func session(
            resumeAgent: Bool?,
            activeRuntimeID: String? = nil,
            runtimeLaunchPending: Bool = false
        ) -> RemoteSessionSummary {
            RemoteSessionSummary(
                id: "live", projectID: "project-unpeel",
                activeRuntimeID: activeRuntimeID,
                runtimeLaunchPending: runtimeLaunchPending,
                providerID: "claude",
                title: "Claude", command: "claude", createdAtUnixMs: 1,
                status: .running, activity: .idle,
                capabilities: RemoteSessionCapabilities(
                    restart: false,
                    resumeAgent: resumeAgent,
                    fork: true,
                    appendSystemContext: true,
                    notifyWhenDone: true
                )
            )
        }

        let oldHost = RemotePreviewStore(snapshot: sessionCreationSnapshot(
            hostProtocol: nil, sessions: [session(resumeAgent: true)]
        ))
        let oldHostResult = await oldHost.performSessionAction(
            sessionID: "live", action: .resumeAgent
        )
        XCTAssertFalse(oldHostResult)

        let unavailable = RemotePreviewStore(snapshot: sessionCreationSnapshot(
            hostProtocol: .init(capabilities: [
                RemoteControlProtocol.sessionRuntimeResumeCapability,
            ]),
            sessions: [session(resumeAgent: false)]
        ))
        let unavailableResult = await unavailable.performSessionAction(
            sessionID: "live", action: .resumeAgent
        )
        XCTAssertFalse(unavailableResult)

        let incompatible = RemotePreviewStore(snapshot: sessionCreationSnapshot(
            hostProtocol: .init(
                majorVersion: RemoteControlProtocol.hostMajorVersion + 1,
                capabilities: [RemoteControlProtocol.sessionRuntimeResumeCapability]
            ),
            sessions: [session(resumeAgent: true)]
        ))
        let incompatibleResult = await incompatible.performSessionAction(
            sessionID: "live", action: .resumeAgent
        )
        XCTAssertFalse(incompatibleResult)

        let active = RemotePreviewStore(snapshot: sessionCreationSnapshot(
            hostProtocol: .init(
                capabilities: [RemoteControlProtocol.sessionRuntimeResumeCapability]
            ),
            sessions: [session(resumeAgent: true, activeRuntimeID: "claude")]
        ))
        let activeResult = await active.performSessionAction(
            sessionID: "live", action: .resumeAgent
        )
        XCTAssertFalse(activeResult)

        let pending = RemotePreviewStore(snapshot: sessionCreationSnapshot(
            hostProtocol: .init(
                capabilities: [RemoteControlProtocol.sessionRuntimeResumeCapability]
            ),
            sessions: [session(resumeAgent: true, runtimeLaunchPending: true)]
        ))
        let pendingResult = await pending.performSessionAction(
            sessionID: "live", action: .resumeAgent
        )
        XCTAssertFalse(pendingResult)

        // Legacy decoding remains possible, but current phone code never
        // emits the active-runtime restart action.
        let legacyResult = await active.performSessionAction(
            sessionID: "live", action: .restartAgent
        )
        XCTAssertFalse(legacyResult)
    }

    func testSessionOrganizerResumePresentationIsHonestForEveryLifecycle() {
        func session(
            status: RemoteSessionStatus = .running,
            activeRuntimeID: String? = nil,
            runtimeLaunchPending: Bool = false,
            restart: Bool = false,
            resumeAgent: Bool? = true,
            archived: Bool = false
        ) -> RemoteSessionSummary {
            RemoteSessionSummary(
                id: "session", projectID: "project",
                activeRuntimeID: activeRuntimeID,
                runtimeLaunchPending: runtimeLaunchPending,
                providerID: "claude",
                title: "Claude", command: "claude", createdAtUnixMs: 1,
                status: status, activity: .idle,
                capabilities: RemoteSessionCapabilities(
                    restart: restart,
                    resumeAgent: resumeAgent,
                    fork: false,
                    appendSystemContext: false,
                    notifyWhenDone: false
                ),
                archived: archived
            )
        }

        let currentHost = RemoteHostProtocolDescriptor(capabilities: [
            RemoteControlProtocol.sessionRuntimeResumeCapability,
        ])
        let legacyHost = RemoteHostProtocolDescriptor(
            minorVersion: 5,
            capabilities: [RemoteControlProtocol.sessionRuntimeRestartCapability]
        )

        XCTAssertEqual(
            sessionOrganizeResumePresentation(
                session: session(activeRuntimeID: "claude"),
                hostProtocol: currentHost
            ),
            .none
        )
        XCTAssertEqual(
            sessionOrganizeResumePresentation(
                session: session(), hostProtocol: currentHost
            ),
            .resumeAgent
        )
        XCTAssertEqual(
            sessionOrganizeResumePresentation(
                session: session(runtimeLaunchPending: true),
                hostProtocol: currentHost
            ),
            .none
        )
        XCTAssertEqual(
            sessionOrganizeResumePresentation(
                session: session(), hostProtocol: legacyHost
            ),
            .none
        )
        XCTAssertEqual(
            sessionOrganizeResumePresentation(
                session: session(
                    status: .exited, restart: false,
                    resumeAgent: nil, archived: true
                ),
                hostProtocol: currentHost
            ),
            .restore
        )
        XCTAssertEqual(
            sessionOrganizeResumePresentation(
                session: session(
                    status: .exited, restart: true,
                    resumeAgent: nil, archived: true
                ),
                hostProtocol: currentHost
            ),
            .restoreAndResume
        )
    }

    func testLegacyRestartIsStoppedResumeOnly() async {
        let live = RemoteSessionSummary(
            id: "live", projectID: "project-unpeel", providerID: "claude",
            title: "Claude", command: "claude", createdAtUnixMs: 1,
            status: .running, activity: .idle,
            capabilities: RemoteSessionCapabilities(
                restart: true,
                fork: true,
                appendSystemContext: true,
                notifyWhenDone: true
            )
        )
        let store = RemotePreviewStore(snapshot: sessionCreationSnapshot(
            hostProtocol: nil, sessions: [live]
        ))

        let result = await store.performSessionAction(
            sessionID: "live", action: .restart
        )
        XCTAssertFalse(result)
    }

    func testReplacementCorrelationIgnoresBaselineCollisionsAndDecoys() {
        let source = replacementSession(
            id: "source", status: .exited, archived: true
        )
        let baselineCollision = replacementSession(id: "baseline")
        let exact = replacementSession(
            id: "exact", command: "claude --resume thread"
        )
        let intent = RemotePreviewStore.ReplacementSelectionIntent(
            source: source,
            hostMacID: "mac",
            knownSessionIDs: [source.id, baselineCollision.id]
        )
        let decoys = [
            replacementSession(id: "wrong-project", projectID: "elsewhere"),
            replacementSession(id: "wrong-created", createdAtUnixMs: 43),
            replacementSession(
                id: "wrong-runtime", command: "codex resume thread", providerID: "codex"
            ),
            replacementSession(id: "wrong-worktree", worktreePath: "/other"),
            replacementSession(id: "still-archived", archived: true),
        ]

        XCTAssertEqual(
            RemotePreviewStore.replacementSelectionResolution(
                intent,
                sessions: [baselineCollision] + decoys + [exact]
            ),
            .select(exact.id),
            "only the unique post-effect source-correlated row may be selected"
        )

        guard case .wait = RemotePreviewStore.replacementSelectionResolution(
            intent,
            sessions: [source, exact]
        ) else {
            return XCTFail("the old id must disappear before its replacement is selectable")
        }
    }

    func testReplacementCorrelationCancelsOnAmbiguityAndBoundedExpiry() {
        let source = replacementSession(
            id: "source", status: .exited, archived: true
        )
        let intent = RemotePreviewStore.ReplacementSelectionIntent(
            source: source,
            hostMacID: "mac",
            knownSessionIDs: [source.id]
        )
        let first = replacementSession(id: "candidate-a")
        let second = replacementSession(id: "candidate-b")

        XCTAssertEqual(
            RemotePreviewStore.replacementSelectionResolution(
                intent, sessions: [first, second]
            ),
            .cancel,
            "Host row order must never break a replacement collision tie"
        )

        let expiring = RemotePreviewStore.ReplacementSelectionIntent(
            source: source,
            hostMacID: "mac",
            knownSessionIDs: [source.id],
            bootstrapObservationsRemaining: 1
        )
        XCTAssertEqual(
            RemotePreviewStore.replacementSelectionResolution(
                expiring, sessions: []
            ),
            .cancel,
            "an old intent must not hijack a future matching Session"
        )
    }

    func testAmbiguousReplacementNeverFallsBackOrLaterHijacksSelection() async throws {
        let source = replacementSession(id: "source", status: .exited)
        let first = replacementSession(id: "candidate-a")
        let second = replacementSession(id: "candidate-b")
        let initial = sessionCreationSnapshot(
            hostProtocol: nil, sessions: [source]
        )
        let ambiguous = sessionCreationSnapshot(
            hostProtocol: nil, sessions: [first, second], capturedAtUnixMs: 2
        )
        let laterUnique = sessionCreationSnapshot(
            hostProtocol: nil, sessions: [first], capturedAtUnixMs: 3
        )
        var restartSessionIDs: [String] = []
        var bootstrapCount = 0
        let store = RemotePreviewStore(
            snapshot: initial,
            client: RemoteMacClient(),
            createSessionOverride: nil,
            bootstrapOverride: {
                defer { bootstrapCount += 1 }
                return bootstrapCount == 0 ? ambiguous : laterUnique
            },
            restartSessionOverride: { restartSessionIDs.append($0) }
        )

        let restart = try XCTUnwrap(store.restartSelectedSession())
        await restart.value

        XCTAssertEqual(restartSessionIDs, [source.id])
        XCTAssertEqual(bootstrapCount, 1)
        XCTAssertNil(store.selectedSessionID)
        XCTAssertFalse(store.isRestartingSelectedSession)

        _ = await store.loadFromBridge()
        XCTAssertEqual(bootstrapCount, 2)
        XCTAssertNil(
            store.selectedSessionID,
            "a canceled two-candidate intent must not revive when one candidate later remains"
        )
    }

    func testExplicitSelectionAndCreateSupersedeWaitingReplacement() async throws {
        let source = replacementSession(id: "source", status: .exited)
        let other = replacementSession(
            id: "other", command: "codex", createdAtUnixMs: 9,
            providerID: "codex", worktreePath: nil, worktreeBranch: nil
        )
        let exact = replacementSession(id: "replacement")
        let created = replacementSession(
            id: "created", command: "codex", createdAtUnixMs: 10,
            providerID: "codex", worktreePath: nil, worktreeBranch: nil
        )
        let initial = sessionCreationSnapshot(
            hostProtocol: nil, sessions: [source, other]
        )
        let waiting = sessionCreationSnapshot(
            hostProtocol: nil, sessions: [source, other, exact], capturedAtUnixMs: 2
        )
        let afterCreate = sessionCreationSnapshot(
            hostProtocol: nil, sessions: [created, exact, other], capturedAtUnixMs: 3
        )
        var bootstrapCount = 0
        let store = RemotePreviewStore(
            snapshot: initial,
            client: RemoteMacClient(),
            createSessionOverride: { _ in
                RemoteCreateSessionResponse(sessionID: created.id, session: created)
            },
            bootstrapOverride: {
                defer { bootstrapCount += 1 }
                return bootstrapCount == 0 ? waiting : afterCreate
            },
            restartSessionOverride: { _ in }
        )

        let restart = try XCTUnwrap(store.restartSelectedSession())
        await restart.value
        XCTAssertEqual(store.selectedSessionID, source.id)

        store.select(other)
        XCTAssertEqual(store.selectedSessionID, other.id)
        _ = await store.loadFromBridge()
        XCTAssertEqual(
            store.selectedSessionID, other.id,
            "a replacement published after explicit selection must not steal focus"
        )

        // Stage another waiting replacement, then prove exact-id creation is
        // the newer focus intent and cannot be hijacked by that replacement.
        let restaged = RemotePreviewStore(
            snapshot: initial,
            client: RemoteMacClient(),
            createSessionOverride: { _ in
                RemoteCreateSessionResponse(sessionID: created.id, session: created)
            },
            bootstrapOverride: { afterCreate },
            restartSessionOverride: { _ in }
        )
        let restagedRestart = try XCTUnwrap(restaged.restartSelectedSession())
        let create = try XCTUnwrap(restaged.startSession(
            projectID: "project-unpeel",
            preset: initial.presets[0]
        ))
        await create.value
        await restagedRestart.value
        XCTAssertEqual(restaged.selectedSessionID, created.id)
    }

    func testHostSwitchCancelsWaitingReplacementBeforeNewBootstrapDefaults() async throws {
        let source = replacementSession(id: "source", status: .exited)
        let oldReplacement = replacementSession(id: "old-host-replacement")
        let newHostDefault = replacementSession(
            id: "new-host-default", command: "codex", createdAtUnixMs: 9,
            providerID: "codex", worktreePath: nil, worktreeBranch: nil
        )
        let initial = sessionCreationSnapshot(
            hostProtocol: nil, sessions: [source], macID: "host-a"
        )
        let waiting = sessionCreationSnapshot(
            hostProtocol: nil, sessions: [source, oldReplacement],
            capturedAtUnixMs: 2, macID: "host-a"
        )
        let hostB = sessionCreationSnapshot(
            hostProtocol: nil, sessions: [newHostDefault, oldReplacement],
            capturedAtUnixMs: 3, macID: "host-b"
        )
        var bootstrapCount = 0
        let store = RemotePreviewStore(
            snapshot: initial,
            client: RemoteMacClient(
                baseURL: URL(string: "http://host-a.local")!, authToken: "a"
            ),
            createSessionOverride: nil,
            bootstrapOverride: {
                defer { bootstrapCount += 1 }
                return bootstrapCount == 0 ? waiting : hostB
            },
            restartSessionOverride: { _ in }
        )

        let restart = try XCTUnwrap(store.restartSelectedSession())
        await restart.value
        XCTAssertEqual(store.selectedSessionID, source.id)

        store.adoptClient(
            RemoteMacClient(
                baseURL: URL(string: "http://host-b.local")!, authToken: "b"
            ),
            connectionEpoch: 1
        )
        _ = await store.loadFromBridge()
        XCTAssertEqual(
            store.selectedSessionID,
            newHostDefault.id,
            "an old Host's pending replacement must not cross client identity"
        )
    }

    func testCreateWithoutSummaryPollsBootstrapUntilSessionIsSelectableWithoutRetryingMutation() async throws {
        let hostProtocol = RemoteHostProtocolDescriptor(capabilities: ["session.create"])
        let initial = sessionCreationSnapshot(hostProtocol: hostProtocol)
        let created = RemoteSessionSummary(
            id: "created-session",
            projectID: "project-unpeel",
            providerID: "codex",
            title: "New Codex session",
            command: "codex",
            createdAtUnixMs: 42,
            status: .running,
            activity: .starting
        )
        let converged = sessionCreationSnapshot(
            hostProtocol: hostProtocol,
            sessions: [created],
            capturedAtUnixMs: 3
        )
        var createRequests: [RemoteCreateSessionRequest] = []
        var bootstrapCount = 0
        let store = RemotePreviewStore(
            snapshot: initial,
            client: RemoteMacClient(),
            createSessionOverride: { request in
                createRequests.append(request)
                return RemoteCreateSessionResponse(sessionID: created.id)
            },
            bootstrapOverride: {
                bootstrapCount += 1
                // Stay stale beyond one full 1s Host fallback interval; the
                // Controller must keep polling bootstrap, never re-create.
                return bootstrapCount <= 5 ? initial : converged
            }
        )

        let task = try XCTUnwrap(
            store.startSession(
                projectID: "project-unpeel",
                preset: initial.presets[0]
            )
        )
        await task.value

        XCTAssertEqual(
            createRequests,
            [RemoteCreateSessionRequest(projectID: "project-unpeel", presetID: "preset-codex")]
        )
        XCTAssertEqual(bootstrapCount, 6)
        XCTAssertEqual(store.selectedSessionID, created.id)
        XCTAssertEqual(store.snapshot.sessions.map(\.id), [created.id])
        XCTAssertTrue(store.expandedProjectIDs.contains("project-unpeel"))
        XCTAssertNil(store.launchingPresetID)
    }

    func testSidebarTreeNestsGroupsAndWorktreesUnderTheirProject() throws {
        let snapshot = RemoteBootstrapSnapshot(
            macID: "mac",
            macName: "Mac",
            folders: [
                .init(id: "folder-client", name: "Client", sortOrder: 0),
            ],
            projects: [
                .init(id: "project-shop", name: "Shop", path: "/work/shop", folderID: "folder-client", sortOrder: 0),
                .init(
                    id: "project-legacy",
                    name: "Legacy",
                    path: "/work/legacy",
                    folderID: "folder-client",
                    parentProjectID: "folder-client",
                    sortOrder: 1
                ),
                .init(id: "project-unpeel", name: "Unpeel", path: "/dev/unpeel", sortOrder: 2),
                .init(
                    id: "group-research",
                    name: "Research",
                    path: "/dev/unpeel",
                    parentProjectID: "project-unpeel",
                    isGroup: true,
                    colorID: "violet",
                    sortOrder: 3
                ),
                .init(
                    id: "worktree-native-b",
                    name: "native-b",
                    path: "/tmp/native-b",
                    parentProjectID: "project-unpeel",
                    worktreeBranch: "native-b",
                    sortOrder: 4
                ),
            ],
            presets: [],
            sessions: [],
            capturedAtUnixMs: 1
        )
        let tree = IOSSidebarProjectTree(snapshot: snapshot)

        XCTAssertEqual(tree.looseProjects.map(\.id), ["project-unpeel"])
        XCTAssertEqual(tree.folderGroups.map(\.folder.id), ["folder-client"])
        XCTAssertEqual(tree.folderGroups.first?.projects.map(\.id), ["project-shop", "project-legacy"])
        let root = try XCTUnwrap(snapshot.projects.first { $0.id == "project-unpeel" })
        XCTAssertEqual(
            tree.childProjects(for: root).map(\.id),
            ["group-research", "worktree-native-b"]
        )
        XCTAssertEqual(
            tree.worktreeProjects(for: root).map(\.id),
            ["worktree-native-b"]
        )
    }

    func testInitialSelectionInWorktreeExpandsParentAndChild() {
        let snapshot = RemoteBootstrapSnapshot(
            macID: "mac",
            macName: "Mac",
            folders: [],
            projects: [
                .init(id: "project-unpeel", name: "Unpeel", path: "/dev/unpeel", sortOrder: 0),
                .init(
                    id: "worktree-native-b",
                    name: "native-b",
                    path: "/tmp/native-b",
                    parentProjectID: "project-unpeel",
                    worktreeBranch: "native-b",
                    sortOrder: 1
                ),
            ],
            presets: [],
            sessions: [
                .init(
                    id: "session-native-b",
                    projectID: "worktree-native-b",
                    providerID: "codex",
                    title: "iOS",
                    command: "codex",
                    createdAtUnixMs: 1,
                    status: .running,
                    activity: .idle
                ),
            ],
            capturedAtUnixMs: 1
        )
        let store = RemotePreviewStore(snapshot: snapshot)

        XCTAssertEqual(store.selectedSessionID, "session-native-b")
        XCTAssertTrue(store.expandedProjectIDs.contains("project-unpeel"))
        XCTAssertTrue(store.expandedProjectIDs.contains("worktree-native-b"))
    }

    func testInitialSelectionInGroupExpandsParentAndChild() {
        let snapshot = RemoteBootstrapSnapshot(
            macID: "mac",
            macName: "Mac",
            folders: [],
            projects: [
                .init(id: "project-unpeel", name: "Unpeel", path: "/dev/unpeel", sortOrder: 0),
                .init(
                    id: "group-research",
                    name: "Research",
                    path: "/dev/unpeel",
                    parentProjectID: "project-unpeel",
                    isGroup: true,
                    colorID: "violet",
                    sortOrder: 1
                ),
            ],
            presets: [],
            sessions: [
                .init(
                    id: "session-research",
                    projectID: "group-research",
                    providerID: "codex",
                    title: "Research notes",
                    command: "codex",
                    createdAtUnixMs: 1,
                    status: .running,
                    activity: .idle
                ),
            ],
            capturedAtUnixMs: 1
        )
        let store = RemotePreviewStore(snapshot: snapshot)

        XCTAssertEqual(store.selectedSessionID, "session-research")
        XCTAssertTrue(store.expandedProjectIDs.contains("project-unpeel"))
        XCTAssertTrue(store.expandedProjectIDs.contains("group-research"))
        XCTAssertEqual(
            store.sidebarTree.sessions(for: snapshot.projects[1]).map(\.id),
            ["session-research"]
        )
    }

    // MARK: - Poll equality gate

    func testSnapshotEqualityIgnoresOutputPreviewChanges() {
        let base = RemoteBootstrapSnapshot.mock
        let noisy = withSessions(base) { session in
            remaking(
                session,
                updatedAtUnixMs: session.updatedAtUnixMs,
                lastOutputPreview: "fresh tail for \(session.id)"
            )
        }

        XCTAssertTrue(RemotePreviewStore.snapshotContentEqual(base, noisy))
    }

    func testSnapshotEqualityBucketsUpdatedAtToTheMinute() {
        let base = RemoteBootstrapSnapshot.mock
        let minuteStart: Int64 = 1_789_996_920_000 // divisible by 60_000
        let early = withSessions(base) {
            remaking($0, updatedAtUnixMs: minuteStart + 5_000, lastOutputPreview: $0.lastOutputPreview)
        }
        let late = withSessions(base) {
            remaking($0, updatedAtUnixMs: minuteStart + 45_000, lastOutputPreview: $0.lastOutputPreview)
        }
        let nextMinute = withSessions(base) {
            remaking($0, updatedAtUnixMs: minuteStart + 61_000, lastOutputPreview: $0.lastOutputPreview)
        }

        // mtime churn inside one minute bucket must not publish…
        XCTAssertTrue(RemotePreviewStore.snapshotContentEqual(early, late))
        // …but crossing a minute boundary must.
        XCTAssertFalse(RemotePreviewStore.snapshotContentEqual(early, nextMinute))
    }

    func testSnapshotEqualityStillSeesRenderedSessionChanges() {
        let base = RemoteBootstrapSnapshot.mock
        let retitled = withSessions(base) {
            remaking(
                $0,
                title: $0.id == base.sessions[0].id ? "Renamed" : $0.title,
                updatedAtUnixMs: $0.updatedAtUnixMs,
                lastOutputPreview: $0.lastOutputPreview
            )
        }

        XCTAssertFalse(RemotePreviewStore.snapshotContentEqual(base, retitled))
    }

    func testSnapshotEqualitySeesActiveRuntimeChanges() {
        let base = RemoteBootstrapSnapshot.mock
        let observed = withSessions(base) {
            remaking(
                $0,
                activeRuntimeID: $0.id == base.sessions[0].id
                    ? "com.anthropic.claude-code"
                    : nil,
                updatedAtUnixMs: $0.updatedAtUnixMs,
                lastOutputPreview: $0.lastOutputPreview
            )
        }

        XCTAssertFalse(RemotePreviewStore.snapshotContentEqual(base, observed))
        XCTAssertEqual(observed.sessions[0].presentationProviderID, "claude")
    }

    func testSnapshotEqualitySeesTerminalBackgroundHexChanges() {
        let base = RemoteBootstrapSnapshot.mock
        let themed = withSessions(base) {
            remaking(
                $0,
                updatedAtUnixMs: $0.updatedAtUnixMs,
                lastOutputPreview: $0.lastOutputPreview,
                terminalBackgroundHex: 0x141414
            )
        }

        XCTAssertFalse(RemotePreviewStore.snapshotContentEqual(base, themed))
    }

    func testSnapshotEqualitySeesHostCapabilityChanges() {
        let legacy = sessionCreationSnapshot(hostProtocol: nil)
        let advertised = sessionCreationSnapshot(
            hostProtocol: .init(capabilities: ["session.create"])
        )

        XCTAssertFalse(RemotePreviewStore.snapshotContentEqual(legacy, advertised))
    }

    // MARK: - Bundled icon SVGs

    func testBundledIconSVGsParseForCoreGraphicsRendering() {
        for icon in UnpeelChromeIcon.allCases {
            XCTAssertNotNil(
                ParsedSVGIcon.parse(icon.svgSource),
                "chrome icon \(icon) no longer parses; it would fall back to an SF Symbol"
            )
        }
        for icon in UnpeelToolIcon.allCases {
            XCTAssertNotNil(
                ParsedSVGIcon.parse(icon.svgSource),
                "tool icon \(icon) no longer parses; it would fall back to an SF Symbol"
            )
        }
    }

    // MARK: - Helpers

    private func sessionCreationSnapshot(
        hostProtocol: RemoteHostProtocolDescriptor?,
        sessions: [RemoteSessionSummary] = [],
        capturedAtUnixMs: Int64 = 1,
        macID: String = "mac"
    ) -> RemoteBootstrapSnapshot {
        RemoteBootstrapSnapshot(
            hostProtocol: hostProtocol,
            macID: macID,
            macName: "Mac",
            folders: [],
            projects: [
                .init(
                    id: "project-unpeel",
                    name: "Unpeel",
                    path: "/dev/unpeel",
                    sortOrder: 0
                ),
            ],
            presets: [
                .init(
                    id: "preset-codex",
                    label: "Codex",
                    command: "codex",
                    cliID: "codex"
                ),
            ],
            sessions: sessions,
            capturedAtUnixMs: capturedAtUnixMs
        )
    }

    private func activitySession(
        id: String,
        activity: RemoteActivityState,
        updatedAtUnixMs: Int64,
        unread: Bool = false
    ) -> RemoteSessionSummary {
        RemoteSessionSummary(
            id: id,
            projectID: "project-unpeel",
            providerID: "codex",
            title: id,
            command: "codex",
            createdAtUnixMs: 1,
            updatedAtUnixMs: updatedAtUnixMs,
            status: .running,
            activity: activity,
            unread: unread
        )
    }

    private func replacementSession(
        id: String,
        projectID: String = "project-unpeel",
        command: String = "claude",
        createdAtUnixMs: Int64 = 42,
        status: RemoteSessionStatus = .running,
        providerID: String? = "claude",
        worktreePath: String? = "/worktree",
        worktreeBranch: String? = "topic",
        archived: Bool = false
    ) -> RemoteSessionSummary {
        RemoteSessionSummary(
            id: id,
            projectID: projectID,
            providerID: providerID,
            title: id,
            command: command,
            createdAtUnixMs: createdAtUnixMs,
            status: status,
            activity: .idle,
            worktreePath: worktreePath,
            worktreeBranch: worktreeBranch,
            capabilities: RemoteSessionCapabilities(
                restart: status == .exited,
                fork: false,
                appendSystemContext: false,
                notifyWhenDone: false
            ),
            archived: archived
        )
    }

    private func withSessions(
        _ snapshot: RemoteBootstrapSnapshot,
        _ transform: (RemoteSessionSummary) -> RemoteSessionSummary
    ) -> RemoteBootstrapSnapshot {
        RemoteBootstrapSnapshot(
            protocolVersion: snapshot.protocolVersion,
            macID: snapshot.macID,
            macName: snapshot.macName,
            folders: snapshot.folders,
            projects: snapshot.projects,
            presets: snapshot.presets,
            sessions: snapshot.sessions.map(transform),
            capturedAtUnixMs: snapshot.capturedAtUnixMs + 1
        )
    }

    private func remaking(
        _ session: RemoteSessionSummary,
        title: String? = nil,
        activeRuntimeID: String? = nil,
        updatedAtUnixMs: Int64?,
        lastOutputPreview: String?,
        terminalBackgroundHex: Int? = nil
    ) -> RemoteSessionSummary {
        RemoteSessionSummary(
            id: session.id,
            projectID: session.projectID,
            activeRuntimeID: activeRuntimeID ?? session.activeRuntimeID,
            providerID: session.providerID,
            title: title ?? session.title,
            command: session.command,
            createdAtUnixMs: session.createdAtUnixMs,
            updatedAtUnixMs: updatedAtUnixMs,
            status: session.status,
            activity: session.activity,
            unread: session.unread,
            pinned: session.pinned,
            worktreePath: session.worktreePath,
            worktreeBranch: session.worktreeBranch,
            lastOutputPreview: lastOutputPreview,
            notifyWhenDone: session.notifyWhenDone,
            terminalBackgroundHex: terminalBackgroundHex ?? session.terminalBackgroundHex,
            capabilities: session.capabilities
        )
    }
}
