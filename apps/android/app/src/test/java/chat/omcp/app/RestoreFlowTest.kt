package chat.omcp.app

import chat.omcp.app.backup.CloudUnavailableException
import chat.omcp.app.backup.DriveHttpException
import chat.omcp.app.backup.InMemoryCloudBackupStore
import chat.omcp.app.backup.RestoreLockout
import chat.omcp.app.core.BackupTooNewException
import chat.omcp.app.core.FakeOmcpCore
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.restore.RestoreCoordinator
import chat.omcp.app.restore.RestoreErrorKind
import chat.omcp.app.restore.RestoreErrors
import chat.omcp.app.restore.RestoreRunner
import chat.omcp.app.restore.RestoreStage
import chat.omcp.app.ui.vm.OnboardingStep
import chat.omcp.app.ui.vm.OnboardingViewModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException
import java.net.UnknownHostException
import kotlin.random.Random

/** DESIGN_SPEC §5L restore flow (§5.83–5.88). */
class RestoreFlowTest {
    private var now = 1_000_000L
    private val cloud = InMemoryCloudBackupStore()
    private val pw = "correct horse battery"

    /** Backs up an identity with one verified contact to [cloud]; returns the peer id. */
    private fun seedBackup(): String {
        val repo = MeshRepository(FakeOmcpCore(), InMemoryAppStore()) { now }
        val vm = OnboardingViewModel(repo, cloud, Random(1), { now })
        vm.getStarted(); vm.setName("Sam"); vm.profileNext()
        repo.addContact("omcp:1:${"c".repeat(64)}:Cleo", verified = true)
        vm.setDrivePassword(pw); assertTrue(vm.backupToDrive())
        return repo.core.identity()!!.peerIdHex
    }

    private val store = InMemoryAppStore()
    private val repo2 = MeshRepository(FakeOmcpCore(), store) { now }
    private fun fresh(runner: ((RestoreCoordinator) -> RestoreRunner)? = null, timeout: Long = 3_000): OnboardingViewModel {
        val rc = RestoreCoordinator(repo2)
        return OnboardingViewModel(repo2, cloud, Random(2), { now }, restore = rc, runner = runner?.invoke(rc) ?: RestoreRunner { rc.run() }, detectTimeoutMs = timeout)
    }

    @Test
    fun `detect finds the backup then password progress and welcome back`() {
        val id = seedBackup()
        val vm = fresh()
        assertEquals(OnboardingStep.RestoreChecking, vm.state.value.step)
        vm.detectBackup()
        assertEquals(OnboardingStep.RestoreDetect, vm.state.value.step)
        assertTrue(vm.state.value.foundBackupSize > 0)
        vm.restoreTapped()
        assertEquals(OnboardingStep.RestorePassword, vm.state.value.step)
        vm.setRestorePassword(pw)
        assertTrue(vm.restoreFromCloud())
        val s = vm.state.value
        assertEquals(OnboardingStep.RestoreSuccess, s.step)
        assertEquals(1, s.restoredContacts)
        assertEquals(1, s.restoredVerified)
        assertEquals(id, repo2.core.identity()!!.peerIdHex)
        assertTrue(repo2.contacts.value.single().verified)
        assertFalse(repo2.onboarded.value)
        vm.openChats()
        assertTrue(repo2.onboarded.value)
    }

    @Test
    fun `progress stages run download decrypt contacts in order`() {
        seedBackup()
        val stages = mutableListOf<RestoreStage>()
        val vm = fresh(runner = { rc -> RestoreRunner { rc.run { stages += it } } })
        vm.detectBackup(); vm.restoreTapped(); vm.setRestorePassword(pw); vm.restoreFromCloud()
        assertEquals(listOf(RestoreStage.Download, RestoreStage.Decrypt, RestoreStage.Contacts), stages)
    }

    @Test
    fun `detect failures fall back silently to Welcome`() {
        seedBackup()
        cloud.failWith = UnknownHostException("offline")
        val vm = fresh()
        vm.detectBackup()
        assertEquals(OnboardingStep.Welcome, vm.state.value.step)
        assertNull(vm.state.value.restoreError)
    }

    @Test
    fun `detect gives up after the timeout`() {
        val slow = object : chat.omcp.app.backup.CloudBackupStore {
            override val label = "Google Drive"
            override val available = true
            override fun upload(blob: ByteArray) = Unit
            override fun download(): ByteArray? { Thread.sleep(2_000); return null }
            override fun delete() = Unit
        }
        val vm = OnboardingViewModel(repo2, slow, Random(2), { now }, detectTimeoutMs = 50)
        val t0 = System.nanoTime()
        vm.detectBackup()
        assertTrue((System.nanoTime() - t0) / 1_000_000 < 1_500)
        assertEquals(OnboardingStep.Welcome, vm.state.value.step)
    }

    @Test
    fun `start fresh confirm defaults to restore instead`() {
        seedBackup()
        val vm = fresh()
        vm.detectBackup(); vm.startFresh()
        assertTrue(vm.state.value.freshConfirm)
        vm.restoreInstead()
        assertFalse(vm.state.value.freshConfirm)
        assertEquals(OnboardingStep.RestoreDetect, vm.state.value.step)
        vm.startFresh(); vm.confirmStartFresh()
        assertEquals(OnboardingStep.Profile, vm.state.value.step)
    }

    @Test
    fun `manual source no backup found`() {
        val vm = fresh()
        vm.detectBackup()
        vm.openRestoreSource()
        assertEquals(OnboardingStep.RestoreSource, vm.state.value.step)
        vm.chooseDrive()
        assertEquals(OnboardingStep.RestoreNoBackup, vm.state.value.step)
    }

    @Test
    fun `lockout escalates 5 15 60 and survives restart while words stay available`() {
        seedBackup()
        var vm = fresh()
        vm.detectBackup(); vm.restoreTapped()
        vm.setRestorePassword("nope nope"); vm.restoreFromCloud()
        assertEquals(2, vm.state.value.restoreTriesLeft)
        assertTrue(vm.state.value.restoreWrong)
        vm.setRestorePassword("nope nope"); vm.restoreFromCloud()
        vm.setRestorePassword("nope nope"); vm.restoreFromCloud()
        assertEquals(5, vm.state.value.restoreLockMinutes)
        // "Restart": new VM, same persisted store → still locked, right password refused.
        vm = fresh(); vm.detectBackup(); vm.restoreTapped()
        assertEquals(5, vm.state.value.restoreLockMinutes)
        vm.setRestorePassword(pw)
        assertFalse(vm.restoreFromCloud())
        assertNull(repo2.core.identity())
        for (expected in listOf(15, 60, 60)) {
            now += 61 * 60_000L
            vm.refreshLock()
            assertEquals(0, vm.state.value.restoreLockMinutes)
            repeat(3) { vm.setRestorePassword("nope nope"); vm.restoreFromCloud() }
            assertEquals(expected, vm.state.value.restoreLockMinutes)
        }
        assertEquals(listOf(5, 15, 60), RestoreLockout.LOCKOUT_MINUTES)
        vm.usePhraseInstead()
        assertEquals(OnboardingStep.RestorePhrase, vm.state.value.step)
    }

    @Test
    fun `quota full warns and restore anyway proceeds`() {
        seedBackup()
        cloud.full = true
        val vm = fresh()
        vm.detectBackup(); vm.restoreTapped()
        assertEquals(OnboardingStep.RestoreError, vm.state.value.step)
        assertEquals(RestoreErrorKind.QuotaFull, vm.state.value.restoreError)
        vm.errorPrimary()
        assertEquals(OnboardingStep.RestorePassword, vm.state.value.step)
        vm.setRestorePassword(pw); vm.restoreFromCloud()
        assertEquals(OnboardingStep.RestoreSuccess, vm.state.value.step)
        assertTrue(vm.state.value.quotaWarning)
    }

    @Test
    fun `corrupted backup changes nothing and offers the 24 words`() {
        cloud.upload("garbage".toByteArray())
        val vm = fresh()
        vm.detectBackup()
        assertEquals(OnboardingStep.Welcome, vm.state.value.step) // silent on first launch
        vm.openRestoreSource(); vm.chooseDrive()
        assertEquals(RestoreErrorKind.Corrupted, vm.state.value.restoreError)
        assertNull(repo2.core.identity())
        vm.errorPrimary()
        assertEquals(OnboardingStep.RestorePhrase, vm.state.value.step)
    }

    @Test
    fun `too new backup asks to update`() {
        cloud.upload("FAKEBK9|1|x|".toByteArray())
        val vm = fresh()
        vm.detectBackup()
        assertEquals(OnboardingStep.Welcome, vm.state.value.step)
        vm.openRestoreSource(); vm.chooseDrive()
        assertEquals(RestoreErrorKind.TooNew, vm.state.value.restoreError)
    }

    @Test
    fun `offline and permission errors after the user chose Restore`() {
        seedBackup()
        val vm = fresh()
        vm.detectBackup()
        vm.openRestoreSource()
        cloud.failWith = UnknownHostException("www.googleapis.com")
        vm.chooseDrive()
        assertEquals(RestoreErrorKind.Offline, vm.state.value.restoreError)
        cloud.failWith = CloudUnavailableException("Google Drive access was not granted")
        vm.errorPrimary() // Try again
        assertEquals(RestoreErrorKind.PermissionRevoked, vm.state.value.restoreError)
        cloud.failWith = null
        vm.errorPrimary() // Allow access → consent again → found
        assertEquals(OnboardingStep.RestoreDetect, vm.state.value.step)
    }

    @Test
    fun `error classification`() {
        assertEquals(RestoreErrorKind.Offline, RestoreErrors.classify(IOException("x")))
        assertEquals(RestoreErrorKind.PermissionRevoked, RestoreErrors.classify(DriveHttpException(401, "x")))
        assertEquals(RestoreErrorKind.PermissionRevoked, RestoreErrors.classify(DriveHttpException(403, "x")))
        assertEquals(RestoreErrorKind.Offline, RestoreErrors.classify(DriveHttpException(503, "x")))
        assertEquals(RestoreErrorKind.Offline, RestoreErrors.classify(CloudUnavailableException("Google Drive sign-in failed: NETWORK_ERROR [code 7]")))
        assertEquals(RestoreErrorKind.TooNew, RestoreErrors.classify(BackupTooNewException()))
        assertEquals(RestoreErrorKind.Corrupted, RestoreErrors.classify(IllegalStateException()))
    }
}
