package chat.omcp.app.ui.screens

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.core.OmcpCoreStub
import chat.omcp.app.ui.theme.LocalOmcpColors

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun YouScreen(
    onGateway: () -> Unit,
    onSecurity: () -> Unit,
) {
    val c = LocalOmcpColors.current
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.tab_you)) }) }) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            Column(Modifier.padding(16.dp)) {
                Text("You", style = MaterialTheme.typography.headlineSmall)
                Text(
                    "PeerID · omcp1…local-stub",
                    fontFamily = FontFamily.Monospace,
                    color = c.ink2,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(top = 4.dp),
                )
                Text(
                    stringResource(R.string.mesh_placeholder),
                    color = c.ink3,
                    style = MaterialTheme.typography.labelSmall,
                    modifier = Modifier.padding(top = 8.dp),
                )
                Text(
                    "Suite: ${OmcpCoreStub.SUITE_LABEL}",
                    color = c.ink3,
                    style = MaterialTheme.typography.labelSmall,
                )
            }
            HorizontalDivider()
            ListItem(
                headlineContent = { Text(stringResource(R.string.gateway_title)) },
                supportingContent = { Text("Wi‑Fi-only default · 500 MB/day cap") },
                trailingContent = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null) },
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable(onClick = onGateway),
            )
            ListItem(
                headlineContent = { Text(stringResource(R.string.security_centre)) },
                supportingContent = { Text("Safety numbers, session, block list") },
                trailingContent = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null) },
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable(onClick = onSecurity),
            )
            ListItem(
                headlineContent = { Text("Notifications") },
                supportingContent = { Text("Previews off by default") },
            )
            ListItem(
                headlineContent = { Text("Backup / restore") },
                supportingContent = { Text("Encrypted local backup — deferred wiring") },
            )
            ListItem(
                headlineContent = { Text("About") },
                supportingContent = { Text("Open Mesh Chat · MIT · PROTOCOL.md") },
            )
        }
    }
}
