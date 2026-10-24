package chat.omcp.app

import chat.omcp.app.backup.InMemoryCloudBackupStore
import chat.omcp.app.backup.RetryLimiter
import chat.omcp.app.core.FakeOmcpCore
import chat.omcp.app.data.BACKUP_DRIVE
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.ui.vm.BackupSettingsViewModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class BackupSettingsViewModelTest {
    private var now = 5_000L
    private val store = InMemoryAppStore()
    private val repo = MeshRepository(FakeOmcpCore(), store) { now }
        .also { it.saveIdentity((1..24).joinToString(" ") { i -> "word$i" }, "Alice") }
    private val cloud = InMemoryCloudBackupStore()

    @Test
    fun `O5 delete removes the remote record`() {
        val vm = BackupSettingsViewModel(repo, cloud, { now })
        vm.setPassword("correct horse battery")
        assertTrue(vm.uploadWithPassword())
        assertNotNull(cloud.download())
        assertTrue(vm.state.value.driveOn)
        assertTrue(vm.deleteCloudBackup())
        assertNull(cloud.download())
        assertFalse(BACKUP_DRIVE in store.backupMethods)
    }

    @Test
    fun `change password re-uploads`() {
        val vm = BackupSettingsViewModel(repo, cloud, { now })
        vm.setPassword("correct horse battery"); vm.uploadWithPassword()
        now += 10
        vm.setPassword("another long passphrase"); vm.uploadWithPassword()
        assertEquals(2, cloud.uploads)
        assertEquals(now, store.lastCloudBackupMs)
    }

    @Test
    fun `view phrase only after auth callback`() {
        val vm = BackupSettingsViewModel(repo, cloud, { now })
        assertNull(vm.state.value.shownPhrase)
        vm.revealPhraseAfterAuth()
        assertEquals(24, vm.state.value.shownPhrase!!.size)
        vm.hidePhrase()
        assertNull(vm.state.value.shownPhrase)
    }

    @Test
    fun `retry limiter backs off`() {
        var t = 0L
        val l = RetryLimiter { t }
        repeat(2) { l.onFailure(); assertTrue(l.canTry()) }
        l.onFailure()
        assertFalse(l.canTry())
        assertEquals(RetryLimiter.BASE_MS, l.remainingLockMs())
        t += RetryLimiter.BASE_MS
        assertTrue(l.canTry())
        l.onFailure()
        assertEquals(2 * RetryLimiter.BASE_MS, l.remainingLockMs())
    }
}
