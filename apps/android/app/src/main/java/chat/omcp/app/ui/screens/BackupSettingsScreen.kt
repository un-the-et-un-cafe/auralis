package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.ui.components.AuralisSection
import chat.omcp.app.ui.components.AuralisSettingToggle
import chat.omcp.app.ui.components.AuralisSettingsPage
import chat.omcp.app.ui.components.BodyText
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.vm.BackupSettingsState
import chat.omcp.app.ui.vm.BackupSettingsViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.text.DateFormat
import java.util.Date

/** Recovery dashboard: identity, local recovery phrase, then conversation/media backup. */
@Composable
fun BackupSettingsScreen(
    vm: BackupSettingsViewModel,
    requestDeviceAuth: (onSuccess: () -> Unit) -> Unit,
    secureWindow: (Boolean) -> Unit,
    onBack: () -> Unit,
) {
    val s by vm.state.collectAsState()
    val io = rememberCoroutineScope()
    var busy by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    LaunchedEffect(s.shownPhrase != null) { secureWindow(s.shownPhrase != null) }
    DisposableEffect(vm) { onDispose { vm.hidePhrase() } }

    fun cloudAction(action: () -> Unit) {
        if (busy) return
        busy = true
        io.launch {
            try { withContext(Dispatchers.IO) { action() } } finally { busy = false }
        }
    }

    AuralisSettingsPage(
        stringResource(R.string.backup_settings_title), onBack,
        stringResource(R.string.auralis_backup_intro),
    ) {
        AuralisSection(stringResource(R.string.backup_drive)) {
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
                    value = s.password, onValueChange = vm::setPassword,
                    label = { Text(stringResource(if (s.driveOn) R.string.backup_change_pw else R.string.onb_backup_password)) },
                    supportingText = { Text(stringResource(R.string.backup_pw_rule)) },
                    visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrect = false),
                    enabled = !busy, singleLine = true, modifier = Modifier.fillMaxWidth(),
                )
                Button(
                    onClick = { cloudAction { vm.uploadWithPassword() } },
                    enabled = s.passwordOk && !busy,
                    modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp),
                ) { Text(stringResource(if (busy) R.string.auralis_backup_working else R.string.backup_drive_cta)) }
                if (s.driveOn) {
                    TextButton(onClick = { confirmDelete = true }, enabled = !busy) {
                        Text(stringResource(R.string.backup_delete), color = MaterialTheme.colorScheme.error)
                    }
                }
            }
        }
        AuralisSection(stringResource(R.string.backup_written)) {
            BodyText(stringResource(if (s.writtenOn) R.string.backup_written_done else R.string.backup_written_off))
            val words = s.shownPhrase
            if (words == null) {
                OutlinedButton(
                    onClick = { requestDeviceAuth { vm.revealPhraseAfterAuth() } },
                    modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp),
                ) { Text(stringResource(R.string.backup_view_phrase)) }
            } else {
                BodyText(stringResource(R.string.onb_words_body))
                words.chunked(2).forEachIndexed { row, pair ->
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                        pair.forEachIndexed { i, word ->
                            Column(Modifier.weight(1f)) {
                                Text("${row * 2 + i + 1}", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
                                Text(word, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodyLarge)
                            }
                        }
                    }
                }
                OutlinedButton(onClick = vm::hidePhrase, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(R.string.backup_hide_phrase))
                }
            }
        }
        MediaBackupSection(s, vm)
        s.message?.let { Callout(it) }
    }
    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text(stringResource(R.string.backup_delete)) },
            text = { Text(stringResource(R.string.auralis_backup_delete_body)) },
            confirmButton = {
                TextButton(onClick = { confirmDelete = false; cloudAction { vm.deleteCloudBackup() } }) {
                    Text(stringResource(R.string.backup_delete), color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

@Composable
private fun MediaBackupSection(s: BackupSettingsState, vm: BackupSettingsViewModel) {
    val media = s.media
    AuralisSection(stringResource(R.string.chatbackup_title)) {
        BodyText(stringResource(R.string.chatbackup_desc))
        s.chatsBackedUpMs?.let {
            Callout(stringResource(R.string.chatbackup_last, DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(it))), CalloutKind.Ok)
        }
        OutlinedButton(onClick = { vm.backUpChatsNow() }, modifier = Modifier.fillMaxWidth()) {
            Text(stringResource(R.string.chatbackup_now))
        }
    }
    AuralisSection {
        AuralisSettingToggle(stringResource(R.string.mediabackup_master), stringResource(R.string.mediabackup_master_sub), media.enabled) {
            vm.setMedia(if (it) media.copy(enabled = true, photos = true, voice = true, files = true, videos = false) else media.copy(enabled = false))
        }
        if (!media.enabled) {
            Callout(stringResource(R.string.mediabackup_off_note))
        } else {
            AuralisSettingToggle(stringResource(R.string.mediabackup_photos), checked = media.photos) { vm.setMedia(media.copy(photos = it)) }
            AuralisSettingToggle(stringResource(R.string.mediabackup_videos), checked = media.videos) { vm.setMedia(media.copy(videos = it)) }
            AuralisSettingToggle(stringResource(R.string.mediabackup_voice), checked = media.voice) { vm.setMedia(media.copy(voice = it)) }
            AuralisSettingToggle(stringResource(R.string.mediabackup_files), checked = media.files) { vm.setMedia(media.copy(files = it)) }
            AuralisSettingToggle(stringResource(R.string.mediabackup_mobile), stringResource(R.string.mediabackup_mobile_sub), media.mobileData) { vm.setMedia(media.copy(mobileData = it)) }
        }
    }
    if (s.confirmMediaOff) {
        AlertDialog(
            onDismissRequest = vm::keepMedia,
            title = { Text(stringResource(R.string.mediabackup_off_title)) },
            text = { Text(stringResource(R.string.mediabackup_off_body)) },
            confirmButton = { TextButton(onClick = { vm.confirmMediaOff() }) { Text(stringResource(R.string.mediabackup_off_confirm), color = MaterialTheme.colorScheme.error) } },
            dismissButton = { TextButton(onClick = vm::keepMedia) { Text(stringResource(R.string.mediabackup_keep)) } },
        )
    }
}
