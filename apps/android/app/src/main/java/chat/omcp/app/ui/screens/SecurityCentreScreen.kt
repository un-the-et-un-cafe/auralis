package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.burn.BurnSpec
import chat.omcp.app.core.Contact
import chat.omcp.app.ui.components.AuralisSection
import chat.omcp.app.ui.components.AuralisSettingToggle
import chat.omcp.app.ui.components.AuralisSettingsPage
import chat.omcp.app.ui.components.BodyText
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.components.SafetyNumberBlock
import chat.omcp.app.ui.theme.LocalOmcpColors

/** Contact trust first, then the conversation's privacy and lifetime controls. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun SecurityCentreScreen(
    contact: Contact?,
    suiteLabel: String,
    onSetVerified: (Boolean) -> Unit,
    onBack: () -> Unit,
    privateNotifications: Boolean = false,
    onSetPrivateNotifications: (Boolean) -> Unit = {},
    burnOption: Int = -1,
    burnUnreadTtl: Int = BurnSpec.UNREAD_7D,
    onSetBurn: (option: Int, unreadTtl: Int) -> Unit = { _, _ -> },
) {
    var private by remember(contact?.id, privateNotifications) { mutableStateOf(privateNotifications) }
    var option by remember(contact?.id, burnOption) { mutableStateOf(burnOption) }
    var ttl by remember(contact?.id, burnUnreadTtl) { mutableStateOf(burnUnreadTtl) }
    val colors = LocalOmcpColors.current
    AuralisSettingsPage(
        stringResource(R.string.security_centre), onBack,
        stringResource(R.string.auralis_security_intro),
    ) {
        if (contact == null) {
            Callout(stringResource(R.string.auralis_security_no_contact))
        } else {
            AuralisSection(contact.displayName) {
                Text(shortPeerId(contact.id), fontFamily = FontFamily.Monospace, color = colors.ink2, style = MaterialTheme.typography.bodySmall)
                Callout(
                    stringResource(if (contact.verified) R.string.verified_contact else R.string.unverified),
                    if (contact.verified) CalloutKind.Ok else CalloutKind.Info,
                )
                Text(stringResource(R.string.safety_number), style = MaterialTheme.typography.titleSmall)
                SafetyNumberBlock(contact.safetyNumber)
                BodyText(stringResource(R.string.auralis_security_verify_body))
                OutlinedButton(onClick = { onSetVerified(!contact.verified) }, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(if (contact.verified) R.string.auralis_security_mark_unverified else R.string.auralis_security_mark_verified))
                }
            }
            AuralisSection(stringResource(R.string.auralis_security_notifications)) {
                AuralisSettingToggle(
                    stringResource(R.string.notif_private), stringResource(R.string.notif_private_chat_desc), private,
                ) { private = it; onSetPrivateNotifications(it) }
            }
            AuralisSection(stringResource(R.string.burn_chat_title)) {
                // Flow rows stay usable with long translations and large accessibility text.
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf(-1 to R.string.burn_off, 0 to R.string.burn_on_open, 10 to R.string.burn_10s, 60 to R.string.burn_1m, 3600 to R.string.burn_1h)
                        .forEach { (value, label) ->
                            FilterChip(
                                selected = option == value,
                                onClick = { option = value; onSetBurn(value, ttl) },
                                label = { Text(stringResource(label)) },
                            )
                        }
                }
                Text(stringResource(R.string.burn_unread_ttl), style = MaterialTheme.typography.titleSmall)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf(3600 to R.string.burn_ttl_1h, 86400 to R.string.burn_ttl_24h, 604800 to R.string.burn_ttl_7d)
                        .forEach { (value, label) ->
                            FilterChip(
                                selected = ttl == value,
                                onClick = { ttl = value; onSetBurn(option, value) },
                                label = { Text(stringResource(label)) },
                            )
                        }
                }
                BodyText(stringResource(R.string.burn_honest_copy))
            }
        }
        AuralisSection(stringResource(R.string.auralis_security_session)) {
            BodyText(stringResource(R.string.auralis_security_session_body, suiteLabel))
            Callout(stringResource(R.string.auralis_security_future))
        }
    }
}
