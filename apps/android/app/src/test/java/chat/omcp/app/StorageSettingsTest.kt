package chat.omcp.app

import chat.omcp.app.core.KeptMediaPrefs
import chat.omcp.app.core.RustOmcpCore
import chat.omcp.app.ui.vm.StorageViewModel
import org.junit.Assert.assertEquals
import org.junit.Test

/** Settings > Storage > Keep sent media over the real Rust core (REQUIREMENTS "Keep sent media", RS3). */
class StorageSettingsTest {
    @Test
    fun defaultsAreNinetyDaysTwoGbAndLimitsApply() {
        val core = RustOmcpCore()
        core.load(core.generateRecoveryPhrase(), "Alice")
        val vm = StorageViewModel(core)
        assertEquals(KeptMediaPrefs(90, 2_000_000_000L), vm.state.value.prefs)
        vm.setDays(30)
        vm.setMaxBytes(500_000_000L)
        assertEquals(KeptMediaPrefs(30, 500_000_000L), core.keptMediaSettings())
        assertEquals(KeptMediaPrefs(30, 500_000_000L), vm.state.value.prefs)
        assertEquals(0, vm.state.value.usage.count)
    }

    @Test
    fun formatsDecimalUnits() {
        assertEquals("2 GB", StorageViewModel.formatBytes(2_000_000_000L))
        assertEquals("500 MB", StorageViewModel.formatBytes(500_000_000L))
        assertEquals("1.5 KB", StorageViewModel.formatBytes(1_500L))
    }
}
