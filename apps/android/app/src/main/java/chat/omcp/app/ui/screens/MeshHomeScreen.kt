package chat.omcp.app.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Bluetooth
import androidx.compose.material.icons.filled.Groups
import androidx.compose.material.icons.filled.Hub
import androidx.compose.material.icons.filled.LiveTv
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.ui.components.AuralisSimulationNotice
import chat.omcp.app.core.MeshStatus
import chat.omcp.app.core.PathKind
import chat.omcp.app.ui.components.AuralisActionCard
import chat.omcp.app.ui.components.AuralisPanel
import chat.omcp.app.ui.components.AuralisSectionLabel
import chat.omcp.app.ui.components.PathChip
import chat.omcp.app.ui.theme.LocalOmcpColors
import chat.omcp.app.ui.theme.OmcpDark

/** Mesh visibility is grounded in the repository; no simulated radar or invented neighbours. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MeshHomeScreen(
    status: MeshStatus,
    onFindNearby: () -> Unit,
    onAddContact: () -> Unit,
    onGateway: () -> Unit,
    onCommunities: () -> Unit,
    onLive: () -> Unit,
) {
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.tab_mesh)) }) }) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Column(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(28.dp)).background(OmcpDark.Bg).padding(24.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Text(stringResource(R.string.auralis_mesh_eyebrow), style = MaterialTheme.typography.labelMedium, color = OmcpDark.Brand)
                Text(stringResource(R.string.auralis_mesh_title), style = MaterialTheme.typography.headlineMedium, color = OmcpDark.Ink)
                Text(stringResource(R.string.auralis_mesh_body), style = MaterialTheme.typography.bodyLarge, color = OmcpDark.Ink2)
                Row(horizontalArrangement = Arrangement.spacedBy(24.dp)) {
                    MeshMetric(status.nearbyPeers, stringResource(R.string.auralis_nearby_connections), Modifier.weight(1f))
                    MeshMetric(status.queuedCount, stringResource(R.string.auralis_waiting_messages), Modifier.weight(1f))
                }
            }
            AuralisPanel {
                PathChip(if (status.nearbyPeers > 0) status.aggregatePath else PathKind.Queued,
                    stringResource(if (status.nearbyPeers > 0) R.string.auralis_mesh_connected else R.string.path_queued))
                Text(stringResource(if (status.nearbyPeers > 0) R.string.auralis_mesh_connected_body else R.string.auralis_mesh_waiting_body),
                    style = MaterialTheme.typography.bodyMedium, color = LocalOmcpColors.current.ink2)
                Button(onClick = onFindNearby, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) {
                    Text(stringResource(R.string.onb_find_nearby))
                }
            }
            AuralisSectionLabel(stringResource(R.string.auralis_connect))
            AuralisSimulationNotice()
            AuralisActionCard(stringResource(R.string.contact_add_title), stringResource(R.string.auralis_contact_interop), Icons.Default.QrCodeScanner, onAddContact)
            AuralisActionCard(stringResource(R.string.gateway_title), stringResource(R.string.auralis_gateway_availability), Icons.Default.Hub, onGateway)
            AuralisSectionLabel(stringResource(R.string.auralis_on_horizon))
            AuralisActionCard(stringResource(R.string.tab_communities), stringResource(R.string.coming_v02_title), Icons.Default.Groups, onCommunities)
            AuralisActionCard(stringResource(R.string.tab_live), stringResource(R.string.coming_v02_title), Icons.Default.LiveTv, onLive)
            Text(stringResource(R.string.auralis_mesh_hardware_notice), style = MaterialTheme.typography.bodySmall, color = LocalOmcpColors.current.ink3)
        }
    }
}

@Composable
private fun MeshMetric(value: Int, label: String, modifier: Modifier = Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(value.toString(), style = MaterialTheme.typography.displaySmall, color = OmcpDark.Brand)
        Text(label, style = MaterialTheme.typography.bodySmall, color = OmcpDark.Ink2)
    }
}
