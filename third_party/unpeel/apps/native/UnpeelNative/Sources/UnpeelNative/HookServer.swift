//
//  HookServer.swift
//  UnpeelNative
//
//  Native port of the hook HTTP server in
//  apps/desktop/src-tauri/src/hook_server.rs (:1431-1570).
//
//  Provider hook scripts/wrappers (installed under ~/.unpeel/hooks by the
//  session host) POST JSON events to http://127.0.0.1:<port>/hook/<session_id>
//  and broadcast to every port listed in ~/.unpeel/app-ports. This server:
//
//  - listens on 127.0.0.1 with an OS-assigned port (port 0),
//  - registers that port in ~/.unpeel/app-ports on startup and removes it
//    on clean shutdown,
//  - answers 404 for session ids it has no manifest for so foreign
//    instances' events are not swallowed (is_known_session,
//    session_activity.rs:493-499),
//  - answers 200 {"ok":true} for accepted events and forwards them to the
//    store on the main actor.
//
//  Transport is a plain BSD socket with a blocking accept loop and one
//  thread per connection, the same structure as the Rust server
//  (TcpListener + thread-per-stream with a 5s read timeout). An earlier
//  Network.framework (NWListener) variant intermittently sat on delivered
//  bytes until the peer closed, which made hook curls (--max-time 2) time
//  out; blocking reads have no such failure mode.
//

import CoreFoundation
import Darwin
import Foundation

enum StrictHTTPContentLengthError: Error, Equatable {
    case invalid
    case tooLarge
}

/// The native listeners support fixed-length request bodies only. Parse that
/// framing header strictly so signed, overflowed, empty, or comma-joined
/// values cannot reach Data indexing with an invalid offset.
enum StrictHTTPContentLength {
    static let maximum = 4 * 1024 * 1024

    static func parse(_ raw: String?) throws -> Int {
        guard let raw else { return 0 }
        guard !raw.isEmpty,
              raw.utf8.allSatisfy({ (48...57).contains($0) }),
              let value = Int(raw)
        else { throw StrictHTTPContentLengthError.invalid }
        guard value <= maximum else { throw StrictHTTPContentLengthError.tooLarge }
        return value
    }
}

/// One parsed hook POST. Field names match the JSON the provider hook
/// scripts send and the Rust `HookEvent` struct (hook_server.rs:557-564).
struct HookEvent {
    let sessionID: String
    let hookEventName: String
    /// Captured on the socket thread before MainActor delivery. In-place
    /// restart can queue the UI-side handler; this receipt time distinguishes
    /// the departing runtime's Stop after a new generation has committed.
    let receivedAt: Date = Date()
    /// Host-owned managed-runtime generation copied into the child process
    /// environment and stamped by Unpeel's hook assets. Older/custom hook
    /// scripts omit it; those events use the conservative legacy path in the
    /// store after an in-place restart.
    let runtimeGeneration: UInt64?
    let toolName: String?
    let promptText: String?
    /// The provider's OWN conversation id from the payload body (Claude
    /// forwards its full hook JSON, whose `session_id` is the conversation
    /// UUID — distinct from `sessionID`, which is Unpeel's route id). Used
    /// to resume the exact conversation on restart (see ResumeCommand).
    let providerSessionID: String?
    /// Provider JSONL transcript path when the hook payload exposes it
    /// (Codex notify hooks do). Stored in the session manifest for MCP
    /// transcript reads.
    let providerTranscriptPath: String?
}

final class HookServer: @unchecked Sendable {
    private(set) var port: UInt16 = 0
    private let onEvent: @Sendable (HookEvent) -> Void
    private var listenFD: Int32 = -1
    private var acceptThread: Thread?

    /// Handler for authenticated /mcp/* lifecycle requests (mcp_bridge.rs
    /// parity). Called off-main with the request path and JSON body; replies
    /// asynchronously with (status, json_body). Set once at startup by
    /// UnpeelStore.attachHookServer; nil answers 404 like the early builds.
    var mcpHandler: (@Sendable (
        _ path: String,
        _ body: Data,
        _ reply: @escaping @Sendable (Int, String) -> Void
    ) -> Void)?

    /// Handler for `/state-changed`: another Unpeel wrote shared state and
    /// wants this one to re-read it now. Unauthenticated by design — it
    /// carries no data and only ever costs a rescan, exactly like the hook
    /// events arriving on the same loopback port.
    var stateChangeHandler: (@Sendable (_ change: String) -> Void)?

    private static let debug = ProcessInfo.processInfo.environment["UNPEEL_DEBUG"] == "1"

    /// Binds 127.0.0.1:0, starts the accept loop, and registers the
    /// assigned port in ~/.unpeel/app-ports. Returns nil if the socket
    /// could not be created.
    init?(onEvent: @escaping @Sendable (HookEvent) -> Void) {
        self.onEvent = onEvent

        let fd = socket(AF_INET, SOCK_STREAM, 0)
        guard fd >= 0 else { return nil }

        var reuse: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &reuse, socklen_t(MemoryLayout<Int32>.size))

        var addr = sockaddr_in()
        addr.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = 0 // OS-assigned
        addr.sin_addr.s_addr = inet_addr("127.0.0.1")

        let bound = withUnsafePointer(to: &addr) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bound == 0, listen(fd, 16) == 0 else {
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

        listenFD = fd
        port = UInt16(bigEndian: assigned.sin_port)

        let thread = Thread { [weak self] in
            self?.acceptLoop(fd: fd)
        }
        thread.name = "unpeel.hook-server"
        thread.start()
        acceptThread = thread

        Self.registerPort(port)
        // Make sure the MCP auth token exists before any agent session can
        // launch and ask the bridge for lifecycle operations.
        MCPAuth.ensureToken()
        NSLog("[UnpeelNative] hook server listening on 127.0.0.1:%d", Int(port))
    }

    /// Closes the listener and removes our port from ~/.unpeel/app-ports.
    /// Call from applicationWillTerminate for a clean shutdown.
    func stop() {
        if listenFD >= 0 {
            close(listenFD)
            listenFD = -1
        }
        if port != 0 {
            Self.unregisterPort(port)
            NSLog("[UnpeelNative] hook server stopped, port %d unregistered", Int(port))
            port = 0
        }
    }

    // MARK: - Accept / connection threads (hook_server.rs:583-589)

    private func acceptLoop(fd: Int32) {
        while true {
            let client = accept(fd, nil, nil)
            guard client >= 0 else {
                // EBADF after stop(); transient errors just retry.
                if errno == EBADF || errno == ECONNABORTED && listenFD < 0 { return }
                if listenFD < 0 { return }
                continue
            }
            let thread = Thread { [weak self] in
                self?.handleConnection(client)
            }
            thread.start()
        }
    }

    private func handleConnection(_ fd: Int32) {
        defer { close(fd) }

        // 5s read timeout like the Rust server (handle_connection :1432).
        var timeout = timeval(tv_sec: 5, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))

        var buffer = Data()
        var chunk = [UInt8](repeating: 0, count: 64 * 1024)
        var headerEndRange: Range<Data.Index>?
        var contentLength = 0
        var contentLengthHeader: String?
        var hasTransferEncoding = false
        var requestLine = ""
        var authHeader: String?

        // Read headers, then exactly content-length body bytes.
        while true {
            if headerEndRange == nil,
               let range = buffer.range(of: Data("\r\n\r\n".utf8)) {
                headerEndRange = range
                guard let header = String(
                    data: buffer[buffer.startIndex..<range.lowerBound], encoding: .utf8
                ) else {
                    respond(fd, status: 400, body: #"{"error":"bad request"}"#)
                    return
                }
                let lines = header.components(separatedBy: "\r\n")
                requestLine = lines.first ?? ""
                for line in lines.dropFirst() {
                    if line.lowercased().hasPrefix("content-length:") {
                        guard contentLengthHeader == nil else {
                            respond(fd, status: 400, body: #"{"error":"duplicate content-length"}"#)
                            return
                        }
                        contentLengthHeader = line.dropFirst("content-length:".count)
                            .trimmingCharacters(in: .whitespaces)
                    }
                    if line.lowercased().hasPrefix("transfer-encoding:") {
                        hasTransferEncoding = true
                    }
                    if line.lowercased().hasPrefix("\(MCPAuth.headerName):") {
                        authHeader = line.split(separator: ":", maxSplits: 1)
                            .dropFirst().first
                            .map { $0.trimmingCharacters(in: .whitespaces) }
                    }
                }
                if hasTransferEncoding {
                    respond(fd, status: 400, body: #"{"error":"unsupported transfer-encoding"}"#)
                    return
                }
                do {
                    contentLength = try StrictHTTPContentLength.parse(contentLengthHeader)
                } catch StrictHTTPContentLengthError.tooLarge {
                    respond(fd, status: 400, body: #"{"error":"body too large"}"#)
                    return
                } catch {
                    respond(fd, status: 400, body: #"{"error":"invalid content-length"}"#)
                    return
                }
            }

            if let headerEnd = headerEndRange {
                let bodyBytes = buffer.distance(from: headerEnd.upperBound, to: buffer.endIndex)
                if bodyBytes >= contentLength {
                    let body = buffer.subdata(
                        in: headerEnd.upperBound..<buffer.index(headerEnd.upperBound, offsetBy: contentLength)
                    )
                    handleRequest(
                        fd, requestLine: requestLine, body: body, authHeader: authHeader
                    )
                    return
                }
            }

            if buffer.count > 8 * 1024 * 1024 { return }
            let read = recv(fd, &chunk, chunk.count, 0)
            guard read > 0 else { return } // timeout, error, or peer closed early
            buffer.append(contentsOf: chunk[0..<read])
        }
    }

    // MARK: - Request handling (hook_server.rs handle_connection :1431-1570)

    private func handleRequest(
        _ fd: Int32, requestLine: String, body: Data, authHeader: String?
    ) {
        let parts = requestLine.split(separator: " ")
        guard parts.count >= 2 else {
            respond(fd, status: 400, body: #"{"error":"bad request"}"#)
            return
        }
        let method = String(parts[0])
        let path = String(parts[1])

        // hook_server.rs:1441-1444 — POST only.
        guard method == "POST" else {
            respond(fd, status: 405, body: #"{"error":"method not allowed"}"#)
            return
        }

        // hook_server.rs:1476-1482 — body must be JSON.
        guard let json = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any] else {
            respond(fd, status: 400, body: #"{"error":"invalid json"}"#)
            return
        }

        // Route: /mcp/* — session lifecycle for Unpeel Sessions MCP
        // (hook_server.rs:1490-1505). Shared-secret header required; the
        // store answers on the main actor while this thread blocks.
        if path.hasPrefix("/mcp/") {
            guard MCPAuth.verify(authHeader) else {
                respond(fd, status: 401, body: #"{"error":"unauthorized"}"#)
                return
            }
            guard let handler = mcpHandler else {
                respond(fd, status: 404, body: #"{"error":"not found"}"#)
                return
            }
            let done = DispatchSemaphore(value: 0)
            let box = ResultBox()
            handler(path, body) { status, body in
                box.set((status, body))
                done.signal()
            }
            // Generous ceiling: controller-driven starts shell out to
            // unpeel-host and the bridge client reads with a 20s timeout.
            // approve-write blocks on the user's approval alert instead — its
            // MCP-host client reads with a ~130s timeout, so give the user
            // longer than that.
            let ceiling: TimeInterval = path.hasPrefix("/mcp/approve-") ? 150 : 25
            _ = done.wait(timeout: .now() + ceiling)
            let (status, responseBody) = box.get()
                ?? (504, #"{"error":"bridge timed out"}"#)
            Self.trace("native-mcp-bridge path=\(path) status=\(status)")
            respond(fd, status: status, body: responseBody)
            return
        }

        // Route: /state-changed — the cross-frontend refresh ping
        // (unpeel-core state_bus). The TUI, the CLI and this app are peers
        // on the same bus; whoever writes shared state tells the others.
        if path == "/state-changed" {
            let change = (try? JSONSerialization.jsonObject(with: body) as? [String: Any])
                .flatMap { $0?["change"] as? String } ?? "unknown"
            stateChangeHandler?(change)
            respond(fd, status: 200, body: #"{"ok":true}"#)
            return
        }

        // Route: /hook/<session_id> (hook_server.rs:1507-1514). Unknown routes
        // intentionally 404 here.
        guard path.hasPrefix("/hook/"), case let sessionID = String(path.dropFirst("/hook/".count)),
              !sessionID.isEmpty
        else {
            respond(fd, status: 404, body: #"{"error":"not found"}"#)
            return
        }

        // hook_server.rs:1516-1524 — hook_event_name is mandatory. Grok's
        // compatibility hook payloads use camelCase, so accept and normalize
        // both shapes at the native boundary.
        let rawHookEventName = (json["hook_event_name"] as? String)
            ?? (json["hookEventName"] as? String)
        guard let rawHookEventName, !rawHookEventName.isEmpty else {
            respond(fd, status: 400, body: #"{"error":"missing hook_event_name"}"#)
            return
        }
        let hookEventName = Self.normalizedHookEventName(rawHookEventName)

        // Mirror is_known_session: hook scripts broadcast to every registered
        // instance port; only an instance that actually knows this session
        // may accept the event (404 otherwise so the owner still gets it).
        guard Self.isKnownSession(sessionID) else {
            Self.trace("native-hook-server-unknown-session session_id=\(sessionID) hook_event_name=\(hookEventName)")
            if Self.debug {
                NSLog("[hook-server] 404 unknown session %@ event=%@", sessionID, hookEventName)
            }
            respond(fd, status: 404, body: #"{"error":"unknown session"}"#)
            return
        }

        let event = HookEvent(
            sessionID: sessionID,
            hookEventName: hookEventName,
            runtimeGeneration: Self.runtimeGeneration(from: json),
            toolName: json["tool_name"] as? String,
            promptText: json["prompt_text"] as? String,
            providerSessionID: Self.providerSessionID(from: json),
            providerTranscriptPath: Self.providerTranscriptPath(from: json)
        )

        Self.trace("native-hook-server session_id=\(sessionID) hook_event_name=\(hookEventName) tool_name=\(event.toolName ?? "None")")
        if Self.debug {
            NSLog(
                "[hook-server] event session=%@ name=%@ tool=%@",
                sessionID, hookEventName, event.toolName ?? "-"
            )
        }

        onEvent(event)
        respond(fd, status: 200, body: #"{"ok":true}"#)
    }

    static func normalizedHookEventName(_ raw: String) -> String {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        let key = trimmed
            .replacingOccurrences(of: "-", with: "_")
            .replacingOccurrences(of: " ", with: "_")
            .lowercased()
        switch key {
        case "start":
            return "Start"
        // SessionStart means "the CLI opened / resumed a conversation", not
        // "the agent is working". Grok (and Cursor-compat) post this as
        // session_start; mapping it to Start latched a busy spinner on every
        // Grok launch, then Grok's idle TUI re-armed it forever.
        case "session_start", "sessionstart":
            return "HookSeen"
        case "user_prompt_submit", "userpromptsubmit", "user_prompt_submitted",
             "before_submit_prompt", "beforesubmitprompt":
            return "UserPromptSubmit"
        case "stop", "session_end", "sessionend", "subagent_stop", "subagentstop":
            return "Stop"
        case "stop_failure", "stopfailure":
            return "StopFailure"
        case "permission_request", "permissionrequest":
            return "PermissionRequest"
        default:
            return trimmed
        }
    }

    static func providerSessionID(from json: [String: Any]) -> String? {
        firstNonEmptyString(
            in: json,
            keys: [
                // Claude, OpenCode, Grok, Cursor, and the shared notify hook.
                "session_id",
                "chatId",
                "chat_id",
                // Manifest field name and explicit provider payload variants.
                "provider_session_id",
                "providerSessionID",
                "providerSessionId",
                // Amp and several CLIs expose thread/conversation ids instead.
                "thread_id",
                "threadID",
                "threadId",
                "conversation_id",
                "conversationID",
                "conversationId",
            ]
        )
    }

    static func providerTranscriptPath(from json: [String: Any]) -> String? {
        firstNonEmptyString(
            in: json,
            keys: [
                // Codex notify hooks.
                "transcript_path",
                "transcriptPath",
                // Manifest field name and explicit provider payload variants.
                "provider_transcript_path",
                "providerTranscriptPath",
            ]
        )
    }

    /// Read the generation stamped by owned hook assets. The snake-case key
    /// is the contract; camelCase is accepted for manually authored adapters.
    /// JSON booleans and fractional/negative values are deliberately rejected
    /// even though they also bridge through NSNumber.
    static func runtimeGeneration(from json: [String: Any]) -> UInt64? {
        for key in ["unpeel_runtime_generation", "unpeelRuntimeGeneration"] {
            guard let number = json[key] as? NSNumber else { continue }
            guard CFGetTypeID(number) != CFBooleanGetTypeID(),
                  let generation = UInt64(number.stringValue)
            else { continue }
            return generation
        }
        return nil
    }

    private static func firstNonEmptyString(in json: [String: Any], keys: [String]) -> String? {
        for key in keys {
            guard let value = json[key] as? String else { continue }
            let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty { return trimmed }
        }
        return nil
    }

    private func respond(_ fd: Int32, status: Int, body: String) {
        let reason: String
        switch status {
        case 200: reason = "OK"
        case 400: reason = "Bad Request"
        case 401: reason = "Unauthorized"
        case 404: reason = "Not Found"
        case 405: reason = "Method Not Allowed"
        case 504: reason = "Gateway Timeout"
        default: reason = "Error"
        }
        let payload =
            "HTTP/1.1 \(status) \(reason)\r\n"
            + "Content-Type: application/json\r\n"
            + "Content-Length: \(body.utf8.count)\r\n"
            + "Connection: close\r\n\r\n"
            + body
        let bytes = Array(payload.utf8)
        var sent = 0
        while sent < bytes.count {
            let n = bytes.withUnsafeBufferPointer { pointer in
                send(fd, pointer.baseAddress! + sent, bytes.count - sent, 0)
            }
            guard n > 0 else { return }
            sent += n
        }
    }

    // MARK: - Session ownership (session_activity.rs is_known_session)

    /// A session is "known" when its hosted manifest exists on disk. The
    /// native store rebuilds all of its session state from these manifests,
    /// so the file check is authoritative (same fallback the Rust server
    /// uses, session_activity.rs:493-499).
    static func isKnownSession(_ sessionID: String) -> Bool {
        guard !sessionID.contains("/"), !sessionID.contains("..") else { return false }
        let manifest = LaunchConfig.appSessionsDir
            .appendingPathComponent(sessionID)
            .appendingPathComponent("manifest.json")
        return FileManager.default.fileExists(atPath: manifest.path)
    }

    // MARK: - Port registry (~/.unpeel/app-ports)

    static var portRegistryURL: URL {
        LaunchConfig.unpeelDir.appendingPathComponent("app-ports")
    }

    private static var portRegistryLockURL: URL {
        LaunchConfig.unpeelDir.appendingPathComponent("app-ports.lock")
    }

    /// Reads one port per line; unparsable lines are dropped.
    static func readPortRegistry() -> [UInt16] {
        guard let raw = try? String(contentsOf: portRegistryURL, encoding: .utf8) else {
            return []
        }
        return raw.split(whereSeparator: \.isNewline).compactMap {
            UInt16($0.trimmingCharacters(in: .whitespaces))
        }
    }

    /// Writes ports as newline-joined values with a trailing newline; the file
    /// is removed when no ports remain.
    private static func writePortRegistry(_ ports: [UInt16]) {
        let url = portRegistryURL
        if ports.isEmpty {
            _ = unlink(url.path)
            return
        }
        try? FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        let body = ports.map(String.init).joined(separator: "\n") + "\n"
        guard let data = body.data(using: .utf8) else { return }
        let temporary = url.deletingLastPathComponent().appendingPathComponent(
            ".app-ports.\(getpid()).\(UUID().uuidString).tmp"
        )
        let descriptor = open(temporary.path, O_CREAT | O_EXCL | O_WRONLY, mode_t(0o600))
        guard descriptor >= 0 else { return }
        var succeeded = true
        data.withUnsafeBytes { raw in
            guard let base = raw.baseAddress else { return }
            var offset = 0
            while offset < raw.count {
                let count = Darwin.write(
                    descriptor,
                    base.advanced(by: offset),
                    raw.count - offset
                )
                if count <= 0 {
                    succeeded = false
                    return
                }
                offset += count
            }
        }
        if fsync(descriptor) != 0 { succeeded = false }
        close(descriptor)
        if succeeded, rename(temporary.path, url.path) == 0 {
            return
        }
        _ = unlink(temporary.path)
    }

    private static func withPortRegistryLock(_ body: () -> Void) {
        try? FileManager.default.createDirectory(
            at: portRegistryURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        let descriptor = open(
            portRegistryLockURL.path,
            O_CREAT | O_RDWR,
            mode_t(0o600)
        )
        guard descriptor >= 0 else { return }
        defer { close(descriptor) }
        guard fchmod(descriptor, mode_t(0o600)) == 0,
              flock(descriptor, LOCK_EX) == 0
        else { return }
        defer { _ = flock(descriptor, LOCK_UN) }
        body()
    }

    /// Dedupes this server's port, appends it last, and caps the registry at
    /// 16 entries (oldest dropped).
    private static func registerPort(_ port: UInt16) {
        withPortRegistryLock {
            var ports = readPortRegistry()
            ports.removeAll { $0 == port }
            ports.append(port)
            let maxEntries = 16
            if ports.count > maxEntries {
                ports.removeFirst(ports.count - maxEntries)
            }
            writePortRegistry(ports)
        }
    }

    /// Removes this server's port from the registry.
    private static func unregisterPort(_ port: UInt16) {
        withPortRegistryLock {
            var ports = readPortRegistry()
            ports.removeAll { $0 == port }
            writePortRegistry(ports)
        }
    }

    // MARK: - Trace log (~/.unpeel/hooks/trace.log, shared with Tauri)

    private static func trace(_ line: String) {
        let url = LaunchConfig.unpeelDir
            .appendingPathComponent("hooks")
            .appendingPathComponent("trace.log")
        let stamped = "\(UInt64(Date().timeIntervalSince1970 * 1000)) \(line)\n"
        guard let data = stamped.data(using: .utf8) else { return }
        try? FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        if let handle = try? FileHandle(forWritingTo: url) {
            defer { try? handle.close() }
            _ = try? handle.seekToEnd()
            try? handle.write(contentsOf: data)
        } else {
            try? data.write(to: url)
        }
    }
}

/// Tiny lock box so the connection thread can wait on a main-actor reply
/// without capturing a mutable local in a @Sendable closure.
private final class ResultBox: @unchecked Sendable {
    private let lock = NSLock()
    private var value: (Int, String)?

    func set(_ newValue: (Int, String)) {
        lock.lock()
        value = newValue
        lock.unlock()
    }

    func get() -> (Int, String)? {
        lock.lock()
        defer { lock.unlock() }
        return value
    }
}
