package chat.omcp.app.ui.vm

import androidx.lifecycle.ViewModel
import chat.omcp.app.core.PathKind
import chat.omcp.app.burn.BurnSpec
import chat.omcp.app.data.MeshRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

class ChatsViewModel(val repo: MeshRepository) : ViewModel() {
    val contacts = repo.contacts
    val messages = repo.messages
    val status = repo.status
}

/** Picker confirm (§5.23). `tooLarge` is decided before any encryption/chunking. */
data class FileConfirm(
    val name: String,
    val sizeBytes: Long,
    val mime: String,
    val maxBytes: Long,
    val route: PathKind,
) {
    val tooLarge get() = sizeBytes > maxBytes
}

class ChatThreadViewModel(val repo: MeshRepository, val contactId: String) : ViewModel() {
    val messages = repo.messages
    val status = repo.status
    private val _file = MutableStateFlow<FileConfirm?>(null)
    val fileConfirm: StateFlow<FileConfirm?> = _file.asStateFlow()

    fun contact() = repo.contact(contactId)

    /** Per-thread path: queued while anything for this contact waits, else the aggregate. */
    fun path(): PathKind {
        val s = repo.core.pathStatus()
        return if (repo.core.queuedFor(contactId) > 0 || s.nearbyPeers == 0) PathKind.Queued else s.aggregatePath
    }

    fun send(text: String, burnOnce: BurnSpec? = null) = repo.send(contactId, text, burnOnce)

    /** Per-chat default (Chat info) so the composer can show the 🔥 state. */
    fun chatBurn() = repo.chatBurn(contactId)

    fun openBurn(id: String) = repo.openBurn(contactId, id)
    fun closeBurn(id: String) = repo.closeBurn(contactId, id)
    fun tickBurn() = repo.tickBurn()

    fun onFilePicked(name: String, size: Long, mime: String) {
        _file.value = FileConfirm(name, size, mime, repo.core.maxFileBytes(), path())
    }

    fun confirmFile() {
        val f = _file.value ?: return
        if (f.tooLarge) return
        repo.queueFile(contactId, f.name, f.sizeBytes, f.mime)
        _file.value = null
    }

    fun cancelFile() { _file.value = null }

    fun setVerified(v: Boolean) = repo.setVerified(contactId, v)
}

class GatewayViewModel(private val repo: MeshRepository) : ViewModel() {
    private val store = repo.appStore
    private val _on = MutableStateFlow(store.gatewayEnabled)
    val gatewayOn: StateFlow<Boolean> = _on.asStateFlow()
    private val _mobile = MutableStateFlow(store.gatewayAllowMobile)
    val allowMobile: StateFlow<Boolean> = _mobile.asStateFlow()

    val dailyCapMb: Int get() = (repo.core.gatewayDailyCapBytes() / 1_000_000L).toInt()

    /** Real metering not wired yet — always 0 (design lock-in). */
    val usedMb: Int = 0

    fun setGateway(on: Boolean) { store.gatewayEnabled = on; _on.value = on }
    fun setAllowMobile(on: Boolean) { store.gatewayAllowMobile = on; _mobile.value = on }

    fun decision(network: chat.omcp.app.core.NetworkType, metered: Boolean) =
        repo.core.gatewayDecision(wifiOnly = !_mobile.value, network = network, metered = metered)
}
