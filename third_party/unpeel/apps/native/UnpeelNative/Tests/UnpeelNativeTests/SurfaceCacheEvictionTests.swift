import XCTest
@testable import UnpeelNative

final class SurfaceCacheEvictionTests: XCTestCase {
    func testReclaimedPaneRejectsItsStaleDeferredTeardown() {
        var evictions = DeferredSurfaceEvictions<String>()
        let staleToken = evictions.schedule("original-pane", for: "session")

        XCTAssertTrue(evictions.contains("session"))
        XCTAssertEqual(evictions.reclaim("session"), "original-pane")
        XCTAssertFalse(evictions.contains("session"))
        XCTAssertNil(evictions.take("session", token: staleToken))
    }

    func testRescheduledPaneCanOnlyBeClaimedByItsLatestToken() {
        var evictions = DeferredSurfaceEvictions<String>()
        let staleToken = evictions.schedule("first-pane", for: "session")
        XCTAssertEqual(evictions.reclaim("session"), "first-pane")

        let currentToken = evictions.schedule("same-pane", for: "session")

        XCTAssertNil(evictions.take("session", token: staleToken))
        XCTAssertEqual(
            evictions.take("session", token: currentToken),
            "same-pane"
        )
        XCTAssertFalse(evictions.contains("session"))
    }
}
