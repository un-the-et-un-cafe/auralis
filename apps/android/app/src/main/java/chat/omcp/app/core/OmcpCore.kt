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

data class IncomingText(val contactId: String, val text: String, val createdMs: Long, val envelopeId: String)

/** PDUs to transmit (in order) + decrypted messages from one link step. */
data class LinkResult(
    val outgoing: List<ByteArray>,
    val received: List<IncomingText>,
    val established: Boolean,
    val peerContactId: String?,
    val flushed: Int,
)

enum class NetworkType { Offline, Wifi, Cellular }

data class GatewayDecision(val bridge: Boolean, val meteredWarning: Boolean)

data class PasswordCheck(val ok: Boolean, val score: Int, val tooShort: Boolean)

data class BackupMeta(val createdAtMs: Long, val deviceHint: String, val needsPassword: Boolean)

data class RestoredBackup(val phrase: String, val contacts: List<Contact>)

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

    /** Load identity from phrase. Must be called before the node APIs below. */
    fun load(phrase: String, displayName: String): Identity
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
}
