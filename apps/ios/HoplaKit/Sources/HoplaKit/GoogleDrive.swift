import Foundation

// Google Drive backup for iOS: OAuth 2.0 for native apps (authorization code + PKCE, no client
// secret, redirect on the reversed client ID scheme) and the Drive v3 REST calls against the
// app-private `appDataFolder`. Platform-independent so it is unit-tested on Linux; the app target
// supplies URLSession, ASWebAuthenticationSession and the Keychain (CloudBackupStores.swift).

/// HTTP seam (URLSession in the app, a fake in tests). Blocking; never call on the main thread.
public protocol HTTPTransport: AnyObject {
    func send(method: String, url: URL, headers: [String: String], body: Data?) throws -> (status: Int, body: Data)
}

/// OAuth client from build config: Info.plist `GOOGLE_OAUTH_CLIENT_ID` (set from an xcconfig,
/// never committed). Empty or an unexpanded `$(…)` placeholder → Drive backup is unavailable.
public struct GoogleOAuthConfig: Equatable, Sendable {
    public static let scope = "https://www.googleapis.com/auth/drive.appdata"
    public let clientId: String

    public init?(clientId: String?) {
        guard let id = clientId?.trimmingCharacters(in: .whitespaces), !id.isEmpty, !id.hasPrefix("$("),
              id.hasSuffix(".apps.googleusercontent.com"),
              id.allSatisfy({ $0.isLetter || $0.isNumber || $0 == "." || $0 == "-" || $0 == "_" }) else { return nil }
        self.clientId = id
    }

    public static func fromInfo(_ info: [String: Any]?) -> GoogleOAuthConfig? {
        GoogleOAuthConfig(clientId: info?["GOOGLE_OAUTH_CLIENT_ID"] as? String)
    }

    /// `com.googleusercontent.apps.<id>`: must also be a CFBundleURLTypes scheme.
    public var redirectScheme: String {
        "com.googleusercontent.apps." + clientId.replacingOccurrences(of: ".apps.googleusercontent.com", with: "")
    }
    public var redirectURI: String { redirectScheme + ":/oauth2redirect" }
}

public enum GoogleOAuthError: Error, Equatable {
    case cancelled
    case stateMismatch
    case denied(String)
    case badResponse(Int)
    case needsSignIn
}

/// RFC 7636 helpers.
public enum PKCE {
    public static func verifier() -> String {
        var g = SystemRandomNumberGenerator()
        return base64url(Data((0..<32).map { _ in UInt8.random(in: 0...255, using: &g) }))
    }
    public static func challenge(_ verifier: String) -> String { base64url(SHA256.hash(Data(verifier.utf8))) }
    public static func base64url(_ d: Data) -> String {
        d.base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}

public struct GoogleToken: Equatable, Sendable {
    public var accessToken: String
    public var expiresAt: Date
    public var refreshToken: String?
}

/// Builds and parses the OAuth requests (no I/O).
public struct GoogleOAuth {
    public static let authEndpoint = "https://accounts.google.com/o/oauth2/v2/auth"
    public static let tokenEndpoint = URL(string: "https://oauth2.googleapis.com/token")!
    public let config: GoogleOAuthConfig
    public init(config: GoogleOAuthConfig) { self.config = config }

    public func authorizationURL(state: String, challenge: String) -> URL {
        var c = URLComponents(string: Self.authEndpoint)!
        c.queryItems = [
            .init(name: "client_id", value: config.clientId),
            .init(name: "redirect_uri", value: config.redirectURI),
            .init(name: "response_type", value: "code"),
            .init(name: "scope", value: GoogleOAuthConfig.scope),
            .init(name: "code_challenge", value: challenge),
            .init(name: "code_challenge_method", value: "S256"),
            .init(name: "state", value: state),
        ]
        return c.url!
    }

    public func code(fromRedirect url: URL, state: String) throws -> String {
        let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        func v(_ n: String) -> String? { items.first { $0.name == n }?.value }
        if let e = v("error") { throw GoogleOAuthError.denied(e) }
        guard v("state") == state else { throw GoogleOAuthError.stateMismatch }
        guard let code = v("code"), !code.isEmpty else { throw GoogleOAuthError.denied("no code") }
        return code
    }

    public func exchangeBody(code: String, verifier: String) -> Data {
        form(["grant_type": "authorization_code", "code": code, "code_verifier": verifier,
              "client_id": config.clientId, "redirect_uri": config.redirectURI])
    }

    public func refreshBody(_ refreshToken: String) -> Data {
        form(["grant_type": "refresh_token", "refresh_token": refreshToken, "client_id": config.clientId])
    }

    public func parseToken(_ data: Data, now: Date) throws -> GoogleToken {
        guard let j = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let access = j["access_token"] as? String else { throw GoogleOAuthError.badResponse(200) }
        let ttl = (j["expires_in"] as? NSNumber)?.doubleValue ?? 3600
        return GoogleToken(accessToken: access, expiresAt: now.addingTimeInterval(ttl - 60), refreshToken: j["refresh_token"] as? String)
    }

    private func form(_ p: [String: String]) -> Data {
        var allowed = CharacterSet.alphanumerics
        allowed.insert(charactersIn: "-._~")
        return Data(p.sorted { $0.key < $1.key }
            .map { "\($0.key)=\($0.value.addingPercentEncoding(withAllowedCharacters: allowed) ?? "")" }
            .joined(separator: "&").utf8)
    }
}

/// Where the refresh token lives (device-only Keychain item in the app).
public protocol RefreshTokenStore: AnyObject {
    func load() -> String?
    func save(_ token: String)
    func clear()
}

public final class InMemoryRefreshTokenStore: RefreshTokenStore {
    public var token: String?
    public init(_ token: String? = nil) { self.token = token }
    public func load() -> String? { token }
    public func save(_ token: String) { self.token = token }
    public func clear() { token = nil }
}

/// Access tokens for `drive.appdata`: cached, refreshed with the stored refresh token, or obtained
/// interactively through `authorize` (shows Google's consent page and returns the redirect URL).
public final class GoogleTokenManager {
    private let oauth: GoogleOAuth
    private let http: HTTPTransport
    private let store: RefreshTokenStore
    private let authorize: (URL, String) throws -> URL
    private let now: () -> Date
    private var cached: GoogleToken?
    private let lock = NSLock()

    /// `authorize(url, callbackScheme)` must present the page and return the redirect URL.
    public init(config: GoogleOAuthConfig, http: HTTPTransport, store: RefreshTokenStore,
                authorize: @escaping (URL, String) throws -> URL, now: @escaping () -> Date = Date.init) {
        oauth = GoogleOAuth(config: config)
        self.http = http
        self.store = store
        self.authorize = authorize
        self.now = now
    }

    public var hasGrant: Bool { store.load() != nil }

    public func accessToken(interactive: Bool) throws -> String {
        lock.lock(); defer { lock.unlock() }
        if let c = cached, c.expiresAt > now() { return c.accessToken }
        if let rt = store.load() {
            let (status, body) = try http.send(method: "POST", url: GoogleOAuth.tokenEndpoint,
                                               headers: ["Content-Type": "application/x-www-form-urlencoded"],
                                               body: oauth.refreshBody(rt))
            if (200..<300).contains(status) {
                let t = try oauth.parseToken(body, now: now())
                cached = t
                return t.accessToken
            }
            if status == 400 || status == 401 { store.clear() } else { throw GoogleOAuthError.badResponse(status) }
        }
        guard interactive else { throw GoogleOAuthError.needsSignIn }
        let verifier = PKCE.verifier()
        let state = PKCE.verifier()
        let redirect = try authorize(oauth.authorizationURL(state: state, challenge: PKCE.challenge(verifier)), oauth.config.redirectScheme)
        let code = try oauth.code(fromRedirect: redirect, state: state)
        let (status, body) = try http.send(method: "POST", url: GoogleOAuth.tokenEndpoint,
                                           headers: ["Content-Type": "application/x-www-form-urlencoded"],
                                           body: oauth.exchangeBody(code: code, verifier: verifier))
        guard (200..<300).contains(status) else { throw GoogleOAuthError.badResponse(status) }
        let t = try oauth.parseToken(body, now: now())
        if let rt = t.refreshToken { store.save(rt) }
        cached = t
        return t.accessToken
    }

    public func invalidate() {
        lock.lock(); defer { lock.unlock() }
        cached = nil
    }

    /// Sign out of Drive backup on this device (the remote file is untouched).
    public func signOut() {
        invalidate()
        store.clear()
    }
}

public struct DriveHTTPError: Error, Equatable { public let status: Int }

/// Drive v3 against `appDataFolder`: one file holding the Rust-sealed backup blob.
public final class DriveAppDataClient {
    public static let fileName = "omcp-backup-v1.cbor"
    private static let api = "https://www.googleapis.com/drive/v3/files"
    private static let upload = "https://www.googleapis.com/upload/drive/v3/files"
    private let http: HTTPTransport
    private let token: () throws -> String
    private let invalidate: () -> Void

    public init(http: HTTPTransport, token: @escaping () throws -> String, invalidate: @escaping () -> Void = {}) {
        self.http = http
        self.token = token
        self.invalidate = invalidate
    }

    public func findId() throws -> String? {
        var c = URLComponents(string: Self.api)!
        c.queryItems = [.init(name: "spaces", value: "appDataFolder"),
                        .init(name: "q", value: "name = '\(Self.fileName)' and trashed = false"),
                        .init(name: "fields", value: "files(id,modifiedTime)"), .init(name: "pageSize", value: "10")]
        let d = try call("GET", c.url!, [:], nil)
        let j = try JSONSerialization.jsonObject(with: d) as? [String: Any]
        return ((j?["files"] as? [[String: Any]])?.first?["id"]) as? String
    }

    public func upload(_ blob: Data) throws {
        if let id = try findId() {
            _ = try call("PATCH", URL(string: "\(Self.upload)/\(id)?uploadType=media")!, ["Content-Type": "application/octet-stream"], blob)
            return
        }
        let boundary = "omcp-backup-boundary"
        var body = Data("--\(boundary)\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n".utf8)
        body.append(Data("{\"name\":\"\(Self.fileName)\",\"parents\":[\"appDataFolder\"],\"mimeType\":\"application/octet-stream\"}\r\n".utf8))
        body.append(Data("--\(boundary)\r\nContent-Type: application/octet-stream\r\n\r\n".utf8))
        body.append(blob)
        body.append(Data("\r\n--\(boundary)--\r\n".utf8))
        _ = try call("POST", URL(string: "\(Self.upload)?uploadType=multipart&fields=id")!,
                     ["Content-Type": "multipart/related; boundary=\(boundary)"], body)
    }

    public func download() throws -> Data? {
        guard let id = try findId() else { return nil }
        return try call("GET", URL(string: "\(Self.api)/\(id)?alt=media")!, [:], nil)
    }

    public func delete() throws {
        guard let id = try findId() else { return }
        _ = try call("DELETE", URL(string: "\(Self.api)/\(id)")!, [:], nil)
    }

    private func call(_ method: String, _ url: URL, _ headers: [String: String], _ body: Data?) throws -> Data {
        var r = try http.send(method: method, url: url, headers: headers.merging(["Authorization": "Bearer \(try token())"]) { $1 }, body: body)
        if r.status == 401 {
            invalidate()
            r = try http.send(method: method, url: url, headers: headers.merging(["Authorization": "Bearer \(try token())"]) { $1 }, body: body)
        }
        guard (200..<300).contains(r.status) else { throw DriveHTTPError(status: r.status) }
        return r.body
    }
}

/// `CloudBackupStore` for Google Drive. Available when a client ID is configured. `download()`
/// never shows UI (a fresh launch must not pop Google sign-in; the refresh token survives app
/// reinstall in the Keychain); `upload`/`delete` may ask the user to sign in.
public final class GoogleDriveBackupStore: CloudBackupStore {
    public let method = backupDrive
    private let config: GoogleOAuthConfig?
    private let tokens: GoogleTokenManager?
    private let http: HTTPTransport

    public init(config: GoogleOAuthConfig?, http: HTTPTransport, tokens: GoogleTokenManager?) {
        self.config = config
        self.http = http
        self.tokens = tokens
    }

    public var available: Bool { config != nil && tokens != nil }

    private func client(interactive: Bool) throws -> DriveAppDataClient {
        guard let tokens, config != nil else { throw CoreError.other("Google Drive backup not configured in this build") }
        return DriveAppDataClient(http: http, token: { try tokens.accessToken(interactive: interactive) }, invalidate: tokens.invalidate)
    }

    public func upload(_ blob: Data) throws { try client(interactive: true).upload(blob) }
    public func download() throws -> Data? {
        guard available else { return nil }
        do { return try client(interactive: false).download() } catch GoogleOAuthError.needsSignIn { return nil }
    }
    public func delete() throws { try client(interactive: true).delete() }
}

/// Small SHA-256 (FIPS 180-4) for PKCE, so HoplaKit stays dependency-free on Linux too.
enum SHA256 {
    private static let k: [UInt32] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ]

    static func hash(_ data: Data) -> Data {
        var h: [UInt32] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]
        var msg = [UInt8](data)
        let bitLen = UInt64(msg.count) * 8
        msg.append(0x80)
        while msg.count % 64 != 56 { msg.append(0) }
        for i in (0..<8).reversed() { msg.append(UInt8((bitLen >> (UInt64(i) * 8)) & 0xff)) }
        func rotr(_ x: UInt32, _ n: UInt32) -> UInt32 { (x >> n) | (x << (32 - n)) }
        var w = [UInt32](repeating: 0, count: 64)
        for chunk in stride(from: 0, to: msg.count, by: 64) {
            for i in 0..<16 {
                let j = chunk + i * 4
                w[i] = UInt32(msg[j]) << 24 | UInt32(msg[j + 1]) << 16 | UInt32(msg[j + 2]) << 8 | UInt32(msg[j + 3])
            }
            for i in 16..<64 {
                let s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >> 3)
                let s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >> 10)
                w[i] = w[i - 16] &+ s0 &+ w[i - 7] &+ s1
            }
            var (a, b, c, d, e, f, g, hh) = (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7])
            for i in 0..<64 {
                let t1 = hh &+ (rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)) &+ ((e & f) ^ (~e & g)) &+ k[i] &+ w[i]
                let t2 = (rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)) &+ ((a & b) ^ (a & c) ^ (b & c))
                (hh, g, f, e, d, c, b, a) = (g, f, e, d &+ t1, c, b, a, t1 &+ t2)
            }
            h = [h[0] &+ a, h[1] &+ b, h[2] &+ c, h[3] &+ d, h[4] &+ e, h[5] &+ f, h[6] &+ g, h[7] &+ hh]
        }
        var out = Data()
        for v in h { out.append(contentsOf: [UInt8(v >> 24), UInt8((v >> 16) & 0xff), UInt8((v >> 8) & 0xff), UInt8(v & 0xff)]) }
        return out
    }
}
