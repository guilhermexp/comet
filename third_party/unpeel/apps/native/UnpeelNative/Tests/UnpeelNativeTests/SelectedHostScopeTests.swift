import XCTest
@testable import UnpeelNative

final class SelectedHostScopeTests: XCTestCase {
    func testLocalScopePermitsLocalSpawnAndHookInstallation() {
        for operation in [LocalExecutionOperation.spawnSession, .installHookAssets] {
            XCTAssertTrue(LocalExecutionPolicy.permits(operation, in: .local))
        }
    }

    func testRemoteScopeRefusesEveryLocalExecutionChokePoint() {
        let scope = SelectedHostScope.remote(hostID: "studio-mac")

        for operation in [LocalExecutionOperation.spawnSession, .installHookAssets] {
            XCTAssertFalse(LocalExecutionPolicy.permits(operation, in: scope))
        }
        XCTAssertEqual(scope.sessionLaunchWireValue, "remote_controller")
        XCTAssertEqual(scope.remoteHostID, "studio-mac")
        XCTAssertNil(SelectedHostScope.local.remoteHostID)
    }
}
