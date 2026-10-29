package chat.omcp.app

import android.system.Os
import android.system.OsConstants
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import chat.omcp.app.core.RustOmcpCore
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Exercises the real JNA/UniFFI/native core; a successful app launch is not proof. */
@RunWith(AndroidJUnit4::class)
class NativePageSizeTest {
    @Test fun nativeIdentityAndContactOperationsOnTheRequiredPageSize() {
        val configured = InstrumentationRegistry.getArguments().getString("expectedPageSize")
        val observed = Os.sysconf(OsConstants._SC_PAGESIZE)
        // A normal developer run is useful on either page size. CI always supplies
        // its exact expectation, so a 16-KB job cannot silently run on 4-KB pages.
        if (configured != null) {
            require(configured == "4096" || configured == "16384")
            assertEquals(configured.toLong(), observed)
        }
        assertTrue("Unsupported runtime page size: $observed", observed == 4096L || observed == 16384L)
        val alice = RustOmcpCore()
        val bob = RustOmcpCore()
        val phrase = alice.generateRecoveryPhrase()
        assertTrue(alice.validateRecoveryPhrase(phrase))
        val aliceIdentity = alice.load(phrase, "Page-size Alice")
        val bobIdentity = bob.load(bob.generateRecoveryPhrase(), "Page-size Bob")
        assertNotEquals(aliceIdentity.peerIdHex, bobIdentity.peerIdHex)
        val bobContact = alice.addContactFromQr(bob.qrPayload())
        val aliceContact = bob.addContactFromQr(alice.qrPayload())
        assertEquals(bobContact.safetyNumber, aliceContact.safetyNumber)
        assertEquals(32, alice.generateStorageKey().size)
        assertEquals(50_000_000L, alice.maxFileBytes())
    }
}
