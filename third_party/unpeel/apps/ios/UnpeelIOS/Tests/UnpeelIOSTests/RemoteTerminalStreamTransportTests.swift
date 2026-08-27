import XCTest
@testable import UnpeelIOS

/// Pure-logic tests for the terminal WebSocket wire contract: hello frame
/// decoding, binary frame offset parsing, fingerprint normalization, and
/// the transport-selection decision. Deliberately no sockets, no surfaces.
final class RemoteTerminalStreamTransportTests: XCTestCase {
    // MARK: - Hello frame

    func testHelloDecodesFullSpecFrame() {
        let text = """
        {"type":"hello","protocol":1,"session_id":"sess-1","state":"running",\
        "output_size":123456,"requested_offset":1024,"start_offset":1024,\
        "rebased":false,"cols":204,"rows":58}
        """
        guard case .hello(let hello) = RemoteTerminalWSServerMessage.parse(text: text) else {
            return XCTFail("expected hello")
        }
        XCTAssertEqual(hello.protocolVersion, 1)
        XCTAssertEqual(hello.sessionID, "sess-1")
        XCTAssertEqual(hello.state, "running")
        XCTAssertEqual(hello.outputSize, 123_456)
        XCTAssertEqual(hello.requestedOffset, 1024)
        XCTAssertEqual(hello.startOffset, 1024)
        XCTAssertFalse(hello.rebased)
        XCTAssertEqual(hello.cols, 204)
        XCTAssertEqual(hello.rows, 58)
    }

    func testHelloDecodesNullOffsetAndMissingGrid() {
        let text = """
        {"type":"hello","protocol":1,"session_id":"sess-2","state":"running",\
        "output_size":9000,"requested_offset":null,"start_offset":8704,\
        "rebased":true,"cols":null,"rows":null}
        """
        guard case .hello(let hello) = RemoteTerminalWSServerMessage.parse(text: text) else {
            return XCTFail("expected hello")
        }
        XCTAssertNil(hello.requestedOffset)
        XCTAssertEqual(hello.startOffset, 8704)
        XCTAssertTrue(hello.rebased)
        XCTAssertNil(hello.cols)
        XCTAssertNil(hello.rows)
    }

    func testParseErrorFrame() {
        let message = RemoteTerminalWSServerMessage.parse(
            text: #"{"type":"error","message":"host went away"}"#
        )
        XCTAssertEqual(message, .error("host went away"))
    }

    func testParseErrorFrameWithoutMessageField() {
        let message = RemoteTerminalWSServerMessage.parse(text: #"{"type":"error"}"#)
        XCTAssertEqual(message, .error("unknown error"))
    }

    func testParseUnknownTypeAndGarbage() {
        XCTAssertEqual(RemoteTerminalWSServerMessage.parse(text: #"{"type":"future-thing"}"#), .unknown)
        XCTAssertEqual(RemoteTerminalWSServerMessage.parse(text: "not json"), .unknown)
        XCTAssertEqual(RemoteTerminalWSServerMessage.parse(text: "[1,2,3]"), .unknown)
        // A hello missing required fields must not decode into a bogus hello.
        XCTAssertEqual(RemoteTerminalWSServerMessage.parse(text: #"{"type":"hello"}"#), .unknown)
    }

    // MARK: - Binary frames

    func testBinaryFrameParsesBigEndianOffsetAndPayload() {
        var data = Data([0, 0, 0, 0, 0, 0, 0x01, 0x02]) // 258
        data.append(Data("hello".utf8))
        let frame = RemoteTerminalWSBinaryFrame.parse(data)
        XCTAssertEqual(frame?.offset, 258)
        XCTAssertEqual(frame.map { Data($0.payload) }, Data("hello".utf8))
    }

    func testBinaryFrameParsesLargeOffset() {
        // 0x0000_0001_0000_0000 = 4 GiB — past the u32 boundary.
        let data = Data([0, 0, 0, 1, 0, 0, 0, 0]) + Data([0x41])
        let frame = RemoteTerminalWSBinaryFrame.parse(data)
        XCTAssertEqual(frame?.offset, 4_294_967_296)
        XCTAssertEqual(frame?.payload.count, 1)
    }

    func testBinaryFrameWithEmptyPayloadParses() {
        let frame = RemoteTerminalWSBinaryFrame.parse(Data([0, 0, 0, 0, 0, 0, 0, 42]))
        XCTAssertEqual(frame?.offset, 42)
        XCTAssertEqual(frame?.payload.count, 0)
    }

    func testBinaryFrameShorterThanHeaderIsRejected() {
        XCTAssertNil(RemoteTerminalWSBinaryFrame.parse(Data([1, 2, 3])))
        XCTAssertNil(RemoteTerminalWSBinaryFrame.parse(Data()))
    }

    func testBinaryFrameParsesFromNonZeroBasedSlice() {
        // Data slices keep their parent's indices; the parser must use
        // relative indexing or this reads garbage.
        let padded = Data([0xFF, 0xFF]) + Data([0, 0, 0, 0, 0, 0, 0, 7]) + Data("x".utf8)
        let slice = padded.dropFirst(2)
        let frame = RemoteTerminalWSBinaryFrame.parse(slice)
        XCTAssertEqual(frame?.offset, 7)
        XCTAssertEqual(frame.map { Data($0.payload) }, Data("x".utf8))
    }

    // MARK: - Fingerprint normalization

    func testFingerprintNormalizationLowercasesAndStripsDecoration() {
        let plain = String(repeating: "ab12", count: 16) // 64 hex chars
        XCTAssertEqual(
            RemoteTerminalTransportSelector.normalizedFingerprint(plain.uppercased()),
            plain
        )
        XCTAssertEqual(
            RemoteTerminalTransportSelector.normalizedFingerprint("  \(plain)\n"),
            plain
        )
        XCTAssertEqual(
            RemoteTerminalTransportSelector.normalizedFingerprint("sha256:\(plain)"),
            plain
        )
        // Colon-separated hex pairs (openssl-style) normalize too.
        let colonized = stride(from: 0, to: plain.count, by: 2).map { index -> String in
            let start = plain.index(plain.startIndex, offsetBy: index)
            let end = plain.index(start, offsetBy: 2)
            return String(plain[start..<end])
        }.joined(separator: ":")
        XCTAssertEqual(
            RemoteTerminalTransportSelector.normalizedFingerprint(colonized),
            plain
        )
    }

    func testFingerprintNormalizationRejectsInvalidInput() {
        XCTAssertNil(RemoteTerminalTransportSelector.normalizedFingerprint(nil))
        XCTAssertNil(RemoteTerminalTransportSelector.normalizedFingerprint(""))
        XCTAssertNil(RemoteTerminalTransportSelector.normalizedFingerprint("abcd")) // too short
        XCTAssertNil(
            RemoteTerminalTransportSelector.normalizedFingerprint(
                String(repeating: "g", count: 64) // not hex
            )
        )
        XCTAssertNil(
            RemoteTerminalTransportSelector.normalizedFingerprint(
                String(repeating: "a", count: 65) // wrong length
            )
        )
    }

    func testFingerprintsMatchIsCaseInsensitive() {
        let plain = String(repeating: "0f", count: 32)
        XCTAssertTrue(
            RemoteTerminalTransportSelector.fingerprintsMatch(plain, plain.uppercased())
        )
        XCTAssertFalse(
            RemoteTerminalTransportSelector.fingerprintsMatch(
                plain,
                String(repeating: "0e", count: 32)
            )
        )
        XCTAssertFalse(RemoteTerminalTransportSelector.fingerprintsMatch(plain, nil))
    }

    // MARK: - Transport selection

    private let validFingerprint = String(repeating: "ab", count: 32)

    func testEndpointRequiresPortAndValidFingerprint() {
        XCTAssertNil(
            RemoteTerminalTransportSelector.endpoint(port: nil, fingerprint: validFingerprint)
        )
        XCTAssertNil(RemoteTerminalTransportSelector.endpoint(port: 50123, fingerprint: nil))
        XCTAssertNil(
            RemoteTerminalTransportSelector.endpoint(port: 50123, fingerprint: "bogus")
        )
        XCTAssertNil(
            RemoteTerminalTransportSelector.endpoint(port: 0, fingerprint: validFingerprint)
        )
        let endpoint = RemoteTerminalTransportSelector.endpoint(
            port: 50123,
            fingerprint: validFingerprint.uppercased()
        )
        XCTAssertEqual(endpoint?.port, 50123)
        XCTAssertEqual(endpoint?.certificateFingerprint, validFingerprint)
    }

    func testCandidateRequiresEndpointHostAndToken() {
        let endpoint = RemoteServerEndpoint(port: 50123, certificateFingerprint: validFingerprint)
        let baseURL = URL(string: "http://192.168.1.20:17661/mobile")!

        // No endpoint (dev bridge / server down / pre-WS Mac) → HTTP only.
        XCTAssertNil(
            RemoteTerminalTransportSelector.candidate(
                endpoint: nil, baseURL: baseURL, authToken: "tok"
            )
        )
        // No token → HTTP only.
        XCTAssertNil(
            RemoteTerminalTransportSelector.candidate(
                endpoint: endpoint, baseURL: baseURL, authToken: nil
            )
        )
        XCTAssertNil(
            RemoteTerminalTransportSelector.candidate(
                endpoint: endpoint, baseURL: baseURL, authToken: ""
            )
        )

        let candidate = RemoteTerminalTransportSelector.candidate(
            endpoint: endpoint, baseURL: baseURL, authToken: "tok"
        )
        XCTAssertEqual(candidate?.host, "192.168.1.20")
        XCTAssertEqual(candidate?.port, 50123)
        XCTAssertEqual(candidate?.certificateFingerprint, validFingerprint)
        XCTAssertEqual(candidate?.token, "tok")
    }

    func testWebSocketOutputURLWithResumeOffset() {
        let url = RemoteTerminalTransportSelector.webSocketOutputURL(
            host: "192.168.1.20",
            port: 50123,
            sessionID: "sess-abc",
            token: "tok",
            offset: 42
        )
        XCTAssertEqual(
            url?.absoluteString,
            "wss://192.168.1.20:50123/api/sessions/sess-abc/output?token=tok&offset=42"
        )
    }

    func testWebSocketOutputURLWithoutOffset() {
        let url = RemoteTerminalTransportSelector.webSocketOutputURL(
            host: "mac.local",
            port: 1234,
            sessionID: "s1",
            token: "tok",
            offset: nil
        )
        XCTAssertEqual(
            url?.absoluteString,
            "wss://mac.local:1234/api/sessions/s1/output?token=tok"
        )
    }

    func testWebSocketOutputURLRejectsEmptyHostOrSession() {
        XCTAssertNil(
            RemoteTerminalTransportSelector.webSocketOutputURL(
                host: "", port: 1, sessionID: "s", token: "t", offset: nil
            )
        )
        XCTAssertNil(
            RemoteTerminalTransportSelector.webSocketOutputURL(
                host: "h", port: 1, sessionID: "", token: "t", offset: nil
            )
        )
    }

    // MARK: - Client input frames

    private struct DecodedInput: Decodable {
        let type: String
        let data: String
        let wid: String?
    }

    private func decodeFrames(_ frames: [String]) throws -> [DecodedInput] {
        try frames.map {
            try JSONDecoder().decode(DecodedInput.self, from: Data($0.utf8))
        }
    }

    func testInputFrameEncodesSingleSmallMessage() throws {
        let frames = RemoteTerminalWSClientMessage.inputFrames(
            for: "ls -la\r",
            writeID: "write-123"
        )
        XCTAssertEqual(frames.count, 1)
        let decoded = try decodeFrames(frames)
        XCTAssertEqual(decoded[0].type, "input")
        XCTAssertEqual(decoded[0].data, "ls -la\r")
        XCTAssertEqual(decoded[0].wid, "write-123")
    }

    func testInputFramePreservesControlAndEscapeBytes() throws {
        let text = "\u{1B}[200~pasted\u{1B}[201~\u{3}"
        let decoded = try decodeFrames(RemoteTerminalWSClientMessage.inputFrames(for: text))
        XCTAssertEqual(decoded.count, 1)
        XCTAssertEqual(decoded[0].data, text)
    }

    func testInputFramesChunkLargeInputOnCharacterBoundaries() throws {
        // 3-byte character: a naive byte split would shear it mid-scalar.
        let text = String(repeating: "€", count: 100)
        let frames = RemoteTerminalWSClientMessage.inputFrames(for: text, maxBytes: 32)
        XCTAssertGreaterThan(frames.count, 1)
        let decoded = try decodeFrames(frames)
        for part in decoded {
            XCTAssertEqual(part.type, "input")
            XCTAssertLessThanOrEqual(part.data.utf8.count, 32)
            XCTAssertNil(part.wid)
        }
        XCTAssertEqual(decoded.map(\.data).joined(), text)
    }

    func testInputFramesEmptyInputProducesNoFrames() {
        XCTAssertTrue(RemoteTerminalWSClientMessage.inputFrames(for: "").isEmpty)
    }
}
