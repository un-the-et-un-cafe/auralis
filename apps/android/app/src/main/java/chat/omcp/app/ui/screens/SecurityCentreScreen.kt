package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.ui.Alignment
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.getValue
import androidx.compose.material3.Switch
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.core.Contact
import chat.omcp.app.ui.components.SafetyNumberBlock
import chat.omcp.app.ui.theme.LocalOmcpColors

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SecurityCentreScreen(
    contact: Contact?,
    suiteLabel: String,
    onSetVerified: (Boolean) -> Unit,
    onBack: () -> Unit,
    privateNotifications: Boolean = false,
    onSetPrivateNotifications: (Boolean) -> Unit = {},
) {
    val c = LocalOmcpColors.current
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.security_centre)) },
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
                    .background(MaterialTheme.colorScheme.surface)
                    .padding(16.dp),
            ) {
                Text(contact?.displayName ?: "—", style = MaterialTheme.typography.titleLarge)
                Text(
                    "PeerID · " + shortPeerId(contact?.id ?: ""),
                    fontFamily = FontFamily.Monospace,
                    color = c.ink2,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(top = 4.dp),
                )
                Text(
                    if (contact?.verified == true) stringResource(R.string.verified_contact) else stringResource(R.string.unverified),
                    color = if (contact?.verified == true) c.ok else c.warn,
                    style = MaterialTheme.typography.labelLarge,
                    modifier = Modifier.padding(top = 8.dp),
                )
            }

            Column(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(14.dp))
                    .background(MaterialTheme.colorScheme.surface)
                    .padding(16.dp),
            ) {
                Text("Safety number", style = MaterialTheme.typography.titleMedium)
                Spacer(Modifier.height(12.dp))
                SafetyNumberBlock(contact?.safetyNumber.orEmpty())
                Row(
                    Modifier.padding(top = 12.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    OutlinedButton(onClick = { onSetVerified(contact?.verified != true) }, enabled = contact != null) {
                        Text(if (contact?.verified == true) "Mark unverified" else "Mark verified")
                    }
                }
            }

            Column(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(14.dp))
                    .background(MaterialTheme.colorScheme.surface)
                    .padding(16.dp),
            ) {
                Text("Session", style = MaterialTheme.typography.titleMedium)
                Text(
                    "End-to-end · Noise hybrid · $suiteLabel",
                    color = c.ink2,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(top = 8.dp),
                )
                // TODO(M5.1): session reset + block list.
                OutlinedButton(onClick = {}, enabled = false, modifier = Modifier.padding(top = 12.dp)) {
                    Text("Reset session")
                }
            }

            Column(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(14.dp))
                    .background(MaterialTheme.colorScheme.surface)
                    .padding(16.dp),
            ) {
                var priv by remember { mutableStateOf(privateNotifications) }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(stringResource(R.string.notif_private), style = MaterialTheme.typography.titleMedium)
                        Text(stringResource(R.string.notif_private_chat_desc), color = c.ink2, style = MaterialTheme.typography.bodyMedium)
                    }
                    Switch(checked = priv, enabled = contact != null, onCheckedChange = { priv = it; onSetPrivateNotifications(it) })
                }
            }

            Button(
                onClick = {},
                enabled = false,
                colors = ButtonDefaults.buttonColors(
                    containerColor = c.danger,
                    contentColor = MaterialTheme.colorScheme.onError,
                ),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Block contact")
            }
        }
    }
}
