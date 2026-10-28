package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.core.PathKind
import chat.omcp.app.ui.components.AuralisSection
import chat.omcp.app.ui.components.AuralisSettingToggle
import chat.omcp.app.ui.components.AuralisSettingsPage
import chat.omcp.app.ui.components.BodyText
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.PathChip
import chat.omcp.app.ui.vm.GatewayViewModel

/** Gateway policy survives in the original store. This beta has no Internet relay transport. */
@Composable
fun GatewaySettingsScreen(vm: GatewayViewModel, onBack: () -> Unit) {
    val gatewayOn by vm.gatewayOn.collectAsState()
    val allowMobile by vm.allowMobile.collectAsState()
    var showDisclosure by remember { mutableStateOf(false) }
    var showMeteredWarn by remember { mutableStateOf(false) }
    AuralisSettingsPage(
        stringResource(R.string.gateway_title), onBack,
        stringResource(R.string.auralis_gateway_intro),
    ) {
        Callout(stringResource(R.string.auralis_gateway_unavailable))
        AuralisSection(stringResource(R.string.auralis_gateway_preferences)) {
            AuralisSettingToggle(
                stringResource(R.string.auralis_gateway_save),
                stringResource(R.string.auralis_gateway_save_desc), gatewayOn,
            ) { on -> if (on && !gatewayOn) showDisclosure = true else vm.setGateway(on) }
            AuralisSettingToggle(
                stringResource(R.string.gateway_mobile_data),
                stringResource(R.string.gateway_mobile_data_desc), allowMobile,
            ) { on -> if (on) showMeteredWarn = true else vm.setAllowMobile(false) }
        }
        AuralisSection(stringResource(R.string.gateway_cap)) {
            Text(stringResource(R.string.auralis_gateway_cap_value, vm.dailyCapMb), style = MaterialTheme.typography.headlineMedium, color = MaterialTheme.colorScheme.primary)
            BodyText(stringResource(R.string.auralis_gateway_cap_note))
        }
        AuralisSection(stringResource(R.string.auralis_gateway_store_forward)) {
            // Formerly a local-only switch; do not imply an unimplemented relay capability works.
            BodyText(stringResource(R.string.auralis_gateway_store_forward_note))
        }
        AuralisSection(stringResource(R.string.auralis_gateway_paths)) {
            Legend(PathKind.Mesh, stringResource(R.string.path_mesh), stringResource(R.string.auralis_gateway_mesh_desc))
            Legend(PathKind.Gateway, stringResource(R.string.path_gateway), stringResource(R.string.auralis_gateway_gateway_desc))
            Legend(PathKind.Mixed, stringResource(R.string.path_mixed), stringResource(R.string.auralis_gateway_mixed_desc))
            Legend(PathKind.Queued, stringResource(R.string.path_queued), stringResource(R.string.auralis_gateway_queued_desc))
        }
    }
    if (showDisclosure) {
        AlertDialog(
            onDismissRequest = { showDisclosure = false },
            title = { Text(stringResource(R.string.auralis_gateway_disclosure_title)) },
            text = { Text(stringResource(R.string.auralis_gateway_disclosure_body)) },
            confirmButton = {
                TextButton(onClick = { showDisclosure = false; vm.setGateway(true) }) { Text(stringResource(R.string.auralis_gateway_disclosure_confirm)) }
            },
            dismissButton = { TextButton(onClick = { showDisclosure = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
    if (showMeteredWarn) {
        AlertDialog(
            onDismissRequest = { showMeteredWarn = false },
            title = { Text(stringResource(R.string.auralis_gateway_metered)) },
            text = { Text(stringResource(R.string.auralis_gateway_metered_body)) },
            confirmButton = {
                TextButton(onClick = { showMeteredWarn = false; vm.setAllowMobile(true) }) { Text(stringResource(R.string.cta_continue)) }
            },
            dismissButton = { TextButton(onClick = { showMeteredWarn = false }) { Text(stringResource(R.string.auralis_gateway_wifi_only)) } },
        )
    }
}

@Composable
private fun Legend(kind: PathKind, label: String, meaning: String) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        PathChip(kind, label)
        BodyText(meaning)
    }
}
