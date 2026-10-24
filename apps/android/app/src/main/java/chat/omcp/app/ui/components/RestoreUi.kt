package chat.omcp.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Sync
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import chat.omcp.app.R
import chat.omcp.app.data.FileAttachment
import chat.omcp.app.ui.theme.LocalOmcpColors
import java.text.DateFormat
import java.util.Date

fun restoreDate(ms: Long): String = DateFormat.getDateInstance(DateFormat.MEDIUM).format(Date(ms))

/** Neutral grey "Restored, not verified" pill with a sync glyph (§5.98). Replaces the verified badge. */
@Composable
fun RestoredPill(compact: Boolean = false, onClick: (() -> Unit)? = null) {
    val c = LocalOmcpColors.current
    val label = stringResource(R.string.restored_pill)
    Row(
        Modifier
            .padding(start = 6.dp)
            .clip(RoundedCornerShape(50))
            .border(1.dp, c.line, RoundedCornerShape(50))
            .background(c.surface2)
            .then(if (onClick != null) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier)
            .padding(horizontal = if (compact) 5.dp else 8.dp, vertical = 2.dp)
            .semantics { contentDescription = label },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Default.Sync, contentDescription = null, modifier = Modifier.size(if (compact) 9.dp else 12.dp), tint = c.ink2)
        Text(label, fontSize = if (compact) 10.sp else 12.sp, color = c.ink2, modifier = Modifier.padding(start = 3.dp))
    }
}

/** Centred neutral in-chat notice: "{name} restored Hopla on a new device · {date}" (RS1, §5.95/§5.98). */
@Composable
fun RestoreNotice(name: String, atMs: Long, onClick: () -> Unit) {
    val c = LocalOmcpColors.current
    val text = stringResource(R.string.restored_notice, name, restoreDate(atMs))
    val link = stringResource(R.string.restored_notice_link)
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
        Column(
            Modifier
                .clip(RoundedCornerShape(12.dp))
                .background(c.surface2)
                .clickable(role = Role.Button, onClick = onClick)
                .padding(horizontal = 14.dp, vertical = 8.dp)
                .semantics { contentDescription = "$text. $link" },
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(text, style = MaterialTheme.typography.bodySmall, color = c.ink2)
            Text(link, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
        }
    }
}

/** The verify sheet behind the notice / pill (§5.98). */
@Composable
fun RestoreSheet(name: String, atMs: Long, onVerifyQr: () -> Unit, onBlock: (() -> Unit)?, onDismiss: () -> Unit) {
    val c = LocalOmcpColors.current
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.restored_sheet_title, name)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.restored_sheet_body, name, restoreDate(atMs)))
                Text(stringResource(R.string.restored_sheet_safety, restoreDate(atMs)), color = c.ink2)
                Text(stringResource(R.string.restored_sheet_old), color = c.ink2)
                if (onBlock != null) {
                    TextButton(onClick = onBlock) { Text(stringResource(R.string.restored_block, name), color = c.danger) }
                }
            }
        },
        confirmButton = { TextButton(onClick = onVerifyQr) { Text(stringResource(R.string.restored_verify_qr)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.restored_it_was, name)) } },
    )
}

/** Missing-media placeholder (§5.92): hatched-feel tile, "NOT RESTORED YET", no red. */
@Composable
fun MissingMediaBubble(mine: Boolean, f: FileAttachment, peerName: String, onRequest: () -> Unit, onAskResend: () -> Unit, resendSent: Boolean) {
    val c = LocalOmcpColors.current
    val status = when {
        resendSent -> stringResource(R.string.media_request_sent, peerName)
        f.state == FileAttachment.State.Requested -> stringResource(R.string.media_requested)
        f.state == FileAttachment.State.Unavailable -> stringResource(R.string.media_gone, peerName)
        else -> stringResource(R.string.media_waiting, peerName)
    }
    val tag = stringResource(R.string.media_not_restored)
    val size = "%.1f MB".format(f.sizeBytes / 1_000_000.0)
    Column(Modifier.fillMaxWidth(), horizontalAlignment = if (mine) Alignment.End else Alignment.Start) {
        Column(
            Modifier
                .fillMaxWidth(0.8f)
                .clip(RoundedCornerShape(14.dp))
                .border(1.dp, c.line, RoundedCornerShape(14.dp))
                .background(c.surface2)
                .then(if (f.state == FileAttachment.State.Missing) Modifier.clickable(role = Role.Button, onClick = onRequest) else Modifier)
                .padding(12.dp)
                .semantics { contentDescription = "${f.name}, $size, $tag. $status" },
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(tag, fontSize = 10.sp, fontWeight = FontWeight.SemiBold, color = c.ink2)
            Text(f.name, style = MaterialTheme.typography.titleSmall)
            Text("${f.mime.substringBefore('/')} · $size", style = MaterialTheme.typography.labelSmall, color = c.ink3)
            Text(status, style = MaterialTheme.typography.bodySmall, color = c.ink2)
            when {
                resendSent -> Unit
                f.state == FileAttachment.State.Missing ->
                    TextButton(onClick = onRequest) { Text(stringResource(R.string.media_request_again)) }
                f.state == FileAttachment.State.Unavailable ->
                    TextButton(onClick = onAskResend) { Text(stringResource(R.string.media_ask_resend)) }
            }
        }
    }
}

/** "Ask {name} to resend?" confirm sheet with an editable message (§5.94). Nothing is sent before Send. */
@Composable
fun ResendDialog(peerName: String, fileName: String, onSend: (String?) -> Unit, onDismiss: () -> Unit) {
    val c = LocalOmcpColors.current
    var text by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.media_resend_title, peerName)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.media_resend_body, peerName))
                Text("“$fileName”", color = c.ink2)
                OutlinedTextField(value = text, onValueChange = { if (it.toByteArray().size <= 280) text = it }, label = { Text(stringResource(R.string.media_resend_label)) })
                Text(stringResource(R.string.media_resend_note), style = MaterialTheme.typography.labelSmall, color = c.ink2)
            }
        },
        confirmButton = { TextButton(onClick = { onSend(text.trim().ifEmpty { null }) }) { Text(stringResource(R.string.media_resend_send)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(android.R.string.cancel)) } },
    )
}
