package chat.omcp.app.core

/** Aggregate path for chips/banners — mesh / gateway / mixed / queued only. */
enum class PathKind { Mesh, Gateway, Mixed, Queued }

data class MeshStatus(val nearbyPeers: Int, val queuedCount: Int, val aggregatePath: PathKind)

data class Identity(val peerIdHex: String, val displayName: String)

data class Contact(
    val id: String,
    val displayName: String,
    /** 12 groups of 5 digits. */
    val safetyNumber: List<String>,
    val verified: Boolean,
)

data class ContactPreview(val contact: Contact, val alreadyKnown: Boolean, val mismatchWith: Contact?)

data class IncomingText(
    val contactId: String,
    /** Empty for burn messages: content only via [OmcpCore.openBurn]. */
    val text: String,
    val createdMs: Long,
    val envelopeId: String,
    val burn: chat.omcp.app.burn.BurnSpec? = null,
    val mime: String? = null,
)

/** A queued outbound file (encrypted into 64 KiB chunks by the Rust core). */
data class FileOfferInfo(val transferId: String, val contactId: String, val name: String, val mime: String, val size: Long, val chunks: Int)

/** Transfer progress: sender = chunks sealed onto the link, receiver = chunks received. */
data class FileProgressInfo(
    val transferId: String,
    val contactId: String,
    val incoming: Boolean,
    val name: String,
    val size: Long,
    val doneChunks: Int,
    val totalChunks: Int,
) {
    val fraction: Float get() = if (totalChunks == 0) 1f else doneChunks.toFloat() / totalChunks
    val doneBytes: Long get() = (size * fraction).toLong()
}

/** A fully received file whose BLAKE3 hash was verified in Rust. */
class ReceivedFileInfo(val transferId: String, val contactId: String, val name: String, val mime: String, val bytes: ByteArray)

/** `integrity` (hash/AEAD failed), `invalid` or `busy`. */
data class FileFailureInfo(val transferId: String, val contactId: String, val reason: String)

/** PDUs to transmit (in order) + decrypted messages from one link step. */
data class LinkResult(
    val outgoing: List<ByteArray>,
    val received: List<IncomingText>,
    val established: Boolean,
    val peerContactId: String?,
    val flushed: Int,
    val opened: List<chat.omcp.app.burn.OpenedInfo> = emptyList(),
    val fileProgress: List<FileProgressInfo> = emptyList(),
    val files: List<ReceivedFileInfo> = emptyList(),
    val fileFailures: List<FileFailureInfo> = emptyList(),
    /** More file chunks wait: call [CoreLink.flush] again once the GATT queue is empty. */
    val morePending: Boolean = false,
)

enum class NetworkType { Offline, Wifi, Cellular }

data class GatewayDecision(val bridge: Boolean, val meteredWarning: Boolean)

data class PasswordCheck(val ok: Boolean, val score: Int, val tooShort: Boolean)

data class BackupMeta(val createdAtMs: Long, val deviceHint: String, val needsPassword: Boolean)

/** [history]: chats, messages, file metadata and pending queue (never burn content); null for older backups. */
class RestoredBackup(val phrase: String, val contacts: List<Contact>, val history: ByteArray? = null)

/** The encrypted local store can't be opened (wrong/lost storage key, other identity, newer schema). */
class StorageUnavailableException(message: String?, cause: Throwable? = null) : Exception(message, cause)

/** File-transfer metadata as persisted by the Rust store; [state] = a `FileAttachment.State` name. */
data class StoredFileInfo(val name: String, val mime: String, val size: Long, val state: String, val doneBytes: Long, val localPath: String?)

/**
 * A message as persisted by the Rust store. [status] = `Queued|Sent|Received`,
 * [burnState] = `None|Unopened|Open|Opened`. Burn messages never carry text here.
 */
data class StoredMessageInfo(
    val id: String,
    val contactId: String,
    val text: String,
    val mine: Boolean,
    val timeMs: Long,
    val status: String,
    val mime: String?,
    val burn: chat.omcp.app.burn.BurnSpec?,
    val burnState: String,
    val burnAtMs: Long?,
    val file: StoredFileInfo?,
)

class WrongBackupPasswordException : Exception("wrong backup password")

/** One BLE connection's secure session (Noise hybrid + M2 fragments). */
interface CoreLink {
    fun start(): LinkResult
    fun onPdu(pdu: ByteArray): LinkResult
    fun sendText(text: String): LinkResult
    fun flush(): LinkResult
    fun isEstablished(): Boolean
    fun close()
}

/**
 * App-facing protocol core. Production: [RustOmcpCore] (omcp-ffi via UniFFI).
 * Previews/tests: [FakeOmcpCore]. Never invents crypto results in the UI layer.
 */
interface OmcpCore {
    val linked: Boolean
    val suiteLabel: String

    fun generateRecoveryPhrase(): String
    fun validateRecoveryPhrase(phrase: String): Boolean
    fun isRecoveryWord(word: String): Boolean
    /** Up to 3 prefix matches from the recovery word list. */
    fun suggestRecoveryWords(prefix: String): List<String>

    /** Load identity from phrase (in memory only). Must be called before the node APIs below. */
    fun load(phrase: String, displayName: String): Identity

    /**
     * Load identity and open its encrypted local store at [dbPath] (contacts, chats, messages,
     * file metadata, queue and burn state survive restarts). [storageKey] (32 bytes) is kept
     * Keystore-wrapped by the app and never hard-coded. Throws [StorageUnavailableException].
     */
    fun open(phrase: String, displayName: String, dbPath: String, storageKey: ByteArray): Identity = load(phrase, displayName)

    /** Fresh 256-bit storage key (wrap it with the Android Keystore before saving). */
    fun generateStorageKey(): ByteArray = ByteArray(32).also { java.security.SecureRandom().nextBytes(it) }

    /** Persisted messages of one chat, oldest first (empty for in-memory cores). */
    fun storedMessages(contactId: String): List<StoredMessageInfo> = emptyList()

    /** Record where a verified received file was saved (null = couldn't save). */
    fun setReceivedFilePath(transferId: String, path: String?) {}

    /** Import [RestoredBackup.history] into the local store. */
    fun restoreHistory(history: ByteArray) {}
    fun identity(): Identity?
    fun qrPayload(): String
    fun advertisement(): ByteArray

    /** Parse + safety number without saving. Throws [IllegalArgumentException] on a bad QR. */
    fun previewContact(payload: String): ContactPreview
    /** Throws [IllegalArgumentException] on a bad QR. */
    fun addContactFromQr(payload: String): Contact
    fun restoreContact(contact: Contact)
    fun contacts(): List<Contact>
    fun setVerified(contactId: String, verified: Boolean)
    fun contactForAdvertisement(adv: ByteArray): String?

    fun queueText(contactId: String, text: String)
    /** Queue text with an optional encrypted burn header. Returns the message id. */
    fun queueMessage(contactId: String, text: String, burn: chat.omcp.app.burn.BurnSpec?): String
    /** Small view-once media (≤ 16 KiB in the beta). Returns the message id. */
    fun queueMedia(contactId: String, mime: String, bytes: ByteArray, burn: chat.omcp.app.burn.BurnSpec?): String

    /** Burn after read, all enforced in Rust. Content is returned exactly once; queues OPENED. */
    fun openBurn(msgId: String, nowMs: Long): chat.omcp.app.burn.BurnContent?
    fun closeBurn(msgId: String)
    /** Timers + unread expiry on sent and received copies; returns shredded ids. */
    fun tickBurn(nowMs: Long): List<String>
    fun burnIsLive(msgId: String): Boolean
    fun burnSpecValid(spec: chat.omcp.app.burn.BurnSpec): Boolean
    /** Queue a file (≤ [maxFileBytes], checked before encryption). Throws [IllegalArgumentException] if too large. */
    fun queueFile(contactId: String, name: String, mime: String, bytes: ByteArray): FileOfferInfo
    fun cancelFile(transferId: String): Boolean
    fun queuedFor(contactId: String): Int
    fun pathStatus(): MeshStatus

    fun openInitiatorLink(contactId: String): CoreLink
    fun openResponderLink(): CoreLink

    /** Backup password policy (≥ 8 chars, zxcvbn ≥ 3) — computed in Rust core. */
    fun checkBackupPassword(password: String): PasswordCheck
    /** Encrypted blob of identity + contacts. Never contains the plaintext phrase. */
    fun createBackup(password: String, deviceHint: String, nowMs: Long): ByteArray
    fun backupInfo(blob: ByteArray): BackupMeta?
    /** Throws [WrongBackupPasswordException] on a wrong password. */
    fun openBackup(blob: ByteArray, password: String): RestoredBackup
    /** The loaded identity's phrase (gate behind device auth in UI). */
    fun recoveryPhrase(): String

    /** Core file cap (decimal bytes, 50 MB). UI must read this, not hard-code. */
    fun maxFileBytes(): Long
    fun gatewayDailyCapBytes(): Long
    fun gatewayDecision(wifiOnly: Boolean, network: NetworkType, metered: Boolean): GatewayDecision
}

/** BLE GATT layout. PROTOCOL §8.1's placeholder UUID is not valid hex, so the
 * beta uses `4f4d4350-…` ("OMCP" in ASCII hex). Finalize before ship. */
object OmcpGatt {
    const val SERVICE = "4f4d4350-0001-4c01-9e00-000000000001"
    const val INBOX = "4f4d4350-0002-4c01-9e00-000000000001"
    const val OUTBOX = "4f4d4350-0003-4c01-9e00-000000000001"
    const val CONTROL = "4f4d4350-0004-4c01-9e00-000000000001"
    const val CCCD = "00002902-0000-1000-8000-00805f9b34fb"
    /** Manufacturer-data company id for the 13-byte advertisement (0xFFFF = testing). */
    const val MANUFACTURER_ID = 0xFFFF
    /** Local name iPhones put in the (foreground) advertisement; Android scans for it too. */
    const val LOCAL_NAME = "OMCP"
}
