import XCTest
@testable import UnpeelNative
import UnpeelShared

final class ProviderCapabilitiesTests: XCTestCase {
    func testDeclaredGatesMirrorGeneratedRuntimeCapabilities() throws {
        for runtime in UnpeelRuntimeCatalog.runtimes(for: .macos) {
            let command = try XCTUnwrap(runtime.commandAliases.first)
            XCTAssertEqual(
                ProviderCapabilities.canRestart(command: command),
                runtime.capabilities.contains(.resume),
                runtime.slug
            )
            XCTAssertEqual(
                ProviderCapabilities.canFork(command: command),
                runtime.capabilities.contains(.fork),
                runtime.slug
            )
            XCTAssertEqual(
                ProviderCapabilities.canAppendSystemContext(command: command),
                runtime.capabilities.contains(.appendSystemContext),
                runtime.slug
            )
            XCTAssertEqual(
                ProviderCapabilities.canNotifyWhenDone(command: command),
                runtime.capabilities.contains(.notifyWhenDone),
                runtime.slug
            )
        }
    }

    func testRestartOnlyForKnownAgentsAndBlankShells() {
        // Every CLI ResumeCommand can resume gets Restart.
        for command in [
            "claude --dangerously-skip-permissions", "cline", "codex", "amp",
            "gemini --yolo", "pi", "opencode", "cursor-agent",
            "grok --always-approve", "kimi --yolo", "kiro-cli --v3", "muse --yolo", "copilot",
        ] {
            XCTAssertTrue(ProviderCapabilities.canRestart(command: command), command)
        }
        // A blank terminal has no conversation to lose.
        XCTAssertTrue(ProviderCapabilities.canRestart(command: ""))
        // An unknown command would silently restart as a fresh conversation.
        XCTAssertFalse(ProviderCapabilities.canRestart(command: "my-custom-agent --serve"))
    }

    func testForkOnlyForNativeForkPrimitives() {
        XCTAssertTrue(ProviderCapabilities.canFork(command: "claude"))
        XCTAssertTrue(ProviderCapabilities.canFork(command: "codex"))
        XCTAssertFalse(ProviderCapabilities.canFork(command: "gemini --yolo"))
        XCTAssertFalse(ProviderCapabilities.canFork(command: "muse --yolo"))
        XCTAssertFalse(ProviderCapabilities.canFork(command: ""))
    }

    func testResumeAgentRequiresReturnedShellAndHostProtocolThree() {
        XCTAssertTrue(ProviderCapabilities.canResumeAgent(
            command: "claude", isLive: true, activeRuntimeID: nil,
            hostProtocolVersion: 3
        ))
        // An active runtime never exposes an action, even when it matches the
        // stable managed launch.
        XCTAssertFalse(ProviderCapabilities.canResumeAgent(
            command: "claude", isLive: true, activeRuntimeID: "claude",
            hostProtocolVersion: 3
        ))
        // A blank terminal remains presentation-only.
        XCTAssertFalse(ProviderCapabilities.canResumeAgent(
            command: "", isLive: true, activeRuntimeID: "claude",
            hostProtocolVersion: 3
        ))
        XCTAssertFalse(ProviderCapabilities.canResumeAgent(
            command: "claude", isLive: true, activeRuntimeID: "codex",
            hostProtocolVersion: 3
        ))
        XCTAssertFalse(ProviderCapabilities.canResumeAgent(
            command: "claude", isLive: false, activeRuntimeID: nil,
            hostProtocolVersion: 3
        ))
        XCTAssertFalse(ProviderCapabilities.canResumeAgent(
            command: "claude", isLive: true, activeRuntimeID: nil,
            runtimeLaunchPending: true,
            hostProtocolVersion: 3
        ))
        XCTAssertFalse(ProviderCapabilities.canResumeAgent(
            command: "claude", isLive: true, activeRuntimeID: nil,
            hostProtocolVersion: 2
        ))
    }

    func testSidebarResumePresentationMatchesRuntimeAndArchiveState() {
        func presentation(
            command: String = "claude",
            status: SessionStatus = .idle,
            activeRuntimeID: String? = nil,
            hostProtocolVersion: Int = 3,
            archived: Bool = false
        ) -> SessionRowResumePresentation {
            let session = SessionEntry(
                id: "session", projectID: "project", label: "Session",
                command: command, createdAt: 1, status: status,
                activeRuntimeID: activeRuntimeID,
                hostProtocolVersion: hostProtocolVersion
            )
            return sessionRowResumePresentation(
                session: session,
                isArchived: archived,
                canRestart: ProviderCapabilities.canRestart(command: command),
                canResumeAgent: ProviderCapabilities.canResumeAgent(
                    command: command,
                    isLive: session.isLive,
                    activeRuntimeID: activeRuntimeID,
                    hostProtocolVersion: hostProtocolVersion
                )
            )
        }

        XCTAssertEqual(
            presentation(activeRuntimeID: "claude"),
            .none,
            "an active runtime must not expose a destructive agent action"
        )
        XCTAssertEqual(presentation(), .resumeAgent)
        XCTAssertEqual(presentation(hostProtocolVersion: 2), .none)
        XCTAssertEqual(
            presentation(command: "custom-tool", status: .exited, archived: true),
            .restore
        )
        XCTAssertEqual(
            presentation(status: .exited, archived: true),
            .restoreAndResume
        )
    }

    func testAppendSystemContextMatchesRuntimeCatalog() {
        XCTAssertTrue(ProviderCapabilities.canAppendSystemContext(command: "claude"))
        XCTAssertTrue(ProviderCapabilities.canAppendSystemContext(command: "grok"))
        XCTAssertTrue(ProviderCapabilities.canAppendSystemContext(command: "codex"))
        XCTAssertTrue(ProviderCapabilities.canAppendSystemContext(command: "pi"))
        XCTAssertFalse(ProviderCapabilities.canAppendSystemContext(command: "amp"))
        XCTAssertFalse(ProviderCapabilities.canAppendSystemContext(command: "opencode"))
        // Muse only exposes replace-the-prompt settings and unstable append
        // env vars — no append-mode CLI flag to wire.
        XCTAssertFalse(ProviderCapabilities.canAppendSystemContext(command: "muse --yolo"))
    }

    func testNotifyWhenDoneRequiresLifecycleHooks() {
        for command in [
            "claude", "cline", "codex", "amp", "gemini", "opencode",
            "cursor-agent", "grok", "kimi", "kiro-cli --v3", "muse --yolo", "copilot",
        ] {
            XCTAssertTrue(ProviderCapabilities.canNotifyWhenDone(command: command), command)
        }
        // pi has no hooks; shells and unknown commands settle by output guess.
        XCTAssertFalse(ProviderCapabilities.canNotifyWhenDone(command: "pi"))
        XCTAssertFalse(ProviderCapabilities.canNotifyWhenDone(command: ""))
        XCTAssertFalse(ProviderCapabilities.canNotifyWhenDone(command: "htop"))
    }

    func testRemoteFormMirrorsTheGates() {
        let pi = ProviderCapabilities.remote(session: SessionEntry(
            id: "pi", projectID: "p", label: "Pi", command: "pi",
            createdAt: 1, status: .idle, activeRuntimeID: nil,
            hostProtocolVersion: 3
        ))
        XCTAssertFalse(pi.restart)
        XCTAssertNil(pi.restartAgent)
        XCTAssertEqual(pi.resumeAgent, true)
        XCTAssertFalse(pi.fork)
        XCTAssertTrue(pi.appendSystemContext)
        XCTAssertFalse(pi.notifyWhenDone)

        let unknown = ProviderCapabilities.remote(session: SessionEntry(
            id: "unknown", projectID: "p", label: "Unknown",
            command: "my-custom-agent", createdAt: 1, status: .idle,
            activeRuntimeID: nil, hostProtocolVersion: 3
        ))
        XCTAssertFalse(unknown.restart)
        XCTAssertNil(unknown.restartAgent)
        XCTAssertEqual(unknown.resumeAgent, false)
        XCTAssertFalse(unknown.fork)
        XCTAssertFalse(unknown.appendSystemContext)
        XCTAssertFalse(unknown.notifyWhenDone)

        let stoppedBlank = ProviderCapabilities.remote(session: SessionEntry(
            id: "blank", projectID: "p", label: "Terminal", command: "",
            createdAt: 1, status: .exited, hostProtocolVersion: 3
        ))
        XCTAssertTrue(stoppedBlank.restart)
        XCTAssertEqual(stoppedBlank.resumeAgent, false)

        let stoppedUnknown = ProviderCapabilities.remote(session: SessionEntry(
            id: "unknown-stopped", projectID: "p", label: "Unknown",
            command: "my-custom-agent", createdAt: 1, status: .exited,
            hostProtocolVersion: 3
        ))
        XCTAssertFalse(stoppedUnknown.restart)
        XCTAssertEqual(stoppedUnknown.resumeAgent, false)

        let starting = ProviderCapabilities.remote(session: SessionEntry(
            id: "starting", projectID: "p", label: "Claude", command: "claude",
            createdAt: 1, status: .starting, activeRuntimeID: nil,
            hostProtocolVersion: 3
        ))
        XCTAssertFalse(starting.restart)
        XCTAssertEqual(starting.resumeAgent, false)

        let active = ProviderCapabilities.remote(session: SessionEntry(
            id: "active", projectID: "p", label: "Claude", command: "claude",
            createdAt: 1, status: .busy, activeRuntimeID: "claude",
            hostProtocolVersion: 3
        ))
        XCTAssertNil(active.restartAgent)
        XCTAssertEqual(active.resumeAgent, false)
    }
}
