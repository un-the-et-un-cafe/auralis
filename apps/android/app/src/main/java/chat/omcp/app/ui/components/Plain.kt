package chat.omcp.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
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
 * Plain building blocks for flows whose visual design is pending (onboarding v0.1.2).
 * Keep screens built from these so a restyle touches one file.
 */
@Composable
fun PlainPage(title: String, modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 24.dp, vertical = 32.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Text(title, style = MaterialTheme.typography.headlineSmall, modifier = Modifier.semantics { heading() })
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
            .clip(RoundedCornerShape(12.dp))
            .background(bg)
            .padding(12.dp),
    )
}

/** 12 groups × 5 digits, 4 per row (DESIGN_SPEC SafetyNumberBlock). */
@Composable
fun SafetyNumberBlock(groups: List<String>, differs: Set<Int> = emptySet()) {
    val c = LocalOmcpColors.current
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(c.surface2)
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        groups.chunked(4).forEachIndexed { row, g ->
            androidx.compose.foundation.layout.Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                g.forEachIndexed { col, digits ->
                    val idx = row * 4 + col
                    Text(
                        digits,
                        fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                        color = if (idx in differs) c.danger else MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.semantics {
                            contentDescription = "Group ${idx + 1}: ${digits.toList().joinToString(" ")}" +
                                if (idx in differs) ", differs" else ""
                        },
                    )
                }
            }
        }
    }
}
