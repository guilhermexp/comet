import Darwin
import Foundation
import XCTest
@testable import UnpeelNative

final class MobileListenerOwnershipTests: XCTestCase {
    private func scratch(_ label: String) throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("unpeel-mobile-listener-\(label)-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    func testKnownEndpointWaitsThenReclaimsTheSamePort() throws {
        let directory = try scratch("handoff")
        let portFile = directory.appendingPathComponent("server-port")
        let first = try XCTUnwrap(
            MobileRemoteServer.claimMobileListener(portFileURL: portFile)
        )
        XCTAssertEqual(
            try String(contentsOf: portFile, encoding: .utf8),
            "\(first.port)\n"
        )

        XCTAssertNil(MobileRemoteServer.claimMobileListener(portFileURL: portFile))
        close(first.descriptor)

        let replacement = try XCTUnwrap(
            MobileRemoteServer.claimMobileListener(portFileURL: portFile)
        )
        defer { close(replacement.descriptor) }
        XCTAssertEqual(replacement.port, first.port)
        XCTAssertEqual(
            try String(contentsOf: portFile, encoding: .utf8),
            "\(first.port)\n",
            "a handoff must never rewrite the paired Direct endpoint"
        )
    }

    func testHeadlessFallbackIsClaimedExactlyAndRepairsCanonicalFile() throws {
        let seedDirectory = try scratch("seed")
        let seedFile = seedDirectory.appendingPathComponent("server-port")
        let seed = try XCTUnwrap(
            MobileRemoteServer.claimMobileListener(portFileURL: seedFile)
        )
        let port = seed.port
        close(seed.descriptor)

        let directory = try scratch("headless-fallback")
        let portFile = directory.appendingPathComponent("server-port")
        try "corrupt\n".write(to: portFile, atomically: true, encoding: .utf8)
        try "\(port)\n".write(
            to: directory.appendingPathComponent("headless-server-port"),
            atomically: true,
            encoding: .utf8
        )

        let claimed = try XCTUnwrap(
            MobileRemoteServer.claimMobileListener(portFileURL: portFile)
        )
        defer { close(claimed.descriptor) }
        XCTAssertEqual(claimed.port, port)
        XCTAssertEqual(try String(contentsOf: portFile, encoding: .utf8), "\(port)\n")
        XCTAssertNil(MobileRemoteServer.claimMobileListener(portFileURL: portFile))
    }
}
