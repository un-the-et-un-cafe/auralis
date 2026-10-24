package chat.omcp.app.core

/**
 * Deterministic in-memory core for Compose previews and JVM unit tests.
 * No real crypto: "links" pass plaintext between two fakes. Never ship as default.
 */
class FakeOmcpCore : OmcpCore {
    private var me: Identity? = null
    private val contacts = linkedMapOf<String, Contact>()
    private val queues = mutableMapOf<String, ArrayDeque<String>>()
    private val linksUp = mutableSetOf<String>()

    override val linked = false
    override val suiteLabel = "OMCP-2026-A (fake core)"

    override fun generateRecoveryPhrase() = (1..24).joinToString(" ") { "word$it" }
    override fun validateRecoveryPhrase(phrase: String) = phrase.trim().split(Regex("\\s+")).size == 24

    private val words = listOf("abandon", "ability", "able", "about", "fossil", "foster", "found", "zoo")
    override fun isRecoveryWord(word: String) = word.lowercase() in words || word.matches(Regex("word\\d+"))
    override fun suggestRecoveryWords(prefix: String) =
        if (prefix.isBlank()) emptyList() else words.filter { it.startsWith(prefix.lowercase()) }.take(3)

    override fun load(phrase: String, displayName: String): Identity {
        require(validateRecoveryPhrase(phrase)) { "invalid recovery phrase" }
        val id = phrase.hashCode().toUInt().toString(16).padStart(8, '0').repeat(8)
        this.phrase = phrase.trim()
        return Identity(id, displayName).also { me = it }
    }

    override fun identity() = me
    override fun qrPayload() = "omcp:1:${me!!.peerIdHex}:${me!!.displayName}"
    override fun advertisement() = me!!.peerIdHex.take(12).toByteArray()

    private fun parse(payload: String): Contact {
        require(payload.startsWith("omcp:1:")) { "not an OMCP contact QR" }
        val parts = payload.removePrefix("omcp:1:").split(":", limit = 2)
        val id = parts[0]
        require(id.length == 64) { "contact id length" }
        require(id != me?.peerIdHex) { "cannot add yourself" }
        val name = parts.getOrNull(1)?.ifBlank { null } ?: "Peer ${id.take(8)}"
        val sn = List(12) { "%05d".format((id.hashCode().toLong() * (it + 1)).mod(100_000L)) }
        return Contact(id, name, sn, false)
    }

    override fun previewContact(payload: String): ContactPreview {
        val c = parse(payload)
        val known = contacts[c.id]
        val mismatch = contacts.values.firstOrNull { it.id != c.id && it.displayName == c.displayName }
        return ContactPreview(known ?: c, known != null, mismatch)
    }

    override fun addContactFromQr(payload: String): Contact {
        val c = parse(payload)
        return contacts.getOrPut(c.id) { c }
    }

    override fun restoreContact(contact: Contact) { contacts[contact.id] = contact }
    override fun contacts() = contacts.values.sortedBy { it.displayName }
    override fun setVerified(contactId: String, verified: Boolean) {
        contacts[contactId] = contacts.getValue(contactId).copy(verified = verified)
    }
    override fun contactForAdvertisement(adv: ByteArray): String? {
        val prefix = String(adv)
        return contacts.keys.firstOrNull { it.startsWith(prefix) }
    }

    override fun queueText(contactId: String, text: String) {
        require(contactId in contacts) { "unknown contact" }
        queues.getOrPut(contactId) { ArrayDeque() }.addLast(text)
    }
    override fun queuedFor(contactId: String) = queues[contactId]?.size ?: 0

    override fun pathStatus(): MeshStatus {
        val q = queues.values.sumOf { it.size }
        return MeshStatus(linksUp.size, q, if (q == 0 && linksUp.isNotEmpty()) PathKind.Mesh else PathKind.Queued)
    }

    /** Test helper: simulate a mesh link coming up and flushing the queue for [contactId]. */
    fun simulateLinkUp(contactId: String): List<String> {
        linksUp += contactId
        return queues.remove(contactId)?.toList() ?: emptyList()
    }

    override fun openInitiatorLink(contactId: String): CoreLink = error("FakeOmcpCore has no BLE links")
    override fun openResponderLink(): CoreLink = error("FakeOmcpCore has no BLE links")

    private var phrase: String? = null
    override fun checkBackupPassword(password: String) =
        PasswordCheck(password.length >= 8 && password != "password", if (password.length >= 12) 4 else 1, password.length < 8)

    /** Fake "encryption": XOR with password hash; never contains the phrase bytes in clear. */
    override fun createBackup(password: String, deviceHint: String, nowMs: Long): ByteArray {
        val body = (phrase!! + "\n" + contacts.values.joinToString("\n") { "${it.id}|${it.displayName}|${it.verified}" })
        val k = password.hashCode()
        return "FAKEBK|$nowMs|$deviceHint|".toByteArray() + body.toByteArray().map { (it.toInt() xor k).toByte() }
    }

    override fun backupInfo(blob: ByteArray): BackupMeta? {
        val parts = String(blob).split("|")
        if (parts.size < 4 || parts[0] != "FAKEBK") return null
        return BackupMeta(parts[1].toLong(), parts[2], true)
    }

    override fun openBackup(blob: ByteArray, password: String): RestoredBackup {
        val head = String(blob).split("|").take(3).joinToString("|").length + 1
        val k = password.hashCode()
        val body = String(blob.drop(head).map { (it.toInt() xor k).toByte() }.toByteArray())
        val lines = body.split("\n")
        if (!validateRecoveryPhrase(lines[0]) || !lines[0].startsWith("word")) throw WrongBackupPasswordException()
        val cs = lines.drop(1).filter { it.isNotBlank() }.map {
            val (id, name, v) = it.split("|")
            Contact(id, name, emptyList(), v.toBoolean())
        }
        return RestoredBackup(lines[0], cs)
    }

    override fun recoveryPhrase() = phrase!!

    override fun maxFileBytes() = 50_000_000L
    override fun gatewayDailyCapBytes() = 500_000_000L
    override fun gatewayDecision(wifiOnly: Boolean, network: NetworkType, metered: Boolean) = when (network) {
        NetworkType.Offline -> GatewayDecision(false, false)
        NetworkType.Wifi -> GatewayDecision(true, metered)
        NetworkType.Cellular -> if (wifiOnly) GatewayDecision(false, false) else GatewayDecision(true, metered)
    }
}
