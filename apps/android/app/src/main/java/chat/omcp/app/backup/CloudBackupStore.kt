package chat.omcp.app.backup

/**
 * Remote store for the **already-encrypted** backup blob (never the phrase).
 * Android: Google Drive `appDataFolder`, OAuth scope `drive.appdata` only.
 */
interface CloudBackupStore {
    /** User-facing name, e.g. "Google Drive". */
    val label: String
    /** False when not configured (e.g. no OAuth client in this build). */
    val available: Boolean
    fun upload(blob: ByteArray)
    /** Null if no backup exists. */
    fun download(): ByteArray?
    /** Removes the remote record. */
    fun delete()
    /** Like [download] but never shows UI (fresh-install probe); null if consent is needed. */
    fun downloadSilently(): ByteArray? = download()
    /** True if the cloud account has no room left (restore still works; the next backup won't). */
    fun storageFull(): Boolean = false
}

class CloudUnavailableException(msg: String) : Exception(msg)

/**
 * Google Drive `appDataFolder` store: uploads / restores / deletes the Rust-sealed backup blob
 * (never the phrase) via [DriveAppDataClient]. Available only when the build has a
 * `GOOGLE_OAUTH_CLIENT_ID` (see README › Google Drive backup). Blocking: call off the main thread.
 * Offline or consent declined → [CloudUnavailableException] / [DriveHttpException]; callers keep
 * the backup method unset so the reminder stays. Never sent over gateway/mesh.
 */
class DriveAppDataBackupStore(
    clientId: String,
    private val drive: (interactive: Boolean) -> DriveAppDataClient,
) : CloudBackupStore {
    override val label = "Google Drive"
    override val available = clientId.isNotBlank()

    private fun d(interactive: Boolean = true): DriveAppDataClient {
        if (!available) throw CloudUnavailableException("Google Drive backup not configured in this build")
        return drive(interactive)
    }

    override fun upload(blob: ByteArray) = d().upload(blob)
    override fun download(): ByteArray? = if (available) d().download() else null
    override fun delete() = d().delete()
    override fun downloadSilently(): ByteArray? = if (available) d(interactive = false).download() else null
    override fun storageFull(): Boolean = available && runCatching { d(interactive = false).storageFull() }.getOrDefault(false)
}

/** In-memory store for tests and the debug "simulated Drive" toggle. */
class InMemoryCloudBackupStore(override val label: String = "Google Drive (simulated)") : CloudBackupStore {
    var blob: ByteArray? = null
        private set
    var uploads = 0
        private set
    override val available = true
    override fun upload(blob: ByteArray) { this.blob = blob.copyOf(); uploads++ }
    override fun download(): ByteArray? { failWith?.let { throw it }; return blob?.copyOf() }
    override fun delete() { blob = null }
    /** Test hooks: next calls fail with this, or report a full account. */
    var failWith: Exception? = null
    var full = false
    override fun storageFull() = full
}
