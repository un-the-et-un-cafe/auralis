package chat.omcp.app.data

import chat.omcp.app.core.Contact
import chat.omcp.app.core.IncomingText
import chat.omcp.app.core.MeshStatus
import chat.omcp.app.core.OmcpCore
import chat.omcp.app.core.PathKind
import chat.omcp.app.burn.BurnControl
import chat.omcp.app.burn.BurnSpec
import chat.omcp.app.burn.BurnVault
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/** File attachment state (DESIGN_SPEC §5.24). Only `Queued` is produced in the beta. */
data class FileAttachment(val name: String, val sizeBytes: Long, val mime: String, val state: State) {
    enum class State { Queued, Sending, Receiving, Paused, Received, Failed, IntegrityFailed }
}

data class ChatMessage(
    val id: String,
    val contactId: String,
    val text: String,
    val mine: Boolean,
    val timeMs: Long,
    val status: Status,
    val file: FileAttachment? = null,
    /** Burn after read (v0.1.3). Content lives in [BurnVault]; [text] stays empty. */
    val burn: BurnSpec? = null,
    val burnState: BurnState = BurnState.None,
    /** When the opened content is deleted (timer); null = on close. */
    val burnAtMs: Long? = null,
) {
    enum class Status { Queued, Sent, Received }
    enum class BurnState { None, Unopened, Open, Opened }

    val isViewOnceMedia get() = burn != null && file != null && (file.mime.startsWith("image/") || file.mime.startsWith("video/"))

    /** Burnable content can't be saved, forwarded, quoted, shared or searched. */
    val canShare get() = burn == null
}

enum class BackupGate { Ok, Banner, Required }

const val BACKUP_DRIVE = "drive"
const val BACKUP_WRITTEN = "written"
const val REMIND_WINDOW_MS = 24 * 60 * 60 * 1000L

/** Something that can push queued texts out now (the BLE mesh service). */
fun interface MeshKick {
    fun kick(contactId: String)
}

/**
 * Single source of truth for identity, contacts, messages and path status.
 * Messages are in-memory for the beta (TODO: Room persistence).
 */
class MeshRepository(
    val core: OmcpCore,
    private val store: AppStore,
    private val clock: () -> Long = System::currentTimeMillis,
) {
    private val _contacts = MutableStateFlow<List<Contact>>(emptyList())
    val contacts: StateFlow<List<Contact>> = _contacts.asStateFlow()
    private val _messages = MutableStateFlow<Map<String, List<ChatMessage>>>(emptyMap())
    val messages: StateFlow<Map<String, List<ChatMessage>>> = _messages.asStateFlow()
    private val _status = MutableStateFlow(MeshStatus(0, 0, PathKind.Queued))
    val status: StateFlow<MeshStatus> = _status.asStateFlow()
    private val _onboarded = MutableStateFlow(false)
    val onboarded: StateFlow<Boolean> = _onboarded.asStateFlow()

    val appStore: AppStore get() = store

    @Volatile var kick: MeshKick? = null
    /** TODO(M5.x): wire to the mesh control channel; until then receipts are dropped. */
    @Volatile var burnControl: BurnControl = BurnControl { _, _, _ -> }
    val burnVault = BurnVault()
    private var seq = 0L

    /** Load an existing identity from the store; returns true if onboarded. */
    fun restore(): Boolean {
        val phrase = store.recoveryPhrase ?: return false
        core.load(phrase, store.displayName ?: "Me")
        store.contacts.forEach { core.restoreContact(it) }
        refresh()
        _onboarded.value = true
        return true
    }

    /** Create or restore identity from [phrase] and persist it (sealed). */
    fun saveIdentity(phrase: String, name: String) {
        core.load(phrase, name)
        store.recoveryPhrase = phrase
        store.displayName = name
        refresh()
    }

    /** Restore identity + contacts + verification state from a decrypted backup (O3). */
    fun restoreFromBackup(r: chat.omcp.app.core.RestoredBackup, name: String) {
        saveIdentity(r.phrase, name)
        r.contacts.forEach { core.restoreContact(it) }
        store.backupMethods = store.backupMethods + BACKUP_DRIVE
        persistContacts()
    }

    /** Backup requirement (REQUIREMENTS "Onboarding & recovery backup", O6). */
    fun backupGate(now: Long = clock()): BackupGate = when {
        store.backupMethods.isNotEmpty() -> BackupGate.Ok
        store.backupRemindDeadlineMs == 0L -> BackupGate.Required
        now < store.backupRemindDeadlineMs -> BackupGate.Banner
        else -> BackupGate.Required
    }

    fun remindBackupLater(now: Long = clock()) {
        if (store.backupRemindDeadlineMs == 0L) store.backupRemindDeadlineMs = now + REMIND_WINDOW_MS
    }

    fun markBackupMethod(method: String, add: Boolean = true) {
        store.backupMethods = if (add) store.backupMethods + method else store.backupMethods - method
    }

    /** Onboarding finished (after permissions step). */
    fun completeOnboarding() {
        _onboarded.value = true
    }

    fun addContact(qr: String, verified: Boolean): Contact {
        val c = core.addContactFromQr(qr)
        if (verified) core.setVerified(c.id, true)
        persistContacts()
        return core.contacts().first { it.id == c.id }
    }

    fun setVerified(id: String, verified: Boolean) {
        core.setVerified(id, verified)
        persistContacts()
    }

    /** Per-chat burn default (Chat info). */
    fun chatBurn(contactId: String): BurnSpec? =
        BurnSpec.fromOption(store.burnChats[contactId] ?: -1, store.burnUnreadTtl[contactId] ?: BurnSpec.UNREAD_7D)

    fun setChatBurn(contactId: String, option: Int, unreadTtl: Int = BurnSpec.UNREAD_7D) {
        BurnSpec.fromOption(option, unreadTtl) // validate
        store.burnChats = if (option == -1) store.burnChats - contactId else store.burnChats + (contactId to option)
        store.burnUnreadTtl = store.burnUnreadTtl + (contactId to unreadTtl)
    }

    /**
     * Send text. [burnOnce] = the composer's per-message 🔥 toggle; otherwise the chat default applies.
     * TODO(M5.x): the burn header isn't carried over the FFI/mesh yet (core.queueText sends plain text).
     */
    fun send(contactId: String, text: String, burnOnce: BurnSpec? = null) {
        val t = text.trim()
        if (t.isEmpty()) return
        val burn = burnOnce ?: chatBurn(contactId)
        core.queueText(contactId, t)
        val id = "local-${seq++}"
        if (burn != null) {
            burnVault.put(id, t.toByteArray())
            append(ChatMessage(id, contactId, "", true, clock(), ChatMessage.Status.Queued, burn = burn, burnState = ChatMessage.BurnState.Unopened))
        } else {
            append(ChatMessage(id, contactId, t, true, clock(), ChatMessage.Status.Queued))
        }
        refresh()
        kick?.kick(contactId)
    }

    /**
     * Record an outgoing file. Size is already checked against [OmcpCore.maxFileBytes].
     * TODO(M5.1): stream encrypt_file chunks over the MeshLink; for now it stays queued.
     */
    fun queueFile(contactId: String, name: String, size: Long, mime: String) {
        require(size <= core.maxFileBytes()) { "file too large" }
        append(
            ChatMessage(
                "file-${seq++}", contactId, "", true, clock(), ChatMessage.Status.Queued,
                FileAttachment(name, size, mime, FileAttachment.State.Queued),
            ),
        )
    }

    /** Called by the transport when [count] queued texts to [contactId] went out. */
    fun markFlushed(contactId: String, count: Int) {
        if (count > 0) {
            _messages.update { all ->
                var left = count
                val list = all[contactId].orEmpty().map {
                    if (left > 0 && it.mine && it.file == null && it.status == ChatMessage.Status.Queued) {
                        left--
                        it.copy(status = ChatMessage.Status.Sent)
                    } else {
                        it
                    }
                }
                all + (contactId to list)
            }
        }
        refresh()
    }

    fun onReceived(msg: IncomingText, burn: BurnSpec? = null, file: FileAttachment? = null, media: ByteArray? = null) {
        if (burn == null) {
            append(ChatMessage(msg.envelopeId, msg.contactId, msg.text, false, msg.createdMs, ChatMessage.Status.Received, file))
            return
        }
        burnVault.put(msg.envelopeId, media ?: msg.text.toByteArray())
        append(
            ChatMessage(
                msg.envelopeId, msg.contactId, "", false, msg.createdMs, ChatMessage.Status.Received, file,
                burn = burn, burnState = ChatMessage.BurnState.Unopened,
            ),
        )
    }

    /**
     * Receiver opens a burn message: returns content exactly once (B2), sends OPENED,
     * starts the timer. View-once content is shredded from the vault immediately.
     */
    fun openBurn(contactId: String, msgId: String, now: Long = clock()): ByteArray? {
        val m = find(contactId, msgId) ?: return null
        val b = m.burn ?: return null
        if (m.mine || m.burnState != ChatMessage.BurnState.Unopened) return null
        val body = burnVault.read(msgId) ?: return null
        val at = if (b.mode == BurnSpec.Mode.Timer) now + b.secs * 1000L else null
        if (at == null) burnVault.shred(msgId)
        replace(m.copy(burnState = ChatMessage.BurnState.Open, burnAtMs = at))
        burnControl.sendOpened(contactId, msgId, now)
        return body
    }

    /** Viewer closed: burn now. */
    fun closeBurn(contactId: String, msgId: String) {
        val m = find(contactId, msgId) ?: return
        if (m.burnState == ChatMessage.BurnState.Open) burnNow(m)
    }

    /** Sender got OPENED: shred its media key/copy now, or start its timer for text. */
    fun onOpenedReceipt(contactId: String, msgId: String, openedMs: Long) {
        val m = find(contactId, msgId) ?: return
        val b = m.burn ?: return
        if (b.mode == BurnSpec.Mode.OnOpen || m.file != null) burnNow(m)
        else replace(m.copy(burnState = ChatMessage.BurnState.Open, burnAtMs = openedMs + b.secs * 1000L))
    }

    /** Timers + unread expiry (B1, B4). Call often (≤ 250 ms). Returns how many burned. */
    fun tickBurn(now: Long = clock()): Int {
        val due = _messages.value.values.flatten().filter { m ->
            val b = m.burn ?: return@filter false
            when (m.burnState) {
                ChatMessage.BurnState.Open -> m.burnAtMs?.let { now >= it } ?: false
                ChatMessage.BurnState.Unopened -> now >= b.expiresAtMs(m.timeMs)
                else -> false
            }
        }
        due.forEach(::burnNow)
        return due.size
    }

    private fun burnNow(m: ChatMessage) {
        burnVault.shred(m.id)
        if (m.file != null && m.isViewOnceMedia) {
            replace(m.copy(burnState = ChatMessage.BurnState.Opened, burnAtMs = null)) // "🔥 Photo opened"
        } else {
            _messages.update { all -> all + (m.contactId to all[m.contactId].orEmpty().filterNot { it.id == m.id }) }
        }
    }

    private fun find(contactId: String, id: String) = _messages.value[contactId]?.firstOrNull { it.id == id }

    private fun replace(m: ChatMessage) {
        _messages.update { all -> all + (m.contactId to all[m.contactId].orEmpty().map { if (it.id == m.id) m else it }) }
    }

    fun refresh() {
        if (core.identity() == null) return
        _contacts.value = core.contacts()
        _status.value = core.pathStatus()
    }

    fun contact(id: String) = _contacts.value.firstOrNull { it.id == id }

    private fun persistContacts() {
        store.contacts = core.contacts()
        refresh()
    }

    private fun append(m: ChatMessage) {
        _messages.update { it + (m.contactId to (it[m.contactId].orEmpty() + m)) }
    }
}
