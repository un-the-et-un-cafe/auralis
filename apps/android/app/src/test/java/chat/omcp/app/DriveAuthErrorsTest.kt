package chat.omcp.app

import chat.omcp.app.backup.DriveAuthErrors
import org.junit.Assert.assertTrue
import org.junit.Test

class DriveAuthErrorsTest {
    @Test fun developerErrorShowsCodeAndHint() {
        val m = DriveAuthErrors.forStatus(10, "10: ")
        assertTrue(m, m.contains("DEVELOPER_ERROR") && m.contains("[code 10]") && m.contains("SHA-1"))
    }

    @Test fun unknownStatusStillShowsCode() {
        val m = DriveAuthErrors.forStatus(12500)
        assertTrue(m, m.contains("[code 12500]"))
    }

    @Test fun canceledResultExplainsTestUsers() {
        val m = DriveAuthErrors.forResultCode(0)
        assertTrue(m, m.contains("RESULT_CANCELED") && m.contains("test user"))
    }
}
