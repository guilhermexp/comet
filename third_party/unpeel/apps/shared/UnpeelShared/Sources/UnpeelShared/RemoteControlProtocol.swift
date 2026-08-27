import CryptoKit
import Foundation
import Security

public enum RemoteControlProtocol {
    public static let version = 1
    public static let hostMajorVersion = 1
    public static let hostMinorVersion = 6
    public static let resumableArtifactUploadCapability = "artifact.upload.resumable"
    public static let sessionOrderCapability = "session.order.set"
    public static let sessionRuntimeRestartCapability = "session.runtime.restart"
    public static let sessionRuntimeResumeCapability = "session.runtime.resume"
}

/// Additive Host-level capability contract carried by bootstrap.
///
/// `protocolVersion` above remains the shipped `/mobile/*` DTO version. This
/// descriptor lets every Controller ask what this particular Host implements
/// without branching on native-vs-headless or probing routes with 404s.
/// Missing on older Hosts means "legacy capabilities unknown". Major versions
/// must match; minor versions and unknown capability ids are additive.
public struct RemoteHostProtocolDescriptor: Codable, Equatable, Sendable {
    public let majorVersion: Int
    public let minorVersion: Int
    public let capabilities: [String]

    public init(
        majorVersion: Int = RemoteControlProtocol.hostMajorVersion,
        minorVersion: Int = RemoteControlProtocol.hostMinorVersion,
        capabilities: [String]
    ) {
        self.majorVersion = majorVersion
        self.minorVersion = minorVersion
        self.capabilities = capabilities
    }

    public func isCompatible(controllerMajorVersion: Int = RemoteControlProtocol.hostMajorVersion) -> Bool {
        majorVersion == controllerMajorVersion
    }

    public func supports(_ capability: String) -> Bool {
        capabilities.contains(capability)
    }
}

public enum RemoteActivityState: String, Codable, Equatable, Sendable {
    case starting
    case working
    case blocked
    case done
    case idle
    case unknown
}

public enum RemoteSessionStatus: String, Codable, Equatable, Sendable {
    case running
    case exited
}

public struct RemoteDeviceIdentity: Codable, Equatable, Identifiable, Sendable {
    public let id: String
    public let name: String
    public let platform: String
    public let appVersion: String?

    public init(id: String, name: String, platform: String, appVersion: String? = nil) {
        self.id = id
        self.name = name
        self.platform = platform
        self.appVersion = appVersion
    }
}

public struct RemotePairingPayload: Codable, Equatable, Sendable {
    public let protocolVersion: Int
    public let macID: String
    public let macName: String
    public let endpoint: URL
    public let token: String
    public let certificateFingerprint: String?
    public let expiresAtUnixMs: Int64

    public init(
        protocolVersion: Int = RemoteControlProtocol.version,
        macID: String,
        macName: String,
        endpoint: URL,
        token: String,
        certificateFingerprint: String? = nil,
        expiresAtUnixMs: Int64
    ) {
        self.protocolVersion = protocolVersion
        self.macID = macID
        self.macName = macName
        self.endpoint = endpoint
        self.token = token
        self.certificateFingerprint = certificateFingerprint
        self.expiresAtUnixMs = expiresAtUnixMs
    }
}

/// Wire form of the pairing QR / paste code.
///
/// The compact form is
/// `UNPEEL:<version>:<host>:<port>:<macID>:<token>:<expiresUnixSeconds>`
/// — kept to the QR alphanumeric charset (digits, uppercase letters, `:`, `.`)
/// so the code stays small and coarse enough to scan instantly; the JSON form
/// of `RemotePairingPayload` is ~4x the bytes and produced dense, slow-to-lock
/// QR codes. The pre-pair payload only needs endpoint + token + expiry: mac
/// identity (id, name) arrives authoritatively in the pairing *response*.
public enum RemotePairingCode {
    private static let prefix = "UNPEEL"

    /// Nil when the endpoint isn't expressible in the compact charset
    /// (no host/port, missing Mac identity, or an IPv6 host with colons) — callers fall back to
    /// the JSON form.
    public static func encode(_ payload: RemotePairingPayload) -> String? {
        guard let host = payload.endpoint.host,
              let port = payload.endpoint.port,
              !host.contains(":"),
              !payload.macID.contains(":"),
              !payload.macID.isEmpty,
              payload.macID.range(of: "^[A-Za-z0-9-]+$", options: .regularExpression) != nil,
              !payload.token.contains(":"),
              !payload.token.isEmpty
        else { return nil }
        let expiresSeconds = payload.expiresAtUnixMs / 1000
        return [
            prefix,
            "\(payload.protocolVersion)",
            host,
            "\(port)",
            payload.macID.uppercased(),
            payload.token,
            "\(expiresSeconds)",
        ].joined(separator: ":")
    }

    public static func decode(_ raw: String) -> RemotePairingPayload? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return decodeCompact(trimmed)
    }

    private static func decodeCompact(_ raw: String) -> RemotePairingPayload? {
        let parts = raw.split(separator: ":", omittingEmptySubsequences: false)
        guard parts.count == 7,
              parts[0].uppercased() == prefix,
              let version = Int(parts[1]),
              let port = Int(parts[3]), (1...65535).contains(port),
              let expiresSeconds = Int64(parts[6])
        else { return nil }
        let host = String(parts[2])
        let macID = String(parts[4]).lowercased()
        // The token is compared verbatim server-side — never case-fold it.
        let token = String(parts[5])
        guard !host.isEmpty, !macID.isEmpty, !token.isEmpty,
              let endpoint = URL(string: "http://\(host):\(port)/mobile")
        else { return nil }
        return RemotePairingPayload(
            protocolVersion: version,
            macID: macID,
            macName: "",
            endpoint: endpoint,
            token: token,
            certificateFingerprint: nil,
            expiresAtUnixMs: expiresSeconds * 1000
        )
    }
}

/// Authenticated envelope for the otherwise-plaintext LAN pairing endpoint.
/// The QR secret derives an independent AES-GCM key for each direction, while
/// associated data binds the message to the scanned Mac identity and endpoint.
public struct RemotePairingEnvelope: Codable, Equatable, Sendable {
    public let v: Int
    public let saltB64: String
    public let sealedB64: String

    public init(v: Int = 1, salt: Data, sealed: Data) {
        self.v = v
        saltB64 = salt.base64EncodedString()
        sealedB64 = sealed.base64EncodedString()
    }

    public var salt: Data? { Data(base64Encoded: saltB64) }
    public var sealed: Data? { Data(base64Encoded: sealedB64) }
}

public enum RemotePairingCryptoError: Error, Equatable {
    case invalidContext
    case invalidEnvelope
    case authenticationFailed
}

public enum RemotePairingCrypto {
    public enum Direction: String, Sendable {
        case request = "phone-to-mac"
        case response = "mac-to-phone"
    }

    public static func seal(
        _ plaintext: Data,
        token: String,
        macID: String,
        endpoint: URL,
        direction: Direction
    ) throws -> RemotePairingEnvelope {
        guard !token.isEmpty, !macID.isEmpty else { throw RemotePairingCryptoError.invalidContext }
        var salt = Data(count: 16)
        let status = salt.withUnsafeMutableBytes { bytes in
            SecRandomCopyBytes(kSecRandomDefault, 16, bytes.baseAddress!)
        }
        guard status == errSecSuccess else { throw RemotePairingCryptoError.invalidContext }
        let key = deriveKey(token: token, salt: salt, direction: direction)
        let box = try AES.GCM.seal(
            plaintext,
            using: key,
            authenticating: associatedData(macID: macID, endpoint: endpoint, direction: direction)
        )
        guard let combined = box.combined else { throw RemotePairingCryptoError.invalidEnvelope }
        return RemotePairingEnvelope(salt: salt, sealed: combined)
    }

    public static func open(
        _ envelope: RemotePairingEnvelope,
        token: String,
        macID: String,
        endpoint: URL,
        direction: Direction
    ) throws -> Data {
        guard envelope.v == 1, !token.isEmpty, !macID.isEmpty,
              let salt = envelope.salt, salt.count == 16,
              let sealed = envelope.sealed
        else { throw RemotePairingCryptoError.invalidEnvelope }
        do {
            let box = try AES.GCM.SealedBox(combined: sealed)
            return try AES.GCM.open(
                box,
                using: deriveKey(token: token, salt: salt, direction: direction),
                authenticating: associatedData(macID: macID, endpoint: endpoint, direction: direction)
            )
        } catch {
            throw RemotePairingCryptoError.authenticationFailed
        }
    }

    private static func deriveKey(token: String, salt: Data, direction: Direction) -> SymmetricKey {
        HKDF<SHA256>.deriveKey(
            inputKeyMaterial: SymmetricKey(data: Data(token.utf8)),
            salt: salt,
            info: Data("unpeel-pairing-v1:\(direction.rawValue)".utf8),
            outputByteCount: 32
        )
    }

    private static func associatedData(macID: String, endpoint: URL, direction: Direction) -> Data {
        Data("unpeel-pairing-v1\u{0}\(direction.rawValue)\u{0}\(macID)\u{0}\(endpoint.absoluteString)".utf8)
    }
}

public struct RemotePairingRequest: Codable, Equatable, Sendable {
    public let token: String
    public let device: RemoteDeviceIdentity

    public init(token: String, device: RemoteDeviceIdentity) {
        self.token = token
        self.device = device
    }
}

public struct RemotePairingResponse: Codable, Equatable, Sendable {
    public let protocolVersion: Int
    public let macID: String
    public let macName: String
    public let endpoint: URL
    public let deviceID: String
    public let authToken: String
    public let pairedAtUnixMs: Int64
    /// TCP port of the Mac's `unpeel-host __remote__` TLS server (WSS
    /// terminal streaming, same `authToken` credential). The port is
    /// OS-assigned per server run, so refresh it from `/mobile/bootstrap`
    /// before every connect. Nil when the server was not running at pairing
    /// time — it auto-starts on pairing, so the next bootstrap normally
    /// carries it.
    public let remoteServerPort: Int?
    /// Lowercase hex SHA-256 of the remote server's self-signed TLS leaf
    /// certificate (DER), for client-side pinning.
    public let remoteServerCertificateFingerprint: String?
    /// Unpeel Remote (relay) credentials: per-device E2E key + relay token,
    /// stored in the phone's Keychain immediately after pairing.
    public let relayCredentials: RelayCredentials

    public init(
        protocolVersion: Int = RemoteControlProtocol.version,
        macID: String,
        macName: String,
        endpoint: URL,
        deviceID: String,
        authToken: String,
        pairedAtUnixMs: Int64,
        remoteServerPort: Int? = nil,
        remoteServerCertificateFingerprint: String? = nil,
        relayCredentials: RelayCredentials
    ) {
        self.protocolVersion = protocolVersion
        self.macID = macID
        self.macName = macName
        self.endpoint = endpoint
        self.deviceID = deviceID
        self.authToken = authToken
        self.pairedAtUnixMs = pairedAtUnixMs
        self.remoteServerPort = remoteServerPort
        self.remoteServerCertificateFingerprint = remoteServerCertificateFingerprint
        self.relayCredentials = relayCredentials
    }
}

public struct RemotePairedDeviceSummary: Codable, Equatable, Identifiable, Sendable {
    public let id: String
    public let name: String
    public let platform: String
    public let appVersion: String?
    public let pairedAtUnixMs: Int64
    public let lastSeenAtUnixMs: Int64?
    /// Whether this device may reach the Host over the Unpeel Link relay.
    /// Nil means allowed (pre-flag records) — the flag only ever narrows.
    public let relayAllowed: Bool?

    public init(
        id: String,
        name: String,
        platform: String,
        appVersion: String? = nil,
        pairedAtUnixMs: Int64,
        lastSeenAtUnixMs: Int64? = nil,
        relayAllowed: Bool? = nil
    ) {
        self.id = id
        self.name = name
        self.platform = platform
        self.appVersion = appVersion
        self.pairedAtUnixMs = pairedAtUnixMs
        self.lastSeenAtUnixMs = lastSeenAtUnixMs
        self.relayAllowed = relayAllowed
    }
}

public struct RemoteProjectFolderSummary: Codable, Equatable, Identifiable, Sendable {
    public let id: String
    public let name: String
    public let parentFolderID: String?
    public let colorID: String?
    public let sortOrder: Int?

    public init(
        id: String,
        name: String,
        parentFolderID: String? = nil,
        colorID: String? = nil,
        sortOrder: Int? = nil
    ) {
        self.id = id
        self.name = name
        self.parentFolderID = parentFolderID
        self.colorID = colorID
        self.sortOrder = sortOrder
    }
}

public struct RemoteProjectSummary: Codable, Equatable, Identifiable, Sendable {
    public let id: String
    public let name: String
    public let path: String
    public let folderID: String?
    public let parentProjectID: String?
    public let worktreeBranch: String?
    /// Plain organizational child folder. Optional for wire compatibility:
    /// older Hosts omit it, which means a normal project/worktree.
    public let isGroup: Bool?
    /// Sidebar folder tint id (`sky`, `blue`, …), resolved by the Host.
    /// Optional so older Hosts and Controllers remain wire-compatible.
    public let colorID: String?
    /// Current git branch of the checkout (HEAD), for the session subtitle —
    /// worktree projects usually match `worktreeBranch`, main checkouts show
    /// whatever is checked out right now.
    public let gitBranch: String?
    public let mcpBlocked: Bool
    public let sortOrder: Int?
    /// How many archived sessions this project's archive library holds, for
    /// the phone's project-organize library entry. Absent on older Macs =>
    /// hide the entry (they don't serve /mobile/archive either).
    public let archivedSessionCount: Int?
    /// Whether this project's sessions are date-sorted (newest first) instead
    /// of the manual drag order. Optional for wire compatibility: older Hosts
    /// omit it, which reads as the default custom order.
    public let dateSorted: Bool?

    public init(
        id: String,
        name: String,
        path: String,
        folderID: String? = nil,
        parentProjectID: String? = nil,
        worktreeBranch: String? = nil,
        isGroup: Bool? = nil,
        colorID: String? = nil,
        gitBranch: String? = nil,
        mcpBlocked: Bool = false,
        sortOrder: Int? = nil,
        archivedSessionCount: Int? = nil,
        dateSorted: Bool? = nil
    ) {
        self.id = id
        self.name = name
        self.path = path
        self.folderID = folderID
        self.parentProjectID = parentProjectID
        self.worktreeBranch = worktreeBranch
        self.isGroup = isGroup
        self.colorID = colorID
        self.gitBranch = gitBranch
        self.mcpBlocked = mcpBlocked
        self.sortOrder = sortOrder
        self.archivedSessionCount = archivedSessionCount
        self.dateSorted = dateSorted
    }
}

/// GET /mobile/archive?project_id= — one project's archived sessions,
/// Mac-resolved like every session summary (newest first).
public struct RemoteArchivedSessionsResponse: Codable, Equatable, Sendable {
    public let projectID: String
    public let sessions: [RemoteSessionSummary]

    public init(projectID: String, sessions: [RemoteSessionSummary]) {
        self.projectID = projectID
        self.sessions = sessions
    }
}

public struct RemotePresetSummary: Codable, Equatable, Identifiable, Sendable {
    public let id: String
    public let label: String
    public let command: String
    public let cliID: String?
    public let enabled: Bool
    public let quickLaunch: Bool
    public let isDefault: Bool
    /// The CLI's brand tint (0xRRGGBB), Mac-resolved like the session
    /// spinner color. Nil = older Mac / no brand color.
    public let tintColorHex: Int?

    public init(
        id: String,
        label: String,
        command: String,
        cliID: String? = nil,
        enabled: Bool = true,
        quickLaunch: Bool = false,
        isDefault: Bool = false,
        tintColorHex: Int? = nil
    ) {
        self.id = id
        self.label = label
        self.command = command
        self.cliID = cliID
        self.enabled = enabled
        self.quickLaunch = quickLaunch
        self.isDefault = isDefault
        self.tintColorHex = tintColorHex
    }
}

/// Which verbs this session's CLI actually supports, computed on the Mac —
/// the single source of truth (`ProviderCapabilities` in the native app), so
/// the phone never parses commands itself. Gates what the phone's session
/// sheet offers. Absent on older Macs ⇒ callers fall back to the permissive
/// pre-capability behavior.
public struct RemoteSessionCapabilities: Codable, Equatable, Sendable {
    /// Legacy terminal-replacing Resume for a stopped Session. It continues a
    /// known agent conversation, or restores a blank shell with none to lose.
    public let restart: Bool
    /// Legacy protocol-minor-5 field. Decode it so newer Controllers remain
    /// wire-compatible with older Hosts, but presentation must never use it:
    /// an active managed runtime is no longer a user-facing restart target.
    public let restartAgent: Bool?
    /// The Host can safely resume the stable managed agent after it has exited
    /// back to the shell, without replacing the Session or PTY. Optional so
    /// Controllers can distinguish an older Host from a current Host where the
    /// operation is unavailable for this Session.
    public let resumeAgent: Bool?
    /// The CLI has a native fork primitive (new conversation branched off
    /// this one, source left running).
    public let fork: Bool
    /// The CLI has an append-mode system-context mechanism.
    public let appendSystemContext: Bool
    /// The CLI reports turn completion through lifecycle hooks, so a
    /// "notify when done" push is reliable.
    public let notifyWhenDone: Bool
    /// The Mac supports the archive verb (`archived` on the organization
    /// patch): non-destructive stop + move to the project's Archived section.
    /// Defaults to false when decoding payloads from older Macs, so the
    /// phone hides Archive instead of offering a silent no-op.
    public let archive: Bool

    public init(
        restart: Bool,
        restartAgent: Bool? = nil,
        resumeAgent: Bool? = nil,
        fork: Bool,
        appendSystemContext: Bool,
        notifyWhenDone: Bool,
        archive: Bool = false
    ) {
        self.restart = restart
        self.restartAgent = restartAgent
        self.resumeAgent = resumeAgent
        self.fork = fork
        self.appendSystemContext = appendSystemContext
        self.notifyWhenDone = notifyWhenDone
        self.archive = archive
    }

    enum CodingKeys: String, CodingKey {
        case restart, restartAgent, resumeAgent, fork, appendSystemContext, notifyWhenDone, archive
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        restart = try c.decode(Bool.self, forKey: .restart)
        restartAgent = try c.decodeIfPresent(Bool.self, forKey: .restartAgent)
        resumeAgent = try c.decodeIfPresent(Bool.self, forKey: .resumeAgent)
        fork = try c.decode(Bool.self, forKey: .fork)
        appendSystemContext = try c.decode(Bool.self, forKey: .appendSystemContext)
        notifyWhenDone = try c.decode(Bool.self, forKey: .notifyWhenDone)
        archive = try c.decodeIfPresent(Bool.self, forKey: .archive) ?? false
    }
}

public struct RemoteSessionSummary: Codable, Equatable, Identifiable, Sendable {
    public let id: String
    public let projectID: String
    /// Runtime currently observed in the Session's foreground job. This is
    /// independent of `providerID`, which remains the legacy launch identity.
    /// A blank shell can therefore advertise `activeRuntimeID == "claude"`
    /// while leaving `providerID` nil.
    public let activeRuntimeID: String?
    /// The Host accepted a same-terminal managed-runtime launch, but has not
    /// observed that process in the foreground yet. Controllers must not
    /// offer another Resume Agent action during this bounded transition.
    /// Absent on older Hosts ⇒ false.
    public let runtimeLaunchPending: Bool
    public let providerID: String?
    public let title: String
    public let command: String
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64?
    public let status: RemoteSessionStatus
    public let activity: RemoteActivityState
    public let unread: Bool
    public let pinned: Bool
    public let worktreePath: String?
    public let worktreeBranch: String?
    /// The session this one was spawned from (Sessions MCP child), used to draw
    /// the nested "branch" connector in the sidebar. Absent on older Macs ⇒ nil.
    public let parentSessionID: String?
    public let lastOutputPreview: String?
    /// Whether the user opted this session into a "finished" push notification
    /// (the Mac-side per-session flag). Absent on older Macs ⇒ false.
    public let notifyWhenDone: Bool
    /// The provider TUI's resolved DARK background color (0xRRGGBB), for
    /// tinting the phone's terminal chrome to match — e.g. opencode/grok read
    /// their theme from config files only the Mac can see. Nil = the default
    /// terminal background.
    public let terminalBackgroundHex: Int?
    /// Verb support for this session's CLI (see RemoteSessionCapabilities).
    /// Absent on older Macs.
    public let capabilities: RemoteSessionCapabilities?
    /// Whether this session is archived on the Mac (a recent archive still
    /// showing in the sidebar's stopped group). Absent on older Macs ⇒ false
    /// (older Macs never send archived sessions at all).
    public let archived: Bool
    /// The CLI's brand/spinner tint (0xRRGGBB), resolved on the Mac — the
    /// single source of truth (`Theme.toolSpinnerColor`), so a new CLI's
    /// color reaches phones without an app update. Nil = older Mac or no
    /// per-tool brand color (plain shells) ⇒ the phone falls back to its own
    /// legacy table / neutral tint.
    public let spinnerColorHex: Int?

    public init(
        id: String,
        projectID: String,
        activeRuntimeID: String? = nil,
        runtimeLaunchPending: Bool = false,
        providerID: String? = nil,
        title: String,
        command: String,
        createdAtUnixMs: Int64,
        updatedAtUnixMs: Int64? = nil,
        status: RemoteSessionStatus,
        activity: RemoteActivityState,
        unread: Bool = false,
        pinned: Bool = false,
        worktreePath: String? = nil,
        worktreeBranch: String? = nil,
        parentSessionID: String? = nil,
        lastOutputPreview: String? = nil,
        notifyWhenDone: Bool = false,
        terminalBackgroundHex: Int? = nil,
        capabilities: RemoteSessionCapabilities? = nil,
        archived: Bool = false,
        spinnerColorHex: Int? = nil
    ) {
        self.id = id
        self.projectID = projectID
        self.activeRuntimeID = activeRuntimeID
        self.runtimeLaunchPending = runtimeLaunchPending
        self.providerID = providerID
        self.title = title
        self.command = command
        self.createdAtUnixMs = createdAtUnixMs
        self.updatedAtUnixMs = updatedAtUnixMs
        self.status = status
        self.activity = activity
        self.unread = unread
        self.pinned = pinned
        self.worktreePath = worktreePath
        self.worktreeBranch = worktreeBranch
        self.parentSessionID = parentSessionID
        self.lastOutputPreview = lastOutputPreview
        self.notifyWhenDone = notifyWhenDone
        self.terminalBackgroundHex = terminalBackgroundHex
        self.capabilities = capabilities
        self.archived = archived
        self.spinnerColorHex = spinnerColorHex
    }

    // Custom decode so the newer fields are optional on the wire — a Mac that
    // predates them simply omits them without failing the whole snapshot
    // decode. Encoding stays synthesized via these keys.
    enum CodingKeys: String, CodingKey {
        case id, projectID, activeRuntimeID, runtimeLaunchPending, providerID, title, command
        case createdAtUnixMs, updatedAtUnixMs, status, activity
        case unread, pinned, worktreePath, worktreeBranch, parentSessionID, lastOutputPreview
        case notifyWhenDone, terminalBackgroundHex, capabilities, archived
        case spinnerColorHex
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        projectID = try c.decode(String.self, forKey: .projectID)
        activeRuntimeID = try c.decodeIfPresent(String.self, forKey: .activeRuntimeID)
        runtimeLaunchPending = try c.decodeIfPresent(
            Bool.self, forKey: .runtimeLaunchPending
        ) ?? false
        providerID = try c.decodeIfPresent(String.self, forKey: .providerID)
        title = try c.decode(String.self, forKey: .title)
        command = try c.decode(String.self, forKey: .command)
        createdAtUnixMs = try c.decode(Int64.self, forKey: .createdAtUnixMs)
        updatedAtUnixMs = try c.decodeIfPresent(Int64.self, forKey: .updatedAtUnixMs)
        status = try c.decode(RemoteSessionStatus.self, forKey: .status)
        activity = try c.decode(RemoteActivityState.self, forKey: .activity)
        unread = try c.decodeIfPresent(Bool.self, forKey: .unread) ?? false
        pinned = try c.decodeIfPresent(Bool.self, forKey: .pinned) ?? false
        worktreePath = try c.decodeIfPresent(String.self, forKey: .worktreePath)
        worktreeBranch = try c.decodeIfPresent(String.self, forKey: .worktreeBranch)
        parentSessionID = try c.decodeIfPresent(String.self, forKey: .parentSessionID)
        lastOutputPreview = try c.decodeIfPresent(String.self, forKey: .lastOutputPreview)
        notifyWhenDone = try c.decodeIfPresent(Bool.self, forKey: .notifyWhenDone) ?? false
        terminalBackgroundHex = try c.decodeIfPresent(Int.self, forKey: .terminalBackgroundHex)
        capabilities = try c.decodeIfPresent(RemoteSessionCapabilities.self, forKey: .capabilities)
        archived = try c.decodeIfPresent(Bool.self, forKey: .archived) ?? false
        spinnerColorHex = try c.decodeIfPresent(Int.self, forKey: .spinnerColorHex)
    }
}

public enum RemoteTranscriptRole: String, Codable, Equatable, Sendable {
    case user
    case assistant
    case system
    case tool
    case reasoning
    case info
    case unknown
}

public enum RemoteTranscriptBlockKind: String, Codable, Equatable, Sendable {
    case text
    case reasoning
    case toolCall
    case toolResult
    case permission
    case info
    case fileChange
    case diff
    case planUpdate
    case usage
    case attachment
}

public struct RemoteTranscriptBlock: Codable, Equatable, Identifiable, Sendable {
    public let id: String
    public let kind: RemoteTranscriptBlockKind
    public let text: String?
    public let toolName: String?
    public let status: String?
    public let metadata: [String: String]

    public init(
        id: String,
        kind: RemoteTranscriptBlockKind,
        text: String? = nil,
        toolName: String? = nil,
        status: String? = nil,
        metadata: [String: String] = [:]
    ) {
        self.id = id
        self.kind = kind
        self.text = text
        self.toolName = toolName
        self.status = status
        self.metadata = metadata
    }
}

public struct RemoteTranscriptEntry: Codable, Equatable, Identifiable, Sendable {
    public let id: String
    public let sequence: UInt64
    public let role: RemoteTranscriptRole
    public let text: String?
    public let blocks: [RemoteTranscriptBlock]
    public let createdAtUnixMs: Int64?

    public init(
        id: String,
        sequence: UInt64,
        role: RemoteTranscriptRole,
        text: String? = nil,
        blocks: [RemoteTranscriptBlock] = [],
        createdAtUnixMs: Int64? = nil
    ) {
        self.id = id
        self.sequence = sequence
        self.role = role
        self.text = text
        self.blocks = blocks
        self.createdAtUnixMs = createdAtUnixMs
    }
}

public struct RemoteTranscriptSnapshot: Codable, Equatable, Sendable {
    public let sessionID: String
    public let providerID: String?
    public let source: String?
    public let resolved: Bool
    public let startOffset: UInt64?
    public let nextOffset: UInt64?
    public let entries: [RemoteTranscriptEntry]
    public let fallbackReason: String?
    public let updatedAtUnixMs: Int64

    public init(
        sessionID: String,
        providerID: String? = nil,
        source: String? = nil,
        resolved: Bool,
        startOffset: UInt64? = nil,
        nextOffset: UInt64? = nil,
        entries: [RemoteTranscriptEntry] = [],
        fallbackReason: String? = nil,
        updatedAtUnixMs: Int64
    ) {
        self.sessionID = sessionID
        self.providerID = providerID
        self.source = source
        self.resolved = resolved
        self.startOffset = startOffset
        self.nextOffset = nextOffset
        self.entries = entries
        self.fallbackReason = fallbackReason
        self.updatedAtUnixMs = updatedAtUnixMs
    }
}

public struct RemoteTranscriptStreamChunk: Codable, Equatable, Sendable {
    public let sessionID: String
    public let providerID: String?
    public let source: String?
    public let resolved: Bool
    public let offset: UInt64
    public let nextOffset: UInt64
    public let partial: String
    public let truncated: Bool
    public let entries: [RemoteTranscriptEntry]
    public let fallbackReason: String?
    public let updatedAtUnixMs: Int64

    public init(
        sessionID: String,
        providerID: String? = nil,
        source: String? = nil,
        resolved: Bool,
        offset: UInt64,
        nextOffset: UInt64,
        partial: String = "",
        truncated: Bool = false,
        entries: [RemoteTranscriptEntry] = [],
        fallbackReason: String? = nil,
        updatedAtUnixMs: Int64
    ) {
        self.sessionID = sessionID
        self.providerID = providerID
        self.source = source
        self.resolved = resolved
        self.offset = offset
        self.nextOffset = nextOffset
        self.partial = partial
        self.truncated = truncated
        self.entries = entries
        self.fallbackReason = fallbackReason
        self.updatedAtUnixMs = updatedAtUnixMs
    }
}

public struct RemoteTranscriptHistoryPage: Codable, Equatable, Sendable {
    public let sessionID: String
    public let providerID: String?
    public let source: String?
    public let resolved: Bool
    public let startOffset: UInt64
    public let endOffset: UInt64
    public let truncated: Bool
    public let entries: [RemoteTranscriptEntry]
    public let fallbackReason: String?
    public let updatedAtUnixMs: Int64

    public init(
        sessionID: String,
        providerID: String? = nil,
        source: String? = nil,
        resolved: Bool,
        startOffset: UInt64,
        endOffset: UInt64,
        truncated: Bool = false,
        entries: [RemoteTranscriptEntry] = [],
        fallbackReason: String? = nil,
        updatedAtUnixMs: Int64
    ) {
        self.sessionID = sessionID
        self.providerID = providerID
        self.source = source
        self.resolved = resolved
        self.startOffset = startOffset
        self.endOffset = endOffset
        self.truncated = truncated
        self.entries = entries
        self.fallbackReason = fallbackReason
        self.updatedAtUnixMs = updatedAtUnixMs
    }
}

/// The whole conversation rendered as Markdown by the Mac (the same
/// `__transcript__ markdown` output "Copy transcript" uses on the desktop,
/// filtered by the Mac's Settings ▸ Transcripts options). Served by
/// `GET /mobile/transcript-markdown`; the phone copies it to the clipboard.
public struct RemoteTranscriptMarkdown: Codable, Equatable, Sendable {
    public let sessionID: String
    public let markdown: String

    public init(sessionID: String, markdown: String) {
        self.sessionID = sessionID
        self.markdown = markdown
    }
}

public struct RemoteBootstrapSnapshot: Codable, Equatable, Sendable {
    public let protocolVersion: Int
    /// Versioned Host operation set. Optional so bootstrap remains decodable
    /// from pre-ledger Hosts; clients then use their existing v1 fallbacks.
    public let hostProtocol: RemoteHostProtocolDescriptor?
    public let macID: String?
    public let macName: String?
    public let folders: [RemoteProjectFolderSummary]
    public let projects: [RemoteProjectSummary]
    public let presets: [RemotePresetSummary]
    public let sessions: [RemoteSessionSummary]
    public let capturedAtUnixMs: Int64
    /// Current TCP port of the Mac's `unpeel-host __remote__` TLS server
    /// (WSS terminal streaming; paired-device tokens are valid there). Nil
    /// while that server is not running. OS-assigned per run — always use
    /// the freshest bootstrap value when (re)connecting.
    public let remoteServerPort: Int?
    /// Lowercase hex SHA-256 of the remote server's self-signed TLS leaf
    /// certificate (DER), for client-side pinning. Stable across server
    /// restarts (the cert persists in `~/.unpeel/remote/tls/`).
    public let remoteServerCertificateFingerprint: String?
    /// Whether the Mac has the experimental **Git worktrees** feature enabled
    /// (Settings ▸ Experimental). Optional for backward compatibility: an
    /// older Mac omits it, and clients treat a missing value as *off* so
    /// worktree UI stays hidden until the Mac explicitly opts in.
    public let experimentalWorktreesEnabled: Bool?
    /// Whether this Mac has an active Unpeel Pro entitlement
    /// (`LicenseManager.isPro`). Optional for backward compatibility: older
    /// Macs omit it and clients must treat a missing value as *unknown* —
    /// never as "not Pro" — because enforcement stays server-side (pairing
    /// and relay entitlement); this flag only informs client UI.
    public let proEntitled: Bool?
    /// MCP approval prompts currently waiting for the user's answer (the
    /// desktop Allow / Don't Allow dialogs), answerable from a controller via
    /// POST /mobile/approvals/answer. Optional for backward compatibility:
    /// older Macs omit it — treat missing as "none / not supported".
    public let pendingApprovals: [RemotePendingApproval]?

    public init(
        protocolVersion: Int = RemoteControlProtocol.version,
        hostProtocol: RemoteHostProtocolDescriptor? = nil,
        macID: String? = nil,
        macName: String? = nil,
        folders: [RemoteProjectFolderSummary],
        projects: [RemoteProjectSummary],
        presets: [RemotePresetSummary],
        sessions: [RemoteSessionSummary],
        capturedAtUnixMs: Int64,
        remoteServerPort: Int? = nil,
        remoteServerCertificateFingerprint: String? = nil,
        experimentalWorktreesEnabled: Bool? = nil,
        proEntitled: Bool? = nil,
        pendingApprovals: [RemotePendingApproval]? = nil
    ) {
        self.protocolVersion = protocolVersion
        self.hostProtocol = hostProtocol
        self.macID = macID
        self.macName = macName
        self.folders = folders
        self.projects = projects
        self.presets = presets
        self.sessions = sessions
        self.capturedAtUnixMs = capturedAtUnixMs
        self.remoteServerPort = remoteServerPort
        self.remoteServerCertificateFingerprint = remoteServerCertificateFingerprint
        self.experimentalWorktreesEnabled = experimentalWorktreesEnabled
        self.proEntitled = proEntitled
        self.pendingApprovals = pendingApprovals
    }
}

/// One MCP approval prompt waiting for the user's answer. The Mac resolves
/// the display copy (`title`/`body`) so controllers render it verbatim —
/// a phone never needs to understand a new `kind` to show it correctly.
public struct RemotePendingApproval: Codable, Equatable, Sendable, Identifiable {
    /// Stable prompt id, the target of `RemoteApprovalAnswerRequest`.
    public let id: String
    /// "write" | "browser" | "computer" today; free-form for new kinds.
    public let kind: String
    public let title: String
    public let body: String
    /// The session asking for access — tap-through target on controllers.
    public let callerSessionID: String
    /// Write approvals only: the session being written into.
    public let targetSessionID: String?
    public let requestedAtUnixMs: Int64

    public init(
        id: String,
        kind: String,
        title: String,
        body: String,
        callerSessionID: String,
        targetSessionID: String? = nil,
        requestedAtUnixMs: Int64
    ) {
        self.id = id
        self.kind = kind
        self.title = title
        self.body = body
        self.callerSessionID = callerSessionID
        self.targetSessionID = targetSessionID
        self.requestedAtUnixMs = requestedAtUnixMs
    }
}

public enum RemoteTextSubmitMode: String, Codable, Equatable, Sendable {
    case pasteOnly
    case pasteAndSubmit
    case raw
}

public struct RemoteCreateSessionRequest: Codable, Equatable, Sendable {
    public let projectID: String
    public let presetID: String?
    public let command: String?
    public let worktreePath: String?
    public let worktreeBranch: String?
    public let initialText: String?
    public let initialTextSubmitMode: RemoteTextSubmitMode

    public init(
        projectID: String,
        presetID: String? = nil,
        command: String? = nil,
        worktreePath: String? = nil,
        worktreeBranch: String? = nil,
        initialText: String? = nil,
        initialTextSubmitMode: RemoteTextSubmitMode = .pasteAndSubmit
    ) {
        self.projectID = projectID
        self.presetID = presetID
        self.command = command
        self.worktreePath = worktreePath
        self.worktreeBranch = worktreeBranch
        self.initialText = initialText
        self.initialTextSubmitMode = initialTextSubmitMode
    }
}

public struct RemoteCreateSessionResponse: Codable, Equatable, Sendable {
    public let sessionID: String
    public let capturedAtUnixMs: Int64?
    /// Present on newer Macs so the phone can select/render the starting
    /// session immediately instead of waiting for the next bootstrap poll.
    public let session: RemoteSessionSummary?

    public init(
        sessionID: String,
        capturedAtUnixMs: Int64? = nil,
        session: RemoteSessionSummary? = nil
    ) {
        self.sessionID = sessionID
        self.capturedAtUnixMs = capturedAtUnixMs
        self.session = session
    }
}

public struct RemoteSessionTextInput: Codable, Equatable, Sendable {
    public let sessionID: String
    public let text: String
    public let submitMode: RemoteTextSubmitMode

    public init(
        sessionID: String,
        text: String,
        submitMode: RemoteTextSubmitMode = .pasteAndSubmit
    ) {
        self.sessionID = sessionID
        self.text = text
        self.submitMode = submitMode
    }
}

public struct RemoteTerminalWriteRequest: Codable, Equatable, Sendable {
    public let sessionID: String
    public let data: String
    /// Optional idempotency key for one logical input send. When the client
    /// retries an ambiguously-delivered WebSocket write over this HTTP path it
    /// reuses the same `wid`, so the Host applies the keystroke once instead
    /// of doubling it. Omitted by older clients; the Host treats an absent
    /// key as "no dedup" and behaves exactly as before.
    public let writeID: String?

    public init(sessionID: String, data: String, writeID: String? = nil) {
        self.sessionID = sessionID
        self.data = data
        self.writeID = writeID
    }

    enum CodingKeys: String, CodingKey {
        case sessionID
        case data
        case writeID = "wid"
    }
}

public struct RemoteTerminalResizeRequest: Codable, Equatable, Sendable {
    public let sessionID: String
    public let columns: Int
    public let rows: Int

    public init(sessionID: String, columns: Int, rows: Int) {
        self.sessionID = sessionID
        self.columns = columns
        self.rows = rows
    }
}

/// Phone-driven temporary desktop resize: letterboxes the Mac's terminal
/// pane for the session to `columns`×`rows` (with a revert banner on the
/// desktop). `clear` reverts to the desktop's natural size.
public struct RemoteDesktopResizeRequest: Codable, Equatable, Sendable {
    public let sessionID: String
    public let columns: Int?
    public let rows: Int?
    public let clear: Bool?

    public init(sessionID: String, columns: Int? = nil, rows: Int? = nil, clear: Bool? = nil) {
        self.sessionID = sessionID
        self.columns = columns
        self.rows = rows
        self.clear = clear
    }
}

public enum RemoteKeyName: String, Codable, Equatable, CaseIterable, Sendable {
    case enter
    case escape
    case tab
    case arrowUp
    case arrowDown
    case arrowLeft
    case arrowRight
    case controlC
    case controlD
    case controlZ
}

public struct RemoteSessionKeyInput: Codable, Equatable, Sendable {
    public let sessionID: String
    public let keys: [RemoteKeyName]

    public init(sessionID: String, keys: [RemoteKeyName]) {
        self.sessionID = sessionID
        self.keys = keys
    }
}

public struct RemoteTerminalColor: Codable, Equatable, Sendable {
    public enum Kind: String, Codable, Equatable, Sendable {
        case defaultForeground
        case defaultBackground
        case ansi
        case rgb
    }

    public let kind: Kind
    public let index: UInt8?
    public let red: UInt8?
    public let green: UInt8?
    public let blue: UInt8?

    public init(kind: Kind, index: UInt8? = nil, red: UInt8? = nil, green: UInt8? = nil, blue: UInt8? = nil) {
        self.kind = kind
        self.index = index
        self.red = red
        self.green = green
        self.blue = blue
    }

    public static let defaultForeground = RemoteTerminalColor(kind: .defaultForeground)
    public static let defaultBackground = RemoteTerminalColor(kind: .defaultBackground)

    public static func ansi(_ index: UInt8) -> RemoteTerminalColor {
        RemoteTerminalColor(kind: .ansi, index: index)
    }

    public static func rgb(red: UInt8, green: UInt8, blue: UInt8) -> RemoteTerminalColor {
        RemoteTerminalColor(kind: .rgb, red: red, green: green, blue: blue)
    }
}

public struct RemoteTerminalStyle: Codable, Equatable, Sendable {
    public let bold: Bool
    public let italic: Bool
    public let underline: Bool
    public let inverse: Bool
    public let dim: Bool
    public let strikethrough: Bool

    public init(
        bold: Bool = false,
        italic: Bool = false,
        underline: Bool = false,
        inverse: Bool = false,
        dim: Bool = false,
        strikethrough: Bool = false
    ) {
        self.bold = bold
        self.italic = italic
        self.underline = underline
        self.inverse = inverse
        self.dim = dim
        self.strikethrough = strikethrough
    }
}

public struct RemoteTerminalCell: Codable, Equatable, Sendable {
    public let text: String
    public let foreground: RemoteTerminalColor?
    public let background: RemoteTerminalColor?
    public let style: RemoteTerminalStyle

    public init(
        text: String,
        foreground: RemoteTerminalColor? = nil,
        background: RemoteTerminalColor? = nil,
        style: RemoteTerminalStyle = .init()
    ) {
        self.text = text
        self.foreground = foreground
        self.background = background
        self.style = style
    }
}

public struct RemoteTerminalCursor: Codable, Equatable, Sendable {
    public enum Shape: String, Codable, Equatable, Sendable {
        case block
        case beam
        case underline
        case hidden
    }

    public let row: Int
    public let column: Int
    public let shape: Shape
    public let visible: Bool

    public init(row: Int, column: Int, shape: Shape = .block, visible: Bool = true) {
        self.row = row
        self.column = column
        self.shape = shape
        self.visible = visible
    }
}

public struct RemoteViewportFrame: Codable, Equatable, Sendable {
    public let sessionID: String
    public let sequence: UInt64
    public let rows: Int
    public let columns: Int
    public let cells: [RemoteTerminalCell]
    public let cursor: RemoteTerminalCursor?
    public let alternateScreen: Bool
    public let capturedAtUnixMs: Int64

    public init(
        sessionID: String,
        sequence: UInt64,
        rows: Int,
        columns: Int,
        cells: [RemoteTerminalCell],
        cursor: RemoteTerminalCursor? = nil,
        alternateScreen: Bool = false,
        capturedAtUnixMs: Int64
    ) {
        self.sessionID = sessionID
        self.sequence = sequence
        self.rows = rows
        self.columns = columns
        self.cells = cells
        self.cursor = cursor
        self.alternateScreen = alternateScreen
        self.capturedAtUnixMs = capturedAtUnixMs
    }
}

public struct RemoteViewportSubscription: Codable, Equatable, Sendable {
    public let sessionID: String
    public let rows: Int
    public let columns: Int
    public let preferredFramesPerSecond: Int
    public let includeScrollbackTailLines: Int

    public init(
        sessionID: String,
        rows: Int,
        columns: Int,
        preferredFramesPerSecond: Int = 12,
        includeScrollbackTailLines: Int = 0
    ) {
        self.sessionID = sessionID
        self.rows = rows
        self.columns = columns
        self.preferredFramesPerSecond = preferredFramesPerSecond
        self.includeScrollbackTailLines = includeScrollbackTailLines
    }
}

public struct RemoteTerminalOutputChunk: Codable, Equatable, Sendable {
    public let sessionID: String
    public let offset: UInt64
    public let nextOffset: UInt64
    public let dataBase64: String
    public let truncated: Bool
    public let capturedAtUnixMs: Int64

    public init(
        sessionID: String,
        offset: UInt64,
        nextOffset: UInt64,
        dataBase64: String,
        truncated: Bool = false,
        capturedAtUnixMs: Int64
    ) {
        self.sessionID = sessionID
        self.offset = offset
        self.nextOffset = nextOffset
        self.dataBase64 = dataBase64
        self.truncated = truncated
        self.capturedAtUnixMs = capturedAtUnixMs
    }
}

public struct RemoteTerminalMetrics: Codable, Equatable, Sendable {
    public let sessionID: String
    public let columns: Int
    public let rows: Int
    public let capturedAtUnixMs: Int64
    /// Whether the desktop app is actively viewing this session (selected in
    /// a frontmost window). Drives the phone's fit policy: when the desktop
    /// isn't looking, the phone keeps the letterbox asserted automatically
    /// instead of dropping to follower mode. `nil` on older Macs — the phone
    /// then falls back to the manual fit button.
    public let desktopViewing: Bool?

    public init(
        sessionID: String,
        columns: Int,
        rows: Int,
        capturedAtUnixMs: Int64,
        desktopViewing: Bool? = nil
    ) {
        self.sessionID = sessionID
        self.columns = columns
        self.rows = rows
        self.capturedAtUnixMs = capturedAtUnixMs
        self.desktopViewing = desktopViewing
    }
}

// MARK: - Browser MCP artifacts (per-session screenshot/download gallery)

/// One file the browser MCP produced for a session — a screenshot or a
/// download. Metadata only; bytes are fetched separately via
/// `RemoteBrowserArtifactChunk` so a large image never has to ride a single
/// relay frame.
public struct RemoteBrowserArtifact: Codable, Equatable, Hashable, Sendable {
    /// `"screenshots"` or `"downloads"` — also the on-disk subdirectory.
    public let kind: String
    public let name: String
    public let size: UInt64
    public let modifiedAtUnixMs: Int64

    public init(kind: String, name: String, size: UInt64, modifiedAtUnixMs: Int64) {
        self.kind = kind
        self.name = name
        self.size = size
        self.modifiedAtUnixMs = modifiedAtUnixMs
    }
}

/// The gallery listing for one session, newest-first.
public struct RemoteBrowserArtifactList: Codable, Equatable, Sendable {
    public let sessionID: String
    public let artifacts: [RemoteBrowserArtifact]
    public let capturedAtUnixMs: Int64

    public init(sessionID: String, artifacts: [RemoteBrowserArtifact], capturedAtUnixMs: Int64) {
        self.sessionID = sessionID
        self.artifacts = artifacts
        self.capturedAtUnixMs = capturedAtUnixMs
    }
}

/// One offset-addressed slice of an artifact's bytes. The client loops on
/// `offset`/`nextOffset` until `nextOffset == totalSize`, then reassembles —
/// the same range pattern `RemoteTerminalOutputChunk` uses, so an image
/// larger than the relay's 512KB frame cap streams over multiple frames with
/// no special path for LAN vs relay.
public struct RemoteBrowserArtifactChunk: Codable, Equatable, Sendable {
    public let sessionID: String
    public let kind: String
    public let name: String
    public let contentType: String
    public let offset: UInt64
    public let nextOffset: UInt64
    public let totalSize: UInt64
    public let dataBase64: String
    public let capturedAtUnixMs: Int64

    public init(
        sessionID: String,
        kind: String,
        name: String,
        contentType: String,
        offset: UInt64,
        nextOffset: UInt64,
        totalSize: UInt64,
        dataBase64: String,
        capturedAtUnixMs: Int64
    ) {
        self.sessionID = sessionID
        self.kind = kind
        self.name = name
        self.contentType = contentType
        self.offset = offset
        self.nextOffset = nextOffset
        self.totalSize = totalSize
        self.dataBase64 = dataBase64
        self.capturedAtUnixMs = capturedAtUnixMs
    }
}

/// Progress acknowledgement for one idempotent image-upload chunk.
///
/// Controllers keep the same `uploadID`, offset, and bytes when retrying an
/// uncertain transport failure. The Host returns the next accepted offset;
/// `path` is present only after the whole image has passed its size, digest,
/// and file-signature checks and has been atomically published.
public struct RemoteArtifactUploadProgress: Codable, Equatable, Sendable {
    public let uploadID: String
    public let nextOffset: UInt64
    public let complete: Bool
    public let path: String?

    public init(
        uploadID: String,
        nextOffset: UInt64,
        complete: Bool,
        path: String? = nil
    ) {
        self.uploadID = uploadID
        self.nextOffset = nextOffset
        self.complete = complete
        self.path = path
    }
}

public struct RemoteTerminalCellRun: Codable, Equatable, Sendable {
    public let row: Int
    public let column: Int
    public let cells: [RemoteTerminalCell]

    public init(row: Int, column: Int, cells: [RemoteTerminalCell]) {
        self.row = row
        self.column = column
        self.cells = cells
    }
}

public struct RemoteViewportPatch: Codable, Equatable, Sendable {
    public let sessionID: String
    public let baseSequence: UInt64
    public let sequence: UInt64
    public let rows: Int
    public let columns: Int
    public let changedRuns: [RemoteTerminalCellRun]
    public let cursor: RemoteTerminalCursor?
    public let alternateScreen: Bool
    public let capturedAtUnixMs: Int64

    public init(
        sessionID: String,
        baseSequence: UInt64,
        sequence: UInt64,
        rows: Int,
        columns: Int,
        changedRuns: [RemoteTerminalCellRun],
        cursor: RemoteTerminalCursor? = nil,
        alternateScreen: Bool = false,
        capturedAtUnixMs: Int64
    ) {
        self.sessionID = sessionID
        self.baseSequence = baseSequence
        self.sequence = sequence
        self.rows = rows
        self.columns = columns
        self.changedRuns = changedRuns
        self.cursor = cursor
        self.alternateScreen = alternateScreen
        self.capturedAtUnixMs = capturedAtUnixMs
    }
}

public enum RemoteStreamEventKind: String, Codable, Equatable, Sendable {
    case bootstrapSnapshot
    case sessionsChanged
    case projectsChanged
    case transcriptSnapshot
    case transcriptChunk
    case viewportFrame
    case viewportPatch
    case inputAccepted
    case inputRejected
    case deviceRevoked
    case heartbeat
    case error
}

public struct RemoteStreamEvent: Codable, Equatable, Sendable {
    public let protocolVersion: Int
    public let id: String
    public let requestID: String?
    public let kind: RemoteStreamEventKind
    public let sessionID: String?
    public let payload: Data?
    public let createdAtUnixMs: Int64

    public init(
        protocolVersion: Int = RemoteControlProtocol.version,
        id: String,
        requestID: String? = nil,
        kind: RemoteStreamEventKind,
        sessionID: String? = nil,
        payload: Data? = nil,
        createdAtUnixMs: Int64
    ) {
        self.protocolVersion = protocolVersion
        self.id = id
        self.requestID = requestID
        self.kind = kind
        self.sessionID = sessionID
        self.payload = payload
        self.createdAtUnixMs = createdAtUnixMs
    }
}

/// Controller → Host: organize one sidebar project/group (capability
/// `project.organization.set`). Nil fields are left unchanged. `displayName`
/// renames (groups only today). `colorID` sets the folder tint (`sky`,
/// `blue`, …); the empty string clears back to the default. `dateSorted`
/// switches the group's session sort between the manual order (false) and
/// date-newest-first (true). `sortOrder` moves the project to that index
/// among its same-parent siblings in the Host's CURRENT display order (the
/// order the last bootstrap advertised) and persists through the Host's own
/// reorder path — the same one a local drag commits. `folderID` (moving a
/// project between legacy folders) is not implemented by any Host today and
/// is rejected rather than silently ignored.
public struct RemoteProjectOrganizationPatch: Codable, Equatable, Sendable {
    public let projectID: String
    public let folderID: String?
    public let sortOrder: Int?
    public let displayName: String?
    public let colorID: String?
    public let dateSorted: Bool?

    public init(
        projectID: String,
        folderID: String? = nil,
        sortOrder: Int? = nil,
        displayName: String? = nil,
        colorID: String? = nil,
        dateSorted: Bool? = nil
    ) {
        self.projectID = projectID
        self.folderID = folderID
        self.sortOrder = sortOrder
        self.displayName = displayName
        self.colorID = colorID
        self.dateSorted = dateSorted
    }
}

public struct RemoteSessionOrganizationPatch: Codable, Equatable, Sendable {
    public let sessionID: String
    public let title: String?
    public let pinned: Bool?
    public let archived: Bool?
    /// Opt this session in/out of a "finished" push notification. Nil = leave
    /// unchanged (only title/pin/etc. being patched).
    public let notifyWhenDone: Bool?

    public init(
        sessionID: String,
        title: String? = nil,
        pinned: Bool? = nil,
        archived: Bool? = nil,
        notifyWhenDone: Bool? = nil
    ) {
        self.sessionID = sessionID
        self.title = title
        self.pinned = pinned
        self.archived = archived
        self.notifyWhenDone = notifyWhenDone
    }

    enum CodingKeys: String, CodingKey {
        case sessionID, title, pinned, archived, notifyWhenDone
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        sessionID = try c.decode(String.self, forKey: .sessionID)
        title = try c.decodeIfPresent(String.self, forKey: .title)
        pinned = try c.decodeIfPresent(Bool.self, forKey: .pinned)
        archived = try c.decodeIfPresent(Bool.self, forKey: .archived)
        notifyWhenDone = try c.decodeIfPresent(Bool.self, forKey: .notifyWhenDone)
    }
}

/// Controller → Host: replace one project's hand-ordered sidebar session
/// ranks (capability `session.order.set`). The list is the combined
/// pinned + regular order exactly as a desktop drag commits it to the shared
/// session-order.json; sessions absent from it keep newest-first on top.
public struct RemoteSessionOrderRequest: Codable, Equatable, Sendable {
    public let projectID: String
    public let orderedSessionIDs: [String]

    public init(projectID: String, orderedSessionIDs: [String]) {
        self.projectID = projectID
        self.orderedSessionIDs = orderedSessionIDs
    }
}

/// Phone → Mac: register (or refresh) this device's APNs token so the Mac can
/// push "needs input" / "finished" notifications while the app is closed. The
/// device identity comes from the authenticated request, not the body.
public struct RemotePushTokenRegistration: Codable, Equatable, Sendable {
    /// Hex-encoded APNs device token from `didRegisterForRemoteNotifications`.
    public let apnsToken: String
    /// `"sandbox"` (debug builds / development APNs) or `"production"`. Selects
    /// which APNs host the Mac/relay targets.
    public let environment: String

    public init(apnsToken: String, environment: String) {
        self.apnsToken = apnsToken
        self.environment = environment
    }
}

/// Restart a session on the Mac — re-runs its original command in its
/// cwd/worktree with a resume flag, preserving title, pin, and grants (the
/// same path as the desktop context menu's Restart). Used to revive an
/// exited session from the phone.
public struct RemoteRestartSessionRequest: Codable, Equatable, Sendable {
    public let sessionID: String

    public init(sessionID: String) {
        self.sessionID = sessionID
    }
}

public enum RemoteSessionAction: String, Codable, Equatable, Sendable {
    /// Kill the hosted PTY but keep the session row/history restartable.
    case stop
    /// Re-run the original command with the desktop resume behavior.
    case restart
    /// Legacy protocol-minor-5 action. Kept only so newer Hosts can decode
    /// requests from older Controllers.
    case restartAgent = "restart_agent"
    /// Resume an ended managed agent inside its still-live terminal.
    case resumeAgent = "resume_agent"
    /// Remove the session row and delete its on-disk artifacts.
    case remove
}

public struct RemoteSessionActionRequest: Codable, Equatable, Sendable {
    public let sessionID: String
    public let action: RemoteSessionAction

    public init(sessionID: String, action: RemoteSessionAction) {
        self.sessionID = sessionID
        self.action = action
    }
}

/// Answer a pending MCP approval prompt from a controller
/// (POST /mobile/approvals/answer). The Mac answers 409 when the id is no
/// longer pending — already answered on the desktop or another device;
/// controllers dismiss silently instead of surfacing an error.
public struct RemoteApprovalAnswerRequest: Codable, Equatable, Sendable {
    public let id: String
    public let approved: Bool

    public init(id: String, approved: Bool) {
        self.id = id
        self.approved = approved
    }
}

/// Tell the Mac the phone opened/observed a session, so it clears the unread
/// "blue dot" badge — the remote counterpart of the desktop clearing unread
/// when a session becomes the observed (frontmost + selected) one.
public struct RemoteMarkReadRequest: Codable, Equatable, Sendable {
    public let sessionID: String

    public init(sessionID: String) {
        self.sessionID = sessionID
    }
}

/// Typed Controller request for a visual review artifact. The Host translates
/// this semantic action into the shared provider-neutral terminal prompt.
public struct RemoteScreenshotRequest: Codable, Equatable, Sendable {
    public let sessionID: String

    public init(sessionID: String) {
        self.sessionID = sessionID
    }
}

public struct RemoteScreenshotRequestResponse: Codable, Equatable, Sendable {
    public let accepted: Bool
    public let requestedAtUnixMs: Int64

    public init(accepted: Bool = true, requestedAtUnixMs: Int64) {
        self.accepted = accepted
        self.requestedAtUnixMs = requestedAtUnixMs
    }
}
