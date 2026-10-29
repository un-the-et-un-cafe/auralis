package chat.omcp.app

import chat.omcp.app.core.*
import chat.omcp.app.data.*
import java.io.File
import org.junit.Assert.*
import org.junit.Test

class StoreOpenFailureTest {
    @Test fun failedOpenPreservesDatabaseAndJournalAndCanRetry() {
        val dir = kotlin.io.path.createTempDirectory("store-refusal").toFile()
        try {
            val db = File(dir, "store.db").apply { writeBytes(byteArrayOf(1, 2, 3)) }
            val journal = File(dir, "store.db-journal").apply { writeBytes(byteArrayOf(4, 5)) }
            val fake = FakeOmcpCore()
            var fail = true
            var opens = 0
            val core = object : OmcpCore by fake {
                override fun open(phrase: String, displayName: String, dbPath: String, storageKey: ByteArray): Identity {
                    opens++
                    if (fail) throw StorageUnavailableException("locked")
                    return fake.load(phrase, displayName)
                }
            }
            val store = InMemoryAppStore().apply {
                recoveryPhrase = fake.generateRecoveryPhrase()
                storageKey = ByteArray(32) { 7 }
            }
            val repo = MeshRepository(core, store, db.path)
            assertFalse(repo.retryRestore())
            assertTrue(repo.storageBlocked.value)
            assertEquals(1, opens)
            assertFalse(repo.retryRestore())
            assertTrue(repo.storageBlocked.value)
            assertEquals(2, opens)
            assertArrayEquals(byteArrayOf(1, 2, 3), db.readBytes())
            assertArrayEquals(byteArrayOf(4, 5), journal.readBytes())
            assertEquals(2, dir.listFiles()!!.size)
            fail = false
            assertTrue(repo.retryRestore())
            assertFalse(repo.storageBlocked.value)
            assertEquals(3, opens)
        } finally { dir.deleteRecursively() }
    }

    @Test fun missingKeyNeverReplacesExistingStoreKey() {
        val dir = kotlin.io.path.createTempDirectory("missing-key").toFile()
        try {
            val db = File(dir, "store.db").apply { writeText("existing") }
            val core = FakeOmcpCore()
            val store = InMemoryAppStore().apply { recoveryPhrase = core.generateRecoveryPhrase() }
            val repo = MeshRepository(core, store, db.path)
            assertFalse(repo.retryRestore())
            assertTrue(repo.storageBlocked.value)
            assertNull(store.storageKey)
            assertNull(core.identity())
            assertEquals("existing", db.readText())
        } finally { dir.deleteRecursively() }
    }

    @Test fun missingIdentityWithExistingStoreBlocksOnboarding() {
        val dir = kotlin.io.path.createTempDirectory("missing-identity").toFile()
        try {
            val db = File(dir, "store.db").apply { writeText("existing") }
            val repo = MeshRepository(FakeOmcpCore(), InMemoryAppStore(), db.path)
            assertFalse(repo.retryRestore())
            assertTrue(repo.storageBlocked.value)
            assertFalse(repo.onboarded.value)
        } finally { dir.deleteRecursively() }
    }

    @Test fun unavailableSealedSecretsAreNotTreatedAsAbsent() {
        val dir = kotlin.io.path.createTempDirectory("sealed-unavailable").toFile()
        try {
            val memory = InMemoryAppStore()
            var missingPhrase = true
            var missingKey = false
            val store = object : AppStore by memory {
                override val hasSavedIdentity: Boolean get() = missingPhrase || memory.recoveryPhrase != null
                override val hasSavedStorageKey: Boolean get() = missingKey || memory.storageKey != null
            }
            val core = FakeOmcpCore()
            val repo = MeshRepository(core, store, File(dir, "absent.db").path)
            assertFalse(repo.retryRestore())
            assertTrue(repo.storageBlocked.value)
            assertNull(store.recoveryPhrase)
            assertNull(store.storageKey)
            missingPhrase = false
            memory.recoveryPhrase = core.generateRecoveryPhrase()
            missingKey = true
            assertFalse(repo.retryRestore())
            assertTrue(repo.storageBlocked.value)
            assertNull(store.storageKey)
            assertNull(core.identity())
            assertTrue(dir.listFiles()!!.isEmpty())
        } finally { dir.deleteRecursively() }
    }

    @Test fun newInstallRemainsEligibleForOnboarding() {
        val repo = MeshRepository(FakeOmcpCore(), InMemoryAppStore())
        assertFalse(repo.retryRestore())
        assertFalse(repo.storageBlocked.value)
    }
}
