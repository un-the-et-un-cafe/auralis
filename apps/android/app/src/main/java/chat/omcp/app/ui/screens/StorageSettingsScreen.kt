package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.ui.components.AuralisSection
import chat.omcp.app.ui.components.AuralisSettingsPage
import chat.omcp.app.ui.components.BodyText
import chat.omcp.app.ui.vm.StorageViewModel

/** The actual Rust retention preferences and usage, presented without invented storage numbers. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun StorageSettingsScreen(vm: StorageViewModel, onBack: () -> Unit) {
    val state by vm.state.collectAsState()
    AuralisSettingsPage(
        stringResource(R.string.storage_title), onBack,
        stringResource(R.string.auralis_storage_intro),
    ) {
        AuralisSection(stringResource(R.string.auralis_storage_usage_title)) {
            Text(StorageViewModel.formatBytes(state.usage.bytes), style = MaterialTheme.typography.displaySmall, color = MaterialTheme.colorScheme.primary)
            BodyText(stringResource(R.string.storage_usage, state.usage.count, StorageViewModel.formatBytes(state.usage.bytes)))
        }
        AuralisSection(stringResource(R.string.storage_keep_title)) {
            BodyText(stringResource(R.string.storage_keep_body))
            Text(stringResource(R.string.storage_keep_days), style = MaterialTheme.typography.titleSmall)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                StorageViewModel.DAY_OPTIONS.forEach { days ->
                    FilterChip(
                        selected = state.prefs.maxDays == days,
                        onClick = { vm.setDays(days) },
                        label = { Text(stringResource(R.string.storage_days, days)) },
                    )
                }
            }
            Text(stringResource(R.string.storage_keep_size), style = MaterialTheme.typography.titleSmall)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                StorageViewModel.SIZE_OPTIONS.forEach { bytes ->
                    FilterChip(
                        selected = state.prefs.maxBytes == bytes,
                        onClick = { vm.setMaxBytes(bytes) },
                        label = { Text(StorageViewModel.formatBytes(bytes)) },
                    )
                }
            }
        }
    }
}
