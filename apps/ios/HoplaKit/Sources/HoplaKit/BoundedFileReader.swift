import Foundation

/// Reads picker bytes off-main without trusting a stale/absent file-size attribute.
/// The byte cap comes from the core. Growth after confirmation cannot trigger an unbounded
/// Data(contentsOf:) allocation before the repository gets a chance to enforce that cap.
public enum BoundedFileReader {
    public static func read(_ url: URL, maxBytes: UInt64) throws -> Data {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var result = Data()
        while let chunk = try handle.read(upToCount: 65_536), !chunk.isEmpty {
            let size = UInt64(result.count) + UInt64(chunk.count)
            guard size <= maxBytes else { throw CoreError.fileTooLarge(size: size, max: maxBytes) }
            result.append(chunk)
        }
        return result
    }
}
