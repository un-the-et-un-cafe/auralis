package chat.omcp.app.burn

import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

/** Mirrors `omcp-core::burn::BurnHeader` (PROTOCOL §19.1). */
data class BurnSpec(val mode: Mode, val secs: Int, val unreadTtlSecs: Int = UNREAD_7D) {
    enum class Mode { OnOpen, Timer }

    init {
        require(unreadTtlSecs in UNREAD_TTLS) { "unread_ttl" }
        require(if (mode == Mode.OnOpen) secs == 0 else secs in TIMERS) { "secs" }
    }

    companion object {
        const val UNREAD_1H = 3_600
        const val UNREAD_24H = 86_400
        const val UNREAD_7D = 604_800
        val UNREAD_TTLS = listOf(UNREAD_1H, UNREAD_24H, UNREAD_7D)
        val TIMERS = listOf(10, 60, 3_600)
        const val MAX_GROUP_MEMBERS = 256

        fun onOpen(ttl: Int = UNREAD_7D) = BurnSpec(Mode.OnOpen, 0, ttl)
        fun timer(secs: Int, ttl: Int = UNREAD_7D) = BurnSpec(Mode.Timer, secs, ttl)

        /** Per-chat option encoding used by settings: -1 off, 0 on open, else timer secs. */
        fun fromOption(opt: Int, ttl: Int): BurnSpec? = when (opt) {
            -1 -> null
            0 -> onOpen(ttl)
            else -> timer(opt, ttl)
        }
    }

    /** Outer envelope expires_at = sent + unread_ttl. */
    fun expiresAtMs(sentMs: Long) = sentMs + unreadTtlSecs * 1000L
}

/**
 * In-memory store for burnable content (never written to disk). Each item is
 * sealed with its own AES-256-GCM key; [shred] zeroizes key + ciphertext.
 */
class BurnVault(private val rng: SecureRandom = SecureRandom()) {
    private class Item(val key: ByteArray, val iv: ByteArray, val sealed: ByteArray)
    private val items = HashMap<String, Item>()

    @Synchronized fun put(id: String, plain: ByteArray) {
        val key = ByteArray(32).also(rng::nextBytes)
        val iv = ByteArray(12).also(rng::nextBytes)
        val c = Cipher.getInstance("AES/GCM/NoPadding")
        c.init(Cipher.ENCRYPT_MODE, SecretKeySpec(key, "AES"), GCMParameterSpec(128, iv))
        items[id] = Item(key, iv, c.doFinal(plain))
        plain.fill(0)
    }

    @Synchronized fun read(id: String): ByteArray? {
        val it = items[id] ?: return null
        val c = Cipher.getInstance("AES/GCM/NoPadding")
        c.init(Cipher.DECRYPT_MODE, SecretKeySpec(it.key, "AES"), GCMParameterSpec(128, it.iv))
        return c.doFinal(it.sealed)
    }

    /** Crypto-shred: zero the key and ciphertext, then forget. */
    @Synchronized fun shred(id: String) {
        items.remove(id)?.let { it.key.fill(0); it.sealed.fill(0) }
    }

    @Synchronized fun contains(id: String) = id in items
    @Synchronized fun size() = items.size
}

/** Sends the encrypted `OPENED` control message (PROTOCOL §19.1). */
fun interface BurnControl {
    fun sendOpened(contactId: String, msgId: String, openedMs: Long)
}
