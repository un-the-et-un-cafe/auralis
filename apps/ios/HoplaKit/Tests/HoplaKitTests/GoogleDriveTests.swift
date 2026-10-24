import Foundation
import XCTest
@testable import HoplaKit

/// Drive appDataFolder + OAuth (PKCE) against fakes; no network.
final class GoogleDriveTests: XCTestCase {
    let config = GoogleOAuthConfig(clientId: "123-abc.apps.googleusercontent.com")!

    final class FakeGoogle: HTTPTransport {
        var file: (id: String, blob: Data)?
        var calls: [String] = []
        var auths: [String?] = []
        var expireNext = false
        var tokenBodies: [String] = []
        var refreshOK = true

        func send(method: String, url: URL, headers: [String: String], body: Data?) throws -> (status: Int, body: Data) {
            let s = url.absoluteString
            calls.append("\(method) \(s.components(separatedBy: "?")[0])")
            if s.hasPrefix("https://oauth2.googleapis.com/token") {
                let b = String(decoding: body ?? Data(), as: UTF8.self)
                tokenBodies.append(b)
                if b.contains("grant_type=refresh_token") && !refreshOK { return (400, Data()) }
                let rt = b.contains("authorization_code") ? ",\"refresh_token\":\"rt1\"" : ""
                return (200, Data("{\"access_token\":\"at\(tokenBodies.count)\",\"expires_in\":3600\(rt)}".utf8))
            }
            auths.append(headers["Authorization"])
            if expireNext { expireNext = false; return (401, Data()) }
            switch method {
            case "GET" where s.contains("spaces=appDataFolder"):
                XCTAssertTrue(s.removingPercentEncoding!.contains("omcp-backup-v1.cbor"))
                let files = file.map { "[{\"id\":\"\($0.id)\"}]" } ?? "[]"
                return (200, Data("{\"files\":\(files)}".utf8))
            case "POST" where s.contains("uploadType=multipart"):
                let text = String(decoding: body!, as: UTF8.self)
                XCTAssertTrue(text.contains("\"parents\":[\"appDataFolder\"]"))
                let start = body!.range(of: Data("application/octet-stream\r\n\r\n".utf8))!.upperBound
                let end = body!.range(of: Data("\r\n--omcp-backup-boundary--".utf8))!.lowerBound
                file = ("f1", body!.subdata(in: start..<end))
                return (200, Data("{\"id\":\"f1\"}".utf8))
            case "PATCH": file = (file!.id, body!); return (200, Data("{}".utf8))
            case "GET" where s.contains("alt=media"): return (200, file!.blob)
            case "DELETE": file = nil; return (204, Data())
            default: return (500, Data())
            }
        }
    }

    func testConfigFromInfoPlist() {
        XCTAssertNil(GoogleOAuthConfig.fromInfo([:]))
        XCTAssertNil(GoogleOAuthConfig.fromInfo(["GOOGLE_OAUTH_CLIENT_ID": ""]))
        XCTAssertNil(GoogleOAuthConfig.fromInfo(["GOOGLE_OAUTH_CLIENT_ID": "$(GOOGLE_OAUTH_CLIENT_ID)"]))
        XCTAssertNil(GoogleOAuthConfig.fromInfo(["GOOGLE_OAUTH_CLIENT_ID": "evil\"id.apps.googleusercontent.com"]))
        XCTAssertEqual(GoogleOAuthConfig.fromInfo(["GOOGLE_OAUTH_CLIENT_ID": "123-abc.apps.googleusercontent.com"]), config)
        XCTAssertEqual(config.redirectScheme, "com.googleusercontent.apps.123-abc")
        XCTAssertEqual(config.redirectURI, "com.googleusercontent.apps.123-abc:/oauth2redirect")
    }

    func testPkceS256MatchesRfc7636Vector() {
        XCTAssertEqual(PKCE.challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM")
        XCTAssertEqual(PKCE.verifier().count, 43)
    }

    func testAuthorizationUrlAndRedirectParsing() throws {
        let o = GoogleOAuth(config: config)
        let u = o.authorizationURL(state: "s1", challenge: "c1").absoluteString
        for part in ["client_id=123-abc.apps.googleusercontent.com", "response_type=code", "code_challenge=c1",
                     "code_challenge_method=S256", "state=s1", "drive.appdata"] {
            XCTAssertTrue(u.contains(part), part)
        }
        XCTAssertEqual(try o.code(fromRedirect: URL(string: "\(config.redirectURI)?state=s1&code=abc")!, state: "s1"), "abc")
        XCTAssertThrowsError(try o.code(fromRedirect: URL(string: "\(config.redirectURI)?state=zz&code=abc")!, state: "s1"))
        XCTAssertThrowsError(try o.code(fromRedirect: URL(string: "\(config.redirectURI)?error=access_denied")!, state: "s1"))
    }

    func testDriveRoundTripWithInteractiveSignInThenRefresh() throws {
        let api = FakeGoogle()
        let store = InMemoryRefreshTokenStore()
        var prompts = 0
        let tokens = GoogleTokenManager(config: config, http: api, store: store, authorize: { url, scheme in
            prompts += 1
            XCTAssertEqual(scheme, "com.googleusercontent.apps.123-abc")
            let state = URLComponents(url: url, resolvingAgainstBaseURL: false)!.queryItems!.first { $0.name == "state" }!.value!
            return URL(string: "com.googleusercontent.apps.123-abc:/oauth2redirect?state=\(state)&code=c0de")!
        })
        let drive = GoogleDriveBackupStore(config: config, http: api, tokens: tokens)
        XCTAssertTrue(drive.available)
        XCTAssertNil(try drive.download(), "download never prompts; no grant yet → nil")
        XCTAssertEqual(prompts, 0)
        let blob = Data([0, 1, 2, 255, 13, 10])
        try drive.upload(blob)
        XCTAssertEqual(prompts, 1)
        XCTAssertEqual(store.token, "rt1")
        XCTAssertTrue(api.tokenBodies[0].contains("code_verifier="))
        XCTAssertEqual(try drive.download(), blob)
        try drive.upload(Data([9]))
        XCTAssertTrue(api.calls.contains("PATCH https://www.googleapis.com/upload/drive/v3/files/f1"))
        // Expired access token → one refresh with the stored refresh token, no prompt.
        api.expireNext = true
        tokens.invalidate()
        XCTAssertEqual(try drive.download(), Data([9]))
        XCTAssertEqual(prompts, 1)
        XCTAssertTrue(api.tokenBodies.contains { $0.contains("grant_type=refresh_token") })
        try drive.delete()
        XCTAssertNil(try drive.download())
    }

    func testRevokedRefreshTokenIsClearedAndUnconfiguredIsUnavailable() throws {
        let api = FakeGoogle()
        api.refreshOK = false
        let store = InMemoryRefreshTokenStore("old")
        let tokens = GoogleTokenManager(config: config, http: api, store: store, authorize: { _, _ in throw GoogleOAuthError.cancelled })
        XCTAssertThrowsError(try tokens.accessToken(interactive: false))
        XCTAssertNil(store.token)
        let off = GoogleDriveBackupStore(config: nil, http: api, tokens: nil)
        XCTAssertFalse(off.available)
        XCTAssertNil(try off.download())
        XCTAssertThrowsError(try off.upload(Data([1])))
    }
}
