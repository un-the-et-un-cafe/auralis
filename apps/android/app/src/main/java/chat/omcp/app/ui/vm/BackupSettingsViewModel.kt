package chat.omcp.app.ui.vm

import androidx.lifecycle.ViewModel
import chat.omcp.app.backup.CloudBackupStore
import chat.omcp.app.data.BACKUP_DRIVE
import chat.omcp.app.data.BACKUP_WRITTEN
import chat.omcp.app.data.MeshRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

data class BackupSettingsState(
    val driveAvailable: Boolean,
    val driveOn: Boolean,
    val writtenOn: Boolean,
    val lastBackupMs: Long,
    val password: String = "",
    val passwordOk: Boolean = false,
    val shownPhrase: List<String>? = null,
    val message: String? = null,
    val media: chat.omcp.app.core.MediaBackupPrefs = chat.omcp.app.core.MediaBackupPrefs(),
    val chatsBackedUpMs: Long? = null,
    /** Turning media off waits for the confirm dialog (§5.96 `--confirm-off`). */
    val confirmMediaOff: Boolean = false,
)

/** Settings › Backup: status, change password / re-upload, delete cloud backup, view phrase. */
class BackupSettingsViewModel(
    private val repo: MeshRepository,
    private val cloud: CloudBackupStore,
    private val clock: () -> Long = System::currentTimeMillis,
    private val deviceHint: String = "Android",
) : ViewModel() {
    private val store = repo.appStore
    private val _state = MutableStateFlow(snapshot())
    val state: StateFlow<BackupSettingsState> = _state.asStateFlow()

    private fun snapshot() = BackupSettingsState(
        cloud.available, BACKUP_DRIVE in store.backupMethods, BACKUP_WRITTEN in store.backupMethods, store.lastCloudBackupMs,
        media = runCatching { repo.core.mediaBackupSettings() }.getOrDefault(chat.omcp.app.core.MediaBackupPrefs()),
    )

    /** Media toggles. Turning the master off asks first; the delete runs off the main thread. */
    fun setMedia(p: chat.omcp.app.core.MediaBackupPrefs, io: (() -> Unit) -> Unit = { Thread(it).start() }) {
        if (_state.value.media.enabled && !p.enabled) { _state.update { it.copy(confirmMediaOff = true) }; return }
        applyMedia(p, io)
    }

    fun confirmMediaOff(io: (() -> Unit) -> Unit = { Thread(it).start() }) =
        applyMedia(_state.value.media.copy(enabled = false), io)

    fun keepMedia() = _state.update { it.copy(confirmMediaOff = false) }

    private fun applyMedia(p: chat.omcp.app.core.MediaBackupPrefs, io: (() -> Unit) -> Unit) {
        _state.update { it.copy(media = p, confirmMediaOff = false) }
        val sync = repo.chatBackup
        io {
            runCatching { if (sync != null) sync.setMedia(p) else repo.core.setMediaBackupSettings(p) }
                .onFailure { e -> _state.update { it.copy(message = e.message) } }
        }
    }

    /** "Back up chats now" (ignores the hourly / Wi-Fi rule). */
    fun backUpChatsNow(io: (() -> Unit) -> Unit = { Thread(it).start() }) {
        val sync = repo.chatBackup ?: return
        io {
            runCatching { sync.runText(onWifi = true, force = true) }
                .fold({ _state.update { it.copy(chatsBackedUpMs = clock()) } }, { e -> _state.update { it.copy(message = e.message) } })
        }
    }

    fun setPassword(pw: String) = _state.update { it.copy(password = pw, passwordOk = repo.core.checkBackupPassword(pw).ok) }

    /** Enable Drive backup or change its password (re-encrypt + re-upload). */
    fun uploadWithPassword(): Boolean {
        val s = _state.value
        if (!s.passwordOk || !cloud.available) return false
        return runCatching {
            cloud.upload(repo.core.createBackup(s.password, deviceHint, clock()))
            store.lastCloudBackupMs = clock()
            repo.markBackupMethod(BACKUP_DRIVE)
        }.fold(
            { _state.value = snapshot().copy(message = "Backed up"); true },
            { e -> _state.update { it.copy(message = e.message) }; false },
        )
    }

    /** O5: removes the remote record. */
    fun deleteCloudBackup(): Boolean = runCatching {
        cloud.delete()
        repo.markBackupMethod(BACKUP_DRIVE, add = false)
        store.lastCloudBackupMs = 0
    }.fold(
        { _state.value = snapshot().copy(message = "Cloud backup deleted"); true },
        { e -> _state.update { it.copy(message = e.message) }; false },
    )

    /** Call only after BiometricPrompt / device credential succeeded. */
    fun revealPhraseAfterAuth() = _state.update { it.copy(shownPhrase = repo.core.recoveryPhrase().split(" ")) }
    fun hidePhrase() = _state.update { it.copy(shownPhrase = null) }
}
