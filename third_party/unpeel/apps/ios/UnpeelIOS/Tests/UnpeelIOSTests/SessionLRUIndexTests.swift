import XCTest
@testable import UnpeelIOS

/// Pure-logic tests for the terminal cache's LRU bookkeeping. Deliberately
/// no `TerminalSessionCache`/renderer/surface involvement — ghostty surfaces
/// cannot exist in headless test runs.
final class SessionLRUIndexTests: XCTestCase {
    func testInsertWithinCapacityEvictsNothing() {
        var index = SessionLRUIndex<String>(capacity: 3)

        XCTAssertTrue(index.insert("A", for: "a").isEmpty)
        XCTAssertTrue(index.insert("B", for: "b").isEmpty)
        XCTAssertTrue(index.insert("C", for: "c").isEmpty)
        XCTAssertEqual(index.count, 3)
        XCTAssertEqual(index.keys, ["a", "b", "c"])
    }

    func testInsertBeyondCapacityEvictsLeastRecentlyUsed() {
        var index = SessionLRUIndex<String>(capacity: 2)
        _ = index.insert("A", for: "a")
        _ = index.insert("B", for: "b")

        let evicted = index.insert("C", for: "c")

        XCTAssertEqual(evicted.map(\.id), ["a"])
        XCTAssertEqual(evicted.map(\.entry), ["A"])
        XCTAssertEqual(index.keys, ["b", "c"])
        XCTAssertNil(index.peek("a"))
    }

    func testLookupTouchesRecencySoEvictionSkipsIt() {
        var index = SessionLRUIndex<String>(capacity: 2)
        _ = index.insert("A", for: "a")
        _ = index.insert("B", for: "b")

        XCTAssertEqual(index.lookup("a"), "A")
        let evicted = index.insert("C", for: "c")

        // "b" became the oldest after the lookup touched "a".
        XCTAssertEqual(evicted.map(\.id), ["b"])
        XCTAssertEqual(index.keys, ["a", "c"])
    }

    func testLookupMissReturnsNilAndDoesNotDisturbOrder() {
        var index = SessionLRUIndex<String>(capacity: 2)
        _ = index.insert("A", for: "a")

        XCTAssertNil(index.lookup("missing"))
        XCTAssertEqual(index.keys, ["a"])
    }

    func testReinsertExistingKeyReplacesValueTouchesAndNeverSelfEvicts() {
        var index = SessionLRUIndex<String>(capacity: 2)
        _ = index.insert("A", for: "a")
        _ = index.insert("B", for: "b")

        let evicted = index.insert("A2", for: "a")

        XCTAssertTrue(evicted.isEmpty)
        XCTAssertEqual(index.count, 2)
        XCTAssertEqual(index.keys, ["b", "a"])
        XCTAssertEqual(index.peek("a"), "A2")
    }

    func testPeekDoesNotTouchRecency() {
        var index = SessionLRUIndex<String>(capacity: 2)
        _ = index.insert("A", for: "a")
        _ = index.insert("B", for: "b")

        XCTAssertEqual(index.peek("a"), "A")
        let evicted = index.insert("C", for: "c")

        // "a" stayed oldest despite the peek.
        XCTAssertEqual(evicted.map(\.id), ["a"])
    }

    func testRemoveReturnsEntryAndDropsIt() {
        var index = SessionLRUIndex<String>(capacity: 3)
        _ = index.insert("A", for: "a")
        _ = index.insert("B", for: "b")

        XCTAssertEqual(index.remove("a"), "A")
        XCTAssertNil(index.remove("a"))
        XCTAssertEqual(index.keys, ["b"])
    }

    func testRemoveAllReturnsEverythingInLRUOrder() {
        var index = SessionLRUIndex<String>(capacity: 3)
        _ = index.insert("A", for: "a")
        _ = index.insert("B", for: "b")
        _ = index.lookup("a")

        let removed = index.removeAll()

        XCTAssertEqual(removed.map(\.id), ["b", "a"])
        XCTAssertEqual(index.count, 0)
        XCTAssertTrue(index.keys.isEmpty)
    }

    func testRetainOnlyDropsMissingSessions() {
        var index = SessionLRUIndex<String>(capacity: 3)
        _ = index.insert("A", for: "a")
        _ = index.insert("B", for: "b")
        _ = index.insert("C", for: "c")

        let dropped = index.retain(only: ["a", "c"])

        XCTAssertEqual(dropped.map(\.id), ["b"])
        XCTAssertEqual(index.keys, ["a", "c"])
    }

    func testRetainOnlySparesTheKeptSessionEvenWhenMissing() {
        var index = SessionLRUIndex<String>(capacity: 3)
        _ = index.insert("A", for: "a")
        _ = index.insert("B", for: "b")

        // Transiently empty session list must not tear down the visible
        // session's terminal.
        let dropped = index.retain(only: [], keeping: "b")

        XCTAssertEqual(dropped.map(\.id), ["a"])
        XCTAssertEqual(index.keys, ["b"])
    }

    func testRemoveAllExceptKeepsOnlyTheGivenSession() {
        var index = SessionLRUIndex<String>(capacity: 3)
        _ = index.insert("A", for: "a")
        _ = index.insert("B", for: "b")
        _ = index.insert("C", for: "c")

        let dropped = index.removeAll(except: "b")

        XCTAssertEqual(Set(dropped.map(\.id)), ["a", "c"])
        XCTAssertEqual(index.keys, ["b"])
    }

    func testRemoveAllExceptNilDropsEverything() {
        var index = SessionLRUIndex<String>(capacity: 3)
        _ = index.insert("A", for: "a")
        _ = index.insert("B", for: "b")

        let dropped = index.removeAll(except: nil)

        XCTAssertEqual(dropped.map(\.id), ["a", "b"])
        XCTAssertEqual(index.count, 0)
    }

    func testCapacityIsClampedToAtLeastOne() {
        var index = SessionLRUIndex<String>(capacity: 0)

        XCTAssertTrue(index.insert("A", for: "a").isEmpty)
        let evicted = index.insert("B", for: "b")

        XCTAssertEqual(evicted.map(\.id), ["a"])
        XCTAssertEqual(index.keys, ["b"])
    }
}
