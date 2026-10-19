import UIKit
import XCTest
@testable import Zeron

/// Stands in for the Rust worker: hands the test the real `VisualSink` the
/// view attaches, so displays and frames take the production path.
private final class FakeVisualSession: VisualSession, @unchecked Sendable {
    var listener: VisualListener?
    let attached = XCTestExpectation(description: "visual attached")

    init() { super.init(noHandle: VisualSession.NoHandle()) }
    required init(unsafeFromHandle handle: UInt64) { super.init(unsafeFromHandle: handle) }

    override func title() -> String { "Fake visual" }
    override func attach(listener: VisualListener, dark: Bool, reduceMotion: Bool) {
        self.listener = listener
        attached.fulfill()
    }
    override func detach() {}
    override func setAppearance(dark: Bool) {}
    override func activate(control: UInt64) {}
    override func pointer(surface: UInt64, phase: PointerPhase, x: Float, y: Float) {}
    override func slide(control: UInt64, fraction: Float) {}
}

final class VisualRasterTests: XCTestCase {
    private let empty = VisualPaint(text: "", runs: [], shapes: [])

    func testPixelSizeIsCheckedBeforeConversionOrAllocation() {
        func pixels(_ w: CGFloat, _ h: CGFloat, _ scale: CGFloat) -> Result<(width: Int, height: Int)?, VisualRasterError> {
            VisualPainter.pixelSize(CGSize(width: w, height: h), scale: scale)
        }
        // 390 points at 3x is 1170 px wide; 16 Mi px allow 14,339 px tall.
        XCTAssertEqual(try pixels(390, 4_779, 3).get()?.width, 1_170)
        XCTAssertEqual(try pixels(390, 4_779, 3).get()?.height, 14_337)
        XCTAssertEqual(pixels(390, 4_781, 3).error, .tooLarge)
        // The reported column: over 200,000 points tall (2.8 GB at 3x).
        XCTAssertEqual(pixels(390, 200_001, 3).error, .tooLarge)
        // Per side first, then values Int(_:) would trap on.
        XCTAssertEqual(pixels(100, 8_200, 2).error, .tooLarge)
        XCTAssertEqual(pixels(1e30, 1e30, 3).error, .tooLarge)
        XCTAssertEqual(pixels(.greatestFiniteMagnitude, 10, 3).error, .tooLarge)
        for (w, h, s) in [(CGFloat.nan, 10, 2), (10, .infinity, 2), (-1, 10, 2), (10, 10, 0), (10, 10, .nan)] as [(CGFloat, CGFloat, CGFloat)] {
            XCTAssertEqual(pixels(w, h, s).error, .invalidSize, "\(w)x\(h)@\(s)")
        }
        // Nothing to draw is not an error.
        XCTAssertNil(try pixels(0, 0, 3).get())
        XCTAssertNil(try pixels(390, 0, 3).get())
    }

    func testRenderReportsTheLimitInsteadOfAnEmptyImage() throws {
        let tall = VisualPainter.render(empty, size: CGSize(width: 390, height: 200_001), scale: 3, fonts: StyleFonts())
        XCTAssertEqual(tall.error, .tooLarge)
        let image = try XCTUnwrap(VisualPainter.render(empty, size: CGSize(width: 390, height: 120), scale: 3, fonts: StyleFonts()).get())
        XCTAssertEqual(image.width, 1_170)
        XCTAssertEqual(image.height, 360)
        XCTAssertNil(try VisualPainter.render(empty, size: .zero, scale: 3, fonts: StyleFonts()).get())
    }

    func testOversizedDisplayShowsAnErrorAndDropsStaleSurfacesAndControls() throws {
        let session = FakeVisualSession()
        let view = VisualView(session: session, scale: 3)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 390, height: 800))
        window.addSubview(view)
        window.isHidden = false
        view.frame = CGRect(x: 0, y: 0, width: 390, height: 400)
        wait(for: [session.attached], timeout: 5)
        let sink = try XCTUnwrap(session.listener)

        let surface = VisualSurface(id: 7, x: 0, y: 40, w: 390, h: 120, gesture: .tap)
        let control = VisualControl(id: 9, kind: .button(primary: true), x: 0, y: 0, w: 80, h: 32, label: "Go")
        func display(height: Float, revision: UInt64) -> VisualDisplay {
            VisualDisplay(revision: revision, width: 390, height: height, dark: false, paint: empty,
                          surfaces: [surface], controls: [control], styles: [])
        }
        func settle() {
            let flushed = expectation(description: "main queue")
            DispatchQueue.main.async { flushed.fulfill() }
            wait(for: [flushed], timeout: 5)
        }

        sink.display(display: display(height: 400, revision: 1))
        settle()
        XCTAssertNil(view.failure)
        XCTAssertEqual(view.surfaceCount, 1)
        XCTAssertEqual(view.controlCount, 1)
        XCTAssertTrue(view.hasStaticImage)

        // Within node/text limits but past the bitmap budget: a visible error,
        // and nothing left that would take stale frames or touches.
        sink.display(display: display(height: 200_001, revision: 2))
        settle()
        XCTAssertEqual(view.failure, .tooLarge)
        XCTAssertEqual(view.surfaceCount, 0)
        XCTAssertEqual(view.controlCount, 0)
        XCTAssertFalse(view.hasStaticImage)
        let label = try XCTUnwrap(view.accessibilityElements?.first as? UILabel)
        XCTAssertEqual(label.text, VisualRasterError.tooLarge.message)
        view.layoutIfNeeded()
        XCTAssertGreaterThan(label.frame.height, 0)
        XCTAssertLessThan(label.frame.height, 200)
        // A frame for the dropped surface is ignored, not drawn.
        sink.frame(frame: SurfaceFrame(surface: 7, image: nil, paint: empty, styles: []))
        settle()
        XCTAssertEqual(view.surfaceCount, 0)

        // The next drawable display recovers in place.
        sink.display(display: display(height: 400, revision: 3))
        settle()
        XCTAssertNil(view.failure)
        XCTAssertEqual(view.surfaceCount, 1)
        XCTAssertEqual(view.controlCount, 1)
        XCTAssertFalse(view.subviews.contains { $0 is UILabel })
        window.isHidden = true
    }

    func testDuplicateSurfaceIdsNeverTrap() {
        let session = FakeVisualSession()
        let view = VisualView(session: session, scale: 2)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 390, height: 800))
        window.addSubview(view)
        window.isHidden = false
        wait(for: [session.attached], timeout: 5)
        let twin = VisualSurface(id: 1, x: 0, y: 0, w: 390, h: 60, gesture: .none)
        // Rust refuses these; the sink must still not crash on them.
        session.listener?.display(display: VisualDisplay(
            revision: 1, width: 390, height: 120, dark: false, paint: empty,
            surfaces: [twin, twin], controls: [], styles: []
        ))
        let flushed = expectation(description: "main queue")
        DispatchQueue.main.async { flushed.fulfill() }
        wait(for: [flushed], timeout: 5)
        XCTAssertEqual(view.surfaceCount, 1)
        window.isHidden = true
    }
}

private extension Result {
    var error: Failure? {
        if case let .failure(error) = self { return error }
        return nil
    }
}
