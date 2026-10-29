import HoplaKit
import SwiftUI
import UIKit

/// Review proposal: supported, best-effort recording/mirroring and lifecycle covers.
/// This does NOT prevent ordinary screenshots or guarantee a zero-frame capture transition.
/// Never inspects private UIKit subviews or reparents app layers.
@MainActor
final class ScenePrivacyController {
    private weak var sourceWindow: UIWindow?
    private weak var scene: UIWindowScene?
    private weak var disconnectedScene: UIWindowScene?
    private let notifications: NotificationCenter
    private var observers: [NSObjectProtocol] = []
    private var traitRegistration: (any UITraitChangeRegistration)?
    private(set) var coverWindow: PrivacyCoverWindow?
    private var phase: ScenePhaseKind = .inactive
    private var capture: CaptureStateKind = .unknown
    private var priorInteraction: Bool?
    private var priorAccessibilityHidden: Bool?
    private var generation = 0
    private var attachmentEpoch = 0
    var contentVisibilityChanged: ((Bool) -> Void)?
    private(set) var cover: CaptureCover = .unavailable

    init(notifications: NotificationCenter = .default) {
        self.notifications = notifications
    }

    func attach(to window: UIWindow) {
        guard sourceWindow !== window || scene !== window.windowScene else { return }
        detach()
        guard let scene = window.windowScene else { return }
        // A disconnect-triggered SwiftUI update must not resurrect an unattached scene.
        guard scene !== disconnectedScene || scene.activationState != .unattached else { return }
        disconnectedScene = nil
        self.sourceWindow = window
        self.scene = scene
        // Cover before observation or sampling. The SwiftUI root is also initially opaque.
        let shield = PrivacyCoverWindow(windowScene: scene)
        shield.frame = scene.coordinateSpace.bounds
        shield.windowLevel = .init(rawValue: UIWindow.Level.alert.rawValue + 1)
        shield.backgroundColor = .systemBackground
        shield.isOpaque = true
        shield.rootViewController = PrivacyCoverViewController()
        coverWindow = shield
        render(.unavailable)

        observe(UIScene.willDeactivateNotification, object: scene) { $0.changePhase(.inactive) }
        observe(UIScene.didEnterBackgroundNotification, object: scene) { $0.changePhase(.background) }
        observe(UIScene.willEnterForegroundNotification, object: scene) { $0.changePhase(.inactive) }
        observe(UIScene.didActivateNotification, object: scene) { $0.changePhase(.active) }
        observe(UIScene.didDisconnectNotification, object: scene) {
            $0.disconnectedScene = $0.scene
            $0.detach()
        }
        // Re-read only our attached scene, never UIScreen.main or the application's first scene.
        observe(UIScreen.capturedDidChangeNotification, object: nil) { $0.refreshCapture() }
        traitRegistration = window.registerForTraitChanges([UITraitSceneCaptureState.self]) {
            [weak self] (_: UIWindow, _: UITraitCollection) in self?.refreshCapture()
        }
        window.updateTraitsIfNeeded()
        phase = Self.phase(of: scene.activationState)
        refreshCapture() // Sample AFTER registration, including recording before launch.
    }

    func detach() {
        generation += 1 // Invalidate queued SwiftUI visibility callbacks.
        attachmentEpoch += 1 // Invalidate callbacks belonging to a previous scene.
        for observer in observers { notifications.removeObserver(observer) }
        observers.removeAll()
        if let registration = traitRegistration { sourceWindow?.unregisterForTraitChanges(registration) }
        traitRegistration = nil
        restoreInteraction()
        coverWindow?.isHidden = true
        coverWindow?.rootViewController = nil
        coverWindow?.windowScene = nil
        coverWindow = nil
        sourceWindow = nil
        scene = nil
        phase = .inactive
        capture = .unknown
        cover = .unavailable
        publishVisibility(false)
    }

    // Explicit event state avoids reading a still-active activationState during willDeactivate.
    // Capture changes cannot uncover an inactive/background scene.
    func changePhase(_ phase: ScenePhaseKind) {
        self.phase = phase
        refreshCapture()
    }

    func refreshCapture() {
        guard let window = sourceWindow, let scene, window.windowScene === scene else {
            render(.unavailable)
            return
        }
        switch window.traitCollection.sceneCaptureState {
        case .active: capture = .active
        case .inactive: capture = .inactive
        case .unspecified:
            // Supported fallback for an unspecified scene trait. It can conservatively
            // cover other scenes on the same display. Apple deprecates it in favor of the trait.
            capture = scene.screen.isCaptured ? .active : .inactive
        @unknown default: capture = .unknown
        }
        render(ScreenCapturePolicy.cover(phase: phase, capture: capture))
    }

    private func observe(_ name: Notification.Name, object: AnyObject?,
                         action: @escaping @MainActor (ScenePrivacyController) -> Void) {
        let epoch = attachmentEpoch
        observers.append(notifications.addObserver(forName: name, object: object, queue: nil) {
            [weak self] _ in
            // UIKit delivers these on main. Do not enqueue a Task: install the cover
            // before the callback returns, rather than after the snapshot transition.
            MainActor.assumeIsolated {
                guard let self, self.attachmentEpoch == epoch else { return }
                action(self)
            }
        })
    }

    private func render(_ newCover: CaptureCover) {
        cover = newCover
        let visible = newCover == .none
        if visible {
            coverWindow?.isHidden = true
            restoreInteraction()
        } else {
            if let window = sourceWindow {
                if priorInteraction == nil { priorInteraction = window.isUserInteractionEnabled }
                if priorAccessibilityHidden == nil { priorAccessibilityHidden = window.accessibilityElementsHidden }
                window.endEditing(true)
                window.isUserInteractionEnabled = false
                window.accessibilityElementsHidden = true
            }
            (coverWindow?.rootViewController as? PrivacyCoverViewController)?.show(newCover)
            coverWindow?.isHidden = false // Never make key: preserve the app key window.
            coverWindow?.layoutIfNeeded()
        }
        publishVisibility(visible)
    }

    func publishCurrentVisibility() {
        publishVisibility(cover == .none)
    }

    private func publishVisibility(_ visible: Bool) {
        generation += 1
        let expected = generation
        // Do not mutate SwiftUI state inside UIViewRepresentable's update pass. UIKit's
        // cover is already synchronous; discard stale queued reveals after newer events.
        DispatchQueue.main.async { [weak self] in
            guard let self, self.generation == expected else { return }
            self.contentVisibilityChanged?(visible)
        }
    }

    private func restoreInteraction() {
        if let value = priorInteraction { sourceWindow?.isUserInteractionEnabled = value }
        if let value = priorAccessibilityHidden { sourceWindow?.accessibilityElementsHidden = value }
        priorInteraction = nil
        priorAccessibilityHidden = nil
    }

    private static func phase(of state: UIScene.ActivationState) -> ScenePhaseKind {
        switch state {
        case .foregroundActive: return .active
        case .background: return .background
        default: return .inactive
        }
    }
}

/// Scene-local window above app sheets/full-screen covers/alerts, not a system UI boundary.
@MainActor
final class PrivacyCoverWindow: UIWindow {
    override var canBecomeKey: Bool { false }
    override func layoutSubviews() {
        // Full scene extent, not the probe's frame or the content safe area.
        if let scene = windowScene, frame != scene.coordinateSpace.bounds {
            frame = scene.coordinateSpace.bounds
        }
        super.layoutSubviews()
    }
}

@MainActor
final class PrivacyCoverViewController: UIViewController {
    private let label = UILabel()

    override func loadView() {
        let view = UIView()
        view.backgroundColor = .systemBackground
        view.isOpaque = true
        view.accessibilityViewIsModal = true
        view.accessibilityIdentifier = "auralis.privacy.cover"
        label.numberOfLines = 0
        label.textAlignment = .center
        label.font = .preferredFont(forTextStyle: .headline)
        label.adjustsFontForContentSizeCategory = true
        label.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(label)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.leadingAnchor, constant: 24),
            label.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -24),
            label.centerYAnchor.constraint(equalTo: view.centerYAnchor)
        ])
        self.view = view
    }

    func show(_ cover: CaptureCover) {
        loadViewIfNeeded()
        label.text = cover == .recording
            ? L("capture_cover_title") + "\n\n" + L("capture_cover_body")
            : L("privacy_cover_title")
    }
}

/// Each live probe owns a lease, not its own controller. Transient SwiftUI replacement
/// can overlap probes for one UIWindow without restoring another probe's privacy flags.
@MainActor
final class WindowPrivacyRegistry {
    static let shared = WindowPrivacyRegistry()
    // Leases retain entries. Neither the registry nor an entry retains the source window.
    // Weak-to-strong maps can retain values after keys disappear; avoid that ownership.
    private let entries = NSMapTable<UIWindow, WindowPrivacyEntry>(keyOptions: [.weakMemory, .objectPointerPersonality],
                                                                  valueOptions: .weakMemory)
    private let notifications: NotificationCenter

    init(notifications: NotificationCenter = .default) { self.notifications = notifications }

    func acquire(for window: UIWindow, visibilityChanged: @escaping (Bool) -> Void) -> WindowPrivacyLease {
        let entry: WindowPrivacyEntry
        if let existing = entries.object(forKey: window) {
            entry = existing
        } else {
            entry = WindowPrivacyEntry(window: window, notifications: notifications)
            entries.setObject(entry, forKey: window)
        }
        return entry.acquire(visibilityChanged: visibilityChanged)
    }
}

@MainActor
fileprivate final class WindowPrivacyEntry {
    private weak var window: UIWindow?
    let controller: ScenePrivacyController
    private var listeners: [UUID: (Bool) -> Void] = [:]

    init(window: UIWindow, notifications: NotificationCenter) {
        self.window = window
        controller = ScenePrivacyController(notifications: notifications)
        controller.contentVisibilityChanged = { [weak self] _ in
            guard let self else { return }
            // Recheck membership AND current state: an earlier listener may release,
            // rebind or synchronously trigger a lifecycle/capture change during fanout.
            for token in Array(self.listeners.keys) {
                self.listeners[token]?(self.controller.cover == .none)
            }
        }
    }

    func acquire(visibilityChanged: @escaping (Bool) -> Void) -> WindowPrivacyLease {
        let token = UUID()
        listeners[token] = visibilityChanged
        refresh(token)
        // Reuse the controller's generation-guarded async path, never update SwiftUI
        // state synchronously inside make/updateUIView, including a second subscriber.
        controller.publishCurrentVisibility()
        return WindowPrivacyLease(entry: self, token: token)
    }

    func refresh(_ token: UUID) {
        guard listeners[token] != nil else { return }
        if let window { controller.attach(to: window) } else { controller.detach() }
    }

    func release(_ token: UUID) {
        guard listeners.removeValue(forKey: token) != nil else { return }
        // One owner leaving cannot uncover content or restore interaction for another.
        if listeners.isEmpty { controller.detach() }
    }
}

@MainActor
final class WindowPrivacyLease {
    private let entry: WindowPrivacyEntry
    private let token: UUID
    var controller: ScenePrivacyController { entry.controller }

    fileprivate init(entry: WindowPrivacyEntry, token: UUID) {
        self.entry = entry
        self.token = token
    }

    func refreshAttachment() { entry.refresh(token) }
    func release() { entry.release(token) }

    deinit {
        // Normal detach is synchronous. An abandoned lease keeps the cover/entry alive
        // until main-actor cleanup, rather than revealing early. No escaping self.
        let entry = entry
        let token = token
        Task { @MainActor in entry.release(token) }
    }
}

private struct WindowPrivacyAttachment: UIViewRepresentable {
    var visibilityChanged: (Bool) -> Void

    final class Probe: UIView {
        var visibilityChanged: ((Bool) -> Void)?
        private weak var boundWindow: UIWindow?
        private var lease: WindowPrivacyLease?
        private var bindingGeneration = 0

        override func didMoveToWindow() {
            super.didMoveToWindow()
            bind(to: window)
        }

        func bind(to window: UIWindow?) {
            if let window, boundWindow === window, let lease {
                lease.refreshAttachment()
                return
            }
            bindingGeneration += 1
            let expected = bindingGeneration
            lease?.release()
            lease = nil
            boundWindow = window
            if let window {
                lease = WindowPrivacyRegistry.shared.acquire(for: window) { [weak self] visible in
                    guard let self, self.bindingGeneration == expected else { return }
                    self.visibilityChanged?(visible)
                }
            } else {
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.bindingGeneration == expected else { return }
                    self.visibilityChanged?(false)
                }
            }
        }
    }

    func makeUIView(context: Context) -> Probe {
        let probe = Probe()
        probe.visibilityChanged = visibilityChanged
        return probe
    }
    func updateUIView(_ probe: Probe, context: Context) {
        probe.visibilityChanged = visibilityChanged
        probe.bind(to: probe.window)
    }
    static func dismantleUIView(_ probe: Probe, coordinator: ()) {
        probe.visibilityChanged = nil
        probe.bind(to: nil)
    }
}

struct ScreenSecurityModifier: ViewModifier {
    @State private var contentVisible = false

    func body(content: Content) -> some View {
        content
            .opacity(contentVisible ? 1 : 0)
            .allowsHitTesting(contentVisible)
            .accessibilityHidden(!contentVisible)
            .overlay { if !contentVisible { Color(.systemBackground).ignoresSafeArea() } }
            .background(WindowPrivacyAttachment { contentVisible = $0 }.frame(width: 0, height: 0))
            .animation(nil, value: contentVisible)
    }
}

extension View {
    func screenSecurity() -> some View { modifier(ScreenSecurityModifier()) }
}
