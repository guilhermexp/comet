import XCTest
@testable import UnpeelNative

final class SidebarGroupDropTests: XCTestCase {
    private func project(
        parentID: String?,
        isFolder: Bool?,
        branch: String?
    ) -> Project {
        Project(
            id: "project",
            name: "Project",
            path: "/tmp/project",
            parentProjectID: parentID,
            sortOrder: nil,
            isFolder: isFolder,
            worktreeBranch: branch,
            workspacesEnabled: nil,
            mcpBlocked: nil
        )
    }

    func testOnlyPlainChildGroupsAcceptSessionDrops() {
        XCTAssertTrue(
            project(parentID: "root", isFolder: true, branch: nil).acceptsSessionDrop
        )
        XCTAssertFalse(
            project(parentID: "root", isFolder: nil, branch: "feature").acceptsSessionDrop
        )
        XCTAssertFalse(
            project(parentID: nil, isFolder: true, branch: nil).acceptsSessionDrop
        )
        XCTAssertFalse(
            project(parentID: "root", isFolder: nil, branch: nil).acceptsSessionDrop
        )
    }

    @MainActor
    func testSessionDropHighlightClearsWithDragState() {
        let state = SidebarDragState()
        var commits = 0
        var cancels = 0
        state.beginSession(
            projectID: "root",
            sessionID: "session",
            pinned: false,
            commitReorder: { commits += 1 },
            cancelReorder: { cancels += 1 }
        )
        state.setSessionDropTarget("group", hovering: true)
        XCTAssertEqual(state.sessionDropTargetProjectID, "group")

        // An exit from a stale row must not clear the current target.
        state.setSessionDropTarget("other", hovering: false)
        XCTAssertEqual(state.sessionDropTargetProjectID, "group")

        state.end()
        XCTAssertNil(state.sessionDropTargetProjectID)
        XCTAssertNil(state.sessionDrag)
        XCTAssertEqual(commits, 0)
        XCTAssertEqual(cancels, 1)
    }

    @MainActor
    func testAcceptedSessionDropCommitsInsteadOfCancelling() {
        let state = SidebarDragState()
        var commits = 0
        var cancels = 0
        state.beginSession(
            projectID: "root",
            sessionID: "session",
            pinned: true,
            commitReorder: { commits += 1 },
            cancelReorder: { cancels += 1 }
        )

        state.finish()

        XCTAssertNil(state.sessionDrag)
        XCTAssertEqual(commits, 1)
        XCTAssertEqual(cancels, 0)
    }
}
