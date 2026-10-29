// swift-tools-version:5.9
// Platform-independent Hopla logic (models, policies, onboarding/backup/burn/notification
// rules). No UIKit/SwiftUI/CoreBluetooth here, so it builds and tests on Linux with SwiftPM
// as well as inside the Xcode project (local package dependency).
import PackageDescription

let package = Package(
    name: "HoplaKit",
    platforms: [.iOS(.v17), .macOS(.v13)],
    products: [.library(name: "HoplaKit", targets: ["HoplaKit"])],
    targets: [
        .target(name: "HoplaKit", resources: [.process("PrivacyInfo.xcprivacy")]),
        .testTarget(name: "HoplaKitTests", dependencies: ["HoplaKit"]),
    ]
)
