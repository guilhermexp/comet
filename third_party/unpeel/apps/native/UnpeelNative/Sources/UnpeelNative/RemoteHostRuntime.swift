//
//  RemoteHostRuntime.swift
//  UnpeelNative
//
//  Remote-only observable state. It deliberately does not project Host rows
//  into UnpeelStore's Local projects/sessions or SurfaceCache.
//

import Foundation
import UnpeelShared

enum RemoteHostConnectionState: Equatable, Sendable {
    case idle
    case connecting
    case connected(name: String)
    /// The last valid sidebar/terminal remains visible while reconnection runs.
    case reconnecting(message: String)
    case repairRequired(message: String)
    case incompatible(message: String)
    case failed(message: String)
}

/// User-facing route for one Host connection. The UI deliberately exposes
/// only the useful distinction (local network or Unpeel Link), never relay
/// endpoints, tokens, or a manual transport picker.
enum RemoteHostConnectionRoute: Equatable, Sendable {
    case ssh
    case direct
    case link

    var shortLabel: String {
        switch self {
        case .ssh: "Connected"
        case .direct: "Direct"
        case .link: "Via Link"
        }
    }
}

/// A transport is only a carrier for the shared Host contract. Keeping this
/// choice at the runtime boundary prevents the native UI from acquiring
/// transport-specific session behavior.
enum RemoteHostTransport: Sendable {
    case ssh(target: String, expectedHostID: String?)
    case direct(endpoint: URL, authToken: String, expectedHostID: String)
    case link(
        credentials: RelayCredentials,
        controllerDeviceID: String,
        authToken: String,
        expectedHostID: String
    )

    fileprivate var expectedHostID: String? {
        switch self {
        case let .ssh(_, expectedHostID): expectedHostID
        case let .direct(_, _, expectedHostID): expectedHostID
        case let .link(_, _, _, expectedHostID): expectedHostID
        }
    }

    fileprivate var route: RemoteHostConnectionRoute {
        switch self {
        case .ssh: .ssh
        case .direct: .direct
        case .link: .link
        }
    }

    fileprivate var continuityKey: RemoteHostContinuityKey {
        if let expectedHostID {
            return .pinnedHost(expectedHostID)
        }
        switch self {
        case let .ssh(target, _):
            return .sshTarget(target)
        case .direct, .link:
            // Paired transports always require a saved Host identity.
            preconditionFailure("A paired Host transport must be identity-pinned")
        }
    }
}

private struct PairedHostConnectionPlan: Sendable {
    let hostID: String
    let direct: RemoteHostTransport
    /// Nil when the user scoped this Host to Direct-only
    /// (`PairedHostRecord.linkEnabled == false`): no Link fallback, grace
    /// race, or background probe may ever open the relay downlink for it.
    let link: RemoteHostTransport?
}

private enum RemoteHostContinuityKey: Hashable, Sendable {
    case pinnedHost(String)
    case sshTarget(String)

    var paneFallbackKey: String {
        switch self {
        case let .pinnedHost(hostID): "host:\(hostID)"
        case let .sshTarget(target): "ssh:\(target)"
        }
    }
}

@MainActor
final class RemoteHostRuntime: ObservableObject {
    typealias BackendFactory = (
        _ transport: RemoteHostTransport
    ) throws -> any NativeRemoteBackendProtocol

    typealias PaneAttachmentProbe = @MainActor @Sendable (
        _ pane: RemoteGhosttyTerminalPane
    ) -> Bool
    typealias PaneOutputFeeder = @MainActor @Sendable (
        _ pane: RemoteGhosttyTerminalPane,
        _ bytes: Data,
        _ resetBeforeFeed: Bool
    ) -> Bool

    @Published private(set) var snapshot: RemoteBootstrapSnapshot?
    @Published private(set) var selectedSessionID: String?
    @Published private(set) var connectionState: RemoteHostConnectionState = .idle
    @Published private(set) var connectionRoute: RemoteHostConnectionRoute?
    /// False only after a successful bootstrap for the current connection.
    /// In particular, an outcome-unknown effect closes this gate until a
    /// later bootstrap proves a fresh, accepted connection generation.
    @Published private(set) var terminalEffectsEnabled = false

    var selectionConnectionIsActive: Bool {
        guard connection != nil, refreshTask != nil else { return false }
        return switch connectionState {
        case .connecting, .connected, .reconnecting:
            true
        case .idle, .repairRequired, .incompatible, .failed:
            false
        }
    }

    private final class ActiveConnection: @unchecked Sendable {
        let epoch: UInt64
        let transport: RemoteHostTransport
        let backend: any NativeRemoteBackendProtocol
        let effectStartBarrier: Task<Void, Never>?
        let desktopFits: DesktopFitOwnership
        let markReadsSuppressedUntilPostBarrier: Set<String>
        var needsPostBarrierRefresh: Bool

        init(
            epoch: UInt64,
            transport: RemoteHostTransport,
            backend: any NativeRemoteBackendProtocol,
            effectStartBarrier: Task<Void, Never>? = nil,
            desktopFits: DesktopFitOwnership = DesktopFitOwnership(),
            markReadsSuppressedUntilPostBarrier: Set<String> = [],
            needsPostBarrierRefresh: Bool = false
        ) {
            self.epoch = epoch
            self.transport = transport
            self.backend = backend
            self.effectStartBarrier = effectStartBarrier
            self.desktopFits = desktopFits
            self.markReadsSuppressedUntilPostBarrier =
                markReadsSuppressedUntilPostBarrier
            self.needsPostBarrierRefresh = needsPostBarrierRefresh
        }
    }

    /// Fit state belongs to the exact backend generation that received it.
    /// The lock lets retirement cleanup wait for an in-flight effect worker,
    /// observe which clears actually landed, then compensate only the fits
    /// that are still owned before closing that backend.
    private final class DesktopFitOwnership: @unchecked Sendable {
        private let lock = NSLock()
        private var sessionIDs: Set<String> = []
        private var inheritedClearPending = false

        func insert(_ sessionID: String) {
            lock.lock()
            sessionIDs.insert(sessionID)
            lock.unlock()
        }

        func remove(_ sessionID: String) {
            lock.lock()
            sessionIDs.remove(sessionID)
            lock.unlock()
        }

        func snapshot() -> Set<String> {
            lock.lock()
            defer { lock.unlock() }
            return sessionIDs
        }

        func armInheritedClear() {
            lock.lock()
            inheritedClearPending = true
            lock.unlock()
        }

        func hasInheritedClearPending() -> Bool {
            lock.lock()
            defer { lock.unlock() }
            return inheritedClearPending
        }

        func claimInheritedFitsForClear() -> Set<String> {
            lock.lock()
            defer { lock.unlock() }
            guard inheritedClearPending else { return [] }
            inheritedClearPending = false
            return sessionIDs
        }
    }

    /// Owns a speculative route backend until it is either promoted or
    /// closed. Cancellation can race a blocking FFI bootstrap, so close must
    /// be independently callable and exactly once.
    private final class RouteProbeCandidate: @unchecked Sendable {
        let backend: any NativeRemoteBackendProtocol
        private let lock = NSLock()
        private var closed = false

        init(backend: any NativeRemoteBackendProtocol) {
            self.backend = backend
        }

        func close() async {
            guard claimClose() else { return }
            await backend.close()
        }

        private func claimClose() -> Bool {
            lock.lock()
            defer { lock.unlock() }
            let shouldClose = !closed
            closed = true
            return shouldClose
        }
    }

    private struct OutputPumpIdentity: Equatable, Sendable {
        let token: UInt64
        let connectionEpoch: UInt64
        let paneKey: RemoteTerminalPaneKey
        let paneIdentity: ObjectIdentifier
    }

    private struct PaneCursorIdentity: Equatable, Sendable {
        let connectionEpoch: UInt64
        let paneIdentity: ObjectIdentifier
    }

    private struct RequestedDesktopFit: Equatable, Sendable {
        let columns: UInt16
        let rows: UInt16
    }

    private enum RemoteEffect: Sendable {
        case write(sessionID: String, data: Data)
        case fit(sessionID: String, columns: UInt16, rows: UInt16)
        case clearFit(sessionID: String)
        case markRead(sessionID: String)

        var sessionID: String {
            switch self {
            case let .write(sessionID, _),
                 let .fit(sessionID, _, _),
                 let .clearFit(sessionID),
                 let .markRead(sessionID):
                sessionID
            }
        }

        func perform(
            on backend: any NativeRemoteBackendProtocol
        ) async throws -> NativeRemoteEffectReceipt {
            switch self {
            case let .write(sessionID, data):
                try await backend.writeTerminal(sessionID: sessionID, data: data)
            case let .fit(sessionID, columns, rows):
                try await backend.fitDesktop(
                    sessionID: sessionID,
                    columns: columns,
                    rows: rows
                )
            case let .clearFit(sessionID):
                try await backend.clearDesktopFit(sessionID: sessionID)
            case let .markRead(sessionID):
                try await backend.markRead(sessionID: sessionID)
            }
        }
    }

    private struct QueuedEffect: Sendable {
        var effect: RemoteEffect
        var notAppliedRetryCount: UInt8 = 0
    }

    private struct OutstandingRetirement {
        let task: Task<Void, Never>
        let pendingMarkReads: Set<String>
        let awaitingMarkReads: Set<String>
        let failedMarkReads: Set<String>
    }

    private let backendFactory: BackendFactory
    private let refreshIntervalNanoseconds: UInt64
    private let initialBootstrapRetryIntervalNanoseconds: UInt64
    private let initialBootstrapFastRetryCount: Int
    private let initialDirectLinkGraceNanoseconds: UInt64
    private let directProbeSuccessfulLinkRefreshes: Int
    private let forceLinkForDevelopment: Bool
    private let outputIdleIntervalNanoseconds: UInt64
    private let resizeDebounceNanoseconds: UInt64
    private let paneAttachmentProbe: PaneAttachmentProbe
    private let paneOutputFeeder: PaneOutputFeeder
    private let paneCache: RemoteGhosttyPaneCache

    private var connection: ActiveConnection?
    private var refreshTask: Task<Void, Never>?
    private var outputTask: Task<Void, Never>?
    private var outputPumpIdentity: OutputPumpIdentity?
    private var effectWorker: Task<Void, Never>?
    private var resizeTask: Task<Void, Never>?
    private var routeProbeTask: Task<Void, Never>?
    private var directLinkGraceTask: Task<Void, Never>?
    private var routeProbeTarget: RemoteHostConnectionRoute?
    private var routeProbeCandidate: RouteProbeCandidate?

    private var connectionEpoch: UInt64 = 0
    /// Changes whenever remote I/O is invalidated, even if the backend object
    /// remains current. A bootstrap may publish only if it began after the
    /// latest failure, so an older health poll cannot reopen a failed gate.
    private var recoveryEpoch: UInt64 = 0
    private var outputPumpToken: UInt64 = 0
    private var effectQueueEpoch: UInt64 = 0
    private var effectWorkerToken: UInt64 = 0
    private var routeProbeToken: UInt64 = 0
    private var pairedPlanEpoch: UInt64 = 0
    private var continuityKey: RemoteHostContinuityKey?
    private var pinnedHostID: String?
    private var paneHostKey: String?
    private var connectionBootstrapped = false
    private var pairedPlan: PairedHostConnectionPlan?
    private var successfulLinkRefreshes = 0
    /// A transport may fail while an effect worker is unwinding. Retain every
    /// such tail so promotion can prove the old backend is quiescent before
    /// closing it or starting effects on its replacement.
    private var retiredEffectTails: [Task<Void, Never>] = []
    /// Disconnecting or a failed replacement open must not discard the old
    /// same-Host effect/fit barrier. A later Local → Host return consumes it
    /// by durable Host continuity before it may bootstrap or send effects.
    private var outstandingRetirements: [
        RemoteHostContinuityKey: OutstandingRetirement
    ] = [:]

    private var queuedEffects: [QueuedEffect] = []
    private var pendingWriteBytes = 0
    private var pendingMarkReadSessionIDs: Set<String> = []
    /// A failed route may still have a mark-read effect unwinding. No route
    /// may reconsider those ids until this tail settles and a bootstrap that
    /// started afterward proves the Host still reports them unread.
    private var markReadRecoveryBarrier: Task<Void, Never>?
    private var markReadsSuppressedUntilRecovery: Set<String> = []
    private var markReadAwaitingSnapshotClearSessionIDs: Set<String> = []
    private var failedAutomaticMarkReadSessionIDs: Set<String> = []
    private var pendingDesktopFitClearSessionIDs: Set<String> = []
    private var enqueuedDesktopFitClearSessionIDs: Set<String> = []
    private var latestViewportByPane: [RemoteTerminalPaneKey: RequestedDesktopFit] = [:]
    private var lastQueuedViewportBySession: [String: RequestedDesktopFit] = [:]
    private var failedFitBySession: [String: RequestedDesktopFit] = [:]
    private var incompleteUTF8InputBySession: [String: Data] = [:]
    private var paneCursorReady: [RemoteTerminalPaneKey: PaneCursorIdentity] = [:]
    /// A Controller-created Session may not be in the published snapshot yet.
    /// The first bootstrap that reports it selects it, exactly like a local
    /// spawn selects its new row.
    private var pendingCreatedSelectionID: String?
    /// Restore & Resume mints a new Session id but the legacy success receipt
    /// carries only `{ok:true}`. Correlate the replacement against a snapshot
    /// latched before either effect, and fail closed on collisions instead of
    /// adopting whichever row happens to sort first.
    private var pendingReplacementSelection: ReplacementSelectionIntent?
    /// Once a combined restore/restart invalidates the selected id, only an
    /// exact correlation or explicit user choice may select another row.
    /// This remains set after ambiguity/expiry so a later health poll cannot
    /// turn a failed correlation into an arbitrary fallback.
    private var replacementDefaultSelectionSuppressed = false

    struct ReplacementSelectionIntent: Equatable {
        static let maximumBootstrapObservations = 30

        let sourceSessionID: String
        let projectID: String
        let createdAtUnixMs: Int64
        /// Provider/runtime identity derived from providerID or the command
        /// family. The replacement command itself contains resume flags, so
        /// exact command-string equality would reject the real replacement.
        let runtimeID: String?
        let worktreePath: String?
        let worktreeBranch: String?
        let baselineSessionIDs: Set<String>
        var bootstrapObservationsRemaining: Int

        init(
            source: RemoteSessionSummary,
            knownSessionIDs: Set<String>,
            bootstrapObservationsRemaining: Int = maximumBootstrapObservations
        ) {
            sourceSessionID = source.id
            projectID = source.projectID
            createdAtUnixMs = source.createdAtUnixMs
            runtimeID = source.providerID ?? SetupTool.detect(in: source.command)?.id
            worktreePath = source.worktreePath
            worktreeBranch = source.worktreeBranch
            baselineSessionIDs = knownSessionIDs.union([source.id])
            self.bootstrapObservationsRemaining = bootstrapObservationsRemaining
        }
    }

    enum ReplacementSelectionResolution: Equatable {
        case wait(ReplacementSelectionIntent)
        case select(String)
        case cancel
    }

    /// The Host rejects a larger write. Input callbacks may be much larger
    /// (notably paste), so the runtime batches them without changing byte
    /// order. The pending bound prevents a fast producer from retaining an
    /// unbounded paste stream while a Host is slow or offline.
    static let maximumTerminalWriteBytes = 64 * 1024
    static let maximumPendingTerminalInputBytes = 4 * 1024 * 1024

    init(
        refreshIntervalNanoseconds: UInt64 = 2_000_000_000,
        initialBootstrapRetryIntervalNanoseconds: UInt64 = 100_000_000,
        initialBootstrapFastRetryCount: Int = 3,
        initialDirectLinkGraceNanoseconds: UInt64 = 750_000_000,
        directProbeSuccessfulLinkRefreshes: Int = 8,
        forceLinkForDevelopment: Bool = RemoteHostRuntime.defaultForceLinkForDevelopment,
        outputIdleIntervalNanoseconds: UInt64 = 50_000_000,
        resizeDebounceNanoseconds: UInt64 = 80_000_000,
        backendFactory: @escaping BackendFactory = { transport in
            switch transport {
            case let .ssh(target, expectedHostID):
                try NativeRemoteBackend(
                    sshTarget: target,
                    expectedHostID: expectedHostID
                )
            case let .direct(endpoint, authToken, expectedHostID):
                try NativeRemoteBackend(
                    directEndpoint: endpoint,
                    authToken: authToken,
                    expectedHostID: expectedHostID
                )
            case let .link(credentials, controllerDeviceID, authToken, expectedHostID):
                try NativeRemoteBackend(
                    relayCredentials: credentials,
                    controllerDeviceID: controllerDeviceID,
                    authToken: authToken,
                    expectedHostID: expectedHostID
                )
            }
        },
        paneCache: RemoteGhosttyPaneCache = RemoteGhosttyPaneCache(),
        paneAttachmentProbe: @escaping PaneAttachmentProbe = { $0.isReadyForHostBytes },
        paneOutputFeeder: @escaping PaneOutputFeeder = { pane, bytes, reset in
            pane.receiveHostBytes(bytes, resetBeforeFeed: reset)
        }
    ) {
        self.refreshIntervalNanoseconds = refreshIntervalNanoseconds
        self.initialBootstrapRetryIntervalNanoseconds =
            initialBootstrapRetryIntervalNanoseconds
        self.initialBootstrapFastRetryCount = max(0, initialBootstrapFastRetryCount)
        self.initialDirectLinkGraceNanoseconds = initialDirectLinkGraceNanoseconds
        self.directProbeSuccessfulLinkRefreshes = max(
            1,
            directProbeSuccessfulLinkRefreshes
        )
        self.forceLinkForDevelopment = forceLinkForDevelopment
        self.outputIdleIntervalNanoseconds = outputIdleIntervalNanoseconds
        self.resizeDebounceNanoseconds = resizeDebounceNanoseconds
        self.backendFactory = backendFactory
        self.paneCache = paneCache
        self.paneAttachmentProbe = paneAttachmentProbe
        self.paneOutputFeeder = paneOutputFeeder
    }

    deinit {
        refreshTask?.cancel()
        outputTask?.cancel()
        resizeTask?.cancel()
        directLinkGraceTask?.cancel()
        routeProbeTask?.cancel()
        if let routeProbeCandidate {
            Task { await routeProbeCandidate.close() }
        }
        if let connection {
            let priorWorkers = retiredEffectTails + [effectWorker].compactMap { $0 }
            Task {
                for worker in priorWorkers {
                    await worker.value
                }
                await Self.clearOwnedDesktopFitsAndClose(connection)
            }
        }
    }

    static var defaultForceLinkForDevelopment: Bool {
        guard RemoteHostFeature.pickerEnabled else { return false }
        return UserDefaults.standard.bool(forKey: "unpeel.native.forceLink")
    }

    /// SSH is the advanced/admin fallback. It carries the same Host contract
    /// as direct pairing; no SSH-specific behavior exists above this call.
    func connectSSH(target: String, expectedHostID: String?) {
        connect(.ssh(target: target, expectedHostID: expectedHostID), pairedPlan: nil)
    }

    /// Normal paired-LAN App-to-TUI/App-to-App connection.
    func connectDirect(endpoint: URL, authToken: String, expectedHostID: String) {
        connect(.direct(
            endpoint: endpoint,
            authToken: authToken,
            expectedHostID: expectedHostID
        ), pairedPlan: nil)
    }

    /// Normal paired App-to-App/App-to-TUI connection. Every selection starts
    /// on the saved LAN endpoint and falls back to the paired Link channel only
    /// for reachability failures. The paired record's Host id is the durable
    /// identity pin for both routes.
    func connectPairedHost(
        record: PairedHostRecord,
        credentials: RemoteHostCredentials
    ) {
        // A Direct-only Host (removed from the Unpeel Link enrollment list)
        // gets no Link transport at all: reachability failures then report
        // Direct-only reachability instead of silently riding the relay.
        let link: RemoteHostTransport? = record.isLinkEnabled
            ? .link(
                credentials: credentials.relayCredentials,
                controllerDeviceID: record.controllerDeviceID,
                authToken: credentials.authToken,
                expectedHostID: record.hostID
            )
            : nil
        let plan = PairedHostConnectionPlan(
            hostID: record.hostID,
            direct: .direct(
                endpoint: record.endpoint,
                authToken: credentials.authToken,
                expectedHostID: record.hostID
            ),
            link: link
        )
        connect(
            forceLinkForDevelopment ? (plan.link ?? plan.direct) : plan.direct,
            pairedPlan: plan
        )
    }

    func disconnect() {
        pairedPlanEpoch &+= 1
        cancelInitialDirectLinkGrace()
        cancelRouteProbe()
        connectionEpoch &+= 1
        let retiringKey = continuityKey
        let existingRetirement = retiringKey.flatMap {
            outstandingRetirements.removeValue(forKey: $0)
        }
        let retiringPendingMarkReads = pendingMarkReadSessionIDs
            .union(existingRetirement?.pendingMarkReads ?? [])
        let retiringAwaitingMarkReads = markReadAwaitingSnapshotClearSessionIDs
            .union(existingRetirement?.awaitingMarkReads ?? [])
        let retiringFailedMarkReads = failedAutomaticMarkReadSessionIDs
            .union(existingRetirement?.failedMarkReads ?? [])
        let oldConnection = connection
        let priorWorker = invalidateConnectionWork()
        connection = nil
        let retirementPrerequisite = combineTasks(
            existingRetirement?.task,
            priorWorker
        )
        let finalRetirement = close(
            oldConnection,
            after: retirementPrerequisite
        ) ?? retirementPrerequisite
        if let retiringKey {
            storeOutstandingRetirement(
                for: retiringKey,
                task: finalRetirement,
                pendingMarkReads: retiringPendingMarkReads,
                awaitingMarkReads: retiringAwaitingMarkReads,
                failedMarkReads: retiringFailedMarkReads
            )
        }
        continuityKey = nil
        pinnedHostID = nil
        paneHostKey = nil
        snapshot = nil
        selectedSessionID = nil
        pendingCreatedSelectionID = nil
        pendingReplacementSelection = nil
        replacementDefaultSelectionSuppressed = false
        connectionState = .idle
        connectionRoute = nil
        pairedPlan = nil
        successfulLinkRefreshes = 0
        paneCache.removeAll()
        latestViewportByPane.removeAll()
        paneCursorReady.removeAll()
        incompleteUTF8InputBySession.removeAll()
    }

    /// Keep the selected remote scope fail-closed when its saved pairing was
    /// minted for a different Controller identity. The only recovery is an
    /// explicit re-pair; presenting idle/disconnected would hide that fact.
    func requirePairingRepair() {
        disconnect()
        connectionState = .repairRequired(
            message: "This Host was paired with a different Controller identity. Pair it again."
        )
    }

    func selectSession(_ sessionID: String) {
        guard snapshot?.sessions.contains(where: { $0.id == sessionID }) == true else {
            return
        }
        // Explicit user selection always wins over an earlier optimistic
        // create/replacement intent; no later bootstrap may steal focus back.
        pendingCreatedSelectionID = nil
        pendingReplacementSelection = nil
        replacementDefaultSelectionSuppressed = false
        guard selectedSessionID != sessionID else {
            rebindSelectedPaneAndEnsurePump()
            requestMarkReadIfNeeded(sessionID)
            return
        }

        transitionSelectedSession(to: sessionID)
        requestMarkReadIfNeeded(sessionID)
        prunePaneCache()
    }

    /// Return the remote-only, in-memory Ghostty pane for a Host Session. The
    /// cache key includes Host identity, so identical Session ids on two Hosts
    /// can never share VT state. The runtime owns the pane and its callbacks;
    /// the UI owns only attaching/presenting the returned NSView.
    func terminalPane(
        for sessionID: String,
        style: TerminalPaneStyle = .resolved()
    ) -> RemoteGhosttyTerminalPane? {
        guard snapshot?.sessions.contains(where: { $0.id == sessionID }) == true,
              let paneHostKey
        else {
            return nil
        }

        let key = RemoteTerminalPaneKey(
            hostID: paneHostKey,
            sessionID: sessionID
        )
        // A failed reconnect may have no active backend while the last valid
        // Host snapshot and VT frame are still useful. Return that exact pane
        // read-only; its old callbacks are epoch-gated and cannot reach a
        // retired connection. Never create a new pane without a connection.
        guard let connection else {
            return paneCache.existingPane(for: key)
        }
        let existingPane = paneCache.existingPane(for: key)
        let pane = paneCache.pane(
            for: key,
            style: style,
            onInput: makeInputHandler(
                sessionID: sessionID,
                paneKey: key,
                connectionEpoch: connection.epoch
            ),
            onResize: makeResizeHandler(
                sessionID: sessionID,
                paneKey: key,
                connectionEpoch: connection.epoch
            )
        )
        if existingPane == nil || existingPane !== pane {
            paneCursorReady.removeValue(forKey: key)
        }
        paneCache.noteShown(key)
        // Creation can take the cache over its LRU limit. Prune before a
        // pump captures pane identity, so an immediately evicted pane can
        // never reset or advance the backend cursor.
        prunePaneCache()
        guard paneCache.existingPane(for: key) === pane else { return nil }
        if selectedSessionID == sessionID {
            ensureOutputPump()
            requestMarkReadIfNeeded(sessionID)
        }
        return pane
    }

    /// Explicit effect seams are useful to non-pane controls and keep every
    /// mutation on the same serial, generation-bound chain as key input.
    func sendTerminalInput(_ data: Data, to sessionID: String) {
        guard !data.isEmpty, selectedSessionID == sessionID else { return }
        guard let prepared = prepareTerminalInput(data, sessionID: sessionID),
              !prepared.isEmpty
        else { return }
        enqueueEffect(.write(sessionID: sessionID, data: prepared))
    }

    func fitDesktop(sessionID: String, columns: Int, rows: Int) {
        guard selectedSessionID == sessionID,
              columns > 0,
              rows > 0,
              let currentPaneKey
        else {
            return
        }
        let viewport = RequestedDesktopFit(
            columns: UInt16(clamping: columns),
            rows: UInt16(clamping: rows)
        )
        latestViewportByPane[currentPaneKey] = viewport
        enqueueDesktopFitIfNeeded(
            sessionID: sessionID,
            columns: viewport.columns,
            rows: viewport.rows,
            viewport: viewport
        )
    }

    func clearDesktopFit(sessionID: String) {
        guard snapshot?.sessions.contains(where: { $0.id == sessionID }) == true else {
            return
        }
        requestDesktopFitClear(sessionID)
    }

    func markRead(sessionID: String) {
        guard snapshot?.sessions.contains(where: { $0.id == sessionID }) == true else {
            return
        }
        enqueueEffect(.markRead(sessionID: sessionID))
    }

    private func connect(
        _ transport: RemoteHostTransport,
        pairedPlan nextPairedPlan: PairedHostConnectionPlan?
    ) {
        pairedPlanEpoch &+= 1
        cancelInitialDirectLinkGrace()
        cancelRouteProbe()
        pairedPlan = nextPairedPlan
        successfulLinkRefreshes = 0
        connectionEpoch &+= 1
        let epoch = connectionEpoch
        let oldConnection = connection
        let retiringContinuityKey = continuityKey
        let currentPendingMarkReads = pendingMarkReadSessionIDs
        let currentAwaitingMarkReads = markReadAwaitingSnapshotClearSessionIDs
        let currentFailedMarkReads = failedAutomaticMarkReadSessionIDs
        let nextContinuityKey = transport.continuityKey
        let existingRetirement = outstandingRetirements.removeValue(
            forKey: nextContinuityKey
        )
        let replacesCurrentSameHost = oldConnection != nil
            && continuityKey == nextContinuityKey
        let carriedPendingMarkReads = (existingRetirement?.pendingMarkReads ?? [])
            .union(replacesCurrentSameHost ? currentPendingMarkReads : [])
        let carriedAwaitingMarkReads = (existingRetirement?.awaitingMarkReads ?? [])
            .union(
                replacesCurrentSameHost
                    ? currentAwaitingMarkReads
                    : []
            )
        let carriedFailedMarkReads = (existingRetirement?.failedMarkReads ?? [])
            .union(
                replacesCurrentSameHost ? currentFailedMarkReads : []
            )
        let priorWorker = invalidateConnectionWork()
        pendingMarkReadSessionIDs.formUnion(carriedPendingMarkReads)
        markReadAwaitingSnapshotClearSessionIDs.formUnion(carriedAwaitingMarkReads)
        failedAutomaticMarkReadSessionIDs.formUnion(carriedFailedMarkReads)
        connection = nil
        let retirementPrerequisite = combineTasks(
            existingRetirement?.task,
            priorWorker
        )
        let predecessorRetirement = close(
            oldConnection,
            after: retirementPrerequisite
        ) ?? retirementPrerequisite
        if let retiringContinuityKey,
           retiringContinuityKey != nextContinuityKey,
           oldConnection != nil {
            // Keep the retiring Host's own continuity entry too. If opening
            // the replacement Host fails, a rapid return must still wait the
            // original Host's tail and preserve its semantic latches.
            storeOutstandingRetirement(
                for: retiringContinuityKey,
                task: predecessorRetirement,
                pendingMarkReads: currentPendingMarkReads,
                awaitingMarkReads: currentAwaitingMarkReads,
                failedMarkReads: currentFailedMarkReads
            )
        }

        if continuityKey != nextContinuityKey {
            snapshot = nil
            selectedSessionID = nil
            pendingCreatedSelectionID = nil
            pendingReplacementSelection = nil
            replacementDefaultSelectionSuppressed = false
            pinnedHostID = transport.expectedHostID
            paneHostKey = nil
            paneCache.removeAll()
            latestViewportByPane.removeAll()
        } else if let expectedHostID = transport.expectedHostID {
            // A stronger saved identity always wins over an earlier
            // transport-only SSH continuity key.
            pinnedHostID = expectedHostID
        }
        continuityKey = nextContinuityKey
        // A new backend owns a new output-cursor table even when the stable
        // Host and retained VT panes are continuous.
        paneCursorReady.removeAll()
        connectionRoute = transport.route
        connectionState = .connecting

        do {
            let backend = try backendFactory(transport)
            let opened = ActiveConnection(
                epoch: epoch,
                transport: transport,
                backend: backend,
                effectStartBarrier: predecessorRetirement,
                markReadsSuppressedUntilPostBarrier: carriedPendingMarkReads,
                // Even a different-Host replacement must not capture a
                // bootstrap before its transitive predecessor closes: a
                // rapid A → B → A chain otherwise reuses a stale A snapshot.
                needsPostBarrierRefresh: predecessorRetirement != nil
            )
            connection = opened
            startRefreshLoop(for: opened)
            scheduleInitialDirectLinkGraceIfPossible(matching: opened)
        } catch {
            storeOutstandingRetirement(
                for: nextContinuityKey,
                task: predecessorRetirement,
                pendingMarkReads: carriedPendingMarkReads,
                awaitingMarkReads: carriedAwaitingMarkReads,
                failedMarkReads: carriedFailedMarkReads
            )
            connectionState = snapshot == nil
                ? .failed(message: error.localizedDescription)
                : .reconnecting(message: error.localizedDescription)
        }

    }

    private func startRefreshLoop(
        for connection: ActiveConnection,
        initiallyAccepted: Bool = false
    ) {
        refreshTask?.cancel()
        refreshTask = Task { [weak self] in
            guard let self else { return }
            await self.runRefreshLoop(connection, initiallyAccepted: initiallyAccepted)
        }
    }

    private func runRefreshLoop(
        _ candidate: ActiveConnection,
        initiallyAccepted: Bool
    ) async {
        var acceptedBootstrap = initiallyAccepted
        var fastRetriesUsed = 0
        if candidate.needsPostBarrierRefresh {
            await candidate.effectStartBarrier?.value
            guard isCurrent(candidate), !Task.isCancelled else { return }
        } else if initiallyAccepted {
            do {
                try await Task.sleep(nanoseconds: refreshIntervalNanoseconds)
            } catch {
                return
            }
        }
        while !Task.isCancelled {
            if let markReadRecoveryBarrier {
                await markReadRecoveryBarrier.value
                guard isCurrent(candidate), !Task.isCancelled else { return }
            }
            let attemptRecoveryEpoch = recoveryEpoch
            do {
                let next = try await candidate.backend.bootstrap()
                guard isCurrent(candidate) else { return }
                // A terminal/output/effect failure may have happened while
                // this health poll was in flight. Only a bootstrap that began
                // afterward is allowed to reopen remote I/O.
                guard recoveryEpoch == attemptRecoveryEpoch else { continue }
                try validateIdentity(next, for: candidate)
                guard isCurrent(candidate) else { return }

                if markReadRecoveryBarrier != nil {
                    pendingMarkReadSessionIDs.subtract(
                        markReadsSuppressedUntilRecovery
                    )
                    markReadsSuppressedUntilRecovery.removeAll()
                    markReadRecoveryBarrier = nil
                }
                if candidate.needsPostBarrierRefresh {
                    // The predecessor effect tail is now settled and this
                    // snapshot was captured afterward. Only now may an old
                    // pending mark-read be reconsidered; promoting from the
                    // candidate snapshot would silently replay it.
                    pendingMarkReadSessionIDs.subtract(
                        candidate.markReadsSuppressedUntilPostBarrier
                    )
                    candidate.needsPostBarrierRefresh = false
                }
                publishAcceptedBootstrap(next, for: candidate)
                acceptedBootstrap = true
            } catch is CancellationError {
                return
            } catch {
                guard isCurrent(candidate) else { return }
                if retireForTerminalError(error, matching: candidate) {
                    return
                }
                if !acceptedBootstrap,
                   fastRetriesUsed < initialBootstrapFastRetryCount {
                    fastRetriesUsed += 1
                    // Link probing is non-destructive: it cannot promote
                    // until an authenticated bootstrap succeeds, and a Direct
                    // recovery cancels it. Start that proof concurrently so a
                    // black-holed LAN endpoint does not make an off-LAN user
                    // wait through every Direct grace attempt first.
                    scheduleFallbackIfPossible(for: error, matching: candidate)
                    // `pair --serve` may briefly close and rebind the exact
                    // paired endpoint while handing ownership to the TUI.
                    // Bootstrap is a safe read, so absorb only this bounded
                    // pre-first-success window without flashing a failure.
                    connectionBootstrapped = false
                    terminalEffectsEnabled = false
                    stopOutputPump()
                    do {
                        try await Task.sleep(
                            nanoseconds: initialBootstrapRetryIntervalNanoseconds
                        )
                    } catch {
                        return
                    }
                    continue
                }
                disableRemoteIO(error, matching: candidate)
            }

            do {
                try await Task.sleep(nanoseconds: refreshIntervalNanoseconds)
            } catch {
                return
            }
        }
    }

    private func publishAcceptedBootstrap(
        _ next: RemoteBootstrapSnapshot,
        for candidate: ActiveConnection
    ) {
        cancelInitialDirectLinkGrace()
        adopt(next)
        connectionBootstrapped = true
        connectionRoute = candidate.transport.route
        let supportsOutput = Self.supportsOutput(in: next)
        terminalEffectsEnabled = supportsOutput && Self.supportsInput(in: next)
        connectionState = supportsOutput
            ? .connected(name: next.macName ?? "Host")
            : .incompatible(
                message: "This Host does not advertise remote terminal output."
            )

        switch candidate.transport.route {
        case .direct:
            successfulLinkRefreshes = 0
            // The preferred route recovered before a Link candidate won.
            cancelRouteProbe(target: .link)
        case .link:
            successfulLinkRefreshes += 1
            if successfulLinkRefreshes >= directProbeSuccessfulLinkRefreshes {
                successfulLinkRefreshes = 0
                scheduleDirectProbeIfPossible(matching: candidate)
            }
        case .ssh:
            break
        }

        rebindSelectedPaneAndEnsurePump()
        if supportsOutput {
            enqueuePendingDesktopFitClears()
            enqueueLatestSelectedDesktopFit()
            if let selectedSessionID {
                requestMarkReadIfNeeded(selectedSessionID)
            }
        }
        startEffectWorkerIfNeeded(candidate)
    }

    private func scheduleFallbackIfPossible(
        for error: Error,
        matching candidate: ActiveConnection
    ) {
        guard Self.failureIsRouteReachability(error),
              let plan = pairedPlan,
              plan.hostID == candidate.transport.expectedHostID
        else { return }

        switch candidate.transport.route {
        case .direct:
            guard let link = plan.link else { return }
            scheduleRouteProbe(link, matching: candidate)
        case .link:
            guard !forceLinkForDevelopment else { return }
            scheduleRouteProbe(plan.direct, matching: candidate)
        case .ssh:
            break
        }
    }

    /// A saved LAN address can black-hole for the full request deadline when
    /// the Controller is away from home. Give Direct a short head start, then
    /// verify Link concurrently; Direct still wins by canceling the candidate
    /// as soon as its bootstrap succeeds.
    private func scheduleInitialDirectLinkGraceIfPossible(
        matching candidate: ActiveConnection
    ) {
        guard candidate.transport.route == .direct,
              let plan = pairedPlan,
              plan.link != nil,
              plan.hostID == candidate.transport.expectedHostID
        else { return }
        directLinkGraceTask = Task { [weak self] in
            guard let self else { return }
            // A safe same-Host reconnect may still be retiring the previous
            // effect/fit generation. Start neither Direct's Link race nor its
            // grace clock before that transitive barrier has settled.
            await candidate.effectStartBarrier?.value
            guard isCurrent(candidate), !Task.isCancelled else { return }
            do {
                try await Task.sleep(nanoseconds: initialDirectLinkGraceNanoseconds)
            } catch {
                return
            }
            guard isCurrent(candidate),
                  !connectionBootstrapped,
                  !Task.isCancelled
            else { return }
            directLinkGraceTask = nil
            guard let link = plan.link else { return }
            scheduleRouteProbe(link, matching: candidate)
        }
    }

    private func cancelInitialDirectLinkGrace() {
        directLinkGraceTask?.cancel()
        directLinkGraceTask = nil
    }

    private func scheduleDirectProbeIfPossible(matching candidate: ActiveConnection) {
        guard !forceLinkForDevelopment,
              candidate.transport.route == .link,
              let plan = pairedPlan,
              plan.hostID == candidate.transport.expectedHostID
        else { return }
        scheduleRouteProbe(plan.direct, matching: candidate)
    }

    private func scheduleRouteProbe(
        _ transport: RemoteHostTransport,
        matching current: ActiveConnection
    ) {
        guard routeProbeTask == nil,
              isCurrent(current),
              transport.route != current.transport.route
        else { return }

        routeProbeToken &+= 1
        let token = routeProbeToken
        let planEpoch = pairedPlanEpoch
        let currentConnectionEpoch = current.epoch
        routeProbeTarget = transport.route
        routeProbeTask = Task { [weak self] in
            guard let self else { return }
            await self.runRouteProbe(
                transport,
                token: token,
                planEpoch: planEpoch,
                currentConnectionEpoch: currentConnectionEpoch
            )
        }
    }

    private func runRouteProbe(
        _ transport: RemoteHostTransport,
        token: UInt64,
        planEpoch: UInt64,
        currentConnectionEpoch: UInt64
    ) async {
        let candidate: RouteProbeCandidate
        do {
            candidate = RouteProbeCandidate(backend: try backendFactory(transport))
        } catch {
            finishRouteProbe(token: token)
            return
        }
        guard routeProbeToken == token, !Task.isCancelled else {
            await candidate.close()
            return
        }
        routeProbeCandidate = candidate
        let backend = candidate.backend

        do {
            let next = try await withTaskCancellationHandler {
                try await backend.bootstrap()
            } onCancel: {
                Task { await candidate.close() }
            }
            try validateProbedIdentity(next, for: transport)
            guard Self.supportsOutput(in: next) else {
                throw NativeRemoteBackendError(
                    result: -1,
                    code: "incompatible_host_protocol",
                    message: "This Host does not advertise remote terminal output."
                )
            }
            guard routeProbeIsCurrent(
                token: token,
                planEpoch: planEpoch,
                currentConnectionEpoch: currentConnectionEpoch
            ) else {
                await candidate.close()
                return
            }
            promoteVerifiedRoute(
                transport,
                candidate: candidate,
                snapshot: next,
                token: token
            )
        } catch {
            await candidate.close()
            finishRouteProbe(token: token)
        }
    }

    private func validateProbedIdentity(
        _ next: RemoteBootstrapSnapshot,
        for transport: RemoteHostTransport
    ) throws {
        if let expectedHostID = transport.expectedHostID,
           next.macID != expectedHostID {
            throw Self.hostIdentityChangedError
        }
        if let pinnedHostID, next.macID != pinnedHostID {
            throw Self.hostIdentityChangedError
        }
    }

    private func routeProbeIsCurrent(
        token: UInt64,
        planEpoch: UInt64,
        currentConnectionEpoch: UInt64
    ) -> Bool {
        !Task.isCancelled
            && routeProbeToken == token
            && pairedPlanEpoch == planEpoch
            && connection?.epoch == currentConnectionEpoch
            && connectionEpoch == currentConnectionEpoch
    }

    private func promoteVerifiedRoute(
        _ transport: RemoteHostTransport,
        candidate: RouteProbeCandidate,
        snapshot next: RemoteBootstrapSnapshot,
        token: UInt64
    ) {
        guard routeProbeToken == token else {
            Task { await candidate.close() }
            return
        }
        let backend = candidate.backend

        routeProbeTask = nil
        routeProbeTarget = nil
        routeProbeCandidate = nil
        connectionEpoch &+= 1
        let epoch = connectionEpoch
        let oldConnection = connection
        let carriedPendingMarkReads = pendingMarkReadSessionIDs
        let carriedAwaitingMarkReads = markReadAwaitingSnapshotClearSessionIDs
        let carriedFailedMarkReads = failedAutomaticMarkReadSessionIDs
        let priorWorker = invalidateConnectionWork()
        // The candidate bootstrap can predate an in-flight mark-read on the
        // retiring route. Keep those latches until the old effect tail has
        // settled and a fresh bootstrap proves the Host's resulting state.
        pendingMarkReadSessionIDs.formUnion(carriedPendingMarkReads)
        markReadAwaitingSnapshotClearSessionIDs.formUnion(carriedAwaitingMarkReads)
        failedAutomaticMarkReadSessionIDs.formUnion(carriedFailedMarkReads)
        connection = nil
        let transferredFits = DesktopFitOwnership()
        let predecessorRetirement = retireForRoutePromotion(
            oldConnection,
            after: priorWorker,
            transferringFitsTo: transferredFits
        )

        // Direct and Link are two routes to one durably pinned Host. Retain
        // the last snapshot, selected Session, and VT panes; only the backend
        // cursor belongs to the replaced connection generation.
        continuityKey = transport.continuityKey
        pinnedHostID = transport.expectedHostID
        paneCursorReady.removeAll()
        connectionRoute = transport.route

        let promoted = ActiveConnection(
            epoch: epoch,
            transport: transport,
            backend: backend,
            effectStartBarrier: predecessorRetirement,
            desktopFits: transferredFits,
            markReadsSuppressedUntilPostBarrier: carriedPendingMarkReads,
            needsPostBarrierRefresh: oldConnection != nil
        )
        connection = promoted
        do {
            try validateIdentity(next, for: promoted)
        } catch {
            // This is defensive: the candidate was checked immediately above
            // and NativeRemoteBackend also enforces its durable expected id.
            connection = nil
            connectionState = .repairRequired(message: error.localizedDescription)
            close(promoted, after: predecessorRetirement)
            return
        }
        publishAcceptedBootstrap(next, for: promoted)
        startRefreshLoop(for: promoted, initiallyAccepted: true)
    }

    private func finishRouteProbe(token: UInt64) {
        guard routeProbeToken == token else { return }
        routeProbeTask = nil
        routeProbeTarget = nil
        routeProbeCandidate = nil
    }

    private func cancelRouteProbe(target: RemoteHostConnectionRoute? = nil) {
        if let target, routeProbeTarget != target { return }
        routeProbeToken &+= 1
        routeProbeTask?.cancel()
        routeProbeTask = nil
        routeProbeTarget = nil
        let candidate = routeProbeCandidate
        routeProbeCandidate = nil
        if let candidate {
            Task { await candidate.close() }
        }
    }

    private static func failureIsRouteReachability(_ error: Error) -> Bool {
        guard let bridgeError = error as? NativeRemoteBackendError else { return false }
        return switch bridgeError.code {
        case "host_connection_launch_failed",
             "host_connection_disconnected",
             "host_connection_timed_out",
             "host_connection_closed":
            true
        default:
            false
        }
    }

    private func validateIdentity(
        _ next: RemoteBootstrapSnapshot,
        for candidate: ActiveConnection
    ) throws {
        if let expectedHostID = candidate.transport.expectedHostID,
           next.macID != expectedHostID {
            throw Self.hostIdentityChangedError
        }
        if let pinnedHostID,
           let reportedHostID = next.macID,
           reportedHostID != pinnedHostID {
            throw Self.hostIdentityChangedError
        }
        if let reportedHostID = next.macID {
            pinnedHostID = reportedHostID
            paneHostKey = "host:\(reportedHostID)"
        } else if let pinnedHostID {
            // A paired/saved Host must never become anonymous.
            guard candidate.transport.expectedHostID == nil else {
                throw Self.hostIdentityChangedError
            }
            paneHostKey = "host:\(pinnedHostID)"
        } else {
            paneHostKey = candidate.transport.continuityKey.paneFallbackKey
        }
    }

    private static var hostIdentityChangedError: NativeRemoteBackendError {
        NativeRemoteBackendError(
            result: -1,
            code: "host_identity_changed",
            message: "Refusing a remote Host whose identity no longer matches the saved Host."
        )
    }

    private func retireForTerminalError(
        _ error: Error,
        matching candidate: ActiveConnection
    ) -> Bool {
        guard let bridgeError = error as? NativeRemoteBackendError else { return false }
        let terminalState: RemoteHostConnectionState
        switch bridgeError.code {
        case "host_identity_changed":
            terminalState = .repairRequired(message: bridgeError.message)
        case "incompatible_host_protocol":
            terminalState = .incompatible(message: bridgeError.message)
        default:
            return false
        }

        guard isCurrent(candidate) else { return true }
        cancelInitialDirectLinkGrace()
        cancelRouteProbe()
        connectionEpoch &+= 1
        let retiringPendingMarkReads = pendingMarkReadSessionIDs
        let retiringAwaitingMarkReads = markReadAwaitingSnapshotClearSessionIDs
        let retiringFailedMarkReads = failedAutomaticMarkReadSessionIDs
        let priorWorker = invalidateConnectionWork()
        connection = nil
        connectionState = terminalState
        if let paneHostKey {
            paneCache.removeHost(paneHostKey)
        }
        paneCursorReady.removeAll()
        let retirement = close(candidate, after: priorWorker) ?? priorWorker
        // Identity/protocol failures leave the Host selected so the user can
        // repair or retry it. Preserve the same continuity barrier as an
        // explicit disconnect: a new backend must not bootstrap or send a fit
        // before this generation's effect tail and compensating clears finish.
        storeOutstandingRetirement(
            for: candidate.transport.continuityKey,
            task: retirement,
            pendingMarkReads: retiringPendingMarkReads,
            awaitingMarkReads: retiringAwaitingMarkReads,
            failedMarkReads: retiringFailedMarkReads
        )
        return true
    }

    @discardableResult
    private func invalidateConnectionWork() -> Task<Void, Never>? {
        cancelInitialDirectLinkGrace()
        var priorWorkers = retiredEffectTails
        retiredEffectTails.removeAll(keepingCapacity: false)
        if let effectWorker {
            priorWorkers.append(effectWorker)
        }
        refreshTask?.cancel()
        refreshTask = nil
        stopOutputPump()
        resizeTask?.cancel()
        resizeTask = nil
        recoveryEpoch &+= 1
        effectQueueEpoch &+= 1
        effectWorkerToken &+= 1
        effectWorker = nil
        queuedEffects.removeAll(keepingCapacity: false)
        pendingWriteBytes = 0
        pendingMarkReadSessionIDs.removeAll()
        markReadRecoveryBarrier = nil
        markReadsSuppressedUntilRecovery.removeAll()
        markReadAwaitingSnapshotClearSessionIDs.removeAll()
        failedAutomaticMarkReadSessionIDs.removeAll()
        incompleteUTF8InputBySession.removeAll()
        pendingDesktopFitClearSessionIDs.removeAll()
        enqueuedDesktopFitClearSessionIDs.removeAll()
        lastQueuedViewportBySession.removeAll()
        failedFitBySession.removeAll()
        connectionBootstrapped = false
        terminalEffectsEnabled = false
        guard !priorWorkers.isEmpty else { return nil }
        return Task {
            for worker in priorWorkers {
                await worker.value
            }
        }
    }

    @discardableResult
    private func close(
        _ connection: ActiveConnection?,
        after priorWorker: Task<Void, Never>?
    ) -> Task<Void, Never>? {
        guard let connection else { return nil }
        return Task {
            await priorWorker?.value
            await connection.effectStartBarrier?.value
            await Self.clearOwnedDesktopFitsAndClose(connection)
        }
    }

    private func combineTasks(
        _ first: Task<Void, Never>?,
        _ second: Task<Void, Never>?
    ) -> Task<Void, Never>? {
        let tasks = [first, second].compactMap { $0 }
        guard !tasks.isEmpty else { return nil }
        return Task {
            for task in tasks {
                await task.value
            }
        }
    }

    private func storeOutstandingRetirement(
        for key: RemoteHostContinuityKey,
        task: Task<Void, Never>?,
        pendingMarkReads: Set<String>,
        awaitingMarkReads: Set<String>,
        failedMarkReads: Set<String>
    ) {
        let existing = outstandingRetirements.removeValue(forKey: key)
        let combined = combineTasks(existing?.task, task)
        let hasLatches = !pendingMarkReads.isEmpty
            || !awaitingMarkReads.isEmpty
            || !failedMarkReads.isEmpty
            || existing != nil
        guard combined != nil || hasLatches else { return }
        outstandingRetirements[key] = OutstandingRetirement(
            task: combined ?? Task {},
            pendingMarkReads: pendingMarkReads
                .union(existing?.pendingMarkReads ?? []),
            awaitingMarkReads: awaitingMarkReads
                .union(existing?.awaitingMarkReads ?? []),
            failedMarkReads: failedMarkReads
                .union(existing?.failedMarkReads ?? [])
        )
    }

    /// A verified Direct/Link candidate reaches the same durable Host. Never
    /// reconnect a failed retiring route just to clear its fit: transfer that
    /// ownership after its effect tail settles, close it, then compensate on
    /// the working route before any replacement effect starts.
    private func retireForRoutePromotion(
        _ connection: ActiveConnection?,
        after priorWorker: Task<Void, Never>?,
        transferringFitsTo destination: DesktopFitOwnership
    ) -> Task<Void, Never>? {
        guard let connection else { return priorWorker }
        destination.armInheritedClear()
        return Task {
            await priorWorker?.value
            await connection.effectStartBarrier?.value
            for sessionID in connection.desktopFits.snapshot() {
                destination.insert(sessionID)
            }
            await connection.backend.close()
        }
    }

    private nonisolated static func clearOwnedDesktopFitsAndClose(
        _ connection: ActiveConnection
    ) async {
        // Stable sort makes teardown deterministic for tests and diagnostics.
        for sessionID in connection.desktopFits.snapshot().sorted() {
            do {
                _ = try await connection.backend.clearDesktopFit(sessionID: sessionID)
                connection.desktopFits.remove(sessionID)
            } catch {
                // Retirement is best effort. Never replay this clear and
                // never keep an unusable backend alive because cleanup failed.
            }
        }
        await connection.backend.close()
    }

    private func isCurrent(_ candidate: ActiveConnection) -> Bool {
        connection?.epoch == candidate.epoch
            && connectionEpoch == candidate.epoch
    }

    private func adopt(_ next: RemoteBootstrapSnapshot) {
        if snapshot.map({ !Self.snapshotContentEqual($0, next) }) ?? true {
            snapshot = next
        }

        let validIDs = Set(next.sessions.map(\.id))
        pendingMarkReadSessionIDs.formIntersection(validIDs)
        markReadAwaitingSnapshotClearSessionIDs.formIntersection(validIDs)
        failedAutomaticMarkReadSessionIDs.formIntersection(validIDs)
        for session in next.sessions where !session.unread {
            pendingMarkReadSessionIDs.remove(session.id)
            markReadAwaitingSnapshotClearSessionIDs.remove(session.id)
            failedAutomaticMarkReadSessionIDs.remove(session.id)
        }
        latestViewportByPane = latestViewportByPane.filter { validIDs.contains($0.key.sessionID) }
        paneCursorReady = paneCursorReady.filter { validIDs.contains($0.key.sessionID) }
        incompleteUTF8InputBySession = incompleteUTF8InputBySession.filter {
            validIDs.contains($0.key)
        }

        if let pending = pendingCreatedSelectionID, validIDs.contains(pending) {
            pendingCreatedSelectionID = nil
            replacementDefaultSelectionSuppressed = false
            transitionSelectedSession(to: pending)
            prunePaneCache(using: next)
            return
        }

        if let pending = pendingReplacementSelection {
            switch Self.replacementSelectionResolution(
                pending,
                sessions: next.sessions
            ) {
            case let .select(replacementID):
                pendingReplacementSelection = nil
                replacementDefaultSelectionSuppressed = false
                transitionSelectedSession(to: replacementID)
                prunePaneCache(using: next)
                return
            case let .wait(updated):
                pendingReplacementSelection = updated
            case .cancel:
                pendingReplacementSelection = nil
            }

            // Preserve a still-valid prior choice while the replacement is
            // unpublished or ambiguous. If it vanished, show no Session
            // rather than silently adopting an unrelated default row.
            if let selectedSessionID, validIDs.contains(selectedSessionID) {
                prunePaneCache(using: next)
                return
            }
            transitionSelectedSession(to: nil)
            prunePaneCache(using: next)
            return
        }

        if let selectedSessionID, validIDs.contains(selectedSessionID) {
            prunePaneCache(using: next)
            return
        }

        if replacementDefaultSelectionSuppressed {
            transitionSelectedSession(to: nil)
            prunePaneCache(using: next)
            return
        }

        transitionSelectedSession(to: Self.defaultSessionID(in: next))
        prunePaneCache(using: next)
    }

    private func transitionSelectedSession(to nextSessionID: String?) {
        guard selectedSessionID != nextSessionID else { return }
        let previousSessionID = selectedSessionID
        if let previousSessionID {
            requestDesktopFitClear(previousSessionID)
        }
        selectedSessionID = nextSessionID
        stopOutputPump()
        resizeTask?.cancel()
        resizeTask = nil
        rebindSelectedPaneAndEnsurePump()
    }

    private func prunePaneCache(using snapshot: RemoteBootstrapSnapshot? = nil) {
        guard let paneHostKey else {
            paneCache.removeAll()
            return
        }
        let current = snapshot ?? self.snapshot
        let liveKeys = Set((current?.sessions ?? []).map {
            RemoteTerminalPaneKey(hostID: paneHostKey, sessionID: $0.id)
        })
        let selectedKey = selectedSessionID.map {
            RemoteTerminalPaneKey(hostID: paneHostKey, sessionID: $0)
        }
        paneCache.prune(keeping: liveKeys, selectedKey: selectedKey)
    }

    private func rebindSelectedPaneAndEnsurePump() {
        guard let selectedSessionID,
              let paneHostKey,
              let connection
        else {
            stopOutputPump()
            return
        }
        let key = RemoteTerminalPaneKey(
            hostID: paneHostKey,
            sessionID: selectedSessionID
        )
        if let pane = paneCache.existingPane(for: key) {
            pane.updateCallbacks(
                onInput: makeInputHandler(
                    sessionID: selectedSessionID,
                    paneKey: key,
                    connectionEpoch: connection.epoch
                ),
                onResize: makeResizeHandler(
                    sessionID: selectedSessionID,
                    paneKey: key,
                    connectionEpoch: connection.epoch
                )
            )
        }
        ensureOutputPump()
    }

    private func makeInputHandler(
        sessionID: String,
        paneKey: RemoteTerminalPaneKey,
        connectionEpoch: UInt64
    ) -> RemoteTerminalInputHandler {
        { [weak self] data in
            guard let self,
                  self.connection?.epoch == connectionEpoch,
                  self.selectedSessionID == sessionID,
                  self.currentPaneKey == paneKey
            else {
                return
            }
            self.sendTerminalInput(data, to: sessionID)
        }
    }

    private func makeResizeHandler(
        sessionID: String,
        paneKey: RemoteTerminalPaneKey,
        connectionEpoch: UInt64
    ) -> RemoteTerminalResizeHandler {
        { [weak self] viewport in
            guard let self,
                  self.connection?.epoch == connectionEpoch,
                  self.selectedSessionID == sessionID,
                  self.currentPaneKey == paneKey
            else {
                return
            }
            self.scheduleDesktopFit(
                sessionID: sessionID,
                paneKey: paneKey,
                connectionEpoch: connectionEpoch,
                viewport: viewport
            )
        }
    }

    private var currentPaneKey: RemoteTerminalPaneKey? {
        guard let paneHostKey, let selectedSessionID else { return nil }
        return RemoteTerminalPaneKey(
            hostID: paneHostKey,
            sessionID: selectedSessionID
        )
    }

    private func scheduleDesktopFit(
        sessionID: String,
        paneKey: RemoteTerminalPaneKey,
        connectionEpoch: UInt64,
        viewport: RemoteTerminalViewport
    ) {
        guard viewport.columns > 0, viewport.rows > 0 else { return }
        resizeTask?.cancel()
        resizeTask = Task { [weak self] in
            do {
                try await Task.sleep(nanoseconds: self?.resizeDebounceNanoseconds ?? 0)
            } catch {
                return
            }
            guard let self,
                  self.connection?.epoch == connectionEpoch,
                  self.selectedSessionID == sessionID,
                  self.currentPaneKey == paneKey
            else {
                return
            }
            let requested = RequestedDesktopFit(
                columns: UInt16(clamping: viewport.columns),
                rows: UInt16(clamping: viewport.rows)
            )
            self.latestViewportByPane[paneKey] = requested
            self.enqueueDesktopFitIfNeeded(
                sessionID: sessionID,
                columns: requested.columns,
                rows: requested.rows,
                viewport: requested
            )
        }
    }

    private func ensureOutputPump() {
        guard connectionBootstrapped,
              Self.supportsOutput(in: snapshot),
              let connection,
              let selectedSessionID,
              let paneHostKey
        else {
            return
        }
        let paneKey = RemoteTerminalPaneKey(
            hostID: paneHostKey,
            sessionID: selectedSessionID
        )
        guard let pane = paneCache.existingPane(for: paneKey) else { return }
        let paneIdentity = ObjectIdentifier(pane)

        if let existing = outputPumpIdentity,
           existing.connectionEpoch == connection.epoch,
           existing.paneKey == paneKey,
           existing.paneIdentity == paneIdentity,
           outputTask != nil {
            return
        }

        stopOutputPump()
        outputPumpToken &+= 1
        let identity = OutputPumpIdentity(
            token: outputPumpToken,
            connectionEpoch: connection.epoch,
            paneKey: paneKey,
            paneIdentity: paneIdentity
        )
        outputPumpIdentity = identity
        outputTask = Task { [weak self] in
            guard let self else { return }
            await self.runOutputPump(connection: connection, identity: identity)
            self.finishOutputPump(identity)
        }
    }

    private func runOutputPump(
        connection: ActiveConnection,
        identity: OutputPumpIdentity
    ) async {
        guard await prepareOutputCursor(connection: connection, identity: identity) else {
            return
        }

        while !Task.isCancelled {
            guard outputPumpIsCurrent(identity, connection: connection),
                  let pane = paneCache.existingPane(for: identity.paneKey)
            else {
                return
            }

            guard paneAttachmentProbe(pane) else {
                if !(await sleepOutputIdle()) { return }
                continue
            }

            let page: NativeRemoteOutputPage
            do {
                // The Host contract caps pages at 200 KiB. Keep a little
                // headroom for adapters while still amortizing long output.
                page = try await connection.backend.pollOutput(
                    sessionID: identity.paneKey.sessionID,
                    limit: 192 * 1024,
                    waitMilliseconds: 1_000
                )
            } catch is CancellationError {
                return
            } catch {
                guard outputPumpIsCurrent(identity, connection: connection) else {
                    return
                }
                if Self.outputFailureIsTransient(error) {
                    if !(await sleepOutputIdle()) { return }
                    continue
                }
                handleTransportFailure(error, matching: connection)
                return
            }

            guard outputPumpIsCurrent(identity, connection: connection),
                  page.metadata.sessionID == identity.paneKey.sessionID,
                  let currentPane = paneCache.existingPane(for: identity.paneKey),
                  currentPane === pane,
                  paneAttachmentProbe(currentPane)
            else {
                await connection.backend.discardOutput(page)
                return
            }

            let accepted = paneOutputFeeder(
                currentPane,
                page.bytes,
                page.metadata.resetBeforeFeed
            )
            guard accepted else {
                await connection.backend.discardOutput(page)
                if !outputPumpIsCurrent(identity, connection: connection) {
                    return
                }
                if !(await sleepOutputIdle()) { return }
                continue
            }

            // Acceptance and commit are intentionally adjacent. Once the VT
            // accepted bytes, committing is correct even if selection changes
            // while this await is in flight; not committing would replay the
            // same bytes into a retained pane on the next selection.
            do {
                try await connection.backend.commitOutput(page)
            } catch {
                // Bytes already entered this VT, but the backend cursor may
                // still point before them. Recovery must request a fresh tail
                // and atomically replace the pane instead of appending a
                // duplicate control sequence.
                paneCursorReady.removeValue(forKey: identity.paneKey)
                guard outputPumpIsCurrent(identity, connection: connection) else {
                    // Selection can move away and back while commit is in
                    // flight. In that race the replacement pump may have
                    // observed the cursor as ready immediately before this
                    // failure revoked it. Restart that exact current pane so
                    // prepareOutputCursor cannot continue polling from the
                    // now-uncertain cursor.
                    if isCurrent(connection),
                       selectedSessionID == identity.paneKey.sessionID,
                       currentPaneKey == identity.paneKey {
                        stopOutputPump()
                        ensureOutputPump()
                    }
                    return
                }
                handleTransportFailure(error, matching: connection)
                return
            }

            // The receipt belongs only to the captured generation. There is
            // no state to publish, but this guard makes that rule explicit.
            guard outputPumpIsCurrent(identity, connection: connection) else {
                return
            }
        }
    }

    private func prepareOutputCursor(
        connection: ActiveConnection,
        identity: OutputPumpIdentity
    ) async -> Bool {
        guard outputPumpIsCurrent(identity, connection: connection),
              let pane = paneCache.existingPane(for: identity.paneKey),
              ObjectIdentifier(pane) == identity.paneIdentity
        else { return false }

        let cursorIdentity = PaneCursorIdentity(
            connectionEpoch: connection.epoch,
            paneIdentity: identity.paneIdentity
        )
        if paneCursorReady[identity.paneKey] == cursorIdentity {
            return true
        }

        do {
            try await connection.backend.resetOutput(
                sessionID: identity.paneKey.sessionID
            )
        } catch is CancellationError {
            return false
        } catch {
            guard outputPumpIsCurrent(identity, connection: connection) else {
                return false
            }
            handleTransportFailure(error, matching: connection)
            return false
        }

        guard outputPumpIsCurrent(identity, connection: connection),
              let currentPane = paneCache.existingPane(for: identity.paneKey),
              ObjectIdentifier(currentPane) == identity.paneIdentity
        else { return false }
        paneCursorReady[identity.paneKey] = cursorIdentity
        return true
    }

    private func outputPumpIsCurrent(
        _ identity: OutputPumpIdentity,
        connection: ActiveConnection
    ) -> Bool {
        !Task.isCancelled
            && self.connection?.epoch == connection.epoch
            && connectionEpoch == connection.epoch
            && outputPumpIdentity == identity
            && paneCache.existingPane(for: identity.paneKey).map(ObjectIdentifier.init)
                == identity.paneIdentity
            && selectedSessionID == identity.paneKey.sessionID
            && currentPaneKey == identity.paneKey
            && connectionBootstrapped
    }

    private func finishOutputPump(_ identity: OutputPumpIdentity) {
        guard outputPumpIdentity == identity else { return }
        outputTask = nil
        outputPumpIdentity = nil
    }

    private func stopOutputPump() {
        outputPumpToken &+= 1
        outputPumpIdentity = nil
        outputTask?.cancel()
        outputTask = nil
    }

    private func sleepOutputIdle() async -> Bool {
        do {
            try await Task.sleep(nanoseconds: outputIdleIntervalNanoseconds)
            return !Task.isCancelled
        } catch {
            return false
        }
    }

    private func handleTransportFailure(
        _ error: Error,
        matching candidate: ActiveConnection
    ) {
        guard isCurrent(candidate) else { return }
        if retireForTerminalError(error, matching: candidate) {
            return
        }
        disableRemoteIO(error, matching: candidate)
    }

    private static func outputFailureIsTransient(_ error: Error) -> Bool {
        (error as? NativeRemoteBackendError)?.code == "output_page_pending"
    }

    @discardableResult
    private func enqueueEffect(_ effect: RemoteEffect) -> Bool {
        guard connectionBootstrapped,
              let connection,
              Self.effectSupported(effect, by: snapshot),
              Self.effectDoesNotRequireSelection(effect)
                  || selectedSessionID == effect.sessionID
        else { return false }

        if case let .clearFit(sessionID) = effect {
            guard connection.desktopFits.snapshot().contains(sessionID) else {
                return false
            }
        } else if !Self.supportsOutput(in: snapshot) {
            return false
        }

        if case .write = effect, !terminalEffectsEnabled { return false }
        if case .clearFit = effect {
            // An owned Session may disappear from the next bootstrap before
            // its compensating clear is sent.
        } else if snapshot?.sessions.contains(where: { $0.id == effect.sessionID }) != true {
            return false
        }

        switch effect {
        case let .write(sessionID, data):
            guard pendingWriteBytes <= Self.maximumPendingTerminalInputBytes - data.count else {
                disableRemoteIO(Self.inputBackpressureError, matching: connection)
                return false
            }
            pendingWriteBytes += data.count
            appendWriteBatches(sessionID: sessionID, data: data)
        case let .markRead(sessionID):
            guard pendingMarkReadSessionIDs.insert(sessionID).inserted else { return false }
            queuedEffects.append(QueuedEffect(effect: effect))
        case let .fit(sessionID, columns, rows):
            if case let .fit(lastID, _, _)? = queuedEffects.last?.effect,
               lastID == sessionID {
                queuedEffects[queuedEffects.count - 1].effect = .fit(
                    sessionID: sessionID,
                    columns: columns,
                    rows: rows
                )
            } else {
                queuedEffects.append(QueuedEffect(effect: effect))
            }
        case .clearFit:
            queuedEffects.append(QueuedEffect(effect: effect))
        }

        startEffectWorkerIfNeeded(connection)
        return true
    }

    /// The current Rust wire effect is UTF-8 text even though Ghostty's input
    /// callback is byte-oriented. Preserve a scalar split across callbacks
    /// (at most three bytes) rather than dispatching an invalid half-scalar.
    private func prepareTerminalInput(_ data: Data, sessionID: String) -> Data? {
        var combined = incompleteUTF8InputBySession.removeValue(forKey: sessionID) ?? Data()
        combined.append(data)
        if String(data: combined, encoding: .utf8) != nil {
            return combined
        }

        for suffixCount in 1...min(3, combined.count) {
            let split = combined.index(combined.endIndex, offsetBy: -suffixCount)
            let prefix = Data(combined[..<split])
            let suffix = Data(combined[split...])
            if String(data: prefix, encoding: .utf8) != nil,
               Self.isIncompleteUTF8ScalarPrefix(suffix) {
                incompleteUTF8InputBySession[sessionID] = suffix
                return prefix
            }
        }

        if let connection {
            disableRemoteIO(Self.invalidTerminalInputError, matching: connection)
        }
        return nil
    }

    private static func isIncompleteUTF8ScalarPrefix(_ bytes: Data) -> Bool {
        guard let first = bytes.first else { return false }
        let expectedLength: Int
        switch first {
        case 0xC2...0xDF: expectedLength = 2
        case 0xE0...0xEF: expectedLength = 3
        case 0xF0...0xF4: expectedLength = 4
        default: return false
        }
        guard bytes.count < expectedLength else { return false }
        let values = Array(bytes)
        if values.count >= 2 {
            let second = values[1]
            let validSecond: Bool
            switch first {
            case 0xE0: validSecond = (0xA0...0xBF).contains(second)
            case 0xED: validSecond = (0x80...0x9F).contains(second)
            case 0xF0: validSecond = (0x90...0xBF).contains(second)
            case 0xF4: validSecond = (0x80...0x8F).contains(second)
            default: validSecond = (0x80...0xBF).contains(second)
            }
            guard validSecond else { return false }
        }
        return values.dropFirst(2).allSatisfy { (0x80...0xBF).contains($0) }
    }

    private func appendWriteBatches(sessionID: String, data: Data) {
        var offset = data.startIndex
        while offset < data.endIndex {
            if case let .write(lastID, existing)? = queuedEffects.last?.effect,
               lastID == sessionID,
               existing.count < Self.maximumTerminalWriteBytes {
                let capacity = Self.maximumTerminalWriteBytes - existing.count
                let end = Self.utf8SafeBatchEnd(in: data, from: offset, limit: capacity)
                // The remaining capacity may be smaller than the next
                // multi-byte scalar. Leave it unused and start a new batch.
                if end == offset {
                    let freshEnd = Self.utf8SafeBatchEnd(
                        in: data,
                        from: offset,
                        limit: Self.maximumTerminalWriteBytes
                    )
                    let boundedEnd = freshEnd == offset
                        ? data.index(
                            offset,
                            offsetBy: min(
                                Self.maximumTerminalWriteBytes,
                                data.distance(from: offset, to: data.endIndex)
                            )
                        )
                        : freshEnd
                    queuedEffects.append(QueuedEffect(effect: .write(
                        sessionID: sessionID,
                        data: Data(data[offset..<boundedEnd])
                    )))
                    offset = boundedEnd
                    continue
                }
                var combined = existing
                combined.append(data[offset..<end])
                queuedEffects[queuedEffects.count - 1].effect = .write(
                    sessionID: sessionID,
                    data: combined
                )
                offset = end
                continue
            }

            let end = Self.utf8SafeBatchEnd(
                in: data,
                from: offset,
                limit: Self.maximumTerminalWriteBytes
            )
            let boundedEnd = end == offset
                ? data.index(
                    offset,
                    offsetBy: min(
                        Self.maximumTerminalWriteBytes,
                        data.distance(from: offset, to: data.endIndex)
                    )
                )
                : end
            queuedEffects.append(QueuedEffect(effect: .write(
                sessionID: sessionID,
                data: Data(data[offset..<boundedEnd])
            )))
            offset = boundedEnd
        }
    }

    private static func utf8SafeBatchEnd(
        in data: Data,
        from start: Data.Index,
        limit: Int
    ) -> Data.Index {
        var end = data.index(start, offsetBy: min(limit, data.distance(from: start, to: data.endIndex)))
        if end < data.endIndex {
            while end > start, data[end] & 0b1100_0000 == 0b1000_0000 {
                end = data.index(before: end)
            }
        }
        return end
    }

    private func startEffectWorkerIfNeeded(_ connection: ActiveConnection) {
        guard effectWorker == nil,
              !queuedEffects.isEmpty || connection.desktopFits.hasInheritedClearPending()
        else { return }
        effectWorkerToken &+= 1
        let token = effectWorkerToken
        let queueEpoch = effectQueueEpoch
        effectWorker = Task { [weak self] in
            guard let self else { return }
            await self.runEffectWorker(
                connection: connection,
                queueEpoch: queueEpoch,
                token: token
            )
            self.finishEffectWorker(token: token)
        }
    }

    private func runEffectWorker(
        connection: ActiveConnection,
        queueEpoch: UInt64,
        token: UInt64
    ) async {
        // A previous connection to the same Host may still own a global
        // desktop fit. Its ordered clear must land before this generation can
        // apply a replacement fit (or any later UI effect).
        await connection.effectStartBarrier?.value
        guard effectWorkerIsCurrent(connection, queueEpoch: queueEpoch, token: token) else {
            return
        }
        let inheritedFits = connection.desktopFits.claimInheritedFitsForClear()
        if !inheritedFits.isEmpty {
            let alreadyQueued = Set(queuedEffects.compactMap { queued -> String? in
                guard case let .clearFit(sessionID) = queued.effect else { return nil }
                return sessionID
            })
            let compensatingClears = inheritedFits
                .subtracting(alreadyQueued)
                .sorted()
                .map { QueuedEffect(effect: .clearFit(sessionID: $0)) }
            queuedEffects.insert(contentsOf: compensatingClears, at: 0)
        }
        while effectWorkerIsCurrent(connection, queueEpoch: queueEpoch, token: token),
              !queuedEffects.isEmpty {
            let queued = queuedEffects.removeFirst()
            do {
                _ = try await queued.effect.perform(on: connection.backend)
                if case let .fit(sessionID, _, _) = queued.effect {
                    // Enqueue-time ownership is conservative for ambiguous
                    // outcomes; success records it again in case a preceding
                    // compensating clear removed the inherited ownership.
                    connection.desktopFits.insert(sessionID)
                }
                if case let .clearFit(sessionID) = queued.effect {
                    let hasReplacementFit = queuedEffects.contains { later in
                        guard case let .fit(laterSessionID, _, _) = later.effect else {
                            return false
                        }
                        return laterSessionID == sessionID
                    }
                    if !hasReplacementFit {
                        connection.desktopFits.remove(sessionID)
                    }
                }
                guard effectWorkerIsCurrent(
                    connection,
                    queueEpoch: queueEpoch,
                    token: token
                ) else { return }
                completeEffect(queued.effect)
            } catch {
                // A failed clear must never be replayed during retirement,
                // unless the Host correlated a semantic NotApplied response.
                // In that one case the inherited fit is proven to remain and
                // its ownership must survive a later replacement fit/close.
                if case let .clearFit(sessionID) = queued.effect {
                    if (error as? NativeRemoteBackendError)?
                        .effectCanContinueOnCurrentGeneration != true {
                        connection.desktopFits.remove(sessionID)
                    }
                }
                guard isCurrent(connection), effectQueueEpoch == queueEpoch else { return }
                if handleEffectFailure(
                    error,
                    queuedEffect: queued,
                    matching: connection
                ) {
                    continue
                }
                return
            }
        }
    }

    private func effectWorkerIsCurrent(
        _ candidate: ActiveConnection,
        queueEpoch: UInt64,
        token: UInt64
    ) -> Bool {
        isCurrent(candidate)
            && connectionBootstrapped
            && effectQueueEpoch == queueEpoch
            && effectWorkerToken == token
    }

    private func finishEffectWorker(token: UInt64) {
        guard effectWorkerToken == token else { return }
        effectWorker = nil
        if let connection,
           !queuedEffects.isEmpty || connection.desktopFits.hasInheritedClearPending() {
            startEffectWorkerIfNeeded(connection)
        }
    }

    private func completeEffect(_ effect: RemoteEffect) {
        switch effect {
        case let .write(_, data):
            pendingWriteBytes = max(0, pendingWriteBytes - data.count)
        case .fit:
            break
        case let .clearFit(sessionID):
            pendingDesktopFitClearSessionIDs.remove(sessionID)
            enqueuedDesktopFitClearSessionIDs.remove(sessionID)
            lastQueuedViewportBySession.removeValue(forKey: sessionID)
        case let .markRead(sessionID):
            pendingMarkReadSessionIDs.remove(sessionID)
            markReadAwaitingSnapshotClearSessionIDs.insert(sessionID)
        }
    }

    /// Returns true only when the Host proved the effect was not applied and
    /// the serial queue may safely continue on the same generation.
    private func handleEffectFailure(
        _ error: Error,
        queuedEffect: QueuedEffect,
        matching candidate: ActiveConnection
    ) -> Bool {
        guard isCurrent(candidate) else { return false }
        let effect = queuedEffect.effect
        if case let .clearFit(sessionID) = effect {
            pendingDesktopFitClearSessionIDs.remove(sessionID)
            enqueuedDesktopFitClearSessionIDs.remove(sessionID)
            lastQueuedViewportBySession.removeValue(forKey: sessionID)
        }
        if case let .markRead(sessionID) = effect {
            pendingMarkReadSessionIDs.remove(sessionID)
            failedAutomaticMarkReadSessionIDs.insert(sessionID)
        }
        if case let .fit(sessionID, columns, rows) = effect {
            failedFitBySession[sessionID] = RequestedDesktopFit(
                columns: columns,
                rows: rows
            )
        }
        if (error as? NativeRemoteBackendError)?.effectCanContinueOnCurrentGeneration == true {
            if case .write = effect {
                guard queuedEffect.notAppliedRetryCount == 0 else {
                    // A missing prefix must never be skipped while later
                    // terminal bytes continue. One correlated NotApplied
                    // retry is safe; a second rejection drops the tail and
                    // requires a fresh bootstrap instead of corrupting order.
                    disableRemoteIO(error, matching: candidate)
                    return false
                }
                var retry = queuedEffect
                retry.notAppliedRetryCount = 1
                queuedEffects.insert(retry, at: 0)
                return true
            }
            // Keep fit ownership conservative. A rejected replacement may be
            // sitting on top of an earlier successful fit; retirement must
            // still clear that Host-global fit before closing or switching.
            return true
        }
        if retireForTerminalError(error, matching: candidate) { return false }
        // An unknown outcome stops the serial queue. Nothing behind the
        // failed call is replayed on this or a fallback transport.
        disableRemoteIO(error, matching: candidate)
        return false
    }

    private func disableRemoteIO(
        _ error: Error,
        matching candidate: ActiveConnection
    ) {
        guard isCurrent(candidate) else { return }
        cancelInitialDirectLinkGrace()
        if !Self.failureIsRouteReachability(error) {
            // A speculative grace probe must never bypass an authentication,
            // identity, capability, or semantic failure on the preferred
            // Direct route.
            cancelRouteProbe()
        }
        let markReadsFromRetiringTail = pendingMarkReadSessionIDs
        recoveryEpoch &+= 1
        connectionBootstrapped = false
        terminalEffectsEnabled = false
        stopOutputPump()
        resizeTask?.cancel()
        resizeTask = nil
        effectQueueEpoch &+= 1
        effectWorkerToken &+= 1
        if let effectWorker {
            retiredEffectTails.append(effectWorker)
        }
        if !markReadsFromRetiringTail.isEmpty {
            let priorBarrier = markReadRecoveryBarrier
            let tails = retiredEffectTails
            markReadsSuppressedUntilRecovery.formUnion(markReadsFromRetiringTail)
            markReadRecoveryBarrier = Task {
                await priorBarrier?.value
                for tail in tails {
                    await tail.value
                }
            }
        }
        effectWorker = nil
        queuedEffects.removeAll(keepingCapacity: false)
        pendingWriteBytes = 0
        pendingMarkReadSessionIDs.removeAll()
        incompleteUTF8InputBySession.removeAll()
        enqueuedDesktopFitClearSessionIDs.removeAll()
        lastQueuedViewportBySession.removeAll()
        scheduleFallbackIfPossible(for: error, matching: candidate)
        pendingMarkReadSessionIDs.formUnion(markReadsFromRetiringTail)
        connectionState = snapshot == nil
            ? (routeProbeTask == nil
                ? .failed(message: error.localizedDescription)
                : .connecting)
            : .reconnecting(message: error.localizedDescription)
    }

    private func enqueueDesktopFitIfNeeded(
        sessionID: String,
        columns: UInt16,
        rows: UInt16,
        viewport: RequestedDesktopFit
    ) {
        if failedFitBySession[sessionID] == viewport { return }
        failedFitBySession.removeValue(forKey: sessionID)
        guard lastQueuedViewportBySession[sessionID] != viewport,
              let connection
        else { return }
        if enqueueEffect(.fit(
            sessionID: sessionID,
            columns: columns,
            rows: rows
        )) {
            lastQueuedViewportBySession[sessionID] = viewport
            connection.desktopFits.insert(sessionID)
        }
    }

    private func requestDesktopFitClear(_ sessionID: String) {
        guard let connection,
              connection.desktopFits.snapshot().contains(sessionID)
        else { return }
        pendingDesktopFitClearSessionIDs.insert(sessionID)
        tryEnqueueDesktopFitClear(sessionID)
    }

    private func tryEnqueueDesktopFitClear(_ sessionID: String) {
        guard pendingDesktopFitClearSessionIDs.contains(sessionID),
              !enqueuedDesktopFitClearSessionIDs.contains(sessionID)
        else { return }
        if enqueueEffect(.clearFit(sessionID: sessionID)) {
            enqueuedDesktopFitClearSessionIDs.insert(sessionID)
        }
    }

    private func enqueuePendingDesktopFitClears() {
        for sessionID in pendingDesktopFitClearSessionIDs.sorted() {
            tryEnqueueDesktopFitClear(sessionID)
        }
    }

    private func enqueueLatestSelectedDesktopFit() {
        guard let selectedSessionID,
              let currentPaneKey,
              let viewport = latestViewportByPane[currentPaneKey]
        else { return }
        enqueueDesktopFitIfNeeded(
            sessionID: selectedSessionID,
            columns: viewport.columns,
            rows: viewport.rows,
            viewport: viewport
        )
    }

    private func requestMarkReadIfNeeded(_ sessionID: String) {
        guard snapshot?.sessions.first(where: { $0.id == sessionID })?.unread == true,
              !markReadAwaitingSnapshotClearSessionIDs.contains(sessionID),
              !failedAutomaticMarkReadSessionIDs.contains(sessionID)
        else { return }
        markRead(sessionID: sessionID)
    }

    private static func effectDoesNotRequireSelection(_ effect: RemoteEffect) -> Bool {
        switch effect {
        case .clearFit, .markRead: true
        case .write, .fit: false
        }
    }

    private static func effectSupported(
        _ effect: RemoteEffect,
        by snapshot: RemoteBootstrapSnapshot?
    ) -> Bool {
        let capability: String
        switch effect {
        case .write: capability = "session.input.write"
        case .fit, .clearFit: capability = "session.resize_desktop"
        case .markRead: capability = "session.mark_read"
        }
        return snapshot?.hostProtocol?.supports(capability) == true
    }

    private static func supportsInput(in snapshot: RemoteBootstrapSnapshot) -> Bool {
        snapshot.hostProtocol?.supports("session.input.write") == true
    }

    private static func supportsOutput(in snapshot: RemoteBootstrapSnapshot?) -> Bool {
        snapshot?.hostProtocol?.supports("session.output.read") == true
    }

    private static var inputBackpressureError: NativeRemoteBackendError {
        NativeRemoteBackendError(
            result: -1,
            code: "remote_input_backpressure",
            message: "Remote input paused because this Host could not keep up with the pending paste. Reconnect before sending it again.",
            kind: "notApplied",
            operation: "terminal write"
        )
    }

    private static var invalidTerminalInputError: NativeRemoteBackendError {
        NativeRemoteBackendError(
            result: -1,
            code: "invalid_terminal_input_utf8",
            message: "Remote input paused because this Host transport cannot encode those terminal bytes safely.",
            kind: "notApplied",
            operation: "terminal write"
        )
    }

    static func defaultSessionID(in snapshot: RemoteBootstrapSnapshot) -> String? {
        let sessions = snapshot.sessions.filter { !$0.archived }
        return sessions.first(where: { $0.activity == .blocked })?.id
            ?? sessions.first(where: { $0.status == .running })?.id
            ?? sessions.first?.id
    }

    /// Resolve a legacy restart receipt to exactly one replacement row. A
    /// valid candidate must be new relative to the pre-effect baseline, must
    /// preserve the stable project/creation/worktree identity, and must belong
    /// to the same runtime command family. The old row must also have vanished,
    /// proving this is a replacement rather than an unrelated concurrent
    /// launch. Multiple candidates permanently cancel automatic selection.
    static func replacementSelectionResolution(
        _ intent: ReplacementSelectionIntent,
        sessions: [RemoteSessionSummary]
    ) -> ReplacementSelectionResolution {
        let sourceStillExists = sessions.contains { $0.id == intent.sourceSessionID }
        let candidates = sessions.filter { session in
            guard session.id != intent.sourceSessionID,
                  !intent.baselineSessionIDs.contains(session.id),
                  session.projectID == intent.projectID,
                  session.createdAtUnixMs == intent.createdAtUnixMs,
                  session.status == .running,
                  !session.archived,
                  session.worktreePath == intent.worktreePath,
                  session.worktreeBranch == intent.worktreeBranch
            else { return false }
            guard let expectedRuntimeID = intent.runtimeID else { return true }
            let candidateRuntimeID = session.providerID
                ?? SetupTool.detect(in: session.command)?.id
            return candidateRuntimeID == expectedRuntimeID
        }

        if candidates.count > 1 { return .cancel }
        if !sourceStillExists, let candidate = candidates.first {
            return .select(candidate.id)
        }
        guard intent.bootstrapObservationsRemaining > 1 else { return .cancel }
        var waiting = intent
        waiting.bootstrapObservationsRemaining -= 1
        return .wait(waiting)
    }

    /// Ignore the capture clock and output-preview churn so a two-second
    /// health poll does not rebuild the native sidebar while an agent types.
    static func snapshotContentEqual(
        _ lhs: RemoteBootstrapSnapshot,
        _ rhs: RemoteBootstrapSnapshot
    ) -> Bool {
        lhs.protocolVersion == rhs.protocolVersion
            && lhs.hostProtocol == rhs.hostProtocol
            && lhs.macID == rhs.macID
            && lhs.macName == rhs.macName
            && lhs.folders == rhs.folders
            && lhs.projects == rhs.projects
            && lhs.presets == rhs.presets
            && lhs.remoteServerPort == rhs.remoteServerPort
            && lhs.remoteServerCertificateFingerprint
                == rhs.remoteServerCertificateFingerprint
            && lhs.experimentalWorktreesEnabled == rhs.experimentalWorktreesEnabled
            && lhs.proEntitled == rhs.proEntitled
            && lhs.pendingApprovals == rhs.pendingApprovals
            && lhs.sessions.count == rhs.sessions.count
            && zip(lhs.sessions, rhs.sessions).allSatisfy(Self.sessionRenderEqual)
    }

    private static func sessionRenderEqual(
        _ lhs: RemoteSessionSummary,
        _ rhs: RemoteSessionSummary
    ) -> Bool {
        lhs.id == rhs.id
            && lhs.projectID == rhs.projectID
            && lhs.activeRuntimeID == rhs.activeRuntimeID
            && lhs.runtimeLaunchPending == rhs.runtimeLaunchPending
            && lhs.providerID == rhs.providerID
            && lhs.title == rhs.title
            && lhs.command == rhs.command
            && lhs.createdAtUnixMs == rhs.createdAtUnixMs
            && minuteBucket(lhs.updatedAtUnixMs) == minuteBucket(rhs.updatedAtUnixMs)
            && lhs.status == rhs.status
            && lhs.activity == rhs.activity
            && lhs.unread == rhs.unread
            && lhs.pinned == rhs.pinned
            && lhs.worktreePath == rhs.worktreePath
            && lhs.worktreeBranch == rhs.worktreeBranch
            && lhs.parentSessionID == rhs.parentSessionID
            && lhs.notifyWhenDone == rhs.notifyWhenDone
            && lhs.terminalBackgroundHex == rhs.terminalBackgroundHex
            && lhs.capabilities == rhs.capabilities
            && lhs.archived == rhs.archived
            && lhs.spinnerColorHex == rhs.spinnerColorHex
    }

    private static func minuteBucket(_ unixMs: Int64?) -> Int64? {
        unixMs.map { $0 / 60_000 }
    }
}

// MARK: - Organization / lifecycle verbs

/// One user-facing failure from a remote organization or lifecycle verb.
/// `outcomeIsUnknown` means the effect may have reached the Host even though
/// its receipt was lost — the UI must surface it and never retry silently.
struct RemoteHostVerbError: Error, LocalizedError, Equatable, Sendable {
    let operation: String
    let message: String
    let outcomeIsUnknown: Bool

    var errorDescription: String? { message }
}

extension RemoteHostRuntime {
    /// Stable Host operation ids (protocol/host-capabilities-v1.json). Menus
    /// gate on these through `supportsHostOperation`; the backend enforces
    /// them again per call.
    enum HostOperation {
        static let write = "session.input.write"
        static let titleSet = "session.title.set"
        static let pinSet = "session.pin.set"
        static let archive = "session.archive"
        static let restore = "session.restore"
        static let stop = "session.stop"
        static let remove = "session.remove"
        static let restart = "session.restart"
        static let resumeAgent = RemoteControlProtocol.sessionRuntimeResumeCapability
        static let create = "session.create"
        static let orderSet = "session.order.set"
        static let projectOrganizationSet = "project.organization.set"
        static let archiveList = "session.archive.list"
        static let transcriptMarkdown = "session.transcript.markdown"
    }

    /// Whether the bootstrapped Host advertises one stable operation id.
    /// Never guessed and never probed: absent ledger means unsupported.
    func supportsHostOperation(_ operation: String) -> Bool {
        snapshot?.hostProtocol?.supports(operation) == true
    }

    func renameSession(_ sessionID: String, to title: String) async throws {
        try await performOrganizationVerb(
            capability: HostOperation.titleSet,
            operation: "rename"
        ) { backend in
            _ = try await backend.setSessionTitle(sessionID: sessionID, title: title)
        }
    }

    func setSessionPinned(_ sessionID: String, pinned: Bool) async throws {
        try await performOrganizationVerb(
            capability: HostOperation.pinSet,
            operation: pinned ? "pin" : "unpin"
        ) { backend in
            _ = try await backend.setSessionPinned(sessionID: sessionID, pinned: pinned)
        }
    }

    func archiveSession(_ sessionID: String) async throws {
        try await performOrganizationVerb(
            capability: HostOperation.archive,
            operation: "archive"
        ) { backend in
            _ = try await backend.archiveSession(sessionID: sessionID)
        }
    }

    func restoreSession(_ sessionID: String) async throws {
        try await performOrganizationVerb(
            capability: HostOperation.restore,
            operation: "restore"
        ) { backend in
            _ = try await backend.restoreSession(sessionID: sessionID)
        }
    }

    func stopSession(_ sessionID: String) async throws {
        try await performOrganizationVerb(
            capability: HostOperation.stop,
            operation: "stop"
        ) { backend in
            _ = try await backend.stopSession(sessionID: sessionID)
        }
    }

    func removeSession(_ sessionID: String) async throws {
        try await performOrganizationVerb(
            capability: HostOperation.remove,
            operation: "remove"
        ) { backend in
            _ = try await backend.removeSession(sessionID: sessionID)
        }
    }

    func restartSession(_ sessionID: String) async throws {
        guard let source = snapshot?.sessions.first(where: { $0.id == sessionID }) else {
            throw RemoteHostVerbError(
                operation: "resume",
                message: "The Session is no longer available on this Host.",
                outcomeIsUnknown: false
            )
        }
        try beginReplacementSelection(
            source,
            knownSessionIDs: Set((snapshot?.sessions ?? []).map(\.id)),
            operation: "resume"
        )
        try await submitReplacementRestart(sessionID)
    }

    private func sendRestartSession(_ sessionID: String) async throws {
        try await performOrganizationVerb(
            capability: HostOperation.restart,
            operation: "restart"
        ) { backend in
            _ = try await backend.restartSession(sessionID: sessionID)
        }
    }

    /// The Host's legacy replacement receipt does not return the new Session
    /// id. Latch correlation before Restore so an immediate bootstrap cannot
    /// race ahead, then keep it through Restart until a unique replacement is
    /// published. This is intentionally one selection intent at a time.
    func restoreAndRestartSession(
        _ source: RemoteSessionSummary,
        knownSessionIDs: Set<String>
    ) async throws {
        try beginReplacementSelection(
            source,
            knownSessionIDs: knownSessionIDs,
            operation: "restore and resume"
        )

        do {
            try await restoreSession(source.id)
        } catch {
            // Restart was never submitted, so no replacement can appear.
            pendingReplacementSelection = nil
            replacementDefaultSelectionSuppressed = false
            throw error
        }

        try await submitReplacementRestart(source.id)
    }

    private func beginReplacementSelection(
        _ source: RemoteSessionSummary,
        knownSessionIDs: Set<String>,
        operation: String
    ) throws {
        guard pendingReplacementSelection == nil else {
            throw RemoteHostVerbError(
                operation: operation,
                message: "Another Session is still being resumed.",
                outcomeIsUnknown: false
            )
        }
        let currentIDs = Set((snapshot?.sessions ?? []).map(\.id))
        pendingCreatedSelectionID = nil
        pendingReplacementSelection = ReplacementSelectionIntent(
            source: source,
            knownSessionIDs: knownSessionIDs.union(currentIDs)
        )
        replacementDefaultSelectionSuppressed = true
    }

    private func submitReplacementRestart(_ sessionID: String) async throws {
        do {
            try await sendRestartSession(sessionID)
        } catch {
            // An outcome-unknown restart may have minted the replacement.
            // Retain the bounded intent in that case; a definitive rejection
            // cannot, and must not steal a later unrelated Session.
            if (error as? RemoteHostVerbError)?.outcomeIsUnknown != true {
                pendingReplacementSelection = nil
                replacementDefaultSelectionSuppressed = false
            }
            throw error
        }
    }

    func resumeAgent(_ sessionID: String) async throws {
        try await performOrganizationVerb(
            capability: HostOperation.resumeAgent,
            operation: "resume agent"
        ) { backend in
            _ = try await backend.resumeAgent(sessionID: sessionID)
        }
    }

    func setSessionOrder(
        projectID: String,
        orderedSessionIDs: [String]
    ) async throws {
        try await performOrganizationVerb(
            capability: HostOperation.orderSet,
            operation: "reorder"
        ) { backend in
            _ = try await backend.setSessionOrder(
                projectID: projectID,
                orderedSessionIDs: orderedSessionIDs
            )
        }
    }

    /// Move a project to `sortOrder` among its same-parent siblings in the
    /// Host's display order — the Controller half of a sidebar project drag
    /// (`project.organization.set`, one-project patch). The Host persists it
    /// through the same path a local drag commits; the refreshed bootstrap
    /// reconciles the projection.
    func setProjectSortOrder(projectID: String, sortOrder: Int) async throws {
        try await performOrganizationVerb(
            capability: HostOperation.projectOrganizationSet,
            operation: "reorder"
        ) { backend in
            _ = try await backend.setProjectOrganization(
                projectID: projectID,
                patch: RemoteProjectOrganizationPatch(
                    projectID: projectID,
                    sortOrder: sortOrder
                )
            )
        }
    }

    /// Create a Session on the Host from an explicit user action (preset chip
    /// or project menu). The created row is selected as soon as a bootstrap
    /// (or the Host's optimistic receipt) reports it.
    @discardableResult
    func createSession(
        projectID: String,
        presetID: String? = nil,
        command: String? = nil
    ) async throws -> String {
        // A newer explicit selection intent supersedes any unresolved legacy
        // replacement correlation.
        pendingReplacementSelection = nil
        replacementDefaultSelectionSuppressed = false
        var createdSessionID: String?
        try await performOrganizationVerb(
            capability: HostOperation.create,
            operation: "new session"
        ) { backend in
            let created = try await backend.createSession(RemoteCreateSessionRequest(
                projectID: projectID,
                presetID: presetID,
                command: command,
                worktreePath: nil,
                worktreeBranch: nil,
                initialText: nil,
                initialTextSubmitMode: .pasteAndSubmit
            ))
            createdSessionID = created.sessionID
        }
        guard let createdSessionID else {
            throw RemoteHostVerbError(
                operation: "new session",
                message: "The Host did not return the created Session.",
                outcomeIsUnknown: true
            )
        }
        pendingCreatedSelectionID = createdSessionID
        if snapshot?.sessions.contains(where: { $0.id == createdSessionID }) == true {
            pendingCreatedSelectionID = nil
            selectSession(createdSessionID)
        }
        return createdSessionID
    }

    /// One project's archived Sessions — a capability-gated read.
    func archivedSessions(projectID: String) async throws -> [RemoteSessionSummary] {
        let (connection, _) = try requireConnection(
            capability: HostOperation.archiveList,
            operation: "archived sessions"
        )
        do {
            return try await connection.backend.listArchivedSessions(projectID: projectID)
        } catch {
            throw Self.verbError(from: error, operation: "archived sessions", isEffect: false)
        }
    }

    /// One Session's Markdown transcript — a capability-gated read.
    func transcriptMarkdown(sessionID: String, entries: Int?) async throws -> String {
        let (connection, _) = try requireConnection(
            capability: HostOperation.transcriptMarkdown,
            operation: "copy transcript"
        )
        do {
            let transcript = try await connection.backend.transcriptMarkdown(
                sessionID: sessionID,
                entries: entries
            )
            return transcript.markdown
        } catch {
            throw Self.verbError(from: error, operation: "copy transcript", isEffect: false)
        }
    }

    // MARK: verb plumbing

    private func requireConnection(
        capability: String,
        operation: String
    ) throws -> (ActiveConnection, UInt64) {
        guard connectionBootstrapped, let connection else {
            throw RemoteHostVerbError(
                operation: operation,
                message: "This Host is not connected.",
                outcomeIsUnknown: false
            )
        }
        guard supportsHostOperation(capability) else {
            throw RemoteHostVerbError(
                operation: operation,
                message: "This Host does not support \(operation) yet. Update Unpeel on the Host.",
                outcomeIsUnknown: false
            )
        }
        return (connection, connection.epoch)
    }

    /// Run one at-most-once organization effect against the current backend
    /// generation. Success and outcome-unknown failures both trigger an
    /// immediate bootstrap refresh; nothing is ever replayed automatically.
    private func performOrganizationVerb(
        capability: String,
        operation: String,
        _ body: (any NativeRemoteBackendProtocol) async throws -> Void
    ) async throws {
        let (connection, _) = try requireConnection(
            capability: capability,
            operation: operation
        )
        // A predecessor connection to the same Host may still be unwinding
        // its effect tail; organization effects respect the same start
        // barrier as terminal effects so they cannot overtake it.
        await connection.effectStartBarrier?.value
        guard isCurrent(connection), connectionBootstrapped else {
            throw RemoteHostVerbError(
                operation: operation,
                message: "The Host connection changed before \(operation) was sent.",
                outcomeIsUnknown: false
            )
        }
        do {
            try await body(connection.backend)
            refreshAfterOrganizationVerb(connection)
        } catch {
            let verbError = Self.verbError(from: error, operation: operation, isEffect: true)
            if verbError.outcomeIsUnknown {
                // The effect may have landed. A fresh bootstrap proves the
                // resulting Host state (and a fresh accepted generation)
                // before anything else is offered.
                refreshAfterOrganizationVerb(connection)
            }
            throw verbError
        }
    }

    private func refreshAfterOrganizationVerb(_ connection: ActiveConnection) {
        guard isCurrent(connection) else { return }
        startRefreshLoop(for: connection)
    }

    private static func verbError(
        from error: Error,
        operation: String,
        isEffect: Bool
    ) -> RemoteHostVerbError {
        if let verbError = error as? RemoteHostVerbError { return verbError }
        if let bridgeError = error as? NativeRemoteBackendError {
            return RemoteHostVerbError(
                operation: operation,
                message: bridgeError.message,
                outcomeIsUnknown: isEffect && !bridgeError.effectWasNotApplied
            )
        }
        if error is CancellationError {
            return RemoteHostVerbError(
                operation: operation,
                message: "\(operation) was interrupted; refresh the Host before retrying.",
                outcomeIsUnknown: isEffect
            )
        }
        return RemoteHostVerbError(
            operation: operation,
            message: error.localizedDescription,
            outcomeIsUnknown: isEffect
        )
    }
}
