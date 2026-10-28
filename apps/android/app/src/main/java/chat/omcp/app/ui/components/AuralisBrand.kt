package chat.omcp.app.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.BuildConfig
import chat.omcp.app.ui.theme.OmcpDark

/** Three open signal strokes: the same geometry as the launcher, with no text baked in. */
@Composable
fun AuralisMark(modifier: Modifier = Modifier, tint: Color = OmcpDark.Brand) {
    Canvas(modifier) {
        fun x(value: Float) = value / 64f * size.width
        fun y(value: Float) = value / 64f * size.height
        val paths = listOf(
            Path().apply { moveTo(x(8f), y(50f)); quadraticBezierTo(x(18f), y(27f), x(32f), y(9f)) },
            Path().apply { moveTo(x(25f), y(50f)); quadraticBezierTo(x(33f), y(33f), x(42f), y(22f)) },
            Path().apply { moveTo(x(42f), y(50f)); quadraticBezierTo(x(47f), y(41f), x(54f), y(35f)) },
        )
        paths.forEach { drawPath(it, tint, style = Stroke(width = size.width * 7f / 64f, cap = StrokeCap.Round)) }
    }
}

@Composable
fun AuralisWordmark(modifier: Modifier = Modifier) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(38.dp).clip(RoundedCornerShape(13.dp)).background(OmcpDark.Bg), contentAlignment = Alignment.Center) {
            AuralisMark(Modifier.size(30.dp))
        }
        Text(stringResource(R.string.app_name), style = MaterialTheme.typography.titleLarge)
    }
}

/** Never let an opted-in test build present volatile in-memory storage as a durable backup. */
@Composable
fun AuralisSimulationNotice() {
    if (BuildConfig.SIMULATED_DRIVE_BACKUP) {
        Callout(stringResource(R.string.auralis_simulated_backup_warning), CalloutKind.Warn)
    }
}
