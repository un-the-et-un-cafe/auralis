import Foundation
import HoplaKit

/// Saves verified received files under the app container
/// (`Application Support/received/<transfer>/<name>`, excluded from iCloud device backup and
/// protected until first unlock). Opening happens in-app via Quick Look; nothing is shared
/// until the user chooses to.
final class ReceivedFiles: FileSink {
    private let root: URL

    init(root: URL? = nil) {
        self.root = root ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("received", isDirectory: true)
    }

    func save(_ file: ReceivedFileInfo) -> String? {
        do {
            var dir = root.appendingPathComponent(Self.safe(file.transferId), isDirectory: true)
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            var values = URLResourceValues()
            values.isExcludedFromBackup = true
            try? dir.setResourceValues(values)
            let name = Self.safe(file.name)
            let url = dir.appendingPathComponent(name.isEmpty ? "file" : name)
            try file.bytes.write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
            return url.path
        } catch {
            return nil
        }
    }

    /// Strip path separators and control characters from a peer-supplied name.
    static func safe(_ name: String) -> String {
        let last = name.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? ""
        let cleaned = last.unicodeScalars.filter { $0.value >= 0x20 && $0 != ":" }
        var s = String(String.UnicodeScalarView(cleaned)).trimmingCharacters(in: .whitespaces)
        while s.hasPrefix(".") { s.removeFirst() }
        return String(s.prefix(120))
    }
}
