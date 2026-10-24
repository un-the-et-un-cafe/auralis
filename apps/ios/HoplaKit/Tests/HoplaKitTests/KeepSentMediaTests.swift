import XCTest
@testable import HoplaKit

final class KeepSentMediaTests: XCTestCase {
    func testDefaultsAndFormatting() {
        XCTAssertEqual(KeptMediaPrefs(), KeptMediaPrefs(maxDays: 90, maxBytes: 2_000_000_000))
        XCTAssertTrue(StorageSettings.dayOptions.contains(90))
        XCTAssertTrue(StorageSettings.sizeOptions.contains(2_000_000_000))
        XCTAssertEqual(StorageSettings.formatBytes(2_000_000_000), "2 GB")
        XCTAssertEqual(StorageSettings.formatBytes(500_000_000), "500 MB")
        XCTAssertEqual(StorageSettings.formatBytes(1_500), "1.5 KB")
    }

    func testResendNoteIsOptionalTrimmedAndBounded() {
        XCTAssertNil(MeshRepository.resendNote(nil))
        XCTAssertNil(MeshRepository.resendNote("   "))
        XCTAssertEqual(MeshRepository.resendNote("  merci "), "merci")
        let long = String(repeating: "é", count: 200) // 400 bytes
        XCTAssertLessThanOrEqual(MeshRepository.resendNote(long)!.utf8.count, 280)
    }
}
