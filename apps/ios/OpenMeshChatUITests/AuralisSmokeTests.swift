import XCTest

/// Runs the actual app + Rust core on a clean simulator. No seeded identity, fake core,
/// screen-security bypass or screenshot assertions: assertions use the accessibility tree.
/// The single ordered journey deliberately retains state for its final relaunch check.
final class AuralisSmokeTests: XCTestCase {
    @MainActor
    func testFreshOnboardingCancellationNavigationSettingsAndRelaunch() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
        addUIInterruptionMonitor(withDescription: "System permissions") { alert in
            for label in ["Allow", "OK", "Don’t Allow", "Don't Allow"] {
                let button = alert.buttons[label]
                if button.exists { button.tap(); return true }
            }
            return false
        }
        app.launch()
        let started = app.buttons["onboarding.getStarted"]
        XCTAssertTrue(started.waitForExistence(timeout: 30), "Use an erased simulator; this tests a real fresh install.")
        XCTAssertGreaterThan(app.windows.firstMatch.frame.height, 480,
                             "The app must use the simulator's native geometry, not 320×480 legacy compatibility mode.")

        // Cancelling recovery must return to welcome, without retaining pasted words.
        tap(app.buttons["onboarding.restorePhrase"], in: app)
        let phrase = app.textViews["onboarding.phraseInput"]
        XCTAssertTrue(phrase.waitForExistence(timeout: 5))
        phrase.tap()
        phrase.typeText("discard these private words")
        tap(app.buttons["onboarding.restoreCancel"], in: app)
        tap(app.buttons["onboarding.restorePhrase"], in: app)
        XCTAssertEqual(phrase.value as? String, "")
        tap(app.buttons["onboarding.restoreCancel"], in: app)

        // Real four-stage onboarding, including opening and cancelling the recovery cards.
        tap(started, in: app)
        XCTAssertTrue(app.textFields["onboarding.displayName"].waitForExistence(timeout: 5))
        tap(app.buttons["onboarding.profileNext"], in: app)
        XCTAssertTrue(app.navigationBars["Secure your account"].waitForExistence(timeout: 10),
                      "Identity creation/persistence did not reach Secure your account. Check the real Keychain roundtrip test and app-host signing; do not treat this as an offscreen button.")
        tap(app.buttons["onboarding.showWords"], in: app)
        tap(app.buttons["onboarding.wordsCancel"], in: app)
        tap(app.buttons["onboarding.remindLater"], in: app)
        tap(app.buttons["onboarding.finish"], in: app)
        app.tap() // Allows the interruption monitor to respond to system permission sheets.
        if app.alerts.buttons["Got it"].waitForExistence(timeout: 5) { app.alerts.buttons["Got it"].tap() }
        XCTAssertTrue(app.tabBars.buttons["Conversations"].waitForExistence(timeout: 15))
        XCTAssertFalse(started.exists)

        // Enter/leave contact navigation without inventing a contact or sending a message.
        tap(app.buttons["chats.addContact"], in: app)
        XCTAssertTrue(app.navigationBars["Add contact"].waitForExistence(timeout: 5))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.navigationBars["Conversations"].waitForExistence(timeout: 5))
        app.tabBars.buttons["Mesh"].tap()
        XCTAssertTrue(app.navigationBars["Mesh"].waitForExistence(timeout: 5))
        app.tabBars.buttons["You"].tap()

        tap(app.buttons["you.backup"], in: app)
        XCTAssertTrue(app.navigationBars["Backup and restore"].waitForExistence(timeout: 5))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        tap(app.buttons["you.privacy"], in: app)
        XCTAssertTrue(app.navigationBars["Privacy"].waitForExistence(timeout: 5))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        tap(app.buttons["you.notifications"], in: app)
        let privateNotifications = app.switches["settings.privateNotifications"]
        XCTAssertTrue(privateNotifications.waitForExistence(timeout: 5))
        XCTAssertEqual(privateNotifications.value as? String, "0")
        privateNotifications.tap()
        XCTAssertEqual(privateNotifications.value as? String, "1")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        tap(app.buttons["you.storage"], in: app)
        XCTAssertTrue(app.navigationBars["Storage"].waitForExistence(timeout: 5))
        app.navigationBars.buttons.element(boundBy: 0).tap()

        // Backgrounding/relaunch keeps the real persisted identity, onboarding and privacy preference.
        XCUIDevice.shared.press(.home)
        app.activate()
        XCTAssertTrue(app.tabBars.buttons["You"].waitForExistence(timeout: 5))
        app.terminate()
        app.launch()
        XCTAssertTrue(app.tabBars.buttons["You"].waitForExistence(timeout: 15))
        XCTAssertFalse(started.exists)
        app.tabBars.buttons["You"].tap()
        tap(app.buttons["you.notifications"], in: app)
        XCTAssertEqual(privateNotifications.value as? String, "1")
    }

    @MainActor
    private func tap(_ element: XCUIElement, in app: XCUIApplication,
                     file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(element.waitForExistence(timeout: 15), file: file, line: line)
        for _ in 0..<8 {
            if element.isHittable { element.tap(); return }
            app.swipeUp()
        }
        XCTFail("Control is not hittable: \(element.identifier)", file: file, line: line)
    }
}
