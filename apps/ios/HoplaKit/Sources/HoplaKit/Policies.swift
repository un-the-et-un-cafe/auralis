/// File size and gateway policy come from the Rust core, never hard-coded in the UI.
public enum FilePolicy {
    public enum Check: Equatable { case ok, tooLarge(max: UInt64) }
    public static func check(size: UInt64, core: OmcpCore) -> Check {
        let max = core.maxFileBytes()
        return size <= max ? .ok : .tooLarge(max: max)
    }
    /// "50 MB" (decimal, like the core).
    public static func label(_ bytes: UInt64) -> String { "\(bytes / 1_000_000) MB" }
}

public struct GatewayUIState: Equatable {
    public var dailyCapBytes: UInt64
    /// Always 0 until real metering exists (Designy lock-in: never a fake fill).
    public var usedBytes: UInt64
    public var decision: GatewayDecision
}

public enum GatewayPolicy {
    public static func state(store: AppStore, core: OmcpCore, network: NetworkType, metered: Bool) -> GatewayUIState {
        let d = store.gatewayEnabled
            ? core.gatewayDecision(wifiOnly: !store.gatewayAllowMobile, network: network, metered: metered)
            : GatewayDecision(bridge: false, meteredWarning: false)
        return GatewayUIState(dailyCapBytes: core.gatewayDailyCapBytes(), usedBytes: 0, decision: d)
    }
}

/// QR contact add (design pack v0.1.1 §5.22): the preview sheet offers both actions.
public enum ContactAddAction: Equatable { case verifyAndChat, chatWithoutVerifying }
