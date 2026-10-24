package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import chat.omcp.app.R
import chat.omcp.app.ui.components.BodyText
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.components.PlainPage
import chat.omcp.app.ui.vm.BackupSettingsViewModel
import java.text.DateFormat
import java.util.Date

/** Settings › Backup (plain layout; design v0.1.2 pending). */
@Composable
fun BackupSettingsScreen(
    vm: BackupSettingsViewModel,
    requestDeviceAuth: (onSuccess: () -> Unit) -> Unit,
    secureWindow: (Boolean) -> Unit,
    onBack: () -> Unit,
) {
    val s by vm.state.collectAsState()
    LaunchedEffect(s.shownPhrase != null) { secureWindow(s.shownPhrase != null) }
    PlainPage(stringResource(R.string.backup_settings_title)) {
        Text(stringResource(R.string.backup_drive), style = MaterialTheme.typography.titleMedium)
        when {
            !s.driveAvailable -> Callout(stringResource(R.string.backup_drive_unavailable), CalloutKind.Warn)
            s.driveOn -> Callout(
                stringResource(R.string.backup_last, DateFormat.getDateTimeInstance().format(Date(s.lastBackupMs))),
                CalloutKind.Ok,
            )
            else -> Callout(stringResource(R.string.backup_drive_off))
        }
        if (s.driveAvailable) {
            BodyText(stringResource(R.string.backup_honest_copy))
            OutlinedTextField(
                s.password, vm::setPassword,
                label = { Text(if (s.driveOn) stringResource(R.string.backup_change_pw) else stringResource(R.string.onb_backup_password)) },
                supportingText = { Text(stringResource(R.string.backup_pw_rule)) },
                visualTransformation = PasswordVisualTransformation(),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrect = false),
                modifier = Modifier.fillMaxWidth(),
            )
            Button(onClick = { vm.uploadWithPassword() }, enabled = s.passwordOk, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.backup_drive_cta))
            }
            if (s.driveOn) {
                OutlinedButton(onClick = { vm.deleteCloudBackup() }, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(R.string.backup_delete))
                }
            }
        }
        Text(stringResource(R.string.backup_written), style = MaterialTheme.typography.titleMedium)
        BodyText(if (s.writtenOn) stringResource(R.string.backup_written_done) else stringResource(R.string.backup_written_off))
        val words = s.shownPhrase
        if (words == null) {
            OutlinedButton(onClick = { requestDeviceAuth { vm.revealPhraseAfterAuth() } }, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.backup_view_phrase))
            }
        } else {
            words.chunked(3).forEachIndexed { row, ws ->
                Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                    ws.forEachIndexed { i, w -> Text("${row * 3 + i + 1}. $w", fontFamily = FontFamily.Monospace) }
                }
            }
            OutlinedButton(onClick = vm::hidePhrase, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.backup_hide_phrase)) }
        }
        s.message?.let { Callout(it) }
        TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
    }
}
