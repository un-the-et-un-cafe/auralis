package chat.omcp.app.ui.vm

import androidx.lifecycle.ViewModel
import chat.omcp.app.core.KeptMediaPrefs
import chat.omcp.app.core.KeptMediaUsageInfo
import chat.omcp.app.core.OmcpCore
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Settings > Storage > Keep sent media (REQUIREMENTS "Keep sent media"); limits live in Rust. */
class StorageViewModel(
    private val core: OmcpCore,
    private val clock: () -> Long = { System.currentTimeMillis() },
) : ViewModel() {
    data class State(val prefs: KeptMediaPrefs, val usage: KeptMediaUsageInfo)

    private val _state = MutableStateFlow(read())
    val state: StateFlow<State> = _state.asStateFlow()

    private fun read() = State(
        runCatching { core.keptMediaSettings() }.getOrDefault(KeptMediaPrefs()),
        runCatching { core.keptMediaUsage() }.getOrDefault(KeptMediaUsageInfo(0, 0)),
    )

    fun setDays(days: Int) = apply(_state.value.prefs.copy(maxDays = days))
    fun setMaxBytes(bytes: Long) = apply(_state.value.prefs.copy(maxBytes = bytes))

    private fun apply(p: KeptMediaPrefs) {
        runCatching { core.setKeptMediaSettings(p, clock()) }
        _state.value = read()
    }

    companion object {
        val DAY_OPTIONS = listOf(7, 30, 90, 180, 365)
        val SIZE_OPTIONS = listOf(500_000_000L, 1_000_000_000L, 2_000_000_000L, 5_000_000_000L, 10_000_000_000L)

        /** Decimal units, like the 50 MB file cap. */
        fun formatBytes(b: Long): String = when {
            b >= 1_000_000_000L -> "%s GB".format(trim(b / 1e9))
            b >= 1_000_000L -> "%s MB".format(trim(b / 1e6))
            b >= 1_000L -> "%s KB".format(trim(b / 1e3))
            else -> "$b B"
        }

        private fun trim(v: Double) = if (v == Math.floor(v)) v.toLong().toString() else "%.1f".format(java.util.Locale.ROOT, v)
    }
}
