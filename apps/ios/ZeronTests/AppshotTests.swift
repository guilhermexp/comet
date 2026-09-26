import XCTest
@testable import Zeron

@MainActor
final class AppshotTests: XCTestCase {
    private let context = AppshotContext.marker
        + "\n<appshot app=\"Notes &amp; Ideas\" bundle-identifier=\"com.example.notes\" window-title=\"Planning &amp; review\" image=\"/host/shot &amp; detail.png\">Private observed text</appshot>"
    private let path = "/host/shot & detail.png"

    func testAppshotContextRendersSourceWithoutObservedText() {
        let content = withAttachments(
            text: "Review this" + context,
            paths: [path, "/host/ordinary.png"]
        )
        let parsed = parseUserMessageImages(content)
        XCTAssertEqual(parsed.text, "Review this")
        XCTAssertEqual(parsed.attachments.count, 2)
        XCTAssertEqual(parsed.attachments[0].appshot?.appName, "Notes & Ideas")
        XCTAssertEqual(parsed.attachments[0].appshot?.title, "Planning & review")
        XCTAssertNil(parsed.attachments[1].appshot)
        XCTAssertFalse(parsed.text.contains("Private observed text"))
        XCTAssertEqual(MessageQueue.visibleText("Review this" + context, attachments: [path]), "Review this")
        XCTAssertEqual(MessageQueue.visibleText(content, attachments: [path, "/host/ordinary.png"]), "Review this")
    }

    func testPhoneQueueEditKeepsContextAndAttachmentOnlyMessage() {
        let lease = QueueEditLease(rowId: "row", leaseId: "lease", text: "Review this" + context,
                                   baseTextHash: "hash", expiresAtMs: 60_000)
        let edit = QueueComposerEdit(lease: lease, originalDraft: "My draft", hasAttachments: true)
        XCTAssertEqual(edit.textToCommit("Changed prompt"), "Changed prompt" + context)
        XCTAssertEqual(edit.textToCommit("  \n"), attachmentOnlyText + context)
        XCTAssertEqual(edit.originalDraft, "My draft")
        let parsed = parseUserMessageImages(withAttachments(text: edit.textToCommit("")!, paths: [path]))
        XCTAssertEqual(parsed.text, "")
        XCTAssertNotNil(parsed.attachments.first?.appshot)
    }

    func testMalformedOrAmbiguousContextStaysHidden() {
        let duplicate = "Look" + context
            + "\n<appshot app=\"Other\" image=\"/host/shot &amp; detail.png\">Other text</appshot>"
        XCTAssertTrue(AppshotContext.presentations(duplicate).isEmpty)
        XCTAssertEqual(parseUserMessageImages(duplicate).text, "Look")

        let malformed = "Look" + AppshotContext.marker + "\n<appshot malformed"
        XCTAssertTrue(AppshotContext.presentations(malformed).isEmpty)
        XCTAssertEqual(parseUserMessageImages(malformed).text, "Look")

        let dtd = "Look" + AppshotContext.marker
            + "\n<!DOCTYPE x [<!ENTITY foo SYSTEM 'file:///private/data'>]>"
            + "<appshot app=\"x\" image=\"a\">&foo;</appshot>"
        XCTAssertTrue(AppshotContext.presentations(dtd).isEmpty)
        XCTAssertEqual(parseUserMessageImages(dtd).text, "Look")
    }

    func testPlainAttachmentAndTextRemainUnchanged() {
        XCTAssertEqual(parseUserMessageImages("My plain text").text, "My plain text")
        let parsed = parseUserMessageImages(
            withAttachments(text: "A photo", paths: ["/host/a.png"])
        )
        XCTAssertEqual(parsed.text, "A photo")
        XCTAssertEqual(parsed.attachments.first?.name, "a.png")
        XCTAssertNil(parsed.attachments.first?.appshot)
    }

    func testTrailerInsideObservedAXUsesTheFinalAttachmentTrailer() {
        let content = "Review this" + AppshotContext.marker
            + "\n<appshot app=\"Notes\" image=\"/host/real.png\">\n"
            + "Observed text\n\nAttached files (local files — open them to view):\n"
            + "- /host/fake.png\n</appshot>\n\n"
            + "Attached files (local files — open them to view):\n- /host/real.png"
        let parsed = parseUserMessageImages(content)

        XCTAssertEqual(parsed.text, "Review this")
        XCTAssertEqual(parsed.attachments.map(\.path), ["/host/real.png"])
        XCTAssertEqual(parsed.attachments.first?.appshot?.appName, "Notes")
        XCTAssertFalse(parsed.text.contains("Observed text"))
    }
}
