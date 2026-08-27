import XCTest
@testable import UnpeelNative

final class OpenURLSanitizerTests: XCTestCase {
    private func clean(_ raw: String) -> String? {
        GhosttyTerminalPane.sanitizedURL(from: raw)?.absoluteString
    }

    func testPlainURLPassesThrough() {
        XCTAssertEqual(clean("https://quiz.ai"), "https://quiz.ai")
    }

    func testStripsWrappingWhitespaceAndNewlines() {
        // Terminal-wrapped link: split across a line break with padding.
        XCTAssertEqual(
            clean("https://search.google.com/\nsearch-console/"),
            "https://search.google.com/search-console/"
        )
    }

    func testPeelsMarkdownWrappers() {
        XCTAssertEqual(clean("(https://quiz.ai)"), "https://quiz.ai")
        XCTAssertEqual(clean("<https://quiz.ai>"), "https://quiz.ai")
    }

    func testDropsTrailingSentencePunctuation() {
        XCTAssertEqual(clean("https://quiz.ai."), "https://quiz.ai")
        XCTAssertEqual(clean("https://quiz.ai,"), "https://quiz.ai")
    }

    func testAddsSchemeToBareHost() {
        XCTAssertEqual(clean("www.example.com/path"), "https://www.example.com/path")
        XCTAssertEqual(clean("example.com"), "https://example.com")
    }

    func testKeepsMailto() {
        XCTAssertEqual(clean("mailto:hi@quiz.ai"), "mailto:hi@quiz.ai")
    }

    func testRejectsUnsupportedScheme() {
        // Avoid handing LaunchServices a scheme that triggers arbitrary apps.
        XCTAssertNil(clean("file:///Applications/Calculator.app"))
        XCTAssertNil(clean("javascript:alert(1)"))
    }

    func testRejectsEmptyAndJunk() {
        XCTAssertNil(clean(""))
        XCTAssertNil(clean("   \n  "))
        XCTAssertNil(clean("just some text"))
    }

    @MainActor
    func testDropReferenceQuotingUsesSingleQuotesWhenNeeded() {
        XCTAssertEqual(
            GhosttyTerminalPane.quoteDropReference("/tmp/Screenshot.png"),
            "/tmp/Screenshot.png"
        )
        XCTAssertEqual(
            GhosttyTerminalPane.quoteDropReference("/tmp/Screenshot 1.png"),
            "'/tmp/Screenshot 1.png'"
        )
        XCTAssertEqual(
            GhosttyTerminalPane.quoteDropReference("/tmp/it's.png"),
            "'/tmp/it'\\''s.png'"
        )
    }
}
