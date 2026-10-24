package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.ui.theme.LocalOmcpColors
import chat.omcp.app.ui.vm.StorageViewModel

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun StorageSettingsScreen(vm: StorageViewModel, onBack: () -> Unit) {
    val c = LocalOmcpColors.current
    val s by vm.state.collectAsState()
    Scaffold(topBar = {
        TopAppBar(
            title = { Text(stringResource(R.string.storage_title)) },
            navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, null) } },
        )
    }) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(stringResource(R.string.storage_keep_title), style = MaterialTheme.typography.titleMedium)
            Text(stringResource(R.string.storage_keep_body), color = c.ink2, style = MaterialTheme.typography.bodySmall)
            Text(stringResource(R.string.storage_keep_days), style = MaterialTheme.typography.labelLarge)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                StorageViewModel.DAY_OPTIONS.forEach { d ->
                    FilterChip(selected = s.prefs.maxDays == d, onClick = { vm.setDays(d) }, label = { Text(stringResource(R.string.storage_days, d)) })
                }
            }
            Text(stringResource(R.string.storage_keep_size), style = MaterialTheme.typography.labelLarge)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                StorageViewModel.SIZE_OPTIONS.forEach { b ->
                    FilterChip(selected = s.prefs.maxBytes == b, onClick = { vm.setMaxBytes(b) }, label = { Text(StorageViewModel.formatBytes(b)) })
                }
            }
            Text(
                stringResource(R.string.storage_usage, s.usage.count, StorageViewModel.formatBytes(s.usage.bytes)),
                color = c.ink3, style = MaterialTheme.typography.labelSmall,
            )
        }
    }
}
