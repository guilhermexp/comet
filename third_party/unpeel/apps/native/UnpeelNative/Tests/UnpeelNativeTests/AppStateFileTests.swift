import XCTest
@testable import UnpeelNative

final class AppStateFileTests: XCTestCase {
    func testDecodesPinnedSessionsByProjectMap() throws {
        let raw = Data("""
        {
          "projects": [{ "id": "p1", "name": "Project", "path": "/tmp/project" }],
          "pinned_sessions": {
            "p1": [
              {
                "key": "session:s1",
                "project_id": "p1",
                "session_id": "s1",
                "pinned_at": 10
              }
            ]
          },
          "presets": []
        }
        """.utf8)

        let state = try JSONDecoder().decode(AppStateFile.self, from: raw)

        XCTAssertEqual(state.pinnedSessions["p1"]?.compactMap(\.sessionID), ["s1"])
    }

    func testDecodesLegacyFlatPinnedSessionsArray() throws {
        let raw = Data("""
        {
          "projects": [{ "id": "p1", "name": "Project", "path": "/tmp/project" }],
          "pinned_sessions": [
            {
              "key": "session:s1",
              "project_id": "p1",
              "session_id": "s1",
              "pinned_at": 10
            }
          ],
          "presets": []
        }
        """.utf8)

        let state = try JSONDecoder().decode(AppStateFile.self, from: raw)

        XCTAssertEqual(state.pinnedSessions["p1"]?.compactMap(\.sessionID), ["s1"])
    }

    func testDecodesMcpGrantsObjectAndLegacyStringForms() throws {
        let raw = Data("""
        {
          "projects": [{ "id": "p1", "name": "Project", "path": "/tmp/project" }],
          "presets": [],
          "mcp_orchestrators": {
            "legacy-global": "global",
            "legacy-project": "project",
            "reader": { "role": "read", "reach": "project" },
            "writer": { "role": "write", "reach": "project" }
          }
        }
        """.utf8)

        let state = try JSONDecoder().decode(AppStateFile.self, from: raw)
        let grants = state.mcpOrchestrators

        // Legacy bare strings decode to the write role at project reach.
        XCTAssertEqual(grants["legacy-global"], McpGrant(role: .write, reach: .project))
        XCTAssertEqual(grants["legacy-project"], McpGrant(role: .write, reach: .project))
        // New object form maps role → access level (reach is always project).
        XCTAssertEqual(grants["reader"]?.accessLevel, .read)
        XCTAssertEqual(grants["writer"]?.accessLevel, .readWrite)
    }

    func testDecodesMcpDefaultAccessWithFallback() throws {
        let withDefault = Data("""
        {
          "projects": [{ "id": "p1", "name": "Project", "path": "/tmp/project" }],
          "presets": [],
          "mcp_default_access": { "role": "write", "reach": "project" }
        }
        """.utf8)
        let state = try JSONDecoder().decode(AppStateFile.self, from: withDefault)
        XCTAssertEqual(state.mcpDefaultAccess.accessLevel, .readWrite)

        // Absent ⇒ the Read/project default.
        let withoutDefault = Data("""
        { "projects": [], "presets": [] }
        """.utf8)
        let fallback = try JSONDecoder().decode(AppStateFile.self, from: withoutDefault)
        XCTAssertEqual(fallback.mcpDefaultAccess, .default)
        XCTAssertEqual(fallback.mcpDefaultAccess.accessLevel, .read)
    }

    func testDecodesBrowserDefaultAccessWithFallback() throws {
        let raw = Data("""
        {
          "projects": [],
          "presets": [],
          "browser_default_access": "off"
        }
        """.utf8)
        let state = try JSONDecoder().decode(AppStateFile.self, from: raw)
        XCTAssertEqual(state.browserDefaultAccess, .off)

        // Absent ⇒ on (the shipped default; Settings Off is the master disable).
        let absent = try JSONDecoder().decode(
            AppStateFile.self,
            from: Data("{ \"projects\": [], \"presets\": [] }".utf8)
        )
        XCTAssertEqual(absent.browserDefaultAccess, .on)
    }

    func testDecodesBrowserScreenshotGalleryPreferenceWithFallback() throws {
        let disabled = try JSONDecoder().decode(
            AppStateFile.self,
            from: Data("""
            {
              "projects": [],
              "presets": [],
              "mcp_auto_add_browser_screenshots": false
            }
            """.utf8)
        )
        XCTAssertFalse(disabled.mcpAutoAddBrowserScreenshots)

        // Absent preserves the original behavior: screenshots appear in the gallery.
        let absent = try JSONDecoder().decode(
            AppStateFile.self,
            from: Data("{ \"projects\": [], \"presets\": [] }".utf8)
        )
        XCTAssertTrue(absent.mcpAutoAddBrowserScreenshots)
    }

    func testDecodesTranscriptSettingsWithFallback() throws {
        let raw = Data("""
        {
          "projects": [],
          "presets": [],
          "transcript_settings": {
            "include_user": false,
            "include_reasoning": true,
            "include_tools": true,
            "max_entries": 50
          }
        }
        """.utf8)
        let state = try JSONDecoder().decode(AppStateFile.self, from: raw)
        XCTAssertFalse(state.transcriptSettings.includeUser)
        XCTAssertTrue(state.transcriptSettings.includeReasoning)
        XCTAssertTrue(state.transcriptSettings.includeTools)
        XCTAssertEqual(state.transcriptSettings.maxEntries, 50)
        // Unspecified fields keep their defaults.
        XCTAssertTrue(state.transcriptSettings.includeAssistant)
        XCTAssertTrue(state.transcriptSettings.includeFileChanges)

        // Absent ⇒ shipped defaults.
        let absent = try JSONDecoder().decode(
            AppStateFile.self,
            from: Data("{ \"projects\": [], \"presets\": [] }".utf8)
        )
        XCTAssertEqual(absent.transcriptSettings, TranscriptSettings())
    }

    func testSessionAccessLevelMapsToCanonicalGrant() {
        XCTAssertEqual(SessionAccessLevel.read.grant, McpGrant(role: .read, reach: .project))
        XCTAssertEqual(SessionAccessLevel.readWrite.grant, McpGrant(role: .write, reach: .project))
    }
}
