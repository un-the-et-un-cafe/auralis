package chat.omcp.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import chat.omcp.app.core.OmcpCoreStub
import chat.omcp.app.ui.theme.LocalOmcpColors

@Composable
fun PathChip(
    kind: OmcpCoreStub.PathKind,
    label: String,
    modifier: Modifier = Modifier,
    micro: Boolean = false,
) {
    val c = LocalOmcpColors.current
    val (fg, bg) = when (kind) {
        OmcpCoreStub.PathKind.Mesh -> c.pathMesh to c.pathMeshSoft
        OmcpCoreStub.PathKind.Gateway -> c.pathGateway to c.pathGatewaySoft
        OmcpCoreStub.PathKind.Mixed -> c.pathMixed to c.pathMixedSoft
        OmcpCoreStub.PathKind.Queued -> c.pathQueue to c.pathQueueSoft
    }
    val padH = if (micro) 7.dp else 10.dp
    val padV = if (micro) 2.dp else 5.dp
    val textSize = if (micro) 10.sp else 12.sp
    Row(
        modifier = modifier
            .clip(RoundedCornerShape(999.dp))
            .background(bg)
            .padding(horizontal = padH, vertical = padV)
            .semantics { contentDescription = "Delivery path: $label" },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(
            Modifier
                .size(if (micro) 5.dp else 7.dp)
                .clip(CircleShape)
                .background(fg),
        )
        Text(
            text = label,
            color = fg,
            fontSize = textSize,
            style = MaterialTheme.typography.labelMedium,
        )
    }
}

@Composable
fun PathBanner(
    kind: OmcpCoreStub.PathKind,
    text: String,
    modifier: Modifier = Modifier,
) {
    val c = LocalOmcpColors.current
    val (fg, bg) = when (kind) {
        OmcpCoreStub.PathKind.Mesh -> c.pathMesh to c.pathMeshSoft
        OmcpCoreStub.PathKind.Gateway -> c.pathGateway to c.pathGatewaySoft
        OmcpCoreStub.PathKind.Mixed -> c.pathMixed to c.pathMixedSoft
        OmcpCoreStub.PathKind.Queued -> c.pathQueue to c.pathQueueSoft
    }
    Row(
        modifier = modifier
            .background(bg)
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .semantics { contentDescription = "Delivery path: $text" },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(Modifier.size(8.dp).clip(CircleShape).background(fg))
        Text(text = text, color = fg, style = MaterialTheme.typography.bodyMedium)
    }
}
