package chat.omcp.app.ui.vm

import androidx.lifecycle.ViewModel
import chat.omcp.app.backup.CloudBackupStore
import chat.omcp.app.backup.RestoreLockout
import chat.omcp.app.core.BackupMeta
import chat.omcp.app.core.BackupTooNewException
import chat.omcp.app.core.WrongBackupPasswordException
import chat.omcp.app.restore.PendingRestore
import chat.omcp.app.restore.RestoreCoordinator
import chat.omcp.app.restore.RestoreErrorKind
import chat.omcp.app.restore.RestoreErrors
import chat.omcp.app.restore.RestoreProgress
import chat.omcp.app.restore.RestoreRunner
import chat.omcp.app.restore.RestoreStage
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import chat.omcp.app.data.BACKUP_DRIVE
import chat.omcp.app.data.BACKUP_WRITTEN
import chat.omcp.app.data.MeshRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlin.random.Random

/**
 * Flow (apps/REQUIREMENTS.md "Onboarding & recovery backup (v0.1.1)"):
 * Welcome → Profile → Secure your account → Done (4 screens, O1).
 * Side paths: WriteDown → Quiz (3 random words); RestorePhrase (24-word fallback, always available).
 *
 * Restore from backup (DESIGN_SPEC §5L, §5.83–5.90): RestoreChecking (silent Drive probe, 3 s,
 * any failure → Welcome) → RestoreDetect → RestorePassword → RestoreProgress → RestoreSuccess;
 * manual path Welcome › Restore → RestoreSource (→ RestoreNoBackup); errors → RestoreError.
 */
enum class OnboardingStep {
    RestoreChecking, Welcome, RestoreDetect, RestoreSource, RestoreNoBackup, RestorePassword,
    RestoreProgress, RestoreSuccess, RestoreError, RestorePhrase, Profile, Secure, WriteDown, Quiz, Done,
}

data class QuizField(val position: Int, val input: String = "", val ok: Boolean = false, val error: Boolean = false)

data class OnboardingState(
    val step: OnboardingStep = OnboardingStep.Welcome,
    val displayName: String = "",
    val busy: Boolean = false,
    val error: String? = null,
    // secure
    val driveAvailable: Boolean = false,
    val drivePassword: String = "",
    val drivePasswordOk: Boolean = false,
    val drivePasswordTooShort: Boolean = false,
    val driveDone: Boolean = false,
    val writtenDone: Boolean = false,
    val words: List<String> = emptyList(),
    val quiz: List<QuizField> = emptyList(),
    // restore
    val foundBackup: BackupMeta? = null,
    val foundBackupSize: Int = 0,
    val serviceLabel: String = "Google Drive",
    val freshConfirm: Boolean = false,
    val restorePassword: String = "",
    /** Argon2id running: spinner on the Restore button. */
    val restoreBusy: Boolean = false,
    val restoreWrong: Boolean = false,
    val restoreTriesLeft: Int = RestoreLockout.TRIES_PER_ROUND,
    val restoreNextLockMinutes: Int = RestoreLockout.LOCKOUT_MINUTES.first(),
    /** > 0 while the security pause runs (minutes, rounded up). */
    val restoreLockMinutes: Int = 0,
    val restoreError: RestoreErrorKind? = null,
    val restoreStage: RestoreStage = RestoreStage.Download,
    val restoreFraction: Float = 0f,
    val restoredName: String? = null,
    val restoredContacts: Int = 0,
    val restoredVerified: Int = 0,
    /** Drive is full: restore works, the next backup won't. */
    val quotaWarning: Boolean = false,
    val restorePhrase: String = "",
) {
    val anyBackupDone get() = driveDone || writtenDone
    val quizOk get() = quiz.size == 3 && quiz.all { it.ok }
}

class OnboardingViewModel(
    private val repo: MeshRepository,
    private val cloud: CloudBackupStore,
    private val random: Random = Random.Default,
    private val clock: () -> Long = System::currentTimeMillis,
    private val deviceHint: String = "Android",
    private val restore: RestoreCoordinator = RestoreCoordinator(repo),
    private val runner: RestoreRunner = RestoreRunner { restore.run() },
    private val detectTimeoutMs: Long = DETECT_TIMEOUT_MS,
) : ViewModel() {
    private val core get() = repo.core
    private val lockout = RestoreLockout(repo.appStore, clock)
    private var pendingBlob: ByteArray? = null
    private var lastAction: () -> Unit = {}
    private val _state = MutableStateFlow(
        OnboardingState(
            step = if (cloud.available && repo.core.identity() == null) OnboardingStep.RestoreChecking else OnboardingStep.Welcome,
            driveAvailable = cloud.available,
            serviceLabel = cloud.label,
        ),
    )
    val state: StateFlow<OnboardingState> = _state.asStateFlow()

    init {
        restore.listen(::onRestoreProgress)
        refreshLock()
    }

    companion object {
        /** §5.83: first-launch probe gives up after 3 s and shows Welcome. */
        const val DETECT_TIMEOUT_MS = 3_000L
    }

    /**
     * Fresh install (§5.83 primary path): silently look for a Drive backup with the account already
     * on the device (`drive.appdata` only, no consent UI). Not found, offline, no consent, timeout,
     * unreadable — all fall back to Welcome without an error. Blocking: call off the main thread.
     */
    fun detectBackup() {
        if (_state.value.step != OnboardingStep.RestoreChecking) return
        val pool = Executors.newSingleThreadExecutor()
        val found = try {
            val blob = pool.submit<ByteArray?> { cloud.downloadSilently() }.get(detectTimeoutMs, TimeUnit.MILLISECONDS)
            blob?.let { b -> core.backupInfo(b)?.let { b to it } }
        } catch (_: Exception) {
            null
        } finally {
            pool.shutdownNow()
        }
        _state.update {
            if (it.step != OnboardingStep.RestoreChecking) it
            else if (found == null) it.copy(step = OnboardingStep.Welcome)
            else {
                pendingBlob = found.first
                it.copy(step = OnboardingStep.RestoreDetect, foundBackup = found.second, foundBackupSize = found.first.size)
            }
        }
    }

    fun getStarted() = _state.update { it.copy(step = OnboardingStep.Profile) }
    fun setName(n: String) = _state.update { it.copy(displayName = n.take(64)) }
    private fun name() = _state.value.displayName.trim().ifEmpty { "Me" }

    /** Profile → Secure. Identity + phrase are generated here ("in the background"). */
    fun profileNext() {
        val s = _state.value
        if (core.identity() == null) {
            val phrase = runCatching { core.generateRecoveryPhrase() }.getOrElse {
                _state.update { it.copy(error = "Couldn’t create your identity. Try again.") }
                return
            }
            repo.saveIdentity(phrase, name())
        }
        _state.update { s.copy(step = OnboardingStep.Secure, error = null) }
    }

    // ---- Google Drive ----
    fun setDrivePassword(pw: String) {
        val c = core.checkBackupPassword(pw)
        _state.update { it.copy(drivePassword = pw, drivePasswordOk = c.ok, drivePasswordTooShort = c.tooShort) }
    }

    fun backupToDrive(): Boolean {
        val s = _state.value
        if (!s.drivePasswordOk || !cloud.available) return false
        return runCatching {
            val blob = core.createBackup(s.drivePassword, deviceHint, clock())
            cloud.upload(blob)
            repo.appStore.lastCloudBackupMs = clock()
            repo.markBackupMethod(BACKUP_DRIVE)
            _state.update { it.copy(driveDone = true, drivePassword = "", error = null) }
        }.onFailure { e -> _state.update { it.copy(error = e.message ?: "Backup failed") } }.isSuccess
    }

    // ---- Write it down + quiz ----
    fun writeItDown() = _state.update {
        it.copy(step = OnboardingStep.WriteDown, words = core.recoveryPhrase().split(" "))
    }

    fun startQuiz() = _state.update { it.copy(step = OnboardingStep.Quiz, quiz = pickPositions().map { p -> QuizField(p) }) }

    /** 3 random positions, never consecutive, never all in one 3-word row. */
    internal fun pickPositions(): List<Int> {
        while (true) {
            val p = (0 until 24).shuffled(random).take(3).sorted()
            if (p.zipWithNext().none { (a, b) -> b - a == 1 } && p.map { it / 3 }.toSet().size > 1) return p
        }
    }

    fun setQuizInput(i: Int, input: String) = _state.update { s ->
        s.copy(
            quiz = s.quiz.mapIndexed { j, f ->
                if (j != i) f else f.copy(input = input, ok = input.trim().equals(s.words[f.position], true), error = false)
            },
        )
    }

    fun submitQuiz(): Boolean {
        val s = _state.value
        if (!s.quizOk) {
            _state.update { it.copy(quiz = it.quiz.map { f -> f.copy(error = !f.ok) }) }
            return false
        }
        repo.markBackupMethod(BACKUP_WRITTEN)
        _state.update { it.copy(step = OnboardingStep.Secure, writtenDone = true, words = emptyList(), quiz = emptyList()) }
        return true
    }

    fun backToSecure() = _state.update { it.copy(step = OnboardingStep.Secure, words = emptyList(), quiz = emptyList()) }

    fun secureNext(): Boolean {
        if (!_state.value.anyBackupDone) return false
        _state.update { it.copy(step = OnboardingStep.Done) }
        return true
    }

    /** "Remind me later": 24 h of use with a persistent banner (O6). */
    fun remindLater() {
        repo.remindBackupLater(clock())
        _state.update { it.copy(step = OnboardingStep.Done) }
    }

    fun finish() = repo.completeOnboarding()

    // ---- Restore (§5L) ----
    /** Welcome › Restore: choose Google Drive or the 24 words (§5.83 fallback). */
    fun openRestoreSource() = _state.update { it.copy(step = OnboardingStep.RestoreSource, error = null, freshConfirm = false) }

    /**
     * Google Drive chosen (or "Other account" / "Allow access" / "Try again"): runs the Google
     * account picker + `drive.appdata` consent, then looks for the backup. Blocking (IO thread).
     */
    fun chooseDrive() {
        lastAction = ::chooseDrive
        _state.update { it.copy(restoreBusy = true, freshConfirm = false) }
        val blob = try {
            cloud.download()
        } catch (e: Exception) {
            return fail(RestoreErrors.classify(e))
        }
        if (blob == null) {
            _state.update { it.copy(step = OnboardingStep.RestoreNoBackup, restoreBusy = false) }
            return
        }
        val meta = try {
            core.backupInfo(blob)
        } catch (e: BackupTooNewException) {
            return fail(RestoreErrorKind.TooNew)
        } ?: return fail(RestoreErrorKind.Corrupted)
        pendingBlob = blob
        _state.update {
            it.copy(step = OnboardingStep.RestoreDetect, foundBackup = meta, foundBackupSize = blob.size, restoreBusy = false, restoreError = null)
        }
    }

    private fun fail(kind: RestoreErrorKind) =
        _state.update { it.copy(step = OnboardingStep.RestoreError, restoreError = kind, restoreBusy = false) }

    /** Detected card › Restore. Checks Drive quota first (§5.88 "Restore anyway"). Blocking (IO). */
    fun restoreTapped() {
        if (pendingBlob == null) return
        lastAction = ::restoreTapped
        if (runCatching { cloud.storageFull() }.getOrDefault(false)) {
            _state.update { it.copy(quotaWarning = true) }
            return fail(RestoreErrorKind.QuotaFull)
        }
        toPassword()
    }

    private fun toPassword() {
        refreshLock()
        _state.update { it.copy(step = OnboardingStep.RestorePassword, restoreError = null, restoreBusy = false, error = null) }
    }

    // Start fresh confirm (§5.84): default focus is "Restore instead".
    fun startFresh() = _state.update { it.copy(freshConfirm = true) }
    fun restoreInstead() = _state.update { it.copy(freshConfirm = false) }
    fun confirmStartFresh() {
        pendingBlob = null
        _state.update { it.copy(freshConfirm = false, foundBackup = null, step = OnboardingStep.Profile) }
    }

    fun setRestorePassword(pw: String) = _state.update { it.copy(restorePassword = pw, restoreWrong = false) }

    /** Recomputes the persisted lockout (call every few seconds while locked). */
    fun refreshLock() = _state.update {
        it.copy(
            restoreLockMinutes = lockout.remainingMinutes(),
            restoreTriesLeft = lockout.triesLeft(),
            restoreNextLockMinutes = lockout.nextLockMinutes(),
        )
    }

    /** Password › Restore: Argon2id + decrypt on device, then the restore job. Blocking (IO). */
    fun restoreFromCloud(): Boolean {
        val blob = pendingBlob ?: return false
        refreshLock()
        if (lockout.locked() || _state.value.restoreBusy) return false
        _state.update { it.copy(restoreBusy = true, restoreWrong = false) }
        val r = try {
            core.openBackup(blob, _state.value.restorePassword)
        } catch (e: WrongBackupPasswordException) {
            lockout.onFailure()
            _state.update { it.copy(restoreBusy = false, restoreWrong = lockout.triesLeft() < RestoreLockout.TRIES_PER_ROUND, restorePassword = "") }
            refreshLock()
            return false
        } catch (e: Exception) {
            _state.update { it.copy(restorePassword = "") }
            fail(RestoreErrors.classify(e))
            return false
        }
        lockout.onSuccess()
        restore.prepare(PendingRestore(blob.size, r, name()))
        pendingBlob = null
        _state.update {
            it.copy(step = OnboardingStep.RestoreProgress, restoreBusy = false, restorePassword = "", restoreWrong = false, error = null)
        }
        runner.start()
        return true
    }

    private fun onRestoreProgress(p: RestoreProgress) = _state.update {
        when (p) {
            is RestoreProgress.Running -> it.copy(restoreStage = p.stage, restoreFraction = p.fraction)
            is RestoreProgress.Done -> it.copy(
                step = OnboardingStep.RestoreSuccess, restoreFraction = 1f,
                restoredName = p.name, restoredContacts = p.contacts, restoredVerified = p.verified,
            )
            is RestoreProgress.Failed -> it.copy(step = OnboardingStep.RestoreError, restoreError = p.kind)
        }
    }

    /** Error screen primary button (§5.88). [RestoreErrorKind.TooNew] opens the store in the UI. */
    fun errorPrimary() {
        when (_state.value.restoreError) {
            RestoreErrorKind.Offline -> lastAction()
            RestoreErrorKind.PermissionRevoked -> chooseDrive() // "Allow access" re-runs consent
            RestoreErrorKind.QuotaFull -> toPassword() // "Restore anyway"
            RestoreErrorKind.Corrupted -> usePhraseInstead()
            RestoreErrorKind.TooNew, null -> Unit
        }
    }

    /** Welcome back › Open my chats. */
    fun openChats() = finish()

    fun usePhraseInstead() = _state.update { it.copy(step = OnboardingStep.RestorePhrase, error = null, freshConfirm = false) }
    fun setRestorePhrase(p: String) = _state.update { it.copy(restorePhrase = p) }

    fun restoreFromPhrase(): Boolean {
        val p = _state.value.restorePhrase.trim().lowercase().split(Regex("\\s+")).joinToString(" ")
        if (!core.validateRecoveryPhrase(p)) {
            _state.update { it.copy(error = "These words don’t form a valid 24-word phrase. Check the order.") }
            return false
        }
        repo.restoreFromPhrase(p, name())
        repo.markBackupMethod(BACKUP_WRITTEN)
        _state.update { it.copy(step = OnboardingStep.Done, restorePhrase = "", error = null) }
        return true
    }

    fun backToWelcome() = _state.update { it.copy(step = OnboardingStep.Welcome, error = null, restoreError = null) }
}
