package chat.omcp.app.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.AttachFile
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Security
import androidx.compose.material.icons.filled.Send
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.core.OmcpCoreStub
import chat.omcp.app.ui.components.ChatRow
import chat.omcp.app.ui.components.ChatRowModel
import chat.omcp.app.ui.components.EmptyState
import chat.omcp.app.ui.components.PathBanner
import chat.omcp.app.ui.components.PathChip
import chat.omcp.app.ui.theme.LocalOmcpColors

/** Placeholder rows matching Designy chats-list mockup (no live mesh yet). */
private val SampleChats = listOf(
    ChatRowModel("1", "Alex Rivera", "See you at the park entrance", "9:38", "AL", true, path = OmcpCoreStub.PathKind.Mesh),
    ChatRowModel("2", "Neighbourhood watch", "Maya: Power’s back on our street", "9:12", "N", false, unread = 3, isGroup = true),
    ChatRowModel("3", "Jules Bernard", "Waiting for mesh… photo.pdf", "8:55", "JB", false, path = OmcpCoreStub.PathKind.Queued),
    ChatRowModel("4", "Sam Kim", "Sent via neighbour’s Internet", "Yesterday", "SK", true, path = OmcpCoreStub.PathKind.Gateway),
)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatsListScreen(
    onOpenChat: (String) -> Unit,
) {
    val status = OmcpCoreStub.meshStatus()
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.tab_chats)) },
                actions = {
                    IconButton(onClick = { /* search shell */ }) {
                        Icon(Icons.Default.Search, contentDescription = "Search")
                    }
                    IconButton(onClick = { /* add contact shell */ }) {
                        Icon(Icons.Default.Add, contentDescription = "Add contact")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surface,
                ),
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .background(MaterialTheme.colorScheme.background),
        ) {
            Row(
                Modifier.padding(horizontal = 16.dp, vertical = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                PathChip(
                    OmcpCoreStub.PathKind.Mesh,
                    if (status.nearbyPeers > 0) "Mesh · ${status.nearbyPeers} nearby" else "Mesh · placeholder",
                )
                if (status.queuedCount > 0 || !OmcpCoreStub.CORE_LINKED) {
                    PathChip(OmcpCoreStub.PathKind.Queued, if (status.queuedCount > 0) "${status.queuedCount} queued" else "Core stub")
                }
            }
            if (SampleChats.isEmpty()) {
                EmptyState(
                    title = stringResource(R.string.chats_empty_title),
                    body = stringResource(R.string.chats_empty_body),
                )
            } else {
                LazyColumn {
                    items(SampleChats, key = { it.id }) { row ->
                        ChatRow(row, onClick = { onOpenChat(row.id) })
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatThreadScreen(
    chatId: String,
    onBack: () -> Unit,
    onSecurity: () -> Unit,
) {
    val chat = SampleChats.find { it.id == chatId } ?: SampleChats.first()
    val c = LocalOmcpColors.current
    var draft by remember { mutableStateOf("") }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text(chat.title)
                        Text(
                            if (chat.isGroup) "MLS group · placeholder" else "Nearby · last seen —",
                            style = MaterialTheme.typography.labelSmall,
                            color = c.ink2,
                        )
                    }
                },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    IconButton(onClick = onSecurity) {
                        Icon(Icons.Default.Security, contentDescription = stringResource(R.string.security_centre))
                    }
                },
            )
        },
        bottomBar = {
            Row(
                Modifier
                    .fillMaxWidth()
                    .background(MaterialTheme.colorScheme.surface)
                    .padding(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = {}) { Icon(Icons.Default.AttachFile, contentDescription = "Attach") }
                TextField(
                    value = draft,
                    onValueChange = { draft = it },
                    modifier = Modifier.weight(1f),
                    placeholder = { Text("Message") },
                    colors = TextFieldDefaults.colors(
                        focusedContainerColor = c.surface2,
                        unfocusedContainerColor = c.surface2,
                        disabledContainerColor = c.surface2,
                        focusedIndicatorColor = Color.Transparent,
                        unfocusedIndicatorColor = Color.Transparent,
                    ),
                    shape = RoundedCornerShape(10.dp),
                )
                IconButton(onClick = { draft = "" }) {
                    Icon(Icons.Default.Send, contentDescription = "Send")
                }
            }
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            PathBanner(
                kind = chat.path ?: OmcpCoreStub.PathKind.Mesh,
                text = when (chat.path) {
                    OmcpCoreStub.PathKind.Gateway -> "Sent via neighbour’s Internet · encrypted end-to-end"
                    OmcpCoreStub.PathKind.Queued -> "Waiting for mesh · will send when a path opens"
                    OmcpCoreStub.PathKind.Mixed -> "Mixed paths · encrypted end-to-end"
                    else -> "Via mesh · placeholder hops · encrypted end-to-end"
                },
                modifier = Modifier.fillMaxWidth(),
            )
            LazyColumn(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
                contentPadding = PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                item {
                    Bubble(mine = false, text = chat.preview, meta = "9:30 · Delivered · Mesh")
                }
                item {
                    Bubble(mine = true, text = "Placeholder reply — mesh not linked yet.", meta = "9:32 · Queued")
                }
            }
        }
    }
}

@Composable
private fun Bubble(mine: Boolean, text: String, meta: String) {
    val c = LocalOmcpColors.current
    val bg = if (mine) c.brandSoft else MaterialTheme.colorScheme.surface
    Column(
        Modifier.fillMaxWidth(),
        horizontalAlignment = if (mine) Alignment.End else Alignment.Start,
    ) {
        Box(
            Modifier
                .clip(RoundedCornerShape(14.dp))
                .then(
                    if (!mine) Modifier.border(1.dp, c.line, RoundedCornerShape(14.dp))
                    else Modifier,
                )
                .background(bg)
                .padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            Text(text, style = MaterialTheme.typography.bodyLarge)
        }
        Text(
            meta,
            style = MaterialTheme.typography.labelSmall,
            color = c.ink3,
            modifier = Modifier.padding(top = 4.dp, start = 4.dp, end = 4.dp),
        )
    }
}
