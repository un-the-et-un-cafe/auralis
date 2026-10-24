package chat.omcp.app.restore

import chat.omcp.app.core.RestoredBackup
import chat.omcp.app.data.MeshRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Progress stages (DESIGN_SPEC §5.86): Download → Decrypt → Contacts and chats. */
enum class RestoreStage { Download, Decrypt, Contacts }

sealed interface RestoreProgress {
    data class Running(val stage: RestoreStage, val fraction: Float) : RestoreProgress
    data class Done(val name: String?, val contacts: Int, val verified: Int) : RestoreProgress
    data class Failed(val kind: RestoreErrorKind) : RestoreProgress
}

/** What the restore job needs; held in memory only (the phrase is never written to WorkManager's DB). */
class PendingRestore(val blobSize: Int, val restored: RestoredBackup, val fallbackName: String)

/** Starts [RestoreCoordinator.run] somewhere that survives leaving the app (WorkManager on device). */
fun interface RestoreRunner { fun start() }

/**
 * Applies a decrypted backup. Download happened when the backup was found and decryption
 * (Argon2id + XChaCha20-Poly1305 in Rust) when the password was accepted; the job reports
 * those stages, then writes identity, contacts and verified badges (chat list is rebuilt
 * from contacts). Drive only — never mesh or gateways.
 */
class RestoreCoordinator(private val repo: MeshRepository) {
    @Volatile private var pending: PendingRestore? = null
    private val _progress = MutableStateFlow<RestoreProgress?>(null)
    val progress: StateFlow<RestoreProgress?> = _progress.asStateFlow()
    private val listeners = mutableListOf<(RestoreProgress) -> Unit>()

    fun listen(l: (RestoreProgress) -> Unit) { synchronized(listeners) { listeners += l } }

    private fun emit(p: RestoreProgress) {
        _progress.value = p
        synchronized(listeners) { listeners.toList() }.forEach { it(p) }
    }

    fun prepare(p: PendingRestore) {
        pending = p
        emit(RestoreProgress.Running(RestoreStage.Download, 0f))
    }

    fun hasPending() = pending != null

    /** Blocking; called by the worker (or directly in tests). */
    fun run(onStage: (RestoreStage) -> Unit = {}): RestoreProgress {
        // Nothing in memory (e.g. the process died and WorkManager re-ran the job): do nothing;
        // the user is back on the first-launch flow and can restore again.
        val p = pending ?: return RestoreProgress.Failed(RestoreErrorKind.Corrupted)
        return try {
            onStage(RestoreStage.Download); emit(RestoreProgress.Running(RestoreStage.Download, 1f / 3))
            onStage(RestoreStage.Decrypt); emit(RestoreProgress.Running(RestoreStage.Decrypt, 2f / 3))
            onStage(RestoreStage.Contacts); emit(RestoreProgress.Running(RestoreStage.Contacts, 2f / 3))
            val name = p.restored.displayName?.takeIf { it.isNotBlank() } ?: p.fallbackName
            repo.restoreFromBackup(p.restored, name)
            // Chats come from the separate chat backup; a failure here never undoes the identity restore.
            runCatching { repo.restoreChats() }
            pending = null
            RestoreProgress.Done(
                p.restored.displayName?.takeIf { it.isNotBlank() },
                p.restored.contacts.size,
                p.restored.contacts.count { it.verified },
            ).also(::emit)
        } catch (e: Exception) {
            RestoreProgress.Failed(RestoreErrors.classify(e)).also(::emit)
        }
    }
}
