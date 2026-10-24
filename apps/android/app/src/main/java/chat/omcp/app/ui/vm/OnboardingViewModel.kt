package chat.omcp.app.ui.vm

import androidx.lifecycle.ViewModel
import chat.omcp.app.backup.CloudBackupStore
import chat.omcp.app.backup.RetryLimiter
import chat.omcp.app.core.BackupMeta
import chat.omcp.app.core.WrongBackupPasswordException
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
 * Side paths: WriteDown → Quiz (3 random words); RestorePrompt (backup found on
 * reinstall) and RestorePhrase (24-word fallback, always available).
 */
enum class OnboardingStep { Welcome, RestorePrompt, RestorePhrase, Profile, Secure, WriteDown, Quiz, Done }

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
    val restorePassword: String = "",
    val restoreLockMs: Long = 0,
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
) : ViewModel() {
    private val core get() = repo.core
    private val limiter = RetryLimiter(clock)
    private var pendingBlob: ByteArray? = null
    private val _state = MutableStateFlow(OnboardingState(driveAvailable = cloud.available))
    val state: StateFlow<OnboardingState> = _state.asStateFlow()

    /** Fresh install: look for an existing cloud backup ("Welcome back"). */
    fun detectBackup() {
        val blob = runCatching { cloud.download() }.getOrNull() ?: return
        val meta = core.backupInfo(blob) ?: return
        pendingBlob = blob
        _state.update { it.copy(step = OnboardingStep.RestorePrompt, foundBackup = meta) }
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

    // ---- Restore ----
    fun setRestorePassword(pw: String) = _state.update { it.copy(restorePassword = pw) }

    fun restoreFromCloud(): Boolean {
        val blob = pendingBlob ?: return false
        if (!limiter.canTry()) {
            _state.update { it.copy(restoreLockMs = limiter.remainingLockMs()) }
            return false
        }
        return try {
            val r = core.openBackup(blob, _state.value.restorePassword)
            limiter.onSuccess()
            repo.restoreFromBackup(r, name())
            _state.update { it.copy(step = OnboardingStep.Done, restorePassword = "", error = null) }
            true
        } catch (e: WrongBackupPasswordException) {
            limiter.onFailure()
            _state.update {
                it.copy(error = "Wrong backup password.", restoreLockMs = limiter.remainingLockMs(), restorePassword = "")
            }
            false
        }
    }

    fun usePhraseInstead() = _state.update { it.copy(step = OnboardingStep.RestorePhrase, error = null) }
    fun setRestorePhrase(p: String) = _state.update { it.copy(restorePhrase = p) }

    fun restoreFromPhrase(): Boolean {
        val p = _state.value.restorePhrase.trim().lowercase().split(Regex("\\s+")).joinToString(" ")
        if (!core.validateRecoveryPhrase(p)) {
            _state.update { it.copy(error = "These words don’t form a valid 24-word phrase. Check the order.") }
            return false
        }
        repo.saveIdentity(p, name())
        repo.markBackupMethod(BACKUP_WRITTEN)
        _state.update { it.copy(step = OnboardingStep.Done, restorePhrase = "", error = null) }
        return true
    }

    fun backToWelcome() = _state.update { it.copy(step = OnboardingStep.Welcome, error = null) }
}
