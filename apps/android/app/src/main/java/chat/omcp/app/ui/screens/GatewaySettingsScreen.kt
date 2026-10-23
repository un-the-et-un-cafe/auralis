package chat.omcp.app.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.core.OmcpCoreStub
import chat.omcp.app.ui.components.PathChip
import chat.omcp.app.ui.theme.LocalOmcpColors

/** Jakob / Designy: 500 MB/day, Wi‑Fi-only default, warn (not hard-block) on metered cellular.
 * Cap meter always starts at **0 of 500 MB** — never ship a fake “200 of 500” fill. */
private const val DEFAULT_DAILY_CAP_MB = 500
private const val USED_MB = 0 // real metering not wired yet; Designy lock-in: 0 used

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GatewaySettingsScreen(onBack: () -> Unit) {
    val c = LocalOmcpColors.current
    var gatewayOn by remember { mutableStateOf(false) }
    var allowMobile by remember { mutableStateOf(false) } // Wi‑Fi-only default
    var storeForward by remember { mutableStateOf(false) }
    var showDisclosure by remember { mutableStateOf(false) }
    var showMeteredWarn by remember { mutableStateOf(false) }

    if (showDisclosure) {
        AlertDialog(
            onDismissRequest = { showDisclosure = false },
            title = { Text("Help nearby peers reach the Internet") },
            text = {
                Text(
                    "When on, your phone forwards only encrypted OMCP envelopes. " +
                        "You never see message content. Uses battery and may use mobile data.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    showDisclosure = false
                    gatewayOn = true
                }) { Text("Enable gateway") }
            },
            dismissButton = {
                TextButton(onClick = { showDisclosure = false }) { Text("Cancel") }
            },
        )
    }
    if (showMeteredWarn) {
        AlertDialog(
            onDismissRequest = { showMeteredWarn = false },
            title = { Text("Metered cellular") },
            text = { Text(stringResource(R.string.gateway_metered_warn)) },
            confirmButton = {
                TextButton(onClick = {
                    showMeteredWarn = false
                    allowMobile = true
                }) { Text("Continue") }
            },
            dismissButton = {
                TextButton(onClick = { showMeteredWarn = false }) { Text("Keep Wi‑Fi only") }
            },
        )
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.gateway_title)) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(14.dp))
                    .background(c.warnSoft)
                    .padding(14.dp),
            ) {
                Text("Help nearby peers reach the Internet", style = MaterialTheme.typography.titleMedium)
                Text(
                    "Forwards encrypted envelopes only. Battery and data may increase.",
                    color = c.ink2,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(top = 6.dp),
                )
            }

            SettingsCard {
                SettingToggle(
                    title = stringResource(R.string.gateway_act_as),
                    desc = stringResource(R.string.gateway_act_as_desc),
                    checked = gatewayOn,
                    onCheckedChange = { on ->
                        if (on && !gatewayOn) showDisclosure = true else gatewayOn = on
                    },
                )
                SettingToggle(
                    title = stringResource(R.string.gateway_mobile_data),
                    desc = stringResource(R.string.gateway_mobile_data_desc),
                    checked = allowMobile,
                    onCheckedChange = { on ->
                        if (on) showMeteredWarn = true else allowMobile = false
                    },
                )
                Text(stringResource(R.string.gateway_cap), style = MaterialTheme.typography.titleSmall)
                Text(
                    stringResource(R.string.gateway_cap_desc, USED_MB, DEFAULT_DAILY_CAP_MB),
                    color = c.ink2,
                    style = MaterialTheme.typography.bodySmall,
                )
                LinearProgressIndicator(
                    progress = { USED_MB / DEFAULT_DAILY_CAP_MB.toFloat() },
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 8.dp)
                        .height(6.dp)
                        .clip(RoundedCornerShape(999.dp)),
                )
            }

            SettingsCard {
                Text("Battery & privacy", style = MaterialTheme.typography.titleMedium)
                Text(
                    "Gateway mode keeps BLE active. Relays see encrypted envelopes and timing — not names or text.",
                    color = c.ink2,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(top = 8.dp),
                )
                SettingToggle(
                    title = "Store-and-forward",
                    desc = "Hold encrypted envelopes up to 24 h. Off by default on mobile.",
                    checked = storeForward,
                    onCheckedChange = { storeForward = it },
                )
            }

            SettingsCard {
                Text("Path indicators", style = MaterialTheme.typography.titleMedium)
                Column(Modifier.padding(top = 12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Legend(OmcpCoreStub.PathKind.Mesh, "Mesh", "Local BLE only")
                    Legend(OmcpCoreStub.PathKind.Gateway, "Gateway", "Via a neighbour’s Internet")
                    Legend(OmcpCoreStub.PathKind.Mixed, "Mixed", "Some via mesh, some via gateway")
                    Legend(OmcpCoreStub.PathKind.Queued, "Queued", "Waiting for a path")
                }
            }

            Text(
                stringResource(R.string.ios_companion_hint),
                color = c.ink2,
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

@Composable
private fun SettingsCard(content: @Composable () -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(MaterialTheme.colorScheme.surface)
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        content = { content() },
    )
}

@Composable
private fun SettingToggle(
    title: String,
    desc: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    val c = LocalOmcpColors.current
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.titleSmall)
            Text(desc, color = c.ink2, style = MaterialTheme.typography.bodySmall)
        }
        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}

@Composable
private fun Legend(kind: OmcpCoreStub.PathKind, label: String, meaning: String) {
    val c = LocalOmcpColors.current
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        PathChip(kind, label)
        Text(meaning, color = c.ink2, style = MaterialTheme.typography.bodySmall)
    }
}
