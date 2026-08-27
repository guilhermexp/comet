import XCTest
@testable import UnpeelNative

final class ComputerContainmentTests: XCTestCase {
    func testComputerUseDefaultsOff() {
        XCTAssertFalse(ExperimentalFeature.computerUse.defaultOn)
    }

    func testComputerUseRequiresBooleanDevelopmentBuildMarker() {
        XCTAssertFalse(UnpeelFeatureFlags.computerUseAvailable(infoDictionary: nil))
        XCTAssertFalse(UnpeelFeatureFlags.computerUseAvailable(infoDictionary: [:]))
        XCTAssertFalse(UnpeelFeatureFlags.computerUseAvailable(
            infoDictionary: ["UnpeelDevelopmentBuild": false]
        ))
        XCTAssertFalse(UnpeelFeatureFlags.computerUseAvailable(
            infoDictionary: ["UnpeelDevelopmentBuild": "true"]
        ))
        XCTAssertTrue(UnpeelFeatureFlags.computerUseAvailable(
            infoDictionary: ["UnpeelDevelopmentBuild": true]
        ))
    }

    func testProductionAvailabilityExcludesOnlyComputerUse() {
        XCTAssertFalse(UnpeelFeatureFlags.isAvailable(
            .computerUse, developmentBuild: false
        ))
        XCTAssertTrue(UnpeelFeatureFlags.isAvailable(
            .computerUse, developmentBuild: true
        ))

        for feature in ExperimentalFeature.all where feature != .computerUse {
            XCTAssertTrue(
                UnpeelFeatureFlags.isAvailable(feature, developmentBuild: false),
                "production unexpectedly hid \(feature.key)"
            )
        }
    }
}
