/// No screenshots anywhere in Hopla (product decision 5, SS1/SS2). Pure decision logic for the
/// iOS layers in `SecureCapture.swift`:
/// - every screen renders inside the secure-capture layer (blank in screenshots/recordings);
/// - the app switcher snapshot shows a privacy cover (scene not active);
/// - if `UIScreen.isCaptured` (recording/mirroring) content is hidden behind a cover.
/// There is no "screenshot taken" notice to the sender.
public enum ScenePhaseKind { case active, inactive, background }

public enum CaptureCover: Equatable {
    case none
    /// Blurred privacy cover for the app switcher.
    case appSwitcher
    /// "Screen recording or mirroring is on" cover until capture stops.
    case recording
}

public enum ScreenCapturePolicy {
    /// The secure layer is never turned off.
    public static let secureLayerAlwaysOn = true
    /// No screenshot notice is ever sent or shown.
    public static let sendsScreenshotNotice = false

    public static func cover(phase: ScenePhaseKind, isCaptured: Bool) -> CaptureCover {
        if isCaptured { return .recording }
        return phase == .active ? .none : .appSwitcher
    }
}
