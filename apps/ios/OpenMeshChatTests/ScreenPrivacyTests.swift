import HoplaKit
import UIKit
import XCTest
@testable import OpenMeshChat

/// Native app-host fixtures only. Injected notifications/trait overrides do NOT establish
/// actual screen-recording, app-switcher snapshot, VoiceOver, or physical-device behavior.
final class ScreenPrivacyTests: XCTestCase {
    @MainActor
    private func setCapture(_ state: UISceneCaptureState, on window: UIWindow) {
        window.traitOverrides.sceneCaptureState = state
        // UIKit may defer trait propagation until layout; make each fixture event explicit.
        window.updateTraitsIfNeeded()
    }

    @MainActor
    private func fixture() throws -> (UIWindowScene, UIWindow, NotificationCenter, ScenePrivacyController) {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first)
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        window.rootViewController = UIViewController()
        setCapture(.inactive, on: window)
        let notifications = NotificationCenter()
        return (scene, window, notifications, ScenePrivacyController(notifications: notifications))
    }

    @MainActor
    func testRecordingAlreadyActiveAtAttachmentAndInitialOpaqueWindow() throws {
        let (scene, window, notifications, controller) = try fixture()
        defer { controller.detach() }
        setCapture(.active, on: window)
        controller.attach(to: window)
        XCTAssertNotEqual(controller.cover, .none)
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        XCTAssertEqual(controller.cover, .recording)
        let shield = try XCTUnwrap(controller.coverWindow)
        XCTAssertTrue(shield.windowScene === scene)
        XCTAssertFalse(shield.isHidden)
        XCTAssertFalse(shield.canBecomeKey)
        XCTAssertFalse(shield.isKeyWindow)
        XCTAssertGreaterThan(shield.windowLevel, UIWindow.Level.alert)
        XCTAssertEqual(shield.frame, scene.coordinateSpace.bounds)
        XCTAssertTrue(shield.isOpaque)
        XCTAssertTrue(try XCTUnwrap(shield.rootViewController?.view).isOpaque)
        XCTAssertFalse(window.isUserInteractionEnabled)
        XCTAssertTrue(window.accessibilityElementsHidden)
    }

    @MainActor
    func testCaptureStopCannotRevealInactiveBackgroundOrEnteringForeground() throws {
        let (scene, window, notifications, controller) = try fixture()
        defer { controller.detach() }
        controller.attach(to: window)
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        XCTAssertEqual(controller.cover, .none)
        setCapture(.active, on: window)
        controller.refreshCapture()
        XCTAssertEqual(controller.cover, .recording)
        notifications.post(name: UIScene.willDeactivateNotification, object: scene)
        XCTAssertEqual(controller.cover, .appSwitcher)
        setCapture(.inactive, on: window)
        controller.refreshCapture()
        XCTAssertEqual(controller.cover, .appSwitcher)
        notifications.post(name: UIScene.didEnterBackgroundNotification, object: scene)
        controller.refreshCapture()
        XCTAssertEqual(controller.cover, .appSwitcher)
        notifications.post(name: UIScene.willEnterForegroundNotification, object: scene)
        XCTAssertEqual(controller.cover, .appSwitcher)
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        XCTAssertEqual(controller.cover, .none)
    }

    @MainActor
    func testUnrelatedLifecycleObjectAndRepeatedAttachmentDoNotChangeOwnership() throws {
        let (scene, window, notifications, controller) = try fixture()
        defer { controller.detach() }
        controller.attach(to: window)
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        let shield = controller.coverWindow
        controller.attach(to: window)
        XCTAssertTrue(shield === controller.coverWindow)
        notifications.post(name: UIScene.willDeactivateNotification, object: NSObject())
        XCTAssertEqual(controller.cover, .none)
        notifications.post(name: UIScene.willDeactivateNotification, object: scene)
        XCTAssertEqual(controller.cover, .appSwitcher)
    }

    @MainActor
    func testDetachRestoresExistingFlagsAndRemovesObservers() throws {
        let (scene, window, notifications, controller) = try fixture()
        window.isUserInteractionEnabled = false
        window.accessibilityElementsHidden = true
        controller.attach(to: window)
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        XCTAssertFalse(window.isUserInteractionEnabled)
        XCTAssertTrue(window.accessibilityElementsHidden)
        setCapture(.active, on: window)
        controller.refreshCapture()
        weak var oldShield = controller.coverWindow
        controller.detach()
        XCTAssertNil(controller.coverWindow)
        XCTAssertNil(oldShield)
        XCTAssertFalse(window.isUserInteractionEnabled)
        XCTAssertTrue(window.accessibilityElementsHidden)
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        setCapture(.inactive, on: window)
        window.layoutIfNeeded()
        XCTAssertEqual(controller.cover, .unavailable)
        XCTAssertNil(controller.coverWindow)
    }

    @MainActor
    func testIndependentControllersAndRebindingDoNotShareCaptureState() throws {
        let (scene, first, notifications, controller) = try fixture()
        let second = UIWindow(windowScene: scene)
        setCapture(.inactive, on: second)
        let other = ScenePrivacyController(notifications: notifications)
        defer { controller.detach(); other.detach() }
        setCapture(.active, on: first)
        controller.attach(to: first)
        other.attach(to: second)
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        XCTAssertEqual(controller.cover, .recording)
        XCTAssertEqual(other.cover, .none)
        controller.detach()
        XCTAssertEqual(other.cover, .none)
        XCTAssertTrue(first.isUserInteractionEnabled)
        XCTAssertFalse(first.accessibilityElementsHidden)
        controller.attach(to: second)
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        XCTAssertEqual(controller.cover, .none)
        notifications.post(name: UIScene.didDisconnectNotification, object: scene)
        XCTAssertNil(controller.coverWindow)
        XCTAssertNil(other.coverWindow)
        // Two fixture windows prove instance isolation; actual distinct scenes are manual QA.
    }

    @MainActor
    func testAppModalIsBelowOpaqueCoverAndEndingEditingKeepsFictionalDraft() async throws {
        let (scene, window, notifications, controller) = try fixture()
        let priorKey = scene.keyWindow
        let root = try XCTUnwrap(window.rootViewController)
        let field = UITextField(frame: CGRect(x: 20, y: 100, width: 200, height: 40))
        field.text = "Fictional review draft"
        root.view.addSubview(field)
        window.makeKeyAndVisible() // Test fixture only; production shield never becomes key.
        window.layoutIfNeeded()
        defer {
            controller.detach()
            root.dismiss(animated: false)
            window.isHidden = true
            priorKey?.makeKey()
        }
        controller.attach(to: window)
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        XCTAssertTrue(field.becomeFirstResponder())
        setCapture(.active, on: window)
        controller.refreshCapture()
        XCTAssertFalse(field.isFirstResponder)
        XCTAssertEqual(field.text, "Fictional review draft")
        setCapture(.inactive, on: window)
        controller.refreshCapture()
        let modal = UIViewController()
        modal.modalPresentationStyle = .fullScreen
        let presented = expectation(description: "Fictional modal presented")
        root.present(modal, animated: false) { presented.fulfill() }
        await fulfillment(of: [presented], timeout: 3)
        notifications.post(name: UIScene.willDeactivateNotification, object: scene)
        let shield = try XCTUnwrap(controller.coverWindow)
        XCTAssertFalse(shield.isHidden)
        XCTAssertTrue(root.presentedViewController === modal)
        XCTAssertGreaterThan(shield.windowLevel, try XCTUnwrap(modal.view.window).windowLevel)
        XCTAssertTrue(window.isKeyWindow)
        XCTAssertTrue(window.accessibilityElementsHidden)
        XCTAssertEqual(field.text, "Fictional review draft")
    }

    @MainActor
    func testDetachPublishesHiddenInsteadOfLeavingPreviouslyRevealedRoot() async throws {
        let (scene, window, notifications, controller) = try fixture()
        var latest: Bool?
        controller.contentVisibilityChanged = { latest = $0 }
        controller.attach(to: window)
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        await withCheckedContinuation { continuation in
            DispatchQueue.main.async { continuation.resume() }
        }
        XCTAssertEqual(latest, true)
        controller.detach()
        await withCheckedContinuation { continuation in
            DispatchQueue.main.async { continuation.resume() }
        }
        XCTAssertEqual(latest, false)
    }

    @MainActor
    func testQueuedRevealIsInvalidatedBySynchronousPrivacyCover() async throws {
        let (scene, window, notifications, controller) = try fixture()
        defer { controller.detach() }
        var visibility: [Bool] = []
        controller.contentVisibilityChanged = { visibility.append($0) }
        controller.attach(to: window)
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        notifications.post(name: UIScene.willDeactivateNotification, object: scene)
        XCTAssertFalse(try XCTUnwrap(controller.coverWindow).isHidden)
        await withCheckedContinuation { continuation in
            DispatchQueue.main.async { continuation.resume() }
        }
        XCTAssertEqual(visibility, [false])
    }
}
