import ImageIO
import UniformTypeIdentifiers
import XCTest
@testable import UnpeelNative

final class MobileSessionControlTests: XCTestCase {
    func testAlignedLiveChunkEndWithholdsPartialEscapeSequence() {
        // "hello" + complete clear + a CSI cut before its final byte: the
        // chunk must end right after the complete sequence.
        let data = Data("hello\u{1B}[2J\u{1B}[1;3".utf8)
        XCTAssertEqual(MobileSessionControl.alignedLiveChunkEnd(data), 9)
    }

    func testAlignedLiveChunkEndKeepsCleanEnd() {
        let data = Data("hello\u{1B}[2Jworld\n".utf8)
        XCTAssertEqual(MobileSessionControl.alignedLiveChunkEnd(data), data.count)
    }

    func testAlignedLiveChunkEndWithholdsPartialUTF8Rune() {
        var data = Data("line\n".utf8)
        // "a" followed by a dangling UTF-8 lead byte; the safe boundary is
        // the newline.
        data.append(contentsOf: [0x61, 0xC3])
        XCTAssertEqual(MobileSessionControl.alignedLiveChunkEnd(data), 5)
    }

    func testAlignedLiveChunkEndNeverEmptiesChunk() {
        // Nothing but a partial CSI: withholding would empty the chunk, so
        // it is sent as-is (never stall).
        let data = Data("\u{1B}[1;3".utf8)
        XCTAssertEqual(MobileSessionControl.alignedLiveChunkEnd(data), data.count)
    }

    func testAlignedLiveChunkEndSingleByte() {
        XCTAssertEqual(MobileSessionControl.alignedLiveChunkEnd(Data([0x1B])), 1)
    }

    func testAlignedLiveChunkEndWithholdsPartialSequenceLongerThan128Bytes() {
        // A complete OSC that sets a long title, then a second OSC cut before
        // its terminator. The partial sequence spans far more than the old
        // 128-byte tail window, so a windowed scan would miss the boundary
        // and cut the chunk mid-OSC. The safe end is right after the first
        // complete OSC.
        var data = Data("hi".utf8)
        let firstOSC = "\u{1B}]0;" + String(repeating: "A", count: 400) + "\u{07}"
        data.append(Data(firstOSC.utf8))
        let safeEnd = data.count
        // Second OSC: opener + long payload, no ST/BEL terminator.
        data.append(Data(("\u{1B}]0;" + String(repeating: "B", count: 400)).utf8))
        XCTAssertEqual(MobileSessionControl.alignedLiveChunkEnd(data), safeEnd)
    }

    func testTailAlignmentHandlesEscapeIntermediateAndRestartedCSI() {
        let intermediate = Data("before\r\n\u{1B}(Bafter".utf8)
        let escape = try! XCTUnwrap(intermediate.firstIndex(of: 0x1B))
        XCTAssertEqual(
            MobileSessionControl.alignTailStart(
                in: intermediate[..<(escape + 2)],
                scanStart: 0,
                desiredStart: UInt64(escape + 2)
            ),
            UInt64(escape)
        )

        let restarted = Data("before\r\n\u{1B}[1;\u{1B}[31mred".utf8)
        let escapes = restarted.indices.filter { restarted[$0] == 0x1B }
        let second = escapes[1]
        XCTAssertEqual(
            MobileSessionControl.alignTailStart(
                in: restarted[..<(second + 3)],
                scanStart: 0,
                desiredStart: UInt64(second + 3)
            ),
            UInt64(second)
        )
    }

    func testOutputChunkRebasesStaleSparseCursorAtRetentionFloor() throws {
        let sessionID = "test-output-retention-\(UUID().uuidString.prefix(8))"
        let dir = LaunchConfig.appSessionsDir.appendingPathComponent(sessionID)
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let floor = 8_192
        let tail = Data("retained line\r\n".utf8)
        var sparse = Data(repeating: 0, count: floor)
        sparse.append(tail)
        try sparse.write(to: dir.appendingPathComponent("output.bin"))
        try JSONSerialization.data(withJSONObject: [
            "version": 1,
            "retained_from": floor,
        ]).write(to: dir.appendingPathComponent("output-retention.json"))

        let rebased = try MobileSessionControl.outputChunk(query: [
            "session_id": sessionID,
            "offset": "0",
            "limit": "\(tail.count)",
        ])
        XCTAssertEqual(rebased.offset, UInt64(floor))
        XCTAssertEqual(Data(base64Encoded: rebased.dataBase64), tail)
        XCTAssertTrue(rebased.truncated)

        let retained = try MobileSessionControl.outputChunk(query: [
            "session_id": sessionID,
            "offset": "\(floor)",
            "limit": "\(tail.count)",
        ])
        XCTAssertEqual(retained.offset, UInt64(floor))
        XCTAssertEqual(Data(base64Encoded: retained.dataBase64), tail)
        XCTAssertFalse(retained.truncated)
    }

    // MARK: - Browser artifacts

    /// Writes `bytes` as a screenshot under a throwaway session dir and returns
    /// (sessionID, cleanup). Uses the real appSessionsDir so the path math
    /// matches production exactly.
    private func makeScreenshot(name: String, bytes: Data) throws -> (String, () -> Void) {
        let sessionID = "test-gallery-\(UUID().uuidString.prefix(8))"
        let dir = LaunchConfig.appSessionsDir
            .appendingPathComponent(sessionID)
            .appendingPathComponent("artifacts")
            .appendingPathComponent("browser")
            .appendingPathComponent("screenshots")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try bytes.write(to: dir.appendingPathComponent(name))
        let sessionRoot = LaunchConfig.appSessionsDir.appendingPathComponent(sessionID)
        return (sessionID, { try? FileManager.default.removeItem(at: sessionRoot) })
    }

    func testBrowserArtifactsListsScreenshots() throws {
        let (sessionID, cleanup) = try makeScreenshot(name: "page.png", bytes: Data([0x89, 0x50, 0x4E, 0x47]))
        defer { cleanup() }

        let list = try MobileSessionControl.browserArtifacts(query: ["session_id": sessionID])
        XCTAssertEqual(list.sessionID, sessionID)
        XCTAssertEqual(list.artifacts.count, 1)
        XCTAssertEqual(list.artifacts.first?.name, "page.png")
        XCTAssertEqual(list.artifacts.first?.kind, "screenshots")
        XCTAssertEqual(list.artifacts.first?.size, 4)
    }

    func testBrowserArtifactChunkReassemblesAcrossOffsets() throws {
        // 450KB forces multiple chunks at the 200KB relay-safe cap.
        let original = Data((0 ..< 450_000).map { UInt8($0 % 251) })
        let (sessionID, cleanup) = try makeScreenshot(name: "big.png", bytes: original)
        defer { cleanup() }

        var assembled = Data()
        var guardIterations = 0
        while assembled.count < original.count, guardIterations < 100 {
            guardIterations += 1
            let chunk = try MobileSessionControl.browserArtifactChunk(query: [
                "session_id": sessionID,
                "kind": "screenshots",
                "name": "big.png",
                "offset": "\(assembled.count)",
            ])
            XCTAssertEqual(chunk.totalSize, UInt64(original.count))
            XCTAssertLessThanOrEqual(chunk.dataBase64.count, 300 * 1024) // stays under a relay frame
            let bytes = try XCTUnwrap(Data(base64Encoded: chunk.dataBase64))
            XCTAssertFalse(bytes.isEmpty, "chunk must make progress")
            assembled.append(bytes)
        }
        XCTAssertEqual(assembled, original)
    }

    func testBrowserArtifactChunkRejectsTraversal() throws {
        let (sessionID, cleanup) = try makeScreenshot(name: "page.png", bytes: Data([0x00]))
        defer { cleanup() }

        XCTAssertThrowsError(try MobileSessionControl.browserArtifactChunk(query: [
            "session_id": sessionID,
            "kind": "screenshots",
            "name": "../../manifest.json",
        ]))
        XCTAssertThrowsError(try MobileSessionControl.browserArtifactChunk(query: [
            "session_id": sessionID,
            "kind": "secrets",
            "name": "page.png",
        ]))
    }

    /// A deterministic-noise PNG: incompressible, so it's guaranteed to be
    /// far larger than the single-chunk threshold that gates thumbnailing.
    private func makeNoisePNG(width: Int, height: Int) throws -> Data {
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        var seed: UInt64 = 0x9E37_79B9_7F4A_7C15
        for i in 0 ..< pixels.count {
            seed = seed &* 6_364_136_223_846_793_005 &+ 1_442_695_040_888_963_407
            pixels[i] = UInt8(truncatingIfNeeded: seed >> 33)
        }
        let context = try XCTUnwrap(pixels.withUnsafeMutableBytes { buffer in
            CGContext(
                data: buffer.baseAddress,
                width: width,
                height: height,
                bitsPerComponent: 8,
                bytesPerRow: width * 4,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue
            )
        })
        let image = try XCTUnwrap(context.makeImage())
        let out = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(
            out, UTType.png.identifier as CFString, 1, nil
        ))
        CGImageDestinationAddImage(destination, image, nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        return out as Data
    }

    private func thumbsDir(_ sessionID: String) -> URL {
        LaunchConfig.appSessionsDir
            .appendingPathComponent(sessionID)
            .appendingPathComponent("artifacts")
            .appendingPathComponent("thumbs")
    }

    func testBrowserArtifactChunkMaxDimServesDownscaledJpeg() throws {
        let png = try makeNoisePNG(width: 1600, height: 1600)
        XCTAssertGreaterThan(png.count, 200 * 1024, "fixture must exceed the thumbnail threshold")
        let (sessionID, cleanup) = try makeScreenshot(name: "big.png", bytes: png)
        defer { cleanup() }

        let query = [
            "session_id": sessionID,
            "kind": "screenshots",
            "name": "big.png",
            "max_dim": "256",
        ]
        let chunk = try MobileSessionControl.browserArtifactChunk(query: query)
        XCTAssertEqual(chunk.contentType, "image/jpeg")
        XCTAssertLessThan(chunk.totalSize, UInt64(png.count))
        let bytes = try XCTUnwrap(Data(base64Encoded: chunk.dataBase64))
        XCTAssertEqual(UInt64(bytes.count), chunk.totalSize, "thumbnail should fit one chunk")
        let source = try XCTUnwrap(CGImageSourceCreateWithData(bytes as CFData, nil))
        let decoded = try XCTUnwrap(CGImageSourceCreateImageAtIndex(source, 0, nil))
        XCTAssertLessThanOrEqual(max(decoded.width, decoded.height), 256)

        // Repeated requests are stable and the derived bytes never create a
        // second Controller-selected path on disk.
        let again = try MobileSessionControl.browserArtifactChunk(query: query)
        XCTAssertEqual(again.totalSize, chunk.totalSize)
        XCTAssertEqual(again.dataBase64, chunk.dataBase64)
        XCTAssertFalse(FileManager.default.fileExists(atPath: thumbsDir(sessionID).path))

        // The original bytes are untouched and still served without max_dim.
        let full = try MobileSessionControl.browserArtifactChunk(query: [
            "session_id": sessionID, "kind": "screenshots", "name": "big.png",
        ])
        XCTAssertEqual(full.contentType, "image/png")
        XCTAssertEqual(full.totalSize, UInt64(png.count))
    }

    func testBrowserArtifactChunkMaxDimSmallFileServesOriginal() throws {
        // At or under one chunk there's nothing to save — the original is one
        // round-trip already, so no thumbnail is generated.
        let png = Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
        let (sessionID, cleanup) = try makeScreenshot(name: "small.png", bytes: png)
        defer { cleanup() }

        let chunk = try MobileSessionControl.browserArtifactChunk(query: [
            "session_id": sessionID,
            "kind": "screenshots",
            "name": "small.png",
            "max_dim": "256",
        ])
        XCTAssertEqual(chunk.contentType, "image/png")
        XCTAssertEqual(Data(base64Encoded: chunk.dataBase64), png)
        XCTAssertFalse(FileManager.default.fileExists(atPath: thumbsDir(sessionID).path))
    }

    func testDeleteArtifactReapsCachedThumbnails() throws {
        let (sessionID, cleanup) = try makeScreenshot(name: "big.png", bytes: Data([0x89]))
        defer { cleanup() }

        // Older builds cached thumbnails on disk. Keep deletion compatible so
        // an upgrade can reap those legacy siblings along with the original.
        let legacyThumbs = thumbsDir(sessionID)
        try FileManager.default.createDirectory(at: legacyThumbs, withIntermediateDirectories: true)
        try Data([0xFF, 0xD8, 0xFF]).write(
            to: legacyThumbs.appendingPathComponent("1-256-screenshots-big.png.jpg")
        )
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: thumbsDir(sessionID).path).count, 1)

        _ = try MobileSessionControl.deleteArtifact(query: [
            "session_id": sessionID, "kind": "screenshots", "name": "big.png",
        ])
        XCTAssertEqual(
            (try? FileManager.default.contentsOfDirectory(atPath: thumbsDir(sessionID).path))?.count ?? 0,
            0
        )
    }

    func testAlignedLiveChunkEndNoFalseBoundaryInsideLongCSIParams() {
        // A single incomplete CSI with a >128B parameter run. The old
        // 128-byte tail scan would begin mid-params in ground state and,
        // finding no ESC/newline in that window, treat the window's start
        // (which is inside the CSI) as the "safe" end — cutting the chunk
        // mid-sequence. A full scan from the start knows we entered a CSI at
        // the ESC after "ok\n" and never left it, so the only safe boundary
        // is the newline.
        var data = Data("ok\n".utf8)
        let boundary = data.count
        // ESC [ then 300 semicolon-separated params, no final byte.
        data.append(Data(("\u{1B}[" + String(repeating: "1;", count: 300)).utf8))
        XCTAssertEqual(MobileSessionControl.alignedLiveChunkEnd(data), boundary)
    }
}
