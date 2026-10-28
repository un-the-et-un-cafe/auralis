import Foundation
import XCTest
@testable import HoplaKit

final class BoundedFileReaderTests: XCTestCase {
    private func file(_ bytes: Data, check: (URL) throws -> Void) throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try bytes.write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        try check(url)
    }

    func testExactCapAndMultipleChunksArePreserved() throws {
        let bytes = Data((0..<131_073).map { UInt8($0 % 251) })
        try file(bytes) { url in
            XCTAssertEqual(try BoundedFileReader.read(url, maxBytes: UInt64(bytes.count)), bytes)
        }
    }

    func testGrowthBeyondConfirmedSizeFailsBeforeQueueing() throws {
        try file(Data(repeating: 1, count: 8)) { url in
            // Simulate a provider changing the file after the picker metadata was inspected.
            try Data(repeating: 2, count: 65_537).write(to: url)
            XCTAssertThrowsError(try BoundedFileReader.read(url, maxBytes: 65_536)) { error in
                guard case CoreError.fileTooLarge(_, let max) = error else { return XCTFail("Unexpected error: \(error)") }
                XCTAssertEqual(max, 65_536)
            }
        }
    }

    func testEmptyFileAndUnreadableFile() throws {
        try file(Data()) { url in XCTAssertEqual(try BoundedFileReader.read(url, maxBytes: 0), Data()) }
        let missing = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        XCTAssertThrowsError(try BoundedFileReader.read(missing, maxBytes: 10))
    }
}
