import Foundation
import XCTest
@testable import HoplaKit

final class LinkWriteQueueTests: XCTestCase {
    func testWritesRemainOrderedAndWaitForATTCompletion() {
        var queue = LinkWriteQueue()
        queue.append(contentsOf: [Data([1]), Data([2])])
        XCTAssertEqual(queue.next(), Data([1]))
        XCTAssertNil(queue.next())
        XCTAssertTrue(queue.complete(succeeded: true))
        XCTAssertEqual(queue.next(), Data([2]))
        XCTAssertTrue(queue.complete(succeeded: true))
        XCTAssertNil(queue.next())
    }

    func testFailedWriteCannotSkipOrReplayCiphertextFromSession() {
        var queue = LinkWriteQueue()
        queue.append(contentsOf: [Data([1]), Data([2]), Data([3])])
        XCTAssertEqual(queue.next(), Data([1]))
        XCTAssertFalse(queue.complete(succeeded: false))
        XCTAssertTrue(queue.failed)
        XCTAssertTrue(queue.isEmpty)
        XCTAssertNil(queue.next())
        queue.append(contentsOf: [Data([4])])
        XCTAssertNil(queue.next())
        XCTAssertFalse(queue.complete(succeeded: true), "late callbacks cannot resurrect the session")
        var newSession = LinkWriteQueue()
        newSession.append(contentsOf: [Data([99])])
        XCTAssertEqual(newSession.next(), Data([99]))
    }

    func testUnexpectedCompletionClosesSession() {
        var queue = LinkWriteQueue()
        XCTAssertFalse(queue.complete(succeeded: true))
        XCTAssertTrue(queue.failed)
    }
}
