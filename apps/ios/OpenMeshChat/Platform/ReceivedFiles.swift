import Foundation
import HoplaKit

/// App-private durable sink. Canonicalize only the trusted OS container anchor;
/// peer data never enters a directory or filename, and all descendant opens are no-follow.
final class ReceivedFiles: FileSink {
    private let suppliedRoot: URL?
    private let sinkLock = NSLock()
    private var sink: DurableReceivedFiles?

    init(root: URL? = nil) { suppliedRoot = root }

    private func resolvedSink() throws -> DurableReceivedFiles {
        sinkLock.lock(); defer { sinkLock.unlock() }
        if let sink { return sink }
        // Only the OS home anchor is resolved. A supplied root is deliberately
        // passed unchanged, and all appended descendants remain no-follow.
        let root: URL
        if let suppliedRoot { root = suppliedRoot }
        else {
            let anchor = try DurableReceivedFiles.physicalTrustedDirectory(
                URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true))
            root = anchor.appendingPathComponent("Library/Application Support/received", isDirectory: true)
        }
        let resolved = DurableReceivedFiles(root: root, prepareRoot: { directory in
            var directory = directory
            var values = URLResourceValues(); values.isExcludedFromBackup = true
            try directory.setResourceValues(values)
        })
        sink = resolved // Pin first successful anchor; failed resolution can retry.
        return resolved
    }
    func finalize(_ receipt: ReceiveReceiptInfo, readBlock: (UInt32) throws -> Data, verifier: ReceiveFileVerifier) throws -> String {
        try resolvedSink().finalize(receipt, readBlock: readBlock, verifier: verifier)
    }
    func discardCancelled(_ receipt: ReceiveReceiptInfo) throws { try resolvedSink().discardCancelled(receipt) }
    func readForResend(_ path: String) throws -> Data? { try resolvedSink().readForResend(path) }
    func isDefinitelyMissing(_ path: String) throws -> Bool { try resolvedSink().isDefinitelyMissing(path) }
    // Backup restoration is separately verified by Rust. It must not reuse a
    // pending receipt ID; the legacy core setter enforces that boundary.
    func save(_ file: ReceivedFileInfo) -> String? { try? resolvedSink().save(file) }
    static func safe(_ name: String) -> String {
        let last = name.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? ""
        let cleaned = last.unicodeScalars.filter { $0.value >= 0x20 && $0 != ":" }
        var s = String(String.UnicodeScalarView(cleaned)).trimmingCharacters(in: .whitespaces)
        while s.hasPrefix(".") { s.removeFirst() }
        return String(s.prefix(120))
    }
}
