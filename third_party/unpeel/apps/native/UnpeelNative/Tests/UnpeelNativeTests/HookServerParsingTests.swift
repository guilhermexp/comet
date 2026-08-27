import XCTest
@testable import UnpeelNative

final class HookServerParsingTests: XCTestCase {
    func testContentLengthParserRejectsNegativeAndAmbiguousValues() throws {
        XCTAssertEqual(try StrictHTTPContentLength.parse(nil), 0)
        XCTAssertEqual(try StrictHTTPContentLength.parse("0"), 0)
        XCTAssertEqual(try StrictHTTPContentLength.parse("42"), 42)

        for invalid in ["-1", "+1", "", "1, 1", " 1", "1 ", "nope"] {
            XCTAssertThrowsError(try StrictHTTPContentLength.parse(invalid)) { error in
                XCTAssertEqual(error as? StrictHTTPContentLengthError, .invalid)
            }
        }
        XCTAssertThrowsError(
            try StrictHTTPContentLength.parse(String(StrictHTTPContentLength.maximum + 1))
        ) { error in
            XCTAssertEqual(error as? StrictHTTPContentLengthError, .tooLarge)
        }
    }

    func testHookEventNamesNormalizeProviderVariants() {
        XCTAssertEqual(HookServer.normalizedHookEventName("session-start"), "HookSeen")
        XCTAssertEqual(HookServer.normalizedHookEventName("session_start"), "HookSeen")
        XCTAssertEqual(HookServer.normalizedHookEventName("Start"), "Start")
        XCTAssertEqual(HookServer.normalizedHookEventName("beforeSubmitPrompt"), "UserPromptSubmit")
        XCTAssertEqual(HookServer.normalizedHookEventName("permission request"), "PermissionRequest")
        XCTAssertEqual(HookServer.normalizedHookEventName("VendorCustomEvent"), "VendorCustomEvent")
    }

    func testProviderSessionIDAcceptsKnownProviderKeys() {
        XCTAssertEqual(
            HookServer.providerSessionID(from: ["session_id": " claude-id \n"]),
            "claude-id"
        )
        XCTAssertEqual(
            HookServer.providerSessionID(from: ["threadID": "amp-thread"]),
            "amp-thread"
        )
        XCTAssertEqual(
            HookServer.providerSessionID(from: ["conversationId": "conversation-1"]),
            "conversation-1"
        )
        XCTAssertEqual(
            HookServer.providerSessionID(from: ["provider_session_id": "provider-1"]),
            "provider-1"
        )
    }

    func testProviderSessionIDIgnoresEmptyAndNonStringValues() {
        XCTAssertNil(HookServer.providerSessionID(from: ["session_id": "  \n"]))
        XCTAssertNil(HookServer.providerSessionID(from: ["session_id": 42]))
    }

    func testProviderTranscriptPathAcceptsKnownKeys() {
        XCTAssertEqual(
            HookServer.providerTranscriptPath(from: ["transcript_path": " /tmp/codex.jsonl "]),
            "/tmp/codex.jsonl"
        )
        XCTAssertEqual(
            HookServer.providerTranscriptPath(from: ["providerTranscriptPath": "/tmp/provider.jsonl"]),
            "/tmp/provider.jsonl"
        )
    }

    func testRuntimeGenerationAcceptsOwnedHookContractAndCamelCaseAdapter() {
        XCTAssertEqual(HookServer.runtimeGeneration(from: [
            "unpeel_runtime_generation": 4,
        ]), 4)
        XCTAssertEqual(HookServer.runtimeGeneration(from: [
            "unpeelRuntimeGeneration": 5,
        ]), 5)
    }

    func testRuntimeGenerationRejectsNonIntegerJSONValues() {
        for value: Any in [true, -1, 1.5, "2"] {
            XCTAssertNil(HookServer.runtimeGeneration(from: [
                "unpeel_runtime_generation": value,
            ]))
        }
    }
}
