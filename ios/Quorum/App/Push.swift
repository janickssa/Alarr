import QuorumCore
import SwiftUI
import UIKit
import UserNotifications

/// Notification categories the server uses (`backend/src/alerts/push.rs`).
enum PushCategory {
    static let request = "SEERR_REQUEST"
    static let down = "SERVICE_DOWN"
    static let approve = "APPROVE"
    static let decline = "DECLINE"
    static let open = "OPEN"

    /// Approve/Decline on a new Seerr request, Open on a down service. Power actions are
    /// never offered from a notification.
    static func register() {
        let approve = UNNotificationAction(identifier: approve, title: "Approve", options: [.authenticationRequired])
        let decline = UNNotificationAction(identifier: decline, title: "Decline", options: [.authenticationRequired, .destructive])
        let open = UNNotificationAction(identifier: open, title: "Open", options: [.foreground])
        UNUserNotificationCenter.current().setNotificationCategories([
            UNNotificationCategory(identifier: request, actions: [approve, decline], intentIdentifiers: []),
            UNNotificationCategory(identifier: down, actions: [open], intentIdentifiers: []),
        ])
    }
}

/// Push registration and notification responses. The token goes to the server with the
/// APNs environment of this build (Xcode builds: sandbox; TestFlight/ad hoc: production).
final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    /// Set by the app once the model exists.
    @MainActor static var model: AppModel?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        PushCategory.register()
        return true
    }

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        let hex = deviceToken.map { String(format: "%02x", $0) }.joined()
        Task { @MainActor in await AppDelegate.model?.registerPushToken(hex) }
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        Task { @MainActor in AppDelegate.model?.pushError = error.localizedDescription }
    }

    /// Show alerts while the app is open, too.
    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        [.banner, .sound, .list]
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse
    ) async {
        let info = response.notification.request.content.userInfo
        let path = info["path"] as? String
        let serviceID = info["service_id"] as? String
        let requestID = (info["request_id"] as? NSNumber)?.int64Value
        await MainActor.run {
            guard let model = AppDelegate.model else { return }
            switch response.actionIdentifier {
            case PushCategory.approve, PushCategory.decline:
                if let serviceID, let requestID {
                    let approve = response.actionIdentifier == PushCategory.approve
                    Task { await model.answerRequest(service: serviceID, request: requestID, approve: approve) }
                }
            default:
                if let path { model.open(path: path) }
            }
        }
    }

    /// The APNs environment this build's tokens belong to.
    static var apnsEnvironment: String {
        #if DEBUG
        "sandbox"
        #else
        "production"
        #endif
    }
}
