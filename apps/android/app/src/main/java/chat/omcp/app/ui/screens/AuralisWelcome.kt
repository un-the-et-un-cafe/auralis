package chat.omcp.app.ui.screens

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.ui.components.AuralisMark
import chat.omcp.app.ui.components.AuralisWordmark
import chat.omcp.app.ui.theme.LocalOmcpColors
import chat.omcp.app.ui.theme.OmcpDark

/** A branded entry point over the existing onboarding actions, not a demo flow. */
@Composable
fun AuralisWelcome(onGetStarted: () -> Unit, onRestore: () -> Unit, onUsePhrase: () -> Unit) {
    val colors = LocalOmcpColors.current
    Column(
        Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)
            .safeDrawingPadding().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        AuralisWordmark(Modifier.padding(bottom = 8.dp))
        Column(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(32.dp)).background(OmcpDark.Bg).padding(28.dp),
            verticalArrangement = Arrangement.spacedBy(18.dp),
        ) {
            Box(Modifier.fillMaxWidth().heightIn(min = 132.dp), contentAlignment = Alignment.Center) {
                Canvas(Modifier.matchParentSize().clipToBounds()) {
                    val center = Offset(size.width / 2, size.height / 2)
                    listOf(0.3f, 0.48f, 0.68f).forEach { radius ->
                        drawCircle(OmcpDark.Brand.copy(alpha = 0.13f), radius = size.width * radius, center = center, style = Stroke(1.dp.toPx()))
                    }
                }
                AuralisMark(Modifier.size(104.dp))
            }
            Text(stringResource(R.string.auralis_welcome_eyebrow), style = MaterialTheme.typography.labelMedium, color = OmcpDark.Brand)
            Text(
                stringResource(R.string.auralis_welcome_headline),
                style = MaterialTheme.typography.displaySmall,
                color = OmcpDark.Ink,
                modifier = Modifier.semantics { heading() },
            )
            Text(stringResource(R.string.onb_tagline), style = MaterialTheme.typography.bodyLarge, color = OmcpDark.Ink2)
        }
        Text(stringResource(R.string.onb_welcome_body), style = MaterialTheme.typography.bodyLarge, color = colors.ink2)
        Button(onClick = onGetStarted, modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp)) {
            Text(stringResource(R.string.onb_get_started))
        }
        OutlinedButton(onClick = onRestore, modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp)) {
            Text(stringResource(R.string.restore_welcome_cta))
        }
        TextButton(onClick = onUsePhrase, modifier = Modifier.align(Alignment.CenterHorizontally)) {
            Text(stringResource(R.string.onb_restore_phrase))
        }
        Text(stringResource(R.string.auralis_beta_notice), style = MaterialTheme.typography.bodySmall, color = colors.ink3)
    }
}
