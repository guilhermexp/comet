//
//  SelectedHostScope.swift
//  UnpeelNative
//
//  The local-execution safety boundary for the future Host picker. This is
//  deliberately smaller than SessionBackend: it exists now so every local
//  spawn is guarded before remote scope becomes a product surface.
//

import Foundation

enum SelectedHostScope: Equatable, Sendable {
    case local
    case remote(hostID: String)

    var permitsLocalExecution: Bool {
        self == .local
    }

    var remoteHostID: String? {
        guard case .remote(let hostID) = self else { return nil }
        return hostID
    }

    /// Additive field in the session-host launch JSON. Rust defaults a missing
    /// value to local for compatibility, and rejects remote_controller before
    /// creating session artifacts or installing provider hooks.
    var sessionLaunchWireValue: String {
        switch self {
        case .local: "local"
        case .remote: "remote_controller"
        }
    }
}

enum LocalExecutionOperation: String, Sendable {
    case spawnSession = "spawn a local session"
    case installHookAssets = "install local hook assets"
}

enum LocalExecutionPolicy {
    static func permits(
        _ operation: LocalExecutionOperation,
        in scope: SelectedHostScope
    ) -> Bool {
        // Keep the operation explicit even though both are currently governed
        // by the same boundary; a future local-only mutation must be added to
        // this enum instead of growing an unguarded call path.
        switch operation {
        case .spawnSession, .installHookAssets:
            scope.permitsLocalExecution
        }
    }
}
