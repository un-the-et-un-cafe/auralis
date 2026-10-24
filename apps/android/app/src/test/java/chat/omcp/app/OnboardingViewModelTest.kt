package chat.omcp.app

import chat.omcp.app.backup.InMemoryCloudBackupStore
import chat.omcp.app.core.FakeOmcpCore
import chat.omcp.app.data.BackupGate
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.data.REMIND_WINDOW_MS
import chat.omcp.app.ui.vm.OnboardingStep
import chat.omcp.app.ui.vm.OnboardingViewModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.random.Random

class OnboardingViewModelTest {
    private var now = 1_000_000L
    private val store = InMemoryAppStore()
    private val repo = MeshRepository(FakeOmcpCore(), store) { now }
    private val cloud = InMemoryCloudBackupStore()
    private fun vm(r: MeshRepository = repo, c: InMemoryCloudBackupStore = cloud) =
        OnboardingViewModel(r, c, Random(7), { now })

    @Test
    fun `O1 cloud backup path reaches chats in 4 screens`() {
        val vm = vm()
        val seen = mutableListOf(vm.state.value.step)
        vm.getStarted(); seen += vm.state.value.step
        vm.setName("Alice"); vm.profileNext(); seen += vm.state.value.step
        vm.setDrivePassword("correct horse battery")
        assertTrue(vm.backupToDrive())
        assertTrue(vm.secureNext()); seen += vm.state.value.step
        vm.finish()
        assertEquals(listOf(OnboardingStep.Welcome, OnboardingStep.Profile, OnboardingStep.Secure, OnboardingStep.Done), seen)
        assertTrue(repo.onboarded.value)
        assertEquals(BackupGate.Ok, repo.backupGate())
        assertEquals(1, cloud.uploads)
    }

    @Test
    fun `O2 uploaded blob does not contain the phrase`() {
        val vm = vm()
        vm.getStarted(); vm.profileNext()
        vm.setDrivePassword("correct horse battery")
        vm.backupToDrive()
        val phrase = repo.core.recoveryPhrase()
        val blob = String(cloud.blob!!)
        assertFalse(blob.contains(phrase))
        phrase.split(" ").forEach { assertFalse(blob.contains(it)) }
    }

    @Test
    fun `secure step requires a method and weak passwords are rejected`() {
        val vm = vm()
        vm.getStarted(); vm.profileNext()
        assertFalse(vm.secureNext())
        vm.setDrivePassword("short")
        assertFalse(vm.state.value.drivePasswordOk)
        assertFalse(vm.backupToDrive())
    }

    @Test
    fun `write it down quiz uses 3 non-consecutive positions across rows`() {
        val vm = vm()
        vm.getStarted(); vm.profileNext()
        repeat(50) {
            val p = vm.pickPositions()
            assertEquals(3, p.size)
            assertTrue(p.zipWithNext().none { (a, b) -> b - a == 1 })
            assertTrue(p.map { it / 3 }.toSet().size > 1)
        }
        vm.writeItDown()
        val words = vm.state.value.words
        assertEquals(24, words.size)
        vm.startQuiz()
        vm.state.value.quiz.forEachIndexed { i, _ -> vm.setQuizInput(i, "nope") }
        assertFalse(vm.submitQuiz())
        assertTrue(vm.state.value.quiz.all { it.error })
        vm.state.value.quiz.forEachIndexed { i, f -> vm.setQuizInput(i, words[f.position].uppercase()) }
        assertTrue(vm.submitQuiz())
        assertTrue(vm.state.value.writtenDone)
        assertTrue(vm.state.value.words.isEmpty())
    }

    @Test
    fun `O6 remind later gives 24h banner then blocks`() {
        val vm = vm()
        vm.getStarted(); vm.profileNext(); vm.remindLater(); vm.finish()
        assertEquals(BackupGate.Banner, repo.backupGate())
        now += REMIND_WINDOW_MS - 1
        assertEquals(BackupGate.Banner, repo.backupGate())
        now += 2
        assertEquals(BackupGate.Required, repo.backupGate())
        // Completing a method clears the block.
        val v2 = vm()
        v2.getStarted(); v2.profileNext()
        v2.setDrivePassword("correct horse battery"); v2.backupToDrive()
        assertEquals(BackupGate.Ok, repo.backupGate())
    }

    @Test
    fun `O3 reinstall restores identity contacts and verification from cloud`() {
        val vm = vm()
        vm.getStarted(); vm.setName("Alice"); vm.profileNext()
        val bob = "b".repeat(64)
        val c = repo.addContact("omcp:1:$bob:Bob", verified = true)
        vm.setDrivePassword("correct horse battery"); vm.backupToDrive()
        val oldId = repo.core.identity()!!.peerIdHex

        // "Reinstall": fresh core + store, same cloud.
        val repo2 = MeshRepository(FakeOmcpCore(), InMemoryAppStore()) { now }
        val vm2 = vm(repo2)
        vm2.detectBackup()
        assertEquals(OnboardingStep.RestorePrompt, vm2.state.value.step)
        assertNotNull(vm2.state.value.foundBackup)
        vm2.setRestorePassword("correct horse battery")
        assertTrue(vm2.restoreFromCloud())
        assertEquals(oldId, repo2.core.identity()!!.peerIdHex)
        val restored = repo2.contacts.value.single()
        assertEquals(c.id, restored.id)
        assertTrue(restored.verified)
    }

    @Test
    fun `O4 wrong password fails cleanly with rate limit and phrase fallback works`() {
        val vm = vm()
        vm.getStarted(); vm.profileNext()
        vm.setDrivePassword("correct horse battery"); vm.backupToDrive()
        val phrase = repo.core.recoveryPhrase()

        val repo2 = MeshRepository(FakeOmcpCore(), InMemoryAppStore()) { now }
        val vm2 = vm(repo2)
        vm2.detectBackup()
        repeat(3) {
            vm2.setRestorePassword("wrong password!")
            assertFalse(vm2.restoreFromCloud())
        }
        assertNotNull(vm2.state.value.error)
        assertTrue(vm2.state.value.restoreLockMs > 0)
        // Locked: even the right password is refused until the delay passes.
        vm2.setRestorePassword("correct horse battery")
        assertFalse(vm2.restoreFromCloud())
        assertNull(repo2.core.identity())
        // Phrase fallback always works.
        vm2.usePhraseInstead()
        vm2.setRestorePhrase(phrase)
        assertTrue(vm2.restoreFromPhrase())
        assertEquals(repo.core.identity()!!.peerIdHex, repo2.core.identity()!!.peerIdHex)
    }
}
