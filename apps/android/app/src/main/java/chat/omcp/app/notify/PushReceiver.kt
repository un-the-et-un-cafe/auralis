package chat.omcp.app.notify

/**
 * Content-free internet wake-up push (REQUIREMENTS "Notifications (v0.1.2)" › Delivery).
 * The payload carries ONLY the opaque mailbox id; the app fetches + decrypts on-device.
 */
data class PushPayload(val mailboxId: String) {
    /** Data-message map as it would be sent through FCM. Exactly one key. */
    fun toData(): Map<String, String> = mapOf(KEY_MAILBOX to mailboxId)

    companion object {
        const val KEY_MAILBOX = "mb"

        /** Rejects any payload with extra keys, so a relay can never smuggle content in. */
        fun fromData(data: Map<String, String>): PushPayload? {
            if (data.keys != setOf(KEY_MAILBOX)) return null
            val id = data.getValue(KEY_MAILBOX)
            if (id.isEmpty() || id.length > 128 || !id.all { it.isLetterOrDigit() || it == '-' || it == '_' }) return null
            return PushPayload(id)
        }
    }
}

/** Abstraction over FCM so the app builds without a Firebase project. */
interface PushReceiver {
    /** Registration token to give the gateway, or null if push is unavailable. */
    fun token(): String?

    /** Called by the platform handler with the raw data map. */
    fun onPush(data: Map<String, String>)
}

/**
 * STUB: no Firebase project yet, so there is no FirebaseMessagingService. Valid
 * wake-ups are forwarded to [onWake] (which should sync the mailbox and raise the
 * notification locally via NotificationPolicy). Swap in an FCM-backed impl later.
 */
class StubPushReceiver(private val onWake: (PushPayload) -> Unit) : PushReceiver {
    override fun token(): String? = null

    override fun onPush(data: Map<String, String>) {
        PushPayload.fromData(data)?.let(onWake)
    }
}
