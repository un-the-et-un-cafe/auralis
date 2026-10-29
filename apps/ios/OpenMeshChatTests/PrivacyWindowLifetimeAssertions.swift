import UIKit
import XCTest

/// Check the synchronous security/ownership boundary separately from UIKit allocation
/// lifetime. A still-retained shell must be hidden, disconnected and content-free.
@MainActor
func assertPrivacyWindowDetached(_ window: UIWindow?, from scene: UIWindowScene,
                                 file: StaticString = #filePath, line: UInt = #line) {
    guard let window else { return }
    XCTAssertTrue(window.isHidden, file: file, line: line)
    XCTAssertFalse(window.isKeyWindow, file: file, line: line)
    XCTAssertNil(window.rootViewController, file: file, line: line)
    XCTAssertNil(window.windowScene, file: file, line: line)
    XCTAssertFalse(scene.windows.contains { $0 === window }, file: file, line: line)
}

/// Yield to UIKit's normal main-run-loop/appearance cleanup, without nested run loops,
/// force-releasing UIKit state, or accepting a persistent leak. Callers retain their
/// final weak-nil assertions. The plain UIWindow control uses the identical bound.
@MainActor
func drainPrivacyWindowLifetime(label: String = #function,
                                _ released: () -> Bool) async throws {
    for turn in 0..<150 {
        if autoreleasepool(invoking: released) {
            print("Window lifetime \(label): released after \(turn) main-run-loop yields")
            return
        }
        try await Task.sleep(nanoseconds: 20_000_000)
    }
    print("Window lifetime \(label): still retained after bounded lifecycle drain")
}
