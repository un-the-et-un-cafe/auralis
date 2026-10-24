package chat.omcp.app.core

import chat.omcp.ffi.ContactInfo
import chat.omcp.ffi.FfiException
import chat.omcp.ffi.LinkOutput
import chat.omcp.ffi.MeshLink
import chat.omcp.ffi.Network
import chat.omcp.ffi.OmcpNode
import chat.omcp.ffi.PathKind as FfiPathKind

/** Real core: omcp-ffi (Rust) through UniFFI-generated Kotlin bindings. */
class RustOmcpCore : OmcpCore {
    @Volatile private var node: OmcpNode? = null

    private fun n(): OmcpNode = node ?: error("identity not loaded")

    override val linked = true
    override val suiteLabel: String get() = chat.omcp.ffi.suiteLabel()

    override fun generateRecoveryPhrase(): String = chat.omcp.ffi.generateRecoveryPhrase()
    override fun validateRecoveryPhrase(phrase: String) = chat.omcp.ffi.validateRecoveryPhrase(phrase)

    override fun isRecoveryWord(word: String) = chat.omcp.ffi.isRecoveryWord(word)
    override fun suggestRecoveryWords(prefix: String) = chat.omcp.ffi.suggestRecoveryWords(prefix)

    override fun load(phrase: String, displayName: String): Identity {
        node = OmcpNode.fromPhrase(phrase, displayName)
        return identity()!!
    }

    override fun identity(): Identity? = node?.identity()?.let { Identity(it.peerIdHex, it.displayName) }
    override fun qrPayload() = n().qrPayload()
    override fun advertisement(): ByteArray = n().advertisement()

    override fun previewContact(payload: String): ContactPreview = try {
        val p = n().previewContactFromQr(payload)
        ContactPreview(p.contact.toContact(), p.alreadyKnown, p.mismatchWith?.toContact())
    } catch (e: FfiException) {
        throw IllegalArgumentException(e.message, e)
    }

    override fun addContactFromQr(payload: String): Contact = try {
        n().addContactFromQr(payload).toContact()
    } catch (e: FfiException) {
        throw IllegalArgumentException(e.message, e)
    }

    override fun restoreContact(contact: Contact) =
        n().restoreContact(contact.id, contact.displayName, contact.verified)

    override fun contacts() = n().contacts().map { it.toContact() }
    override fun setVerified(contactId: String, verified: Boolean) = n().setVerified(contactId, verified)

    override fun contactForAdvertisement(adv: ByteArray): String? =
        chat.omcp.ffi.parseAdvertisement(adv)?.let { n().contactForPrefix(it) }

    override fun queueText(contactId: String, text: String) = n().queueText(contactId, text)
    override fun queuedFor(contactId: String) = n().queuedFor(contactId).toInt()

    override fun pathStatus(): MeshStatus {
        val s = n().pathStatus()
        return MeshStatus(
            s.nearbyPeers.toInt(),
            s.queuedCount.toInt(),
            when (s.path) {
                FfiPathKind.MESH -> PathKind.Mesh
                FfiPathKind.GATEWAY -> PathKind.Gateway
                FfiPathKind.MIXED -> PathKind.Mixed
                FfiPathKind.QUEUED -> PathKind.Queued
            },
        )
    }

    override fun openInitiatorLink(contactId: String): CoreLink = RustLink(n().openInitiatorLink(contactId))
    override fun openResponderLink(): CoreLink = RustLink(n().openResponderLink())

    override fun checkBackupPassword(password: String): PasswordCheck =
        chat.omcp.ffi.checkBackupPassword(password).let { PasswordCheck(it.ok, it.score.toInt(), it.tooShort) }

    override fun createBackup(password: String, deviceHint: String, nowMs: Long): ByteArray =
        n().createBackup(password, deviceHint, nowMs.toULong())

    override fun backupInfo(blob: ByteArray): BackupMeta? = runCatching {
        chat.omcp.ffi.backupInfo(blob).let { BackupMeta(it.createdAtMs.toLong(), it.deviceHint, it.needsPassword) }
    }.getOrNull()

    override fun openBackup(blob: ByteArray, password: String): RestoredBackup = try {
        val r = chat.omcp.ffi.openBackupWithPassword(blob, password)
        RestoredBackup(r.phrase, r.contacts.map { Contact(it.contactId, it.displayName, emptyList(), it.verified) })
    } catch (e: FfiException.WrongBackupPassword) {
        throw WrongBackupPasswordException()
    }

    override fun recoveryPhrase(): String = n().recoveryPhrase()

    override fun maxFileBytes() = chat.omcp.ffi.maxFileBytes().toLong()
    override fun gatewayDailyCapBytes() = chat.omcp.ffi.defaultGatewayDailyCapBytes().toLong()

    override fun gatewayDecision(wifiOnly: Boolean, network: NetworkType, metered: Boolean): GatewayDecision {
        val net = when (network) {
            NetworkType.Offline -> Network.Offline
            NetworkType.Wifi -> Network.Wifi(metered)
            NetworkType.Cellular -> Network.Cellular(metered)
        }
        val d = chat.omcp.ffi.gatewayDecision(wifiOnly, net)
        return GatewayDecision(d.bridge, d.meteredWarning)
    }
}

private fun ContactInfo.toContact() = Contact(contactId, displayName, safetyNumber, verified)

private fun LinkOutput.toResult() = LinkResult(
    outgoing = outgoing,
    received = received.map { IncomingText(it.contactId, it.text, it.createdMs.toLong(), it.envelopeIdHex) },
    established = established,
    peerContactId = peerContactId,
    flushed = flushed.toInt(),
)

private class RustLink(private val link: MeshLink) : CoreLink {
    override fun start() = link.start().toResult()
    override fun onPdu(pdu: ByteArray) = link.onPdu(pdu).toResult()
    override fun sendText(text: String) = link.sendText(text).toResult()
    override fun flush() = link.flush().toResult()
    override fun isEstablished() = link.isEstablished()
    override fun close() {
        link.shutdown()
        link.destroy()
    }
}
