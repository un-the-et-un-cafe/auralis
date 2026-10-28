import Foundation
import XCTest
@testable import HoplaKit

final class LinkWriteQueueTests: XCTestCase {
    func testWritesRemainOrderedAndCommitWaitsForFinalATTCompletion() {
        var queue = LinkWriteQueue()
        XCTAssertTrue(queue.append(contentsOf: [Data([1]), Data([2])], batchToken: "batch"))
        XCTAssertNil(queue.readyToken)
        XCTAssertEqual(queue.next(), Data([1]))
        XCTAssertNil(queue.next())
        XCTAssertTrue(queue.complete(succeeded: true))
        XCTAssertNil(queue.readyToken)
        XCTAssertEqual(queue.next(), Data([2]))
        XCTAssertNil(queue.readyToken, "submission alone is not acceptance")
        XCTAssertFalse(queue.isEmpty)
        XCTAssertTrue(queue.complete(succeeded: true))
        XCTAssertEqual(queue.readyToken, "batch")
        XCTAssertNil(queue.next())
        XCTAssertTrue(queue.committed("batch"))
        XCTAssertNil(queue.batchToken)
    }

    func testFailureAtFirstMiddleOrFinalPduNeverCommitsOrReusesCiphertext() {
        for failedIndex in 0..<3 {
            var queue = LinkWriteQueue()
            XCTAssertTrue(queue.append(contentsOf: [Data([1]), Data([2]), Data([3])], batchToken: "batch"))
            for index in 0...failedIndex {
                XCTAssertEqual(queue.next(), Data([UInt8(index + 1)]))
                XCTAssertEqual(queue.complete(succeeded: index != failedIndex), index != failedIndex)
            }
            XCTAssertTrue(queue.failed)
            XCTAssertTrue(queue.isEmpty)
            XCTAssertNil(queue.readyToken)
            XCTAssertNil(queue.next())
            XCTAssertFalse(queue.committed("batch"))
            XCTAssertFalse(queue.append(contentsOf: [Data([4])], batchToken: "new"))
            XCTAssertFalse(queue.complete(succeeded: true), "late callbacks cannot resurrect the session")
        }
    }

    func testReceiveOnlyResultWhileWriteOutstandingPreservesTokenAndOrder() {
        var queue = LinkWriteQueue()
        queue.append(contentsOf: [Data([1]), Data([2])], batchToken: "handshake")
        XCTAssertEqual(queue.next(), Data([1]))
        XCTAssertTrue(queue.append(contentsOf: [], batchToken: nil))
        XCTAssertEqual(queue.batchToken, "handshake")
        XCTAssertTrue(queue.inFlight)
        XCTAssertTrue(queue.complete(succeeded: true))
        XCTAssertEqual(queue.next(), Data([2]))
        XCTAssertTrue(queue.complete(succeeded: true))
        XCTAssertEqual(queue.readyToken, "handshake")
    }

    func testOverlappingOrMalformedBatchCannotAcknowledgeDifferentWork() {
        var queue = LinkWriteQueue()
        XCTAssertFalse(queue.append(contentsOf: [Data([1])], batchToken: nil))
        XCTAssertFalse(queue.append(contentsOf: [], batchToken: "empty"))
        XCTAssertFalse(queue.append(contentsOf: [Data([1])], batchToken: ""))
        XCTAssertTrue(queue.append(contentsOf: [Data([1])], batchToken: "original"))
        XCTAssertFalse(queue.append(contentsOf: [Data([2])], batchToken: "replacement"))
        XCTAssertFalse(queue.committed("original"), "not accepted yet")
        XCTAssertEqual(queue.next(), Data([1]))
        XCTAssertTrue(queue.complete(succeeded: true))
        XCTAssertFalse(queue.committed("replacement"))
        XCTAssertEqual(queue.readyToken, "original")
        XCTAssertTrue(queue.committed("original"))
        XCTAssertFalse(queue.committed("original"), "duplicate local retirement cannot affect the next batch")
        queue.append(contentsOf: [Data([2])], batchToken: "next")
        XCTAssertFalse(queue.committed("original"))
        XCTAssertEqual(queue.next(), Data([2]))
    }

    func testPeripheralBackpressureRetainsIdenticalBytesAndTokenAtEveryPosition() {
        for blockedIndex in 0..<3 {
            var queue = LinkWriteQueue()
            queue.append(contentsOf: [Data([1]), Data([2]), Data([3])], batchToken: "notify")
            for index in 0..<3 {
                let bytes = Data([UInt8(index + 1)])
                XCTAssertEqual(queue.next(), bytes)
                if index == blockedIndex {
                    queue.deferUnaccepted()
                    XCTAssertNil(queue.readyToken)
                    XCTAssertEqual(queue.batchToken, "notify")
                    XCTAssertEqual(queue.next(), bytes, "updateValue(false) must retry the same ciphertext")
                }
                XCTAssertTrue(queue.complete(succeeded: true))
            }
            XCTAssertEqual(queue.readyToken, "notify")
        }
    }

    func testCommitFailureAndShutdownCannotReleaseNextBatch() {
        var queue = LinkWriteQueue()
        queue.append(contentsOf: [Data([1])], batchToken: "database-fails")
        XCTAssertNotNil(queue.next())
        XCTAssertTrue(queue.complete(succeeded: true))
        // A thrown core commit must close, never call committed or flush another batch.
        XCTAssertFalse(queue.append(contentsOf: [Data([2])], batchToken: "too-early"))
        queue.fail()
        XCTAssertNil(queue.readyToken)
        XCTAssertFalse(queue.complete(succeeded: true))
        XCTAssertFalse(queue.committed("database-fails"))
    }

    func testStaleCompletionWithoutInFlightOperationIsHarmless() {
        var queue = LinkWriteQueue()
        XCTAssertFalse(queue.complete(succeeded: true))
        XCTAssertFalse(queue.failed)
        queue.append(contentsOf: [Data([1])], batchToken: "one")
        XCTAssertNotNil(queue.next())
        XCTAssertTrue(queue.complete(succeeded: true))
        XCTAssertFalse(queue.complete(succeeded: true))
        XCTAssertEqual(queue.readyToken, "one")
        queue.fail()
        XCTAssertFalse(queue.complete(succeeded: true))
        var replacement = LinkWriteQueue()
        replacement.append(contentsOf: [Data([9])], batchToken: "replacement")
        XCTAssertEqual(replacement.next(), Data([9]))
    }
}
