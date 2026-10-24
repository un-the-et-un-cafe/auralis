package chat.omcp.app.ui.vm

import androidx.lifecycle.ViewModel
import chat.omcp.app.core.ContactPreview
import chat.omcp.app.data.MeshRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

data class ContactAddState(
    val preview: ContactPreview? = null,
    val scannedPayload: String? = null,
    /** Last scan was not an OMCP identity code (never open scanned URLs). */
    val notOmcp: Boolean = false,
    /** RS2: the scanned code is not the contact's current device (e.g. a pre-restore QR). */
    val qrStale: Boolean = false,
)

/** Scan → preview with safety number → Verify & chat / Chat without verifying (§5.20–§5.22). */
class ContactAddViewModel(private val repo: MeshRepository) : ViewModel() {
    private val _state = MutableStateFlow(ContactAddState())
    val state: StateFlow<ContactAddState> = _state.asStateFlow()

    val qrPayload: String get() = repo.core.qrPayload()
    val identity get() = repo.core.identity()

    fun onScanned(payload: String) {
        val p = runCatching { repo.core.previewContact(payload) }.getOrNull()
        _state.value = if (p == null) ContactAddState(notOmcp = true) else ContactAddState(p, payload)
    }

    fun dismissError() { _state.value = _state.value.copy(notOmcp = false) }

    /** Returns the contact id to open, or null if nothing to add (mismatch blocks chat). */
    fun verifyAndChat(): String? = commit(verified = true)

    fun chatWithoutVerifying(): String? = commit(verified = false)

    private fun commit(verified: Boolean): String? {
        val s = _state.value
        val payload = s.scannedPayload ?: return null
        if (s.preview?.mismatchWith != null) return null
        val c = try {
            repo.addContact(payload, verified)
        } catch (e: chat.omcp.app.core.VerificationFailedException) {
            _state.value = ContactAddState(qrStale = true)
            return null
        }
        _state.value = ContactAddState()
        return c.id
    }

    fun cancel() { _state.value = ContactAddState() }
}
