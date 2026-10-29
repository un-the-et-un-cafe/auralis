import HoplaKit
import SwiftUI
import UIKit
import UserNotifications

/// Auralis (protocol: OMCP). Root: onboarding until done, then tabs; the whole window is under
/// the secure-capture layer with app-switcher and recording covers.
@main
struct OpenMeshChatApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @StateObject private var model = AppModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            Group {
                if model.repo.storageBlocked {
                    VStack(spacing: 16) {
                        Text(L("store_blocked_title")).font(.title2)
                        Text(L("store_blocked_body"))
                        Button(L("store_retry")) { model.retryStore() }
                    }.padding(24)
                } else if model.onboarded {
                    AuralisRootView()
                } else {
                    OnboardingView()
                }
            }
            .environmentObject(model)
            .auralisTheme()
            .screenSecurity()
            .onChange(of: model.onboarded) { _, on in
                if on { model.startMesh(); delegate.requestNotifications() }
            }
            .onChange(of: scenePhase) { _, phase in
                if phase == .active { model.refreshTimeSensitiveState() }
            }
            .task { if model.onboarded { delegate.requestNotifications() } }
        }
    }
}

/// Notifications default ON (asked after onboarding) and content-free APNs registration.
final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    private var notificationsRequested = false

    func application(_ app: UIApplication, didFinishLaunchingWithOptions _: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        LocalNotifier.registerCategories()
        return true
    }

    @MainActor
    func requestNotifications() {
        guard !notificationsRequested else { return }
        notificationsRequested = true
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { ok, _ in
            if ok { DispatchQueue.main.async { UIApplication.shared.registerForRemoteNotifications() } }
        }
    }

    /// STUB: the APNs token would be registered with the user's gateway mailbox. The gateway
    /// only ever sends `PushPayload.apnsPayload()` (opaque mailbox id, generic alert).
    func application(_ app: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken token: Data) {
        UserDefaults.standard.set(token.map { String(format: "%02x", $0) }.joined(), forKey: "omcp.apnsToken")
    }

    func application(_ app: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {}

    func userNotificationCenter(_ c: UNUserNotificationCenter, willPresent n: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .sound, .list]
    }
}
