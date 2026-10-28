package chat.omcp.app.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Backup
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Hub
import androidx.compose.material.icons.filled.QrCode
import androidx.compose.material.icons.filled.Security
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
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
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.ui.components.AuralisSimulationNotice
import chat.omcp.app.core.Contact
import chat.omcp.app.core.Identity
import chat.omcp.app.ui.components.AuralisActionCard
import chat.omcp.app.ui.components.AuralisPanel
import chat.omcp.app.ui.components.AuralisSectionLabel
import chat.omcp.app.ui.components.AuralisSettingToggle
import chat.omcp.app.ui.theme.LocalOmcpColors
import chat.omcp.app.ui.theme.OmcpDark

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun YouScreen(
    identity: Identity?,
    suiteLabel: String,
    privateNotifications: Boolean,
    onSetPrivateNotifications: (Boolean) -> Unit,
    hideOnLockScreen: Boolean,
    onSetHideOnLockScreen: (Boolean) -> Unit,
    onGateway: () -> Unit,
    contacts: List<Contact>,
    onSecurity: (String) -> Unit,
    onBackup: () -> Unit,
    onStorage: () -> Unit = {},
    onMyQr: () -> Unit,
) {
    val c = LocalOmcpColors.current
    var priv by remember(privateNotifications) { mutableStateOf(privateNotifications) }
    var hideLock by remember(hideOnLockScreen) { mutableStateOf(hideOnLockScreen) }
    var chooseContact by remember { mutableStateOf(false) }
    if (chooseContact) {
        AlertDialog(
            onDismissRequest = { chooseContact = false },
            title = { Text(stringResource(R.string.security_centre)) },
            text = {
                Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (contacts.isEmpty()) Text(stringResource(R.string.auralis_security_empty))
                    contacts.forEach { contact ->
                        TextButton(onClick = { chooseContact = false; onSecurity(contact.id) }, modifier = Modifier.fillMaxWidth()) {
                            Text(contact.displayName)
                        }
                    }
                }
            },
            confirmButton = { TextButton(onClick = { chooseContact = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.tab_you)) }) }) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            val name = identity?.displayName?.takeIf { it.isNotBlank() } ?: stringResource(R.string.tab_you)
            AuralisSimulationNotice()
            Column(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(28.dp)).background(OmcpDark.Bg).padding(24.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Box(Modifier.size(64.dp).clip(RoundedCornerShape(22.dp)).background(OmcpDark.BrandSoft), contentAlignment = Alignment.Center) {
                    Text(name.take(1).uppercase(), style = MaterialTheme.typography.headlineMedium, color = OmcpDark.Brand)
                }
                Text(name, style = MaterialTheme.typography.headlineMedium, color = OmcpDark.Ink)
                Text(stringResource(R.string.auralis_your_identity), style = MaterialTheme.typography.bodyMedium, color = OmcpDark.Ink2)
                Text(shortPeerId(identity?.peerIdHex.orEmpty()), fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall, color = OmcpDark.Ink2)
            }
            AuralisActionCard(stringResource(R.string.contact_add_my_qr), stringResource(R.string.auralis_contact_interop), Icons.Default.QrCode, onMyQr)
            AuralisSectionLabel(stringResource(R.string.auralis_your_controls))
            AuralisActionCard(stringResource(R.string.security_centre), stringResource(R.string.auralis_security_choose), Icons.Default.Security, { chooseContact = true })
            AuralisActionCard(stringResource(R.string.backup_settings_title), stringResource(R.string.backup_settings_desc), Icons.Default.Backup, onBackup)
            AuralisActionCard(stringResource(R.string.storage_title), stringResource(R.string.storage_keep_title), Icons.Default.Folder, onStorage)
            AuralisActionCard(stringResource(R.string.gateway_title), stringResource(R.string.auralis_gateway_availability), Icons.Default.Hub, onGateway)
            AuralisSectionLabel(stringResource(R.string.auralis_notifications))
            AuralisPanel {
                AuralisSettingToggle(stringResource(R.string.notif_private), stringResource(R.string.notif_private_desc), priv) { priv = it; onSetPrivateNotifications(it) }
                AuralisSettingToggle(stringResource(R.string.notif_hide_lock), stringResource(R.string.notif_hide_lock_desc), hideLock) { hideLock = it; onSetHideOnLockScreen(it) }
            }
            AuralisSectionLabel(stringResource(R.string.auralis_about))
            AuralisPanel {
                Text(stringResource(R.string.auralis_about_body), style = MaterialTheme.typography.bodyMedium, color = c.ink2)
                Text(suiteLabel, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall, color = c.ink3)
                Text(stringResource(R.string.auralis_beta_notice), style = MaterialTheme.typography.bodySmall, color = c.ink3)
            }
        }
    }
}
