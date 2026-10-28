import SwiftUI
import UIKit

/// Auralis appearance tokens. Internal OMCP names remain stable across the fork.
struct OmcpColors {
    let bg: Color
    let surface: Color
    let surface2: Color
    let ink: Color
    let ink2: Color
    let ink3: Color
    let line: Color
    let brand: Color
    let brandSoft: Color
    let ok: Color
    let okSoft: Color
    let warn: Color
    let warnSoft: Color
    let danger: Color
    let dangerSoft: Color
    let pathMesh: Color
    let pathMeshSoft: Color
    let pathGateway: Color
    let pathGatewaySoft: Color
    let pathMixed: Color
    let pathMixedSoft: Color
    let pathQueue: Color
    let pathQueueSoft: Color

    static let light = OmcpColors(
        bg: Color(hex: 0xF4F8F6),
        surface: Color(hex: 0xFFFFFF),
        surface2: Color(hex: 0xE8F0EC),
        ink: Color(hex: 0x071A21),
        ink2: Color(hex: 0x425F63),
        ink3: Color(hex: 0x526B6E),
        line: Color(hex: 0xD5E3DD),
        brand: Color(hex: 0x176B5B),
        brandSoft: Color(hex: 0xDDF4E9),
        ok: Color(hex: 0x0F6B3A),
        okSoft: Color(hex: 0xE3F4EA),
        warn: Color(hex: 0x8A5A10),
        warnSoft: Color(hex: 0xFAF0D8),
        danger: Color(hex: 0x9B1C2E),
        dangerSoft: Color(hex: 0xFCE8EB),
        pathMesh: Color(hex: 0x176B5B),
        pathMeshSoft: Color(hex: 0xDDF4E9),
        pathGateway: Color(hex: 0x1A5F9E),
        pathGatewaySoft: Color(hex: 0xE4EEF8),
        pathMixed: Color(hex: 0x5A4A9E),
        pathMixedSoft: Color(hex: 0xEEEBF8),
        pathQueue: Color(hex: 0x8A5A10),
        pathQueueSoft: Color(hex: 0xFAF0D8)
    )

    static let dark = OmcpColors(
        bg: Color(hex: 0x071A21),
        surface: Color(hex: 0x102A32),
        surface2: Color(hex: 0x19353D),
        ink: Color(hex: 0xF4F8F6),
        ink2: Color(hex: 0xB3C9CA),
        ink3: Color(hex: 0x8EA9AC),
        line: Color(hex: 0x29464C),
        brand: Color(hex: 0x84E4CA),
        brandSoft: Color(hex: 0x143F3B),
        ok: Color(hex: 0x5DCA8E),
        okSoft: Color(hex: 0x143525),
        warn: Color(hex: 0xE0B45A),
        warnSoft: Color(hex: 0x3A2E14),
        danger: Color(hex: 0xF07178),
        dangerSoft: Color(hex: 0x3A1820),
        pathMesh: Color(hex: 0x84E4CA),
        pathMeshSoft: Color(hex: 0x143F3B),
        pathGateway: Color(hex: 0x6BA3D9),
        pathGatewaySoft: Color(hex: 0x1A2E42),
        pathMixed: Color(hex: 0xA89AD9),
        pathMixedSoft: Color(hex: 0x2A2540),
        pathQueue: Color(hex: 0xE0B45A),
        pathQueueSoft: Color(hex: 0x3A2E14)
    )

    static func current() -> OmcpColors {
        UITraitCollection.current.userInterfaceStyle == .dark ? .dark : .light
    }
}

private struct OmcpColorsKey: EnvironmentKey {
    static let defaultValue = OmcpColors.light
}

extension EnvironmentValues {
    var omcpColors: OmcpColors {
        get { self[OmcpColorsKey.self] }
        set { self[OmcpColorsKey.self] = newValue }
    }
}

extension Color {
    init(hex: UInt32, opacity: Double = 1) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: opacity
        )
    }
}
