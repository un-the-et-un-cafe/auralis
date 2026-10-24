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
}

class CloudUnavailableException(msg: String) : Exception(msg)

/**
 * Google Drive appDataFolder store — **stub**.
 *
 * TODO(M5.1, needs OAuth client): sign in via Credential Manager
 * (`GetGoogleIdOption` + `AuthorizationClient.authorize` with scope
 * `https://www.googleapis.com/auth/drive.appdata`), then Drive v3 REST:
 * `files.create` (parents=["appDataFolder"], name="omcp-backup-v1.cbor"),
 * `files.list?spaces=appDataFolder`, `files.get?alt=media`, `files.delete`.
 * Offline → queue and retry via WorkManager. Never over gateway/mesh.
 * No OAuth client id is available in this repository, so it reports unavailable.
 */
class DriveAppDataBackupStore : CloudBackupStore {
    override val label = "Google Drive"
    override val available = false
    override fun upload(blob: ByteArray) = throw CloudUnavailableException("Google Drive backup not configured in this build")
    override fun download(): ByteArray? = null
    override fun delete() = throw CloudUnavailableException("Google Drive backup not configured in this build")
}

/** In-memory store for tests and the debug "simulated Drive" toggle. */
class InMemoryCloudBackupStore(override val label: String = "Google Drive (simulated)") : CloudBackupStore {
    var blob: ByteArray? = null
        private set
    var uploads = 0
        private set
    override val available = true
    override fun upload(blob: ByteArray) { this.blob = blob.copyOf(); uploads++ }
    override fun download() = blob?.copyOf()
    override fun delete() { blob = null }
}
