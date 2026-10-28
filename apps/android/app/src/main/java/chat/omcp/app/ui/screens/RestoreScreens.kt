package chat.omcp.app.ui.screens

import android.content.Intent
import android.net.Uri
import android.text.format.Formatter
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.restore.RestoreErrorKind
import chat.omcp.app.restore.RestoreStage
import chat.omcp.app.ui.components.AuralisSection
import chat.omcp.app.ui.components.BodyText
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.components.PlainPage
import chat.omcp.app.ui.theme.LocalOmcpColors
import chat.omcp.app.ui.vm.OnboardingState
import chat.omcp.app.ui.vm.OnboardingViewModel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.text.DateFormat
import java.util.Date

/** Auralis recovery journey. State, throttling, Drive consent and restore jobs remain in the VM. */
private fun CoroutineScope.bg(action: () -> Unit) = launch(Dispatchers.IO) { action() }

@Composable
fun RestoreCheckingScreen() = PlainPage(stringResource(R.string.restore_detect_title)) {
    AuralisSection {
        CircularProgressIndicator(Modifier.size(32.dp), strokeWidth = 3.dp)
        BodyText(stringResource(R.string.restore_checking))
    }
}

@Composable
private fun BackupDetail(label: String, value: String) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(label, style = MaterialTheme.typography.labelMedium, color = LocalOmcpColors.current.ink2)
        Text(value, style = MaterialTheme.typography.bodyLarge)
    }
}

@Composable
fun RestoreDetectScreen(s: OnboardingState, vm: OnboardingViewModel, io: CoroutineScope) {
    val context = LocalContext.current
    PlainPage(stringResource(R.string.restore_detect_title)) {
        BodyText(stringResource(R.string.restore_detect_body))
        AuralisSection(s.serviceLabel) {
            Callout(stringResource(R.string.restore_found), CalloutKind.Ok)
            s.foundBackup?.let { metadata ->
                BackupDetail(stringResource(R.string.restore_kv_backed_up), DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(metadata.createdAtMs)))
                if (metadata.deviceHint.isNotBlank()) BackupDetail(stringResource(R.string.restore_kv_device), metadata.deviceHint)
                BackupDetail(stringResource(R.string.restore_kv_size), Formatter.formatShortFileSize(context, s.foundBackupSize.toLong()))
                BackupDetail(stringResource(R.string.restore_kv_protection), stringResource(R.string.restore_protection_password))
            }
        }
        Button(onClick = { io.bg(vm::restoreTapped) }, enabled = !s.restoreBusy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) {
            Text(stringResource(R.string.restore_cta))
        }
        OutlinedButton(onClick = vm::usePhraseInstead, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.restore_use_words)) }
        TextButton(onClick = { io.bg(vm::chooseDrive) }, enabled = !s.restoreBusy) { Text(stringResource(R.string.restore_other_account)) }
        TextButton(onClick = vm::startFresh) { Text(stringResource(R.string.restore_start_fresh)) }
    }
    if (s.freshConfirm) {
        val focus = remember { FocusRequester() }
        LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
        AlertDialog(
            onDismissRequest = vm::restoreInstead,
            title = { Text(stringResource(R.string.restore_fresh_title)) },
            text = { Text(stringResource(R.string.restore_fresh_body, s.serviceLabel)) },
            confirmButton = {
                Button(onClick = vm::restoreInstead, modifier = Modifier.focusRequester(focus)) { Text(stringResource(R.string.restore_fresh_restore_instead)) }
            },
            dismissButton = { TextButton(onClick = vm::confirmStartFresh) { Text(stringResource(R.string.restore_fresh_create)) } },
        )
    }
}

@Composable
private fun RecoveryOption(title: String, description: String, enabled: Boolean = true, busy: Boolean = false, onClick: () -> Unit) {
    OutlinedCard(onClick = onClick, enabled = enabled, modifier = Modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large) {
        Column(Modifier.padding(22.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
            BodyText(description)
            if (busy) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
        }
    }
}

@Composable
fun RestoreSourceScreen(s: OnboardingState, vm: OnboardingViewModel, io: CoroutineScope) = PlainPage(stringResource(R.string.restore_source_title)) {
    BodyText(stringResource(R.string.restore_source_body))
    if (s.driveAvailable) {
        RecoveryOption(
            stringResource(R.string.restore_source_drive), stringResource(R.string.restore_source_drive_sub),
            enabled = !s.restoreBusy, busy = s.restoreBusy,
        ) { io.bg(vm::chooseDrive) }
    }
    RecoveryOption(stringResource(R.string.restore_source_words), stringResource(R.string.restore_source_words_sub), onClick = vm::usePhraseInstead)
    TextButton(onClick = vm::backToWelcome) { Text(stringResource(R.string.back)) }
}

@Composable
fun RestoreNoBackupScreen(s: OnboardingState, vm: OnboardingViewModel, io: CoroutineScope) = PlainPage(stringResource(R.string.restore_nobackup_title)) {
    AuralisSection(s.serviceLabel) { BodyText(stringResource(R.string.restore_nobackup_body, s.serviceLabel)) }
    Button(onClick = { io.bg(vm::chooseDrive) }, enabled = !s.restoreBusy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) {
        Text(stringResource(R.string.restore_try_other_account))
    }
    OutlinedButton(onClick = vm::usePhraseInstead, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.restore_use_words)) }
    TextButton(onClick = vm::confirmStartFresh) { Text(stringResource(R.string.restore_start_fresh)) }
}

@Composable
fun RestorePasswordScreen(s: OnboardingState, vm: OnboardingViewModel, io: CoroutineScope) {
    val locked = s.restoreLockMinutes > 0
    LaunchedEffect(locked) { while (locked) { delay(5_000); vm.refreshLock() } }
    PlainPage(stringResource(R.string.restore_pw_title)) {
        AuralisSection(s.serviceLabel) {
            BodyText(stringResource(R.string.restore_pw_body))
            OutlinedTextField(
                value = s.restorePassword, onValueChange = vm::setRestorePassword,
                label = { Text(stringResource(R.string.restore_pw_label)) },
                enabled = !locked && !s.restoreBusy, isError = s.restoreWrong, singleLine = true,
                visualTransformation = PasswordVisualTransformation(),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrect = false),
                modifier = Modifier.fillMaxWidth(),
            )
            if (locked) {
                Column(Modifier.semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite }) {
                    Callout(
                        stringResource(R.string.restore_lockout_title, s.restoreLockMinutes) + "\n" + stringResource(R.string.restore_lockout_body),
                        CalloutKind.Warn,
                    )
                }
            } else if (s.restoreWrong) {
                Column(Modifier.semantics { liveRegion = LiveRegionMode.Assertive }) {
                    Callout(pluralStringResource(R.plurals.restore_pw_wrong, s.restoreTriesLeft, s.restoreTriesLeft, s.restoreNextLockMinutes), CalloutKind.Danger)
                }
            }
        }
        val checking = stringResource(R.string.restore_pw_checking)
        Button(
            onClick = { io.bg { vm.restoreFromCloud() } },
            enabled = !locked && !s.restoreBusy && s.restorePassword.isNotEmpty(),
            modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp),
        ) {
            if (s.restoreBusy) {
                CircularProgressIndicator(Modifier.size(18.dp).semantics { contentDescription = checking }, color = MaterialTheme.colorScheme.onPrimary, strokeWidth = 2.dp)
            } else Text(stringResource(R.string.restore_cta))
        }
        // Recovery words always remain available, including while password attempts are locked.
        OutlinedButton(onClick = vm::usePhraseInstead, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.restore_use_words)) }
    }
}

@Composable
fun RestoreProgressScreen(s: OnboardingState) {
    val context = LocalContext.current
    val stages = listOf(
        RestoreStage.Download to (stringResource(R.string.restore_stage_download) to stringResource(R.string.restore_stage_download_sub, Formatter.formatShortFileSize(context, s.foundBackupSize.toLong()), s.serviceLabel)),
        RestoreStage.Decrypt to (stringResource(R.string.restore_stage_decrypt) to stringResource(R.string.restore_stage_decrypt_sub)),
        RestoreStage.Contacts to (stringResource(R.string.restore_stage_contacts) to stringResource(R.string.restore_stage_contacts_sub)),
    )
    PlainPage(stringResource(R.string.restore_progress_title)) {
        BodyText(stringResource(R.string.restore_progress_body))
        AuralisSection {
            Text(
                stringResource(R.string.auralis_restore_step, (s.restoreStage.ordinal + 1).coerceAtMost(stages.size), stages.size),
                color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.titleLarge,
            )
            LinearProgressIndicator(progress = { s.restoreFraction.coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
        }
        stages.forEachIndexed { index, (stage, labels) ->
            val done = stage.ordinal < s.restoreStage.ordinal
            val current = stage == s.restoreStage
            AuralisSection {
                Row(
                    if (current) Modifier.semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite } else Modifier,
                    horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.Top,
                ) {
                    Surface(shape = CircleShape, color = if (done || current) LocalOmcpColors.current.brandSoft else LocalOmcpColors.current.surface2) {
                        Text(if (done) "✓" else "${index + 1}", color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(horizontal = 13.dp, vertical = 8.dp))
                    }
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(labels.first, style = MaterialTheme.typography.titleSmall)
                        BodyText(labels.second)
                    }
                }
            }
        }
    }
}

@Composable
fun RestoreSuccessScreen(s: OnboardingState, vm: OnboardingViewModel) {
    val title = s.restoredName?.let { stringResource(R.string.restore_success_title_named, it) } ?: stringResource(R.string.restore_success_title)
    PlainPage(title) {
        Callout(stringResource(R.string.restore_success_body), CalloutKind.Ok)
        if (s.quotaWarning) Callout(stringResource(R.string.restore_quota_warning, s.serviceLabel), CalloutKind.Warn)
        AuralisSection(stringResource(R.string.restore_restored)) {
            BodyText("✓ " + stringResource(R.string.restore_restored_identity))
            BodyText("✓ " + stringResource(R.string.restore_stage_contacts) + " (${s.restoredContacts})")
            BodyText("✓ " + stringResource(R.string.restore_restored_verified) + " (${s.restoredVerified})")
            BodyText("✓ " + stringResource(R.string.restore_restored_profile))
        }
        AuralisSection(stringResource(R.string.restore_not_restored)) {
            BodyText(stringResource(R.string.restore_not_history))
            BodyText(stringResource(R.string.restore_not_burn))
            BodyText(stringResource(R.string.restore_not_nearby))
        }
        Button(onClick = vm::openChats, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) { Text(stringResource(R.string.restore_open_chats)) }
    }
}

@Composable
fun RestoreErrorScreen(s: OnboardingState, vm: OnboardingViewModel, io: CoroutineScope) {
    val context = LocalContext.current
    val service = s.serviceLabel
    val (title, body) = when (s.restoreError) {
        RestoreErrorKind.Offline -> stringResource(R.string.restore_err_offline_title) to stringResource(R.string.restore_err_offline_body, service)
        RestoreErrorKind.PermissionRevoked -> stringResource(R.string.restore_err_permission_title, service) to stringResource(R.string.restore_err_permission_body, service)
        RestoreErrorKind.QuotaFull -> stringResource(R.string.restore_err_quota_title, service) to stringResource(R.string.restore_err_quota_body)
        RestoreErrorKind.TooNew -> stringResource(R.string.restore_err_toonew_title) to stringResource(R.string.restore_err_toonew_body)
        RestoreErrorKind.Corrupted, null -> stringResource(R.string.restore_err_corrupted_title) to stringResource(R.string.restore_err_corrupted_body)
    }
    PlainPage(title) {
        AuralisSection {
            Column(Modifier.semantics { liveRegion = LiveRegionMode.Assertive }) { Callout(body, CalloutKind.Danger) }
        }
        val primary = when (s.restoreError) {
            RestoreErrorKind.Offline -> R.string.restore_try_again
            RestoreErrorKind.PermissionRevoked -> R.string.restore_allow_access
            RestoreErrorKind.QuotaFull -> R.string.restore_anyway
            RestoreErrorKind.TooNew -> R.string.restore_update
            RestoreErrorKind.Corrupted, null -> R.string.restore_use_words
        }
        Button(
            onClick = {
                if (s.restoreError == RestoreErrorKind.TooNew) {
                    val packageName = context.packageName
                    runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("market://details?id=$packageName")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
                        .onFailure { runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://play.google.com/store/apps/details?id=$packageName")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) } }
                } else io.bg(vm::errorPrimary)
            },
            enabled = !s.restoreBusy, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp),
        ) { Text(stringResource(primary)) }
        if (s.restoreError == RestoreErrorKind.Corrupted) {
            OutlinedButton(onClick = { io.bg(vm::chooseDrive) }, enabled = !s.restoreBusy, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.restore_try_other_account)) }
        } else {
            OutlinedButton(onClick = vm::usePhraseInstead, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.restore_use_words)) }
        }
    }
}
