import HoplaKit
import SwiftUI
import UIKit

/// No screenshots anywhere in Hopla (product decision 5, SS1/SS2).
///
/// 1. **Secure-capture layer**: the whole key window's layer is re-parented into the
///    secure canvas layer of a `UITextField` with `isSecureTextEntry = true`. iOS excludes that
///    layer from screenshots, screen recordings, AirPlay mirroring and the app-switcher
///    snapshot, so every screen, sheet, alert and media viewer in the window comes out blank.
///    Applied once per window, never removed (`ScreenCapturePolicy.secureLayerAlwaysOn`).
/// 2. **App-switcher cover**: when the scene isn't active a blurred privacy cover is drawn.
/// 3. **Recording cover**: while `UIScreen.isCaptured` (recording/mirroring) content is hidden
///    behind a cover until capture stops.
/// There is no "screenshot taken" notice (`ScreenCapturePolicy.sendsScreenshotNotice == false`).
enum SecureCapture {
    private static var protected = Set<ObjectIdentifier>()

    /// The secure text field's internal canvas view (`_UITextLayoutCanvasView` on iOS 17–26).
    private static func secureCanvasLayer(of field: UITextField) -> CALayer? {
        field.subviews.first { NSStringFromClass(type(of: $0)).contains("CanvasView") }?.layer
            ?? field.layer.sublayers?.first
    }

    @MainActor
    static func protect(_ window: UIWindow) {
        guard ScreenCapturePolicy.secureLayerAlwaysOn, !protected.contains(ObjectIdentifier(window)),
              let superlayer = window.layer.superlayer else { return }
        let field = UITextField()
        field.isSecureTextEntry = true
        field.isUserInteractionEnabled = false
        window.addSubview(field)
        field.centerYAnchor.constraint(equalTo: window.centerYAnchor).isActive = true
        field.centerXAnchor.constraint(equalTo: window.centerXAnchor).isActive = true
        superlayer.addSublayer(field.layer)
        guard let canvas = secureCanvasLayer(of: field) else { return }
        canvas.addSublayer(window.layer)
        protected.insert(ObjectIdentifier(window))
    }
}

/// Finds the hosting UIWindow and protects it.
private struct WindowProtector: UIViewRepresentable {
    final class Probe: UIView {
        override func didMoveToWindow() {
            super.didMoveToWindow()
            if let w = window { DispatchQueue.main.async { SecureCapture.protect(w) } }
        }
    }
    func makeUIView(context: Context) -> Probe { Probe() }
    func updateUIView(_ uiView: Probe, context: Context) {}
}

/// Root modifier: secure layer + app-switcher cover + recording cover.
struct ScreenSecurityModifier: ViewModifier {
    @Environment(\.scenePhase) private var phase
    @State private var captured = UIScreen.main.isCaptured

    private var cover: CaptureCover {
        let p: ScenePhaseKind = phase == .active ? .active : (phase == .inactive ? .inactive : .background)
        return ScreenCapturePolicy.cover(phase: p, isCaptured: captured)
    }

    func body(content: Content) -> some View {
        content
            .background(WindowProtector().frame(width: 0, height: 0))
            .overlay {
                switch cover {
                case .none: EmptyView()
                case .appSwitcher:
                    ZStack {
                        Rectangle().fill(.ultraThinMaterial)
                        Image(systemName: "lock.fill").font(.largeTitle).foregroundStyle(.secondary)
                    }.ignoresSafeArea()
                case .recording:
                    ZStack {
                        Color(.systemBackground)
                        VStack(spacing: 12) {
                            Image(systemName: "record.circle").font(.largeTitle)
                            Text(L("capture_cover_title")).font(.headline)
                            Text(L("capture_cover_body")).font(.subheadline).multilineTextAlignment(.center).padding(.horizontal, 32)
                        }
                    }.ignoresSafeArea()
                }
            }
            .onReceive(NotificationCenter.default.publisher(for: UIScreen.capturedDidChangeNotification)) { _ in
                captured = UIScreen.main.isCaptured
            }
    }
}

extension View {
    func screenSecurity() -> some View { modifier(ScreenSecurityModifier()) }
}
