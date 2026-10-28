package chat.omcp.app.ui.screens

import kotlinx.coroutines.launch
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import chat.omcp.app.ui.components.AuralisPanel
import chat.omcp.app.ui.components.AuralisSection
import chat.omcp.app.ui.theme.LocalOmcpColors
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import chat.omcp.app.R
import chat.omcp.app.ui.components.BodyText
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.components.PlainPage
import chat.omcp.app.ui.vm.OnboardingStep
import chat.omcp.app.ui.vm.OnboardingViewModel

/**
 * Onboarding per apps/REQUIREMENTS.md "Onboarding & recovery backup (v0.1.1)".
 * Auralis presentation; every recovery and backup action retains its original view-model path.
 */
@Composable
fun OnboardingFlow(
    vm: OnboardingViewModel,
    myQr: @Composable () -> Unit,
    onFindNearby: () -> Unit,
    secureWindow: (Boolean) -> Unit,
) {
    val s by vm.state.collectAsState()
    // Cloud I/O (Drive REST, consent) never runs on the main thread.
    val io = androidx.compose.runtime.rememberCoroutineScope()
    var backingUp by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { vm.detectBackup() } }
    // FLAG_SECURE while the phrase is on screen.
    LaunchedEffect(s.step) { secureWindow(s.step == OnboardingStep.WriteDown || s.step == OnboardingStep.Quiz) }

    BackHandler(enabled = s.step in setOf(OnboardingStep.Profile, OnboardingStep.RestorePhrase, OnboardingStep.RestoreSource, OnboardingStep.WriteDown, OnboardingStep.Quiz)) {
        when (s.step) {
            OnboardingStep.WriteDown -> vm.backToSecure()
            OnboardingStep.Quiz -> vm.writeItDown()
            else -> vm.backToWelcome()
        }
    }

    when (s.step) {
        OnboardingStep.Welcome -> AuralisWelcome(
            onGetStarted = vm::getStarted,
            onRestore = vm::openRestoreSource,
            onUsePhrase = vm::usePhraseInstead,
        )

        OnboardingStep.RestoreChecking -> RestoreCheckingScreen()
        OnboardingStep.RestoreDetect -> RestoreDetectScreen(s, vm, io)
        OnboardingStep.RestoreSource -> RestoreSourceScreen(s, vm, io)
        OnboardingStep.RestoreNoBackup -> RestoreNoBackupScreen(s, vm, io)
        OnboardingStep.RestorePassword -> RestorePasswordScreen(s, vm, io)
        OnboardingStep.RestoreProgress -> RestoreProgressScreen(s)
        OnboardingStep.RestoreSuccess -> RestoreSuccessScreen(s, vm)
        OnboardingStep.RestoreError -> RestoreErrorScreen(s, vm, io)

        OnboardingStep.RestorePhrase -> PlainPage(stringResource(R.string.onb_restore_title)) {
            BodyText(stringResource(R.string.onb_restore_body))
            OutlinedTextField(
                s.restorePhrase, vm::setRestorePhrase,
                label = { Text(stringResource(R.string.onb_24_words)) },
                keyboardOptions = KeyboardOptions(autoCorrect = false, capitalization = KeyboardCapitalization.None),
                minLines = 4,
                modifier = Modifier.fillMaxWidth(),
            )
            s.error?.let { Callout(it, CalloutKind.Danger) }
            Button(onClick = { vm.restoreFromPhrase() }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.onb_restore)) }
            TextButton(onClick = vm::backToWelcome) { Text(stringResource(R.string.back)) }
        }

        OnboardingStep.Profile -> PlainPage(stringResource(R.string.onb_profile_title)) {
            SetupProgress(1)
            AuralisSection(stringResource(R.string.auralis_identity_panel)) {
                BodyText(stringResource(R.string.auralis_identity_panel_body))
                OutlinedTextField(
                    s.displayName, vm::setName,
                    label = { Text(stringResource(R.string.onb_display_name)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            BodyText(stringResource(R.string.onb_profile_body))
            s.error?.let { Callout(it, CalloutKind.Danger) }
            Button(onClick = vm::profileNext, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) { Text(stringResource(R.string.cta_continue)) }
        }

        OnboardingStep.Secure -> PlainPage(stringResource(R.string.onb_secure_title)) {
            SetupProgress(2)
            BodyText(stringResource(R.string.onb_secure_body))
            AuralisSection(stringResource(R.string.backup_drive)) {
                if (s.driveDone) {
                    Callout(stringResource(R.string.backup_drive_done), CalloutKind.Ok)
                } else if (!s.driveAvailable) {
                    Callout(stringResource(R.string.backup_drive_unavailable), CalloutKind.Warn)
                } else {
                    BodyText(stringResource(R.string.backup_honest_copy))
                    OutlinedTextField(
                        s.drivePassword, vm::setDrivePassword,
                        enabled = !backingUp,
                        label = { Text(stringResource(R.string.onb_backup_password)) },
                        supportingText = {
                            Text(
                                when {
                                    s.drivePassword.isEmpty() -> stringResource(R.string.backup_pw_rule)
                                    s.drivePasswordTooShort -> stringResource(R.string.backup_pw_short)
                                    !s.drivePasswordOk -> stringResource(R.string.backup_pw_weak)
                                    else -> stringResource(R.string.backup_pw_ok)
                                },
                            )
                        },
                        visualTransformation = PasswordVisualTransformation(),
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrect = false),
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Button(
                        onClick = {
                            if (!backingUp) {
                                backingUp = true
                                io.launch {
                                    try { kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { vm.backupToDrive() } }
                                    finally { backingUp = false }
                                }
                            }
                        },
                        enabled = s.drivePasswordOk && !backingUp,
                        modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp),
                    ) {
                        Text(stringResource(if (backingUp) R.string.auralis_backup_working else R.string.backup_drive_cta))
                    }
                }
            }
            AuralisSection(stringResource(R.string.backup_written)) {
                if (s.writtenDone) {
                    Callout(stringResource(R.string.backup_written_done), CalloutKind.Ok)
                } else {
                    OutlinedButton(onClick = vm::writeItDown, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.backup_written_cta)) }
                }
            }
            s.error?.let { Callout(it, CalloutKind.Danger) }
            Button(onClick = { vm.secureNext() }, enabled = s.anyBackupDone, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.cta_continue))
            }
            if (!s.anyBackupDone && vm.canRemindLater) TextButton(onClick = vm::remindLater) { Text(stringResource(R.string.backup_remind_later)) }
        }

        OnboardingStep.WriteDown -> PlainPage(stringResource(R.string.backup_written)) {
            BodyText(stringResource(R.string.onb_words_body))
            Callout(stringResource(R.string.onb_no_screenshot), CalloutKind.Warn)
            AuralisSection(stringResource(R.string.auralis_recovery_words)) {
                s.words.chunked(2).forEachIndexed { row, words ->
                    Row(horizontalArrangement = Arrangement.spacedBy(16.dp), modifier = Modifier.fillMaxWidth()) {
                        words.forEachIndexed { i, word ->
                            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                Text("${row * 2 + i + 1}", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
                                Text(word, style = MaterialTheme.typography.bodyLarge, fontFamily = FontFamily.Monospace)
                            }
                        }
                    }
                }
            }
            Button(onClick = vm::startQuiz, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.onb_wrote_it)) }
            TextButton(onClick = vm::backToSecure) { Text(stringResource(R.string.back)) }
        }

        OnboardingStep.Quiz -> PlainPage(stringResource(R.string.onb_quiz_title)) {
            BodyText(stringResource(R.string.onb_quiz_body))
            AuralisSection {
                s.quiz.forEachIndexed { i, f ->
                    OutlinedTextField(
                        f.input, { vm.setQuizInput(i, it) },
                        label = { Text(stringResource(R.string.onb_word_n, f.position + 1)) },
                        isError = f.error,
                        supportingText = if (f.error) ({ Text(stringResource(R.string.onb_word_wrong, f.position + 1)) }) else null,
                        trailingIcon = if (f.ok) ({ Text("✓") }) else null,
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(autoCorrect = false, capitalization = KeyboardCapitalization.None),
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
            Button(onClick = { vm.submitQuiz() }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.onb_confirm)) }
            TextButton(onClick = vm::writeItDown) { Text(stringResource(R.string.onb_show_again)) }
        }

        OnboardingStep.Done -> PlainPage(stringResource(R.string.onb_done_title)) {
            SetupProgress(3)
            BodyText(stringResource(R.string.onb_done_body))
            myQr()
            OutlinedButton(onClick = onFindNearby, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.onb_find_nearby)) }
            Button(onClick = vm::finish, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.onb_start_chatting)) }
        }
    }
}

/** Small progress rail with explicit text, so progress never depends on colour alone. */
@Composable
private fun SetupProgress(step: Int) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(stringResource(R.string.auralis_setup_progress, step), style = MaterialTheme.typography.labelMedium, color = LocalOmcpColors.current.ink2)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            repeat(3) { index ->
                Box(Modifier.weight(1f).height(4.dp).clip(RoundedCornerShape(2.dp))
                    .background(if (index < step) MaterialTheme.colorScheme.primary else LocalOmcpColors.current.surface2))
            }
        }
    }
}
