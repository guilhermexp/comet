import XCTest
@testable import UnpeelNative

final class ActivityMenuSessionsTests: XCTestCase {
    func testBlockersHaveTheirOwnSectionAndCannotAlsoBeWorkingOrFinished() {
        let working = session("working", status: .busy)
        let blocked = session("blocked", status: .attention)
        let finished = session("finished", status: .idle)
        let node = projectNode(sessions: [working, blocked, finished])

        let activity = ActivityMenuSessions(
            nodes: [node],
            allSessions: [working, blocked, finished],
            jobs: [working, blocked],
            finished: [blocked, finished]
        )

        XCTAssertEqual(activity.jobs.map(\.id), ["working"])
        XCTAssertEqual(activity.blockers.map(\.id), ["blocked"])
        XCTAssertEqual(activity.finished.map(\.id), ["finished"])
        XCTAssertEqual(activity.sectionCount, 3)
    }

    func testBlockerOrderFollowsTheProjectTreeAndDuplicateRowsAreRemoved() {
        let first = session("first", status: .attention)
        let second = session("second", status: .attention)
        let child = projectNode(id: "child", sessions: [second, first])
        let parent = projectNode(id: "parent", sessions: [first], worktrees: [child])

        let activity = ActivityMenuSessions(
            nodes: [parent],
            allSessions: [second, first],
            jobs: [],
            finished: []
        )

        XCTAssertEqual(activity.blockers.map(\.id), ["first", "second"])
        XCTAssertEqual(activity.sectionCount, 1)
    }

    func testOrphanBlockersAppendAfterTreeInLifecycleAndIDOrder() {
        let rendered = session("rendered", status: .attention, lifecycleAtMs: 10)
        let orphanZ = session("orphan-z", status: .attention, lifecycleAtMs: 300)
        let orphanA = session("orphan-a", status: .attention, lifecycleAtMs: 300)
        let orphanOld = session("orphan-old", status: .attention, lifecycleAtMs: 100)
        let orphanIdle = session("orphan-idle", status: .idle, lifecycleAtMs: 500)

        let activity = ActivityMenuSessions(
            nodes: [projectNode(sessions: [rendered])],
            // Deliberately unordered, including the rendered blocker again:
            // orphan ranking must not inherit Dictionary.Values iteration.
            allSessions: [orphanOld, rendered, orphanZ, orphanIdle, orphanA],
            jobs: [],
            finished: []
        )

        XCTAssertEqual(
            activity.blockers.map(\.id),
            ["rendered", "orphan-a", "orphan-z", "orphan-old"]
        )
    }

    private func session(
        _ id: String,
        status: SessionStatus,
        lifecycleAtMs: Int64 = 0
    ) -> SessionEntry {
        SessionEntry(
            id: id,
            projectID: "project",
            label: id,
            command: "codex",
            createdAt: 0,
            status: status,
            lifecycleAtMs: lifecycleAtMs
        )
    }

    private func projectNode(
        id: String = "project",
        sessions: [SessionEntry],
        worktrees: [ProjectNode] = []
    ) -> ProjectNode {
        ProjectNode(
            project: Project(
                id: id,
                name: id,
                path: "/tmp/\(id)",
                parentProjectID: nil,
                sortOrder: 0,
                isFolder: nil,
                worktreeBranch: nil,
                workspacesEnabled: nil,
                mcpBlocked: nil
            ),
            sessions: sessions,
            worktrees: worktrees
        )
    }
}
