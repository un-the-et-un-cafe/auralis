/// Supported iOS privacy covers, not screenshot prevention. UIKit reports recording and
/// mirroring state; an ordinary screenshot notification arrives only after capture.
public enum ScenePhaseKind { case active, inactive, background }
public enum CaptureStateKind { case unknown, inactive, active }

public enum CaptureCover: Equatable {
    case none
    /// Opaque cover while inactive/background, including app-switcher transitions.
    case appSwitcher
    case recording
    /// Fail closed before the attached scene's capture state has been sampled.
    case unavailable
}

public enum ScreenCapturePolicy {
    public static let preventsOrdinaryScreenshots = false
    /// No screenshot receipt is sent; no new network or burn semantics.
    public static let sendsScreenshotNotice = false

    public static func cover(phase: ScenePhaseKind, capture: CaptureStateKind) -> CaptureCover {
        guard phase == .active else { return .appSwitcher }
        switch capture {
        case .unknown: return .unavailable
        case .active: return .recording
        case .inactive: return .none
        }
    }
}
