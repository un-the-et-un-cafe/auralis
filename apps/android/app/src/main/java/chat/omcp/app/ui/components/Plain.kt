package chat.omcp.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import chat.omcp.app.ui.theme.LocalOmcpColors

/**
 * Shared Auralis page shell for onboarding, backup and restore.
 * Scrollable at large font scales; existing flow controls remain untouched.
 */
@Composable
fun PlainPage(title: String, modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .safeDrawingPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 24.dp, vertical = 24.dp),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        AuralisWordmark(Modifier.padding(bottom = 12.dp))
        Text(title, style = MaterialTheme.typography.headlineMedium, modifier = Modifier.semantics { heading() })
        content()
    }
}

@Composable
fun BodyText(text: String, color: Color? = null) {
    Text(text, style = MaterialTheme.typography.bodyMedium, color = color ?: LocalOmcpColors.current.ink2)
}

enum class CalloutKind { Info, Warn, Danger, Ok }

@Composable
fun Callout(text: String, kind: CalloutKind = CalloutKind.Info, modifier: Modifier = Modifier) {
    val c = LocalOmcpColors.current
    val bg = when (kind) {
        CalloutKind.Info -> c.surface2
        CalloutKind.Warn -> c.warnSoft
        CalloutKind.Danger -> c.dangerSoft
        CalloutKind.Ok -> c.okSoft
    }
    Text(
        text,
        style = MaterialTheme.typography.bodyMedium,
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(20.dp))
            .background(bg)
            .padding(16.dp),
    )
}

/** 12 groups × 5 digits, 4 per row (DESIGN_SPEC SafetyNumberBlock). */
@Composable
fun SafetyNumberBlock(groups: List<String>, differs: Set<Int> = emptySet()) {
    val c = LocalOmcpColors.current
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val columns = (maxWidth.value / (72f * LocalDensity.current.fontScale)).toInt().coerceIn(1, 4)
        Column(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(20.dp)).background(c.surface2).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            groups.chunked(columns).forEachIndexed { row, digitsInRow ->
                androidx.compose.foundation.layout.Row(horizontalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxWidth()) {
                    digitsInRow.forEachIndexed { col, digits ->
                        val idx = row * columns + col
                        Text(
                            digits,
                            fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                            style = MaterialTheme.typography.bodyLarge,
                            color = if (idx in differs) c.danger else MaterialTheme.colorScheme.onSurface,
                            modifier = Modifier.weight(1f).semantics {
                                contentDescription = "Group ${idx + 1}: ${digits.toList().joinToString(" ")}" +
                                    if (idx in differs) ", differs" else ""
                            },
                        )
                    }
                }
            }
        }
    }
}
