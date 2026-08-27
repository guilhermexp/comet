import CryptoKit
import Foundation
import UnpeelShared

/// A non-2xx reply from the Mac. Carries the HTTP status and the server's
/// `{"error": ...}` body (every remote-server route sends one) instead of
/// collapsing everything into `URLError(.badServerResponse)`.
public struct RemoteMacClientError: Error, LocalizedError, CustomStringConvertible, Sendable {
    public let statusCode: Int
    public let serverMessage: String?

    public var description: String {
        if let serverMessage, !serverMessage.isEmpty {
            return "HTTP \(statusCode): \(serverMessage)"
        }
        return "HTTP \(statusCode)"
    }

    public var errorDescription: String? { description }
}

public struct RemoteMacClient: Sendable {
    public let baseURL: URL
    public let authToken: String?
    /// Non-nil when this client tunnels through Unpeel Remote (the relay)
    /// instead of direct LAN HTTP. Same routes, same auth — different pipe.
    let relay: RemoteRelayConnection?
    private let decoder: JSONDecoder
    private let encoder: JSONEncoder

    public var isRelay: Bool { relay != nil }

    public init(baseURL: URL = URL(string: "http://127.0.0.1:17661")!, authToken: String? = nil) {
        self.init(baseURL: baseURL, authToken: authToken, relay: nil)
    }

    init(baseURL: URL, authToken: String?, relay: RemoteRelayConnection?) {
        self.baseURL = baseURL
        self.authToken = authToken
        self.relay = relay
        decoder = JSONDecoder()
        encoder = JSONEncoder()
    }

    public init(pairingResponse: RemotePairingResponse) {
        self.init(baseURL: pairingResponse.endpoint, authToken: pairingResponse.authToken)
    }

    public func pair(
        payload: RemotePairingPayload,
        device: RemoteDeviceIdentity
    ) async throws -> RemotePairingResponse {
        do {
            return try await RemotePairingClient().pair(payload: payload, device: device)
        } catch let error as RemotePairingClientError {
            switch error {
            case .expired:
                throw error
            case .invalidHostIdentity:
                throw URLError(.userAuthenticationRequired)
            case .incompatibleProtocol:
                throw URLError(.unsupportedURL)
            case .invalidHTTPResponse:
                throw URLError(.badServerResponse)
            case let .httpStatus(statusCode, serverMessage):
                throw RemoteMacClientError(
                    statusCode: statusCode,
                    serverMessage: serverMessage
                )
            case .responseHostIdentityMismatch,
                 .responseEndpointMismatch,
                 .responseDeviceIdentityMismatch,
                 .invalidCredentials:
                throw URLError(.secureConnectionFailed)
            }
        }
    }

    public func bootstrap() async throws -> RemoteBootstrapSnapshot {
        // The bootstrap poll is the connection health signal: a Mac that
        // vanished mid-request should flip the UI to "Connection lost" in
        // seconds, not after the general 10s timeout.
        let data = try await perform(path: "bootstrap", timeout: 4)
        return try decoder.decode(RemoteBootstrapSnapshot.self, from: data)
    }

    public func outputChunk(
        sessionID: String,
        offset: UInt64? = nil,
        limit: Int = 512 * 1024,
        waitMs: Int = 0
    ) async throws -> RemoteTerminalOutputChunk {
        // Over the relay the tail is base64'd inside the JSON body and then
        // base64'd + AES-GCM sealed again by the tunnel (~1.8x total), so a
        // 512KB tail would exceed RelayProtocol.maxFrameBytes (512KB) and the
        // relay closes the socket with 1009 "frame too large" — killing the
        // whole connection. Cap the raw tail so the framed response always
        // fits; the host caps its read at `limit`, and offset polling catches
        // up over multiple chunks with no data loss.
        let effectiveLimit = isRelay ? min(limit, 200 * 1024) : limit
        var query = [
            "session_id": sessionID,
            "limit": "\(effectiveLimit)",
        ]
        if let offset { query["offset"] = "\(offset)" }
        if waitMs > 0 { query["wait_ms"] = "\(waitMs)" }
        // The long poll holds server-side for waitMs; the global fail-fast
        // timeout would cut it off. Wait time + 5s headroom.
        let timeout: TimeInterval = waitMs > 0 ? TimeInterval(waitMs) / 1_000 + 5 : 10
        let data = try await perform(path: "output", query: query, timeout: timeout)
        return try decoder.decode(RemoteTerminalOutputChunk.self, from: data)
    }

    public func terminalMetrics(sessionID: String) async throws -> RemoteTerminalMetrics {
        let data = try await perform(path: "metrics", query: ["session_id": sessionID])
        return try decoder.decode(RemoteTerminalMetrics.self, from: data)
    }

    /// The session's conversation rendered as Markdown by the Mac, for the
    /// session sheet's "Copy transcript". `entries` overrides the Mac's
    /// Settings ▸ Transcripts range (count, 0 = whole conversation). Generous
    /// timeout: the Mac shells out to `unpeel-host __transcript__ markdown`
    /// per request.
    public func transcriptMarkdown(
        sessionID: String,
        entries: Int? = nil
    ) async throws -> RemoteTranscriptMarkdown {
        var query = ["session_id": sessionID]
        if let entries {
            query["entries"] = String(entries)
        }
        let data = try await perform(
            path: "transcript-markdown",
            query: query,
            timeout: 20
        )
        return try decoder.decode(RemoteTranscriptMarkdown.self, from: data)
    }

    public func write(sessionID: String, data: String, writeID: String? = nil) async throws {
        _ = try await perform(
            path: "write",
            method: "POST",
            body: try encoder.encode(
                RemoteTerminalWriteRequest(sessionID: sessionID, data: data, writeID: writeID)
            )
        )
    }

    public func resizeTerminal(sessionID: String, columns: Int, rows: Int) async throws {
        _ = try await perform(
            path: "resize",
            method: "POST",
            body: try encoder.encode(
                RemoteTerminalResizeRequest(sessionID: sessionID, columns: columns, rows: rows)
            )
        )
    }

    /// Temporarily letterbox the desktop terminal for this session to the
    /// phone's grid (the Mac shows a "Resized for phone" banner with revert).
    public func resizeDesktopTerminal(sessionID: String, columns: Int, rows: Int) async throws {
        _ = try await perform(
            path: "resize-desktop",
            method: "POST",
            body: try encoder.encode(
                RemoteDesktopResizeRequest(sessionID: sessionID, columns: columns, rows: rows)
            )
        )
    }

    /// Revert a phone-driven desktop resize to the Mac's natural size.
    public func revertDesktopTerminal(sessionID: String) async throws {
        _ = try await perform(
            path: "resize-desktop",
            method: "POST",
            body: try encoder.encode(RemoteDesktopResizeRequest(sessionID: sessionID, clear: true))
        )
    }

    /// Upload image bytes to the Mac. Lands in the session's
    /// `artifacts/uploads/` dir (so it appears in that session's gallery) and
    /// returns the Mac-side path to paste into the agent's composer.
    public func uploadImage(
        sessionID: String,
        data: Data,
        contentType: String = "image/jpeg",
        resumable: Bool = false
    ) async throws -> String {
        if resumable {
            let uploadID = UUID().uuidString.lowercased()
            return try await ResumableArtifactUploader.upload(
                sessionID: sessionID,
                data: data,
                contentType: contentType,
                uploadID: uploadID
            ) { chunk in
                let responseData = try await perform(
                    path: "upload-chunk",
                    method: "POST",
                    query: [
                        "session_id": chunk.sessionID,
                        "upload_id": chunk.uploadID,
                        "offset": String(chunk.offset),
                        "total_size": String(chunk.totalSize),
                        "sha256": chunk.sha256,
                    ],
                    body: chunk.body,
                    contentType: chunk.contentType,
                    timeout: 30
                )
                return try decoder.decode(RemoteArtifactUploadProgress.self, from: responseData)
            }
        }

        // Compatibility path for Hosts without the advertised resumable
        // capability. Keep the shipped one-shot route and response untouched.
        let responseData = try await perform(
            path: "upload",
            method: "POST",
            query: ["session_id": sessionID],
            body: data,
            contentType: contentType,
            timeout: 30
        )
        guard let object = (try? JSONSerialization.jsonObject(with: responseData))
            as? [String: Any],
            let path = object["path"] as? String, !path.isEmpty
        else {
            throw URLError(.cannotParseResponse)
        }
        return path
    }

    /// One project's archived sessions (the Mac's archive library), for the
    /// phone's Archive list. 404 = the Mac predates the archive route.
    public func archivedSessions(projectID: String) async throws -> RemoteArchivedSessionsResponse {
        let data = try await perform(path: "archive", query: ["project_id": projectID])
        return try JSONDecoder().decode(RemoteArchivedSessionsResponse.self, from: data)
    }

    /// Rename and/or (un)pin a session on the Mac. The change lands in the
    /// desktop's own overlays, so the next bootstrap poll reflects it.
    public func updateSessionOrganization(_ patch: RemoteSessionOrganizationPatch) async throws {
        _ = try await perform(
            path: "session-organization",
            method: "POST",
            body: try encoder.encode(patch)
        )
    }

    /// Organize a sidebar project/group on the Host (capability
    /// `project.organization.set`): rename (groups only), folder color
    /// ("" clears), and the custom/date session sort.
    public func updateProjectOrganization(_ patch: RemoteProjectOrganizationPatch) async throws {
        _ = try await perform(
            path: "project-organization",
            method: "POST",
            body: try encoder.encode(patch)
        )
    }

    /// Persist one project's hand-ordered sidebar session ranks on the Host
    /// (capability `session.order.set`) — the same shared order a desktop
    /// drag commits, so the new arrangement shows up on every frontend.
    public func updateSessionOrder(_ request: RemoteSessionOrderRequest) async throws {
        _ = try await perform(
            path: "session-order",
            method: "POST",
            body: try encoder.encode(request)
        )
    }

    /// Restart an exited (or live) session on the Mac. Reuses the desktop's
    /// own restart path, so the conversation resumes and title/pin/grants
    /// carry over. Restart mints a new session id; the caller re-selects it
    /// after the next bootstrap poll.
    public func restartSession(sessionID: String) async throws {
        _ = try await perform(
            path: "restart-session",
            method: "POST",
            body: try encoder.encode(RemoteRestartSessionRequest(sessionID: sessionID))
        )
    }

    /// Stop/restart/remove a session on the Mac. Used by the iOS session sheet
    /// for desktop-parity actions.
    public func performSessionAction(_ request: RemoteSessionActionRequest) async throws {
        _ = try await perform(
            path: "session-action",
            method: "POST",
            body: try encoder.encode(request)
        )
    }

    /// Register this device's APNs token with the Mac so it can push
    /// "needs input" / "finished" notifications while the app is closed.
    public func registerPushToken(apnsToken: String, environment: String) async throws {
        _ = try await perform(
            path: "push-token",
            method: "POST",
            body: try encoder.encode(
                RemotePushTokenRegistration(apnsToken: apnsToken, environment: environment)
            )
        )
    }

    // MARK: - Relay output push

    /// Subscribe to host-pushed output frames for one session (relay only).
    /// Registers the frame stream BEFORE subscribing so the first push can't
    /// beat registration. Returns nil off-relay, on an older Mac (its tunnel
    /// pipeline 404s the `/relay/` path — the fallback-to-long-poll signal),
    /// or on any transport failure. `offset` nil = host tail-replays first.
    public func relayOutputSubscribe(
        sessionID: String,
        offset: UInt64?,
        credit: Int
    ) async -> AsyncStream<RelayStreamPush>? {
        guard let relay else { return nil }
        let frames = await relay.outputPushFrames()
        var query = ["session_id": sessionID, "credit": "\(credit)"]
        if let offset { query["offset"] = "\(offset)" }
        do {
            let response = try await relay.perform(
                method: "GET",
                path: RelayStreamPaths.subscribe,
                query: query,
                auth: authToken.map { "Bearer \($0)" },
                contentType: nil,
                body: nil,
                timeout: 10
            )
            guard response.status == 200 else { return nil }
            return frames
        } catch {
            return nil
        }
    }

    /// Grant the host more send budget (payload bytes) for the push stream.
    /// Fire-and-forget: the ack is uninteresting and must never block feeding.
    public func relayOutputCredit(sessionID: String, bytes: Int) {
        guard let relay, let authToken else { return }
        Task {
            _ = try? await relay.perform(
                method: "POST",
                path: RelayStreamPaths.credit,
                query: ["session_id": sessionID, "bytes": "\(bytes)"],
                auth: "Bearer \(authToken)",
                contentType: nil,
                body: nil,
                timeout: 10
            )
        }
    }

    /// End the push stream (session switch / view close). Fire-and-forget;
    /// the host also ends it when the relay connection drops.
    public func relayOutputStop(sessionID: String) {
        guard let relay, let authToken else { return }
        Task {
            _ = try? await relay.perform(
                method: "POST",
                path: RelayStreamPaths.unsubscribe,
                query: ["session_id": sessionID],
                auth: "Bearer \(authToken)",
                contentType: nil,
                body: nil,
                timeout: 10
            )
        }
    }

    /// Tell the Mac this session was opened on the phone, clearing its unread
    /// "blue dot". Transport-independent (the WS stream never hits
    /// `/mobile/output`, so the Mac can't infer it from presence alone).
    public func markRead(sessionID: String) async throws {
        _ = try await perform(
            path: "mark-read",
            method: "POST",
            body: try encoder.encode(RemoteMarkReadRequest(sessionID: sessionID))
        )
    }

    /// Answer a pending MCP approval prompt on the Mac (Allow / Don't Allow).
    /// The Mac answers 409 when the prompt is no longer pending — answered on
    /// the desktop or another device first.
    public func answerApproval(id: String, approved: Bool) async throws {
        _ = try await perform(
            path: "approvals/answer",
            method: "POST",
            body: try encoder.encode(RemoteApprovalAnswerRequest(id: id, approved: approved))
        )
    }

    /// Delete one gallery artifact (screenshot / download / upload) on the Mac.
    public func deleteArtifact(sessionID: String, kind: String, name: String) async throws {
        _ = try await perform(
            path: "artifact-delete",
            method: "POST",
            query: ["session_id": sessionID, "kind": kind, "name": name]
        )
    }

    /// List the session gallery (browser screenshots/downloads + user uploads).
    public func browserArtifacts(sessionID: String) async throws -> RemoteBrowserArtifactList {
        let data = try await perform(path: "artifacts", query: ["session_id": sessionID])
        return try decoder.decode(RemoteBrowserArtifactList.self, from: data)
    }

    /// Ask the session's active agent to capture its current visual result as
    /// an Unpeel Browser screenshot artifact. The Host owns terminal encoding;
    /// the phone sends only this typed semantic action.
    public func requestScreenshot(sessionID: String) async throws -> RemoteScreenshotRequestResponse {
        let data = try await perform(
            path: "request-screenshot",
            method: "POST",
            body: try encoder.encode(RemoteScreenshotRequest(sessionID: sessionID))
        )
        return try decoder.decode(RemoteScreenshotRequestResponse.self, from: data)
    }

    /// Fetch a single artifact's full bytes, reassembling the offset-addressed
    /// chunks the Mac serves (each capped so it fits one relay frame). Returns
    /// the raw bytes plus its content type. `expectedSize` (from the listing)
    /// short-circuits the loop when known; otherwise `totalSize` from the first
    /// chunk drives it. `maxDimension` asks the Mac for a downscaled JPEG
    /// variant of an image (the gallery-grid path — usually one chunk instead
    /// of many); an older Mac ignores it and serves the original bytes.
    public func browserArtifactData(
        sessionID: String,
        kind: String,
        name: String,
        maxDimension: Int? = nil
    ) async throws -> (data: Data, contentType: String) {
        var assembled = Data()
        var contentType = "application/octet-stream"
        // Cap iterations defensively; 200KB chunks cover a ~100MB file well
        // before this trips, so it only guards against a stuck nextOffset.
        for _ in 0 ..< 4096 {
            var query = [
                "session_id": sessionID,
                "kind": kind,
                "name": name,
                "offset": "\(assembled.count)",
            ]
            if let maxDimension {
                query["max_dim"] = "\(maxDimension)"
            }
            let data = try await perform(path: "artifact", query: query, timeout: 20)
            let chunk = try decoder.decode(RemoteBrowserArtifactChunk.self, from: data)
            contentType = chunk.contentType
            guard let bytes = Data(base64Encoded: chunk.dataBase64) else {
                throw URLError(.cannotParseResponse)
            }
            assembled.append(bytes)
            // Done when we've caught up to the end, or the server returned no
            // progress (empty chunk) to avoid an infinite loop.
            if assembled.count >= Int(chunk.totalSize) || bytes.isEmpty {
                break
            }
        }
        return (assembled, contentType)
    }

    public func createSession(_ requestBody: RemoteCreateSessionRequest) async throws -> RemoteCreateSessionResponse {
        let data = try await perform(
            path: "sessions",
            method: "POST",
            body: try encoder.encode(requestBody)
        )
        return try decoder.decode(RemoteCreateSessionResponse.self, from: data)
    }

    /// Rotate + fetch this device's Unpeel Remote credentials over the LAN
    /// channel — the upgrade path for phones paired before the relay.
    public func relayCredentials() async throws -> RelayCredentials {
        let data = try await perform(path: "relay-credentials")
        return try decoder.decode(RelayCredentials.self, from: data)
    }

    // MARK: - Transport

    /// Every route goes through here: direct LAN HTTP, or the encrypted
    /// relay tunnel. Identical request semantics either way.
    private func perform(
        path: String,
        method: String = "GET",
        query: [String: String] = [:],
        body: Data? = nil,
        contentType: String? = nil,
        timeout: TimeInterval = 10
    ) async throws -> Data {
        if let relay {
            let response = try await relay.perform(
                method: method,
                path: "/mobile/\(path)",
                query: query,
                auth: authToken.map { "Bearer \($0)" },
                contentType: contentType,
                body: body,
                timeout: timeout
            )
            guard (200 ..< 300).contains(response.status) else {
                throw RemoteMacClientError(
                    statusCode: response.status,
                    serverMessage: Self.serverErrorMessage(from: response.body)
                )
            }
            return response.body
        }

        var components = URLComponents(
            url: baseURL.appendingPathComponent(path),
            resolvingAgainstBaseURL: false
        )!
        if !query.isEmpty {
            components.queryItems = query
                .sorted { $0.key < $1.key }
                .map { URLQueryItem(name: $0.key, value: $0.value) }
        }
        var request = URLRequest(url: components.url!)
        request.httpMethod = method
        request.timeoutInterval = timeout
        if let contentType {
            request.setValue(contentType, forHTTPHeaderField: "Content-Type")
        } else if method != "GET" {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        if let authToken, !authToken.isEmpty {
            request.setValue("Bearer \(authToken)", forHTTPHeaderField: "Authorization")
        }
        request.httpBody = body
        let (data, response) = try await URLSession.shared.data(for: request)
        try Self.validate(response, body: data)
        return data
    }

    private static func validate(_ response: URLResponse, body: Data? = nil) throws {
        guard let http = response as? HTTPURLResponse else {
            throw URLError(.badServerResponse)
        }
        guard (200..<300).contains(http.statusCode) else {
            throw RemoteMacClientError(
                statusCode: http.statusCode,
                serverMessage: Self.serverErrorMessage(from: body)
            )
        }
    }

    private static func serverErrorMessage(from body: Data?) -> String? {
        guard let body, !body.isEmpty,
              let object = try? JSONSerialization.jsonObject(with: body) as? [String: Any],
              let message = object["error"] as? String, !message.isEmpty
        else { return nil }
        return message
    }
}

enum RemoteArtifactUploadError: Error, LocalizedError, Equatable, Sendable {
    case emptyPayload
    case payloadTooLarge
    case unsupportedContentType
    case invalidProgress

    var errorDescription: String? {
        switch self {
        case .emptyPayload:
            "The image is empty."
        case .payloadTooLarge:
            "The image is larger than 4 MB."
        case .unsupportedContentType:
            "Only JPEG and PNG images can be uploaded."
        case .invalidProgress:
            "The Host returned invalid upload progress."
        }
    }
}

/// Pure resumable-upload state machine. Keeping transport behind a closure
/// makes the idempotency and retry contract independently testable for both
/// direct HTTP and the encrypted relay path.
enum ResumableArtifactUploader {
    static let chunkSize = 256 * 1024
    static let maximumSize = 4 * 1024 * 1024
    static let maximumAttemptsPerChunk = 3

    struct Chunk: Equatable, Sendable {
        let sessionID: String
        let uploadID: String
        let offset: Int
        let totalSize: Int
        let sha256: String
        let contentType: String
        let body: Data
    }

    typealias SendChunk = @Sendable (Chunk) async throws -> RemoteArtifactUploadProgress

    static func upload(
        sessionID: String,
        data: Data,
        contentType: String,
        uploadID: String,
        sendChunk: SendChunk
    ) async throws -> String {
        guard !data.isEmpty else { throw RemoteArtifactUploadError.emptyPayload }
        guard data.count <= maximumSize else { throw RemoteArtifactUploadError.payloadTooLarge }
        guard contentType == "image/jpeg" || contentType == "image/png" else {
            throw RemoteArtifactUploadError.unsupportedContentType
        }

        let digest = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        var offset = 0
        while offset < data.count {
            try Task.checkCancellation()
            let end = min(offset + chunkSize, data.count)
            let chunk = Chunk(
                sessionID: sessionID,
                uploadID: uploadID,
                offset: offset,
                totalSize: data.count,
                sha256: digest,
                contentType: contentType,
                body: data.subdata(in: offset ..< end)
            )
            let progress = try await sendWithRetry(chunk, sendChunk: sendChunk)
            guard progress.uploadID == uploadID,
                  progress.nextOffset == UInt64(end),
                  progress.complete == (end == data.count)
            else {
                throw RemoteArtifactUploadError.invalidProgress
            }
            if progress.complete {
                guard let path = progress.path, !path.isEmpty else {
                    throw RemoteArtifactUploadError.invalidProgress
                }
                return path
            }
            guard progress.path == nil else {
                throw RemoteArtifactUploadError.invalidProgress
            }
            offset = end
        }
        throw RemoteArtifactUploadError.invalidProgress
    }

    private static func sendWithRetry(
        _ chunk: Chunk,
        sendChunk: SendChunk
    ) async throws -> RemoteArtifactUploadProgress {
        var attempt = 0
        while true {
            do {
                return try await sendChunk(chunk)
            } catch let error as RemoteMacClientError {
                // HTTP replies are authoritative. In particular, never turn
                // a validation/authorization 4xx into repeated requests.
                throw error
            } catch is CancellationError {
                throw CancellationError()
            } catch let error as URLError where error.code == .cancelled {
                throw error
            } catch {
                attempt += 1
                guard attempt < maximumAttemptsPerChunk else { throw error }
                try Task.checkCancellation()
                let delay = UInt64(150 * attempt) * 1_000_000
                try await Task.sleep(nanoseconds: delay)
            }
        }
    }
}
