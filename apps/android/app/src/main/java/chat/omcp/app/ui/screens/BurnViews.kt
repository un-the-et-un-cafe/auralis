package chat.omcp.app.ui.screens

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.SecureFlagPolicy
import chat.omcp.app.R
import chat.omcp.app.data.ChatMessage
import chat.omcp.app.ui.theme.LocalOmcpColors
import kotlinx.coroutines.delay

/** Content handed to the secure viewer after [ChatMessage] was opened (once). */
class OpenedBurn(val msg: ChatMessage, val body: ByteArray)

/** Burn bubble: blurred "Tap to read" / view-once tile / "🔥 Photo opened" placeholder. No save/forward/quote. */
@Composable
fun BurnBubble(m: ChatMessage, onOpen: () -> Unit) {
    val c = LocalOmcpColors.current
    val label = when {
        m.burnState == ChatMessage.BurnState.Opened && m.file?.mime?.startsWith("video/") == true -> stringResource(R.string.burn_video_opened)
        m.burnState == ChatMessage.BurnState.Opened -> stringResource(R.string.burn_photo_opened)
        m.mine && m.isViewOnceMedia -> stringResource(R.string.burn_view_once_sent)
        m.mine -> stringResource(R.string.burn_sent)
        m.isViewOnceMedia -> stringResource(R.string.burn_view_once_tap)
        m.burnState == ChatMessage.BurnState.Open -> stringResource(R.string.burn_reading)
        else -> stringResource(R.string.burn_tap_to_read)
    }
    val tappable = !m.mine && m.burnState == ChatMessage.BurnState.Unopened
    Column(Modifier.fillMaxWidth(), horizontalAlignment = if (m.mine) Alignment.End else Alignment.Start) {
        Box(
            Modifier
                .clip(RoundedCornerShape(14.dp))
                .border(1.dp, c.line, RoundedCornerShape(14.dp))
                .background(if (m.mine) c.brandSoft else MaterialTheme.colorScheme.surface)
                .then(if (tappable) Modifier.clickable(onClick = onOpen) else Modifier)
                .padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (tappable && !m.isViewOnceMedia) {
                    Text("••••••••••", modifier = Modifier.blur(6.dp), color = c.ink2)
                }
                Text("🔥 $label", style = MaterialTheme.typography.bodyLarge)
            }
        }
    }
}

/**
 * Full-screen viewer. FLAG_SECURE via [SecureFlagPolicy.SecureOn]: screenshots and
 * screen recording show black (B3). Countdown ring for timer burns; closing burns.
 */
@Composable
fun BurnViewer(open: OpenedBurn, now: () -> Long, onClose: () -> Unit) {
    val m = open.msg
    Dialog(
        onDismissRequest = onClose,
        properties = DialogProperties(
            securePolicy = SecureFlagPolicy.SecureOn,
            usePlatformDefaultWidth = false,
        ),
    ) {
        var t by remember { mutableLongStateOf(now()) }
        val start = remember { now() }
        val end = m.burnAtMs
        LaunchedEffect(m.id) {
            while (true) {
                delay(100)
                t = now()
                if (end != null && t >= end) { onClose(); break }
            }
        }
        Column(
            Modifier.fillMaxSize().background(Color.Black).padding(16.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                if (end != null) {
                    val total = (end - start).coerceAtLeast(1)
                    CircularProgressIndicator(
                        progress = { ((end - t).coerceAtLeast(0).toFloat() / total) },
                        modifier = Modifier.size(28.dp),
                        color = Color(0xFFFF7A3D),
                    )
                    Text(" ${((end - t).coerceAtLeast(0) / 1000) + 1}s", color = Color.White)
                }
                Box(Modifier.weight(1f))
                TextButton(onClick = onClose) { Text(stringResource(R.string.burn_close), color = Color.White) }
            }
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                val mime = m.file?.mime.orEmpty()
                when {
                    mime.startsWith("image/") -> {
                        val bmp = remember(open) { BitmapFactory.decodeByteArray(open.body, 0, open.body.size) }
                        if (bmp != null) Image(bmp.asImageBitmap(), contentDescription = null, modifier = Modifier.fillMaxSize())
                        else Text(stringResource(R.string.burn_media_unavailable), color = Color.White)
                    }
                    // TODO(M5.x): view-once video playback (Media3) — not bundled in the beta.
                    mime.startsWith("video/") -> Text(stringResource(R.string.burn_media_unavailable), color = Color.White)
                    else -> Text(String(open.body), color = Color.White, style = MaterialTheme.typography.headlineSmall)
                }
            }
            Text(stringResource(R.string.burn_honest_copy), color = Color(0xFFBBBBBB), style = MaterialTheme.typography.bodySmall)
        }
    }
}
