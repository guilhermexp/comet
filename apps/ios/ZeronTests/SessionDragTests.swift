import XCTest
@testable import Zeron

final class SessionDragTests: XCTestCase {
    func testGroupedProjectChatCanStartDragWithoutMakingItsGroupADestination() {
        // Same policy used by itemsForBeginning: a real row id is sufficient.
        let items = SessionDragPolicy.items(id: "chat-in-project", movable: true, archived: false, query: "")
        XCTAssertEqual(items.count, 1)
        XCTAssertEqual(items.first?.localObject as? String, "chat-in-project")
        XCTAssertNil(SessionDragPolicy.destination("project:repo", query: "", sections: ["review"]))
        XCTAssertEqual(SessionDragPolicy.destination("pinned", query: "", sections: []), .pinned)
        XCTAssertEqual(SessionDragPolicy.destination("review", query: "", sections: ["review"]), .section("review"))
        XCTAssertEqual(SessionDragPolicy.destination("recent", query: "", sections: []), .recent)
    }

    func testSearchArchiveHeadersAndOtherListsCannotStartDrag() {
        XCTAssertTrue(SessionDragPolicy.items(id: "chat", movable: true, archived: false, query: "find").isEmpty)
        XCTAssertTrue(SessionDragPolicy.items(id: "chat", movable: true, archived: true, query: "").isEmpty)
        XCTAssertTrue(SessionDragPolicy.items(id: "chat", movable: false, archived: false, query: "").isEmpty)
        XCTAssertTrue(SessionDragPolicy.items(id: nil, movable: true, archived: false, query: "").isEmpty)
        XCTAssertNil(SessionDragPolicy.destination("pinned", query: "find", sections: []))
        XCTAssertEqual(SessionDragPolicy.items(id: "chat", movable: true, archived: false, query: "  ").count, 1)
    }
}
