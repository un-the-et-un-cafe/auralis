package chat.omcp.app.backup

import chat.omcp.app.core.ChatBackupSummaryInfo
import chat.omcp.app.core.MediaBackupPrefs
import chat.omcp.app.core.OmcpCore

/**
 * Named, already-encrypted blobs in the provider's app-private space (Drive `appDataFolder`).
 * Only Rust-sealed, 64 KiB-padded chat-backup blobs go here (REQUIREMENTS "Encrypted chat backup").
 */
interface ChatBlobStore {
    val available: Boolean
    fun list(prefix: String): List<String>
    fun put(name: String, blob: ByteArray)
    fun get(name: String): ByteArray?
    fun delete(name: String)
}

/** Drive `appDataFolder` (scope `drive.appdata` only), next to the identity backup file. */
class DriveChatBlobStore(
    clientId: String,
    private val drive: (interactive: Boolean) -> DriveAppDataClient,
) : ChatBlobStore {
    override val available = clientId.isNotBlank()
    private fun d() = if (available) drive(false) else throw CloudUnavailableException("Google Drive backup not configured in this build")
    override fun list(prefix: String) = d().listNames(prefix)
    override fun put(name: String, blob: ByteArray) = d().upload(name, blob)
    override fun get(name: String) = d().download(name)
    override fun delete(name: String) = d().delete(name)
}

/** Tests and the debug "simulated Drive" build. */
class InMemoryChatBlobStore : ChatBlobStore {
    val blobs = linkedMapOf<String, ByteArray>()
    override val available = true
    override fun list(prefix: String) = synchronized(blobs) { blobs.keys.filter { it.startsWith(prefix) } }
    override fun put(name: String, blob: ByteArray) { synchronized(blobs) { blobs[name] = blob.copyOf() } }
    override fun get(name: String) = synchronized(blobs) { blobs[name]?.copyOf() }
    override fun delete(name: String) { synchronized(blobs) { blobs.remove(name) } }
}

/** What a chat restore brought back (design §5.93 / §5.97 counts). */
data class ChatRestoreResult(val messages: Int, val mediaPlaceholders: Int, val mediaRestored: Int)

/**
 * Runs the encrypted chat backup: hourly text pass (Wi-Fi only by default) and the separate daily
 * media pass (CB5, off by default), plus restore. All crypto, padding and the "what changed"
 * bookkeeping are in Rust; this class only moves opaque blobs. Blocking: call off the main thread.
 * Never uses the mesh or gateways.
 */
class ChatBackupSync(
    private val core: OmcpCore,
    private val blobs: ChatBlobStore,
    private val readFile: (path: String) -> ByteArray?,
    private val clock: () -> Long = System::currentTimeMillis,
) {
    companion object {
        const val CHAT_PREFIX = "hopla-chat-"
        const val MEDIA_PREFIX = "hopla-media-"

        /** `hopla-chat-<seq 16 digits>-<kind>.bin` → seq. */
        fun seqOf(name: String): Long? = name.removePrefix(CHAT_PREFIX).substringBefore('-').toLongOrNull()
    }

    /** One text upload if due (or [force]). Returns the number of blobs uploaded. */
    fun runText(onWifi: Boolean, allowMobile: Boolean = false, force: Boolean = false): Int {
        if (!blobs.available) return 0
        val now = clock()
        if (!force && !core.chatBackupDue(now, onWifi, allowMobile)) return 0
        val b = core.chatBackupNext(now, false) ?: return 0
        blobs.put(b.name, b.blob)
        core.chatBackupCommit(b.seq)
        // After a snapshot, older blobs are redundant.
        b.deleteBelowSeq?.let { below ->
            blobs.list(CHAT_PREFIX).filter { (seqOf(it) ?: Long.MAX_VALUE) < below }.forEach { runCatching { blobs.delete(it) } }
        }
        return 1
    }

    /** Daily media pass (CB5). Returns the number of media blobs uploaded. */
    fun runMedia(onWifi: Boolean): Int {
        if (!blobs.available || !core.mediaBackupDue(clock(), onWifi)) return 0
        var n = 0
        for (item in core.mediaBackupTodo()) {
            val bytes = item.localPath?.let(readFile) ?: continue
            val b = runCatching { core.sealMediaBackup(item.hash, bytes) }.getOrNull() ?: continue
            blobs.put(b.name, b.blob)
            core.mediaBackupCommit(item.hash, clock())
            n++
        }
        return n
    }

    /** Save media settings; turning it off deletes the media blobs (the UI confirmed first). */
    fun setMedia(p: MediaBackupPrefs) {
        core.setMediaBackupSettings(p).forEach { runCatching { blobs.delete(it) } }
    }

    /**
     * Restore chats on a new device. Media in the backup are restored hash-verified via [saveMedia]
     * (returns the saved path); everything else stays a placeholder that refills from partners.
     */
    fun restore(saveMedia: (msgId: String, mime: String, bytes: ByteArray) -> String?): ChatRestoreResult {
        if (!blobs.available) return ChatRestoreResult(0, 0, 0)
        val names = blobs.list(CHAT_PREFIX)
        if (names.isEmpty()) return ChatRestoreResult(0, 0, 0)
        val summary: ChatBackupSummaryInfo = core.chatBackupRestore(names.mapNotNull { blobs.get(it) })
        var restored = 0
        for (p in core.mediaPlaceholders()) {
            val blob = runCatching { blobs.get(p.blobName) }.getOrNull() ?: continue
            val bytes = runCatching { core.restoreMediaFromBackup(p.msgId, blob) }.getOrNull() ?: continue
            val path = saveMedia(p.msgId, p.mime, bytes) ?: continue
            core.setReceivedFilePath(p.msgId, path)
            restored++
        }
        return ChatRestoreResult(summary.messages, summary.mediaPlaceholders - restored, restored)
    }
}
