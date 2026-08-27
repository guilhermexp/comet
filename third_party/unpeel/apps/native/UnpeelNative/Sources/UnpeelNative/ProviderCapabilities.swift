import Foundation
import UnpeelShared

/// The single source of truth for which session verbs a CLI supports —
/// Resume, Resume Agent, Fork, Append system context, Notify when done. The
/// desktop
/// context menu reads it directly (`UnpeelStore.sessionCan*`); the phone's
/// session sheet gets the same answers as `RemoteSessionCapabilities` on
/// every session summary, so the phone never parses commands itself and an
/// old phone against a new Mac (or vice versa) degrades gracefully.
///
/// Declared provider support comes from `runtimes/*/runtime.toml`. Runtime
/// recipes and the final per-Session decision remain Host-owned.
enum ProviderCapabilities {
    /// First hosted-PTY protocol that implements shell-only, generation-bound
    /// Resume Agent.
    static let resumeAgentHostProtocolVersion = 3

    /// Resume is offered for a stopped Session when relaunching CONTINUES the
    /// conversation
    /// (`ResumeCommand` knows the CLI's resume flags) or when there is no
    /// conversation to lose (a blank-terminal shell). An unknown non-empty
    /// command would silently start a fresh conversation, so it gets no
    /// Resume verb. The historical name stays aligned with the legacy
    /// `session.restart` wire operation; callers also gate on stopped state.
    static func canRestart(command: String) -> Bool {
        let trimmed = command.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty { return true }
        return SetupTool.detect(in: trimmed)?.metadata?.capabilities.contains(.resume) == true
    }

    /// Resume Agent applies only after a stable managed launch has exited to
    /// the shell in a still-live terminal. An active runtime — including the
    /// matching managed one — never exposes an action, and a passively
    /// observed agent in a blank terminal never acquires this capability.
    static func canResumeAgent(
        command: String,
        isLive: Bool,
        activeRuntimeID: String?,
        runtimeLaunchPending: Bool = false,
        hostProtocolVersion: Int?
    ) -> Bool {
        guard isLive,
              activeRuntimeID == nil,
              !runtimeLaunchPending,
              (hostProtocolVersion ?? 0) >= resumeAgentHostProtocolVersion,
              let launch = SetupTool.detect(in: command),
              launch.metadata?.capabilities.contains(.restartAgent) == true
        else { return false }
        return true
    }

    /// Only CLIs whose runtime package declares a native fork primitive.
    static func canFork(command: String) -> Bool {
        SetupTool.detect(in: command)?.metadata?.capabilities.contains(.fork) == true
    }

    /// Runtime package has an append-mode system-context adapter.
    static func canAppendSystemContext(command: String) -> Bool {
        SetupTool.detect(in: command)?.metadata?.capabilities.contains(.appendSystemContext) == true
    }

    /// "Notify when done" fires on the hook Stop event; without lifecycle
    /// hooks (pi, plain shells, unknown commands) "done" is an
    /// output-settling guess, so the verb is not offered.
    static func canNotifyWhenDone(command: String) -> Bool {
        SetupTool.detect(in: command)?.metadata?.capabilities.contains(.notifyWhenDone) == true
    }

    /// The wire form shipped to paired phones on each session summary.
    static func remote(session: SessionEntry) -> RemoteSessionCapabilities {
        RemoteSessionCapabilities(
            // Legacy `session.restart` replaces the terminal and is now the
            // stopped-Session Resume operation. Live Sessions advertise the
            // separate in-place `resumeAgent` capability instead.
            restart: !session.isLive && canRestart(command: session.command),
            // Decode-only compatibility field. New Hosts never advertise the
            // old active-runtime restart affordance.
            restartAgent: nil,
            resumeAgent: session.status != .starting && canResumeAgent(
                command: session.command,
                isLive: session.isLive,
                activeRuntimeID: session.activeRuntimeID,
                runtimeLaunchPending: session.runtimeLaunchPending,
                hostProtocolVersion: session.hostProtocolVersion
            ),
            fork: canFork(command: session.command),
            appendSystemContext: canAppendSystemContext(command: session.command),
            notifyWhenDone: canNotifyWhenDone(command: session.command),
            // Archive is offered only for resumable commands — filing away
            // a session whose CLI can't resume just strands it in the
            // library, so non-resumable sessions offer Remove instead. (The
            // flag also hides the verb against older Macs whose organization
            // patch ignores `archived`.)
            archive: canRestart(command: session.command)
        )
    }
}
