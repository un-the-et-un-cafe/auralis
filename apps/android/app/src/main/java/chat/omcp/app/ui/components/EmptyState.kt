package chat.omcp.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import chat.omcp.app.ui.theme.LocalOmcpColors

@Composable
fun EmptyState(title: String, body: String, modifier: Modifier = Modifier, actionLabel: String? = null, onAction: (() -> Unit)? = null, fillAvailableSpace: Boolean = true) {
    val c = LocalOmcpColors.current
    val scroll = rememberScrollState()
    Column(
        modifier.then(if (fillAvailableSpace) Modifier.fillMaxSize().verticalScroll(scroll) else Modifier.fillMaxWidth()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(Modifier.size(88.dp).clip(RoundedCornerShape(30.dp)).background(c.brandSoft), contentAlignment = Alignment.Center) {
            AuralisMark(Modifier.size(64.dp), tint = MaterialTheme.colorScheme.primary)
        }
        Text(title, style = MaterialTheme.typography.headlineSmall, color = MaterialTheme.colorScheme.onBackground, textAlign = TextAlign.Center)
        Text(body, style = MaterialTheme.typography.bodyLarge, color = c.ink2, textAlign = TextAlign.Center)
        if (actionLabel != null && onAction != null) Button(onClick = onAction) { Text(actionLabel) }
    }
}
