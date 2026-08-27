//
//  DesktopNotifier.swift
//  UnpeelNative
//
//  macOS Notification Center banners for the same two triggers that drive the
//  phone push: a session that needs input (attention) and an opted-in session
//  that finished. The two channels are independent — the Mac notifies when the
//  desktop isn't already showing the session, the phone push fires when no
//  phone is viewing it — so you're told on whichever device you're at.
//

import Foundation
import UserNotifications

@MainActor
final class DesktopNotifier: NSObject, UNUserNotificationCenterDelegate {
    static let shared = DesktopNotifier()

    /// Tapping a banner selects that session. Wired by `UnpeelStore`.
    var onSelectSession: ((String) -> Void)?

    private var authorized = false
    private var requested = false

    /// Request banner/sound authorization once, early in launch. Safe to call
    /// again; only the first prompts. A denied grant just no-ops later posts.
    func requestAuthorizationIfNeeded() {
        UNUserNotificationCenter.current().delegate = self
        guard !requested else { return }
        requested = true
        UNUserNotificationCenter.current().requestAuthorization(
            options: [.alert, .sound]
        ) { @Sendable [weak self] granted, _ in
            Task { @MainActor in self?.authorized = granted }
        }
    }

    /// Verify the pipeline end to end from Settings: (re)request authorization,
    /// then post a banner immediately — bypassing the observed-session gate the
    /// real triggers use. If the user previously denied, `granted` is false and
    /// this no-ops (they'd re-enable in System Settings ▸ Notifications).
    func sendTestNotification() {
        UNUserNotificationCenter.current().delegate = self
        UNUserNotificationCenter.current().requestAuthorization(
            options: [.alert, .sound]
        ) { @Sendable [weak self] granted, _ in
            Task { @MainActor in
                guard let self, granted else { return }
                self.authorized = true
                self.notify(
                    title: "Unpeel",
                    body: "Test notification — this is what a session alert looks like.",
                    sessionID: "test",
                    kind: "test"
                )
            }
        }
    }

    /// Post a banner. `sessionID` rides the userInfo so a tap can reselect it;
    /// `kind` matches the phone payload ("needs_input" / "done").
    func notify(title: String, body: String, sessionID: String, kind: String) {
        guard authorized else { return }
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = .default
        // Collapse repeats for one session into a single banner.
        content.threadIdentifier = sessionID
        content.userInfo = ["sessionID": sessionID, "kind": kind]
        let request = UNNotificationRequest(
            identifier: "\(kind):\(sessionID)",
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request)
    }

    // Show the banner even while Unpeel is frontmost (the caller already gated
    // on the session not being the observed one, so a banner for a different
    // session is wanted).
    nonisolated func userNotificationCenter(
        _: UNUserNotificationCenter,
        willPresent _: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .sound])
    }

    nonisolated func userNotificationCenter(
        _: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        let sessionID = response.notification.request.content.userInfo["sessionID"] as? String
        // Call back on this (system) queue synchronously; only the store hop
        // needs the main actor. Extracting the String first keeps the
        // non-Sendable response off the hop.
        completionHandler()
        if let sessionID {
            Task { @MainActor [weak self] in self?.onSelectSession?(sessionID) }
        }
    }
}
