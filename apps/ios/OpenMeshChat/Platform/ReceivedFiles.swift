import Foundation
import HoplaKit

/// App-private durable sink. Canonicalize only the trusted OS container anchor;
/// peer data never enters a directory or filename, and all descendant opens are no-follow.
final class ReceivedFiles: FileSink {
    private let sink: DurableReceivedFiles
    private let root: URL
    init(root: URL? = nil) {
        let anchor = URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true).resolvingSymlinksInPath()
        self.root = root ?? anchor.appendingPathComponent("Library/Application Support/received", isDirectory: true)
        sink = DurableReceivedFiles(root: self.root, prepareRoot: { directory in
            var directory = directory
            var values = URLResourceValues(); values.isExcludedFromBackup = true
            try directory.setResourceValues(values)
        })
    }
    func finalize(_ receipt: ReceiveReceiptInfo, readBlock: (UInt32) throws -> Data, verifier: ReceiveFileVerifier) throws -> String {
        try sink.finalize(receipt, readBlock: readBlock, verifier: verifier)
    }
    func discardCancelled(_ receipt: ReceiveReceiptInfo) throws { try sink.discardCancelled(receipt) }
    func readForResend(_ path: String) throws -> Data? { try sink.readForResend(path) }
    func isDefinitelyMissing(_ path: String) throws -> Bool { try sink.isDefinitelyMissing(path) }
    // Backup restoration is separately verified by Rust. It must not reuse a
    // pending receipt ID; the legacy core setter enforces that boundary.
    func save(_ file: ReceivedFileInfo) -> String? { sink.save(file) }
    static func safe(_ name: String) -> String {
        let last = name.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? ""
        let cleaned = last.unicodeScalars.filter { $0.value >= 0x20 && $0 != ":" }
        var s = String(String.UnicodeScalarView(cleaned)).trimmingCharacters(in: .whitespaces)
        while s.hasPrefix(".") { s.removeFirst() }
        return String(s.prefix(120))
    }
}
