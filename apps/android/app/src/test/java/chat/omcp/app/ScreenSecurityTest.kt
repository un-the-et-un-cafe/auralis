package chat.omcp.app

import android.app.Activity
import android.view.WindowManager
import chat.omcp.app.security.ScreenSecurity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * SS1/SS2 and updated B3 (no screenshots anywhere). JVM-level: the lifecycle hook secures
 * every activity before content, and the source never clears FLAG_SECURE or opts a dialog
 * out. A black screenshot itself is a device check (see README).
 */
class ScreenSecurityTest {
    private val src = File("src/main")

    @Test fun `flag is FLAG_SECURE`() = assertEquals(WindowManager.LayoutParams.FLAG_SECURE, ScreenSecurity.FLAGS)

    @Test fun `every activity is secured on pre-create and create`() {
        val seen = mutableListOf<Activity>()
        val cb = ScreenSecurity.Callbacks { seen += it }
        val main = Activity()
        val scanner = Activity() // stands in for the third-party QR CaptureActivity
        cb.onActivityPreCreated(main, null)
        cb.onActivityCreated(scanner, null)
        assertEquals(listOf(main, scanner), seen)
    }

    @Test fun `application registers the hook`() {
        val app = File(src, "java/chat/omcp/app/OmcpApplication.kt").readText()
        assertTrue(app.contains("registerActivityLifecycleCallbacks(chat.omcp.app.security.ScreenSecurity.Callbacks())"))
    }

    @Test fun `no code clears FLAG_SECURE or opts a window out`() {
        val kt = src.walk().filter { it.extension == "kt" }.map { it.readText() }.joinToString("\n")
        assertFalse(kt.contains("clearFlags(WindowManager.LayoutParams.FLAG_SECURE"))
        assertFalse(kt.contains("SecureFlagPolicy.SecureOff"))
        assertFalse(kt.contains("setRecentsScreenshotEnabled(true)"))
    }

    @Test fun `no screenshot notice remains for burn after read`() {
        val all = src.walk().filter { it.isFile && (it.extension == "kt" || it.extension == "xml") }.map { it.readText() }.joinToString("\n")
        assertFalse(all.contains("Took a screenshot"))
        assertFalse(all.contains("📸"))
    }
}
