import Foundation

/// BLAKE2b-256 (RFC 7693, unkeyed), used to name iCloud `MediaBlob` records by their ciphertext.
public enum Blake2b256 {
    private static let iv: [UInt64] = [
        0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1,
        0x510e527fade682d1, 0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
    ]
    private static let sigma: [[Int]] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
        [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
        [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
        [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
        [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
        [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
        [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
        [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
        [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
    ]

    @inline(__always) private static func rotr(_ x: UInt64, _ n: UInt64) -> UInt64 { (x >> n) | (x << (64 - n)) }

    private static func compress(_ h: inout [UInt64], _ block: ArraySlice<UInt8>, _ t: UInt64, _ last: Bool) {
        var m = [UInt64](repeating: 0, count: 16)
        let b = Array(block)
        for i in 0..<16 {
            var w: UInt64 = 0
            for j in 0..<8 { w |= UInt64(b[i * 8 + j]) << (8 * UInt64(j)) }
            m[i] = w
        }
        var v = h + iv
        v[12] ^= t
        if last { v[14] = ~v[14] }
        func g(_ a: Int, _ b: Int, _ c: Int, _ d: Int, _ x: UInt64, _ y: UInt64) {
            v[a] = v[a] &+ v[b] &+ x; v[d] = rotr(v[d] ^ v[a], 32)
            v[c] = v[c] &+ v[d]; v[b] = rotr(v[b] ^ v[c], 24)
            v[a] = v[a] &+ v[b] &+ y; v[d] = rotr(v[d] ^ v[a], 16)
            v[c] = v[c] &+ v[d]; v[b] = rotr(v[b] ^ v[c], 63)
        }
        for r in 0..<12 {
            let s = sigma[r % 10]
            g(0, 4, 8, 12, m[s[0]], m[s[1]]); g(1, 5, 9, 13, m[s[2]], m[s[3]])
            g(2, 6, 10, 14, m[s[4]], m[s[5]]); g(3, 7, 11, 15, m[s[6]], m[s[7]])
            g(0, 5, 10, 15, m[s[8]], m[s[9]]); g(1, 6, 11, 12, m[s[10]], m[s[11]])
            g(2, 7, 8, 13, m[s[12]], m[s[13]]); g(3, 4, 9, 14, m[s[14]], m[s[15]])
        }
        for i in 0..<8 { h[i] ^= v[i] ^ v[i + 8] }
    }

    public static func hash(_ data: Data) -> Data {
        var h = iv
        h[0] ^= 0x0101_0000 ^ 32
        let bytes = [UInt8](data)
        var off = 0
        while bytes.count - off > 128 {
            off += 128
            compress(&h, bytes[(off - 128)..<off], UInt64(off), false)
        }
        var last = [UInt8](repeating: 0, count: 128)
        let rest = bytes.count - off
        for i in 0..<rest { last[i] = bytes[off + i] }
        compress(&h, last[0..<128], UInt64(bytes.count), true)
        var out = Data(count: 32)
        for i in 0..<32 { out[i] = UInt8(truncatingIfNeeded: h[i / 8] >> (8 * UInt64(i % 8))) }
        return out
    }

    public static func hex(_ data: Data) -> String { hash(data).map { String(format: "%02x", $0) }.joined() }
}

/// One CloudKit record (REQUIREMENTS v0.1.4a): fields `blob` (CKAsset), `seq`, `created`; nothing else.
public struct ChatCloudRecord: Equatable, Sendable {
    public var type: String
    public var name: String
    public var blob: Data
    public var seq: UInt64
    public var createdMs: Int64
    public init(type: String, name: String, blob: Data, seq: UInt64, createdMs: Int64) {
        self.type = type; self.name = name; self.blob = blob; self.seq = seq; self.createdMs = createdMs
    }
}

/// Record storage in the private database, zone `HoplaBackupV1` (CloudKit in the app, memory in tests).
public protocol ChatRecordBackend: AnyObject {
    var available: Bool { get }
    func save(_ r: ChatCloudRecord) throws
    func fetch(_ name: String) throws -> ChatCloudRecord?
    func delete(_ name: String) throws
}

public final class InMemoryChatRecordBackend: ChatRecordBackend {
    public private(set) var records: [String: ChatCloudRecord] = [:]
    public let available = true
    public init() {}
    public func save(_ r: ChatCloudRecord) { records[r.name] = r }
    public func fetch(_ name: String) -> ChatCloudRecord? { records[name] }
    public func delete(_ name: String) { records[name] = nil }
}

/// iCloud record naming (REQUIREMENTS v0.1.4a, PROTOCOL §11.1).
public enum ICloudChatRecords {
    public static let zone = "HoplaBackupV1"
    public static let segmentType = "ChatLogSegment"
    public static let snapshotType = "ChatSnapshot"
    public static let mediaType = "MediaBlob"
    public static let manifestType = "BackupManifest"
    public static let manifestName = "manifest"

    /// ISO-8601 week of `ms` as YYYYWW (UTC).
    public static func isoWeek(_ ms: Int64) -> String {
        var cal = Calendar(identifier: .iso8601)
        cal.timeZone = TimeZone(identifier: "UTC")!
        let d = Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
        let c = cal.dateComponents([.yearForWeekOfYear, .weekOfYear], from: d)
        return String(format: "%04d%02d", c.yearForWeekOfYear ?? 0, c.weekOfYear ?? 0)
    }

    /// Core blob name (`hopla-chat-<seq>-log|snap.bin`, `hopla-media-<id>.bin`) → record.
    public static func record(coreName: String, blob: Data, createdMs: Int64) -> ChatCloudRecord? {
        if let seq = ChatBackupSync.seq(of: coreName) {
            if coreName.hasSuffix("-snap.bin") {
                return ChatCloudRecord(type: snapshotType, name: "snap-" + isoWeek(createdMs), blob: blob, seq: seq, createdMs: createdMs)
            }
            return ChatCloudRecord(type: segmentType, name: "seg-" + String(seq, radix: 16), blob: blob, seq: seq, createdMs: createdMs)
        }
        if coreName.hasPrefix(ChatBackupSync.mediaPrefix) {
            return ChatCloudRecord(type: mediaType, name: "media-" + Blake2b256.hex(blob), blob: blob, seq: 0, createdMs: createdMs)
        }
        return nil
    }
}

/// Index from the core's opaque blob names to record names, kept in the `manifest` record.
/// Core names are already opaque (sequence numbers and keyed hashes), so it reveals nothing
/// the record names don't.
public struct ChatBackupManifest: Codable, Equatable, Sendable {
    public var v = 1
    public var names: [String: String] = [:]
    public init() {}
}

/// `ChatBlobStore` over the v0.1.4a CloudKit records; `ChatBackupSync` keeps using core names.
public final class RecordChatBlobStore: ChatBlobStore {
    private let backend: ChatRecordBackend
    private let clock: () -> Int64
    private var cached: ChatBackupManifest?

    public init(backend: ChatRecordBackend, clock: @escaping () -> Int64 = { Int64(Date().timeIntervalSince1970 * 1000) }) {
        self.backend = backend; self.clock = clock
    }

    public var available: Bool { backend.available }

    private func manifest() throws -> ChatBackupManifest {
        if let cached { return cached }
        let m = try backend.fetch(ICloudChatRecords.manifestName)
            .flatMap { try? JSONDecoder().decode(ChatBackupManifest.self, from: $0.blob) } ?? ChatBackupManifest()
        cached = m
        return m
    }

    private func saveManifest(_ m: ChatBackupManifest) throws {
        let data = try JSONEncoder().encode(m)
        try backend.save(ChatCloudRecord(type: ICloudChatRecords.manifestType, name: ICloudChatRecords.manifestName,
                                         blob: data, seq: UInt64(m.names.count), createdMs: clock()))
        cached = m
    }

    public func list(prefix: String) throws -> [String] {
        try manifest().names.keys.filter { $0.hasPrefix(prefix) }.sorted()
    }

    public func put(_ name: String, _ blob: Data) throws {
        guard let r = ICloudChatRecords.record(coreName: name, blob: blob, createdMs: clock()) else {
            throw CoreError.other("unknown backup blob name")
        }
        var m = try manifest()
        // A newer snapshot in the same week replaces the record: drop the stale index entry.
        for (k, v) in m.names where v == r.name && k != name { m.names[k] = nil }
        try backend.save(r)
        m.names[name] = r.name
        try saveManifest(m)
    }

    public func get(_ name: String) throws -> Data? {
        guard let rn = try manifest().names[name] else { return nil }
        return try backend.fetch(rn)?.blob
    }

    public func delete(_ name: String) throws {
        var m = try manifest()
        guard let rn = m.names[name] else { return }
        m.names[name] = nil
        if !m.names.values.contains(rn) { try backend.delete(rn) }
        try saveManifest(m)
    }
}
