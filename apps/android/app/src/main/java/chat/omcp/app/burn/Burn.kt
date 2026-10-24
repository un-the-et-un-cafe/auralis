package chat.omcp.app.burn

/**
 * UI-side value for the encrypted `burn` header (PROTOCOL §19.1). All burn rules
 * (validation, timers, unread expiry, single use, OPENED, crypto-shredding) live in
 * Rust (`omcp-core::burn` through omcp-ffi); see [chat.omcp.app.core.OmcpCore].
 */
data class BurnSpec(val mode: Mode, val secs: Int, val unreadTtlSecs: Int = UNREAD_7D) {
    enum class Mode { OnOpen, Timer }

    companion object {
        const val UNREAD_1H = 3_600
        const val UNREAD_24H = 86_400
        const val UNREAD_7D = 604_800

        fun onOpen(ttl: Int = UNREAD_7D) = BurnSpec(Mode.OnOpen, 0, ttl)
        fun timer(secs: Int, ttl: Int = UNREAD_7D) = BurnSpec(Mode.Timer, secs, ttl)

        /** Per-chat setting encoding: -1 off, 0 on open, else timer secs. */
        fun fromOption(opt: Int, ttl: Int): BurnSpec? = when (opt) {
            -1 -> null
            0 -> onOpen(ttl)
            else -> timer(opt, ttl)
        }
    }
}

/** Burn content handed out exactly once by the core. */
class BurnContent(val text: String, val bytes: ByteArray, val mime: String?, val burnAtMs: Long?) {
    val body: ByteArray get() = if (mime != null) bytes else text.toByteArray()
}

/** Sender side: the peer opened our burn message (from an encrypted OPENED receipt). */
data class OpenedInfo(val contactId: String, val msgId: String, val openedMs: Long, val shredded: Boolean, val burnAtMs: Long?)
