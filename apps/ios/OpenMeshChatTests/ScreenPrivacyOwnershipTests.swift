import HoplaKit
import UIKit
import XCTest
@testable import OpenMeshChat

/// Synthetic app-host ownership regressions. No captures or real-user data.
final class ScreenPrivacyOwnershipTests: XCTestCase {
    @MainActor
    private func fixture() throws -> (UIWindowScene, UIWindow, NotificationCenter, WindowPrivacyRegistry) {
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first)
        let window = UIWindow(windowScene: scene)
        window.rootViewController = UIViewController()
        window.traitOverrides.sceneCaptureState = .active
        window.updateTraitsIfNeeded()
        let notifications = NotificationCenter()
        return (scene, window, notifications, WindowPrivacyRegistry(notifications: notifications))
    }

    @MainActor
    private func flushVisibility() async {
        await withCheckedContinuation { continuation in
            DispatchQueue.main.async { continuation.resume() }
        }
    }

    @MainActor
    func testEitherOwnerRemovalOrderPreservesCoverUntilLastLease() throws {
        for reverse in [false, true] {
            let (scene, window, notifications, registry) = try fixture()
            let first = registry.acquire(for: window) { _ in }
            let second = registry.acquire(for: window) { _ in }
            defer { first.release(); second.release() }
            XCTAssertTrue(first.controller === second.controller)
            notifications.post(name: UIScene.didActivateNotification, object: scene)
            let shield = try XCTUnwrap(first.controller.coverWindow)
            XCTAssertEqual(first.controller.cover, .recording)
            let leaving = reverse ? second : first
            let remaining = reverse ? first : second
            leaving.release()
            leaving.release() // Idempotent: cannot release someone else's owner count.
            XCTAssertTrue(remaining.controller.coverWindow === shield)
            XCTAssertFalse(shield.isHidden)
            XCTAssertFalse(window.isUserInteractionEnabled)
            XCTAssertTrue(window.accessibilityElementsHidden)
            remaining.release()
            XCTAssertNil(remaining.controller.coverWindow)
            XCTAssertTrue(window.isUserInteractionEnabled)
            XCTAssertFalse(window.accessibilityElementsHidden)
        }
    }

    @MainActor
    func testSharedOwnershipRestoresPreviouslyDisabledAndHiddenFlags() throws {
        let (scene, window, notifications, registry) = try fixture()
        window.isUserInteractionEnabled = false
        window.accessibilityElementsHidden = true
        let first = registry.acquire(for: window) { _ in }
        let second = registry.acquire(for: window) { _ in }
        defer { first.release(); second.release() }
        notifications.post(name: UIScene.willDeactivateNotification, object: scene)
        second.release()
        XCTAssertNotNil(first.controller.coverWindow)
        first.release()
        XCTAssertFalse(window.isUserInteractionEnabled)
        XCTAssertTrue(window.accessibilityElementsHidden)
    }

    @MainActor
    func testRemovedOwnerGetsNoQueuedRevealOrHideWhileRemainingOwnerGetsLatestState() async throws {
        let (scene, window, notifications, registry) = try fixture()
        window.traitOverrides.sceneCaptureState = .inactive
        window.updateTraitsIfNeeded()
        var removedValues: [Bool] = []
        var remainingValues: [Bool] = []
        let first = registry.acquire(for: window) { removedValues.append($0) }
        let second = registry.acquire(for: window) { remainingValues.append($0) }
        defer { first.release(); second.release() }
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        notifications.post(name: UIScene.willDeactivateNotification, object: scene)
        first.release()
        await flushVisibility()
        XCTAssertEqual(removedValues, [])
        XCTAssertEqual(remainingValues, [false])
        XCTAssertFalse(try XCTUnwrap(second.controller.coverWindow).isHidden)
    }

    @MainActor
    func testNewSubscriberGetsExistingStateAsynchronouslyWithoutAnotherWindow() async throws {
        let (scene, window, notifications, registry) = try fixture()
        let first = registry.acquire(for: window) { _ in }
        defer { first.release() }
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        await flushVisibility()
        let shield = first.controller.coverWindow
        var values: [Bool] = []
        let second = registry.acquire(for: window) { values.append($0) }
        defer { second.release() }
        XCTAssertEqual(values, [])
        XCTAssertTrue(first.controller === second.controller)
        XCTAssertTrue(second.controller.coverWindow === shield)
        second.refreshAttachment()
        second.refreshAttachment()
        XCTAssertTrue(second.controller.coverWindow === shield)
        await flushVisibility()
        XCTAssertEqual(values, [false])
    }

    @MainActor
    func testReleasedTokenAndItsDeferredDeinitCannotTearDownNewLease() async throws {
        let (scene, window, notifications, registry) = try fixture()
        var first: WindowPrivacyLease? = registry.acquire(for: window) { _ in }
        first?.release()
        let second = registry.acquire(for: window) { _ in }
        defer { second.release() }
        XCTAssertTrue(first?.controller === second.controller)
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        let shield = second.controller.coverWindow
        first?.release()
        first?.refreshAttachment() // A released token cannot reattach or claim ownership.
        first = nil
        await flushVisibility()
        XCTAssertTrue(second.controller.coverWindow === shield)
        XCTAssertEqual(second.controller.cover, .recording)
        XCTAssertFalse(window.isUserInteractionEnabled)
    }

    @MainActor
    func testAbandonedFinalLeaseEventuallyRestoresFlagsAndReleasesController() async throws {
        let (scene, window, notifications, registry) = try fixture()
        var lease: WindowPrivacyLease? = registry.acquire(for: window) { _ in }
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        weak var controller = lease?.controller
        weak var shield = controller?.coverWindow
        XCTAssertNotNil(shield)
        lease = nil
        // Deinit fallback must hop to main; bounded wait tests eventual cleanup, not timing.
        for _ in 0..<100 {
            if controller == nil { break }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertNil(controller)
        XCTAssertNil(shield)
        XCTAssertTrue(window.isUserInteractionEnabled)
        XCTAssertFalse(window.accessibilityElementsHidden)
    }

    @MainActor
    func testRebindingOneOwnerDoesNotUncoverRemainingOwnerOrShareCaptureState() throws {
        let (scene, firstWindow, notifications, registry) = try fixture()
        let first = registry.acquire(for: firstWindow) { _ in }
        let staying = registry.acquire(for: firstWindow) { _ in }
        let secondWindow = UIWindow(windowScene: scene)
        secondWindow.traitOverrides.sceneCaptureState = .inactive
        secondWindow.updateTraitsIfNeeded()
        first.release()
        let rebound = registry.acquire(for: secondWindow) { _ in }
        defer { staying.release(); rebound.release() }
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        XCTAssertFalse(staying.controller === rebound.controller)
        XCTAssertEqual(staying.controller.cover, .recording)
        XCTAssertEqual(rebound.controller.cover, .none)
        XCTAssertFalse(firstWindow.isUserInteractionEnabled)
        XCTAssertTrue(secondWindow.isUserInteractionEnabled)
    }

    @MainActor
    func testReentrantLifecycleChangeCannotFanOutStaleRevealToAnotherOwner() async throws {
        let (scene, window, notifications, registry) = try fixture()
        window.traitOverrides.sceneCaptureState = .inactive
        window.updateTraitsIfNeeded()
        var values: [Bool] = []
        var interrupted = false
        let callback: (Bool) -> Void = { visible in
            values.append(visible)
            if visible && !interrupted {
                interrupted = true
                notifications.post(name: UIScene.willDeactivateNotification, object: scene)
            }
        }
        let first = registry.acquire(for: window, visibilityChanged: callback)
        let second = registry.acquire(for: window, visibilityChanged: callback)
        defer { first.release(); second.release() }
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        await flushVisibility()
        await flushVisibility()
        XCTAssertEqual(Array(values.prefix(2)), [true, false])
        XCTAssertTrue(values.dropFirst().allSatisfy { !$0 })
        XCTAssertEqual(first.controller.cover, .appSwitcher)
    }

    @MainActor
    func testDisconnectInvalidatesRevealsAndReusesSingleControllerForSurvivingLeases() async throws {
        let (scene, window, notifications, registry) = try fixture()
        window.traitOverrides.sceneCaptureState = .inactive
        window.updateTraitsIfNeeded()
        var firstValues: [Bool] = []
        var secondValues: [Bool] = []
        let first = registry.acquire(for: window) { firstValues.append($0) }
        let second = registry.acquire(for: window) { secondValues.append($0) }
        defer { first.release(); second.release() }
        notifications.post(name: UIScene.didActivateNotification, object: scene)
        weak var oldShield = first.controller.coverWindow
        notifications.post(name: UIScene.didDisconnectNotification, object: scene)
        await flushVisibility()
        XCTAssertEqual(firstValues, [false])
        XCTAssertEqual(secondValues, [false])
        XCTAssertNil(first.controller.coverWindow)
        XCTAssertNil(oldShield)
        // The synthetic event doesn't really disconnect UIKit. Re-subscription must still
        // reuse the Entry. Actual unattached/reconnected scene behavior remains device QA.
        let third = registry.acquire(for: window) { _ in }
        defer { third.release() }
        XCTAssertTrue(first.controller === third.controller)
        XCTAssertTrue(second.controller === third.controller)
        first.release()
        second.release()
        XCTAssertNotNil(third.controller.coverWindow)
    }
}
