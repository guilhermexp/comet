import XCTest
@testable import UnpeelNative

@MainActor
final class SidebarSessionOrderTests: XCTestCase {
    private func pin(_ id: String, at: UInt64) -> PinnedSidebarSession {
        PinnedSidebarSession(
            key: PinnedSidebarSession.key(forSessionID: id),
            projectID: "project",
            sessionID: id,
            pinnedAt: at
        )
    }

    func testSharedPinRanksWinAndUnrankedPinsKeepBaseOrder() {
        let base = [pin("a", at: 1), pin("b", at: 2), pin("c", at: 3)]

        let ordered = UnpeelStore.orderedPinnedSessions(
            base,
            sharedOrder: ["regular", "c", "a"],
            localOrder: ["b", "a", "c"]
        )

        XCTAssertEqual(ordered.compactMap(\.sessionID), ["c", "a", "b"])
    }

    func testLegacyPinOrderRemainsFallbackWhenSharedListHasNoPins() {
        let base = [pin("a", at: 1), pin("b", at: 2), pin("c", at: 3)]

        let ordered = UnpeelStore.orderedPinnedSessions(
            base,
            sharedOrder: ["regular-a", "regular-b"],
            localOrder: ["b", "a"]
        )

        XCTAssertEqual(ordered.compactMap(\.sessionID), ["b", "a", "c"])
    }

    func testCombinedOrderPreservesBothBucketsWithoutDuplicates() {
        XCTAssertEqual(
            UnpeelStore.combinedSessionOrder(
                pinnedIDs: ["pin-b", "pin-a"],
                regularIDs: ["regular-b", "pin-a", "regular-a"]
            ),
            ["pin-b", "pin-a", "regular-b", "regular-a"]
        )
    }

    func testRestartReplacementKeepsTheSameSharedRank() {
        XCTAssertEqual(
            UnpeelStore.replacingSessionID(
                in: ["a", "old", "c"], oldID: "old", newID: "new"
            ),
            ["a", "new", "c"]
        )
        XCTAssertNil(
            UnpeelStore.replacingSessionID(
                in: ["a", "c"], oldID: "old", newID: "new"
            )
        )
    }

    func testRecentSortOnlyPrivilegesWorkingThenUsesLifecycleAndStableID() {
        func session(
            _ id: String, status: SessionStatus, created: Int64, lifecycle: Int64
        ) -> SessionEntry {
            SessionEntry(
                id: id,
                projectID: "project",
                label: id,
                command: "codex",
                createdAt: created,
                status: status,
                lifecycleAtMs: lifecycle
            )
        }
        let rows = [
            session("idle-newest", status: .idle, created: 10, lifecycle: 900),
            session("attention", status: .attention, created: 20, lifecycle: 800),
            session("exited", status: .exited, created: 30, lifecycle: 700),
            session("busy", status: .busy, created: 40, lifecycle: 100),
            session("restart", status: .exited, created: 50, lifecycle: 90),
            session("tie-b", status: .idle, created: 60, lifecycle: 600),
            session("tie-a", status: .idle, created: 60, lifecycle: 600),
        ]

        let ordered = UnpeelStore.sessionsSortedByRecentActivity(
            rows,
            restartingSessionIDs: ["restart"]
        )

        XCTAssertEqual(
            ordered.map(\.id),
            ["busy", "restart", "idle-newest", "attention", "exited", "tie-a", "tie-b"]
        )
    }

    func testLifecycleStampIgnoresRepaintFallbackForHookCapableAgent() {
        XCTAssertEqual(
            UnpeelStore.resolvedLifecycleAtMs(
                createdAtMs: 100,
                command: "codex --full-auto",
                hookEventAtMs: nil,
                screenChangedAtMs: 900,
                outputAtMs: 1_000,
                finalExitedAtMs: nil
            ),
            100,
            "a hook-capable session with no hook event stays at creation"
        )
        XCTAssertEqual(
            UnpeelStore.resolvedLifecycleAtMs(
                createdAtMs: 100,
                command: "codex --full-auto",
                hookEventAtMs: 400,
                screenChangedAtMs: 900,
                outputAtMs: 1_000,
                finalExitedAtMs: nil
            ),
            400,
            "the durable hook seed is authoritative even when output repaints later"
        )
    }

    func testHooklessLifecyclePrefersScreenThenOutputAndExitedFinalStamp() {
        XCTAssertEqual(
            UnpeelStore.resolvedLifecycleAtMs(
                createdAtMs: 100,
                command: "pi",
                hookEventAtMs: nil,
                screenChangedAtMs: 300,
                outputAtMs: 900,
                finalExitedAtMs: nil
            ),
            300
        )
        XCTAssertEqual(
            UnpeelStore.resolvedLifecycleAtMs(
                createdAtMs: 100,
                command: "pi",
                hookEventAtMs: nil,
                screenChangedAtMs: nil,
                outputAtMs: 900,
                finalExitedAtMs: nil
            ),
            900
        )
        XCTAssertEqual(
            UnpeelStore.resolvedLifecycleAtMs(
                createdAtMs: 100,
                command: "pi",
                hookEventAtMs: nil,
                screenChangedAtMs: 300,
                outputAtMs: 900,
                finalExitedAtMs: 1_200
            ),
            1_200
        )
    }

    func testRowAgeAndCleanupAnchorUseLifecycleStamp() {
        let session = SessionEntry(
            id: "recent",
            projectID: "project",
            label: "Recent",
            command: "codex",
            createdAt: 1_000,
            status: .idle,
            lifecycleAtMs: 121_000
        )
        let now = Date(timeIntervalSince1970: 181)

        XCTAssertEqual(session.ageString(now: now), "3m")
        XCTAssertEqual(session.ageString(since: session.lifecycleAtMs, now: now), "1m")
        XCTAssertEqual(UnpeelStore.inactivityAnchorMs(session), 121_000)
    }
}
