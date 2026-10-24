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
import chat.omcp.app.burn.BurnSpec
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.LaunchedEffect
import kotlinx.coroutines.delay
import chat.omcp.app.core.PathKind
import chat.omcp.app.data.BackupGate
import chat.omcp.app.data.ChatMessage
import chat.omcp.app.data.FileAttachment
import chat.omcp.app.ui.vm.ChatThreadViewModel
import chat.omcp.app.ui.vm.ChatsViewModel
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import androidx.compose.runtime.collectAsState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.TextButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.foundation.clickable
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.ui.platform.LocalContext
import java.text.DateFormat
import java.util.Date
import chat.omcp.app.ui.components.ChatRow
import chat.omcp.app.ui.components.ChatRowModel
import chat.omcp.app.ui.components.EmptyState
import chat.omcp.app.ui.components.PathBanner
import chat.omcp.app.ui.components.PathChip
import chat.omcp.app.ui.debug.DesignReviewFlags
import chat.omcp.app.ui.theme.LocalOmcpColors

/**
 * Design lock-in: sample rows only when [DesignReviewFlags.showSampleChatsForDesignReview]
 * is on (off by default). Default UI shows real contacts or the empty state.
 */
private val DesignReviewSampleChats = listOf(
    ChatRowModel("1", "Alex Rivera", "See you at the park entrance", "9:38", "AL", true, path = PathKind.Mesh),
    ChatRowModel("2", "Neighbourhood watch", "Maya: Power’s back on our street", "9:12", "N", false, unread = 3, isGroup = true),
)

private fun time(ms: Long) = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(ms))

private fun mb(bytes: Long) = "%.1f".format(bytes / 1_000_000.0)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatsListScreen(
    vm: ChatsViewModel,
    backupGate: BackupGate,
    onOpenChat: (String) -> Unit,
    onAddContact: () -> Unit,
    onBackup: () -> Unit,
) {
    val status by vm.status.collectAsState()
    val contacts by vm.contacts.collectAsState()
    val messages by vm.messages.collectAsState()
    val chats = if (DesignReviewFlags.showSampleChatsForDesignReview) DesignReviewSampleChats else contacts.map { c ->
        val last = messages[c.id]?.lastOrNull()
        ChatRowModel(
            id = c.id,
            title = c.displayName,
            preview = last?.let { if (it.burn != null) "🔥" else it.file?.name ?: it.text } ?: "",
            time = last?.let { time(it.timeMs) } ?: "",
            initials = c.displayName.split(" ").mapNotNull { it.firstOrNull()?.uppercase() }.take(2).joinToString(""),
            verified = c.verified,
            path = if (messages[c.id]?.any { it.mine && it.status == ChatMessage.Status.Queued } == true) PathKind.Queued else null,
        )
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.tab_chats)) },
                actions = {
                    IconButton(onClick = onAddContact) {
                        Icon(Icons.Default.Add, contentDescription = stringResource(R.string.contact_add_title))
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.surface),
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .background(MaterialTheme.colorScheme.background),
        ) {
            if (backupGate == BackupGate.Banner) {
                Callout(
                    stringResource(R.string.backup_banner),
                    CalloutKind.Warn,
                    Modifier.padding(horizontal = 16.dp, vertical = 8.dp).clickable(onClick = onBackup),
                )
            }
            Row(
                Modifier.padding(horizontal = 16.dp, vertical = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (status.nearbyPeers > 0) PathChip(PathKind.Mesh, "Mesh · ${status.nearbyPeers} nearby")
                if (status.queuedCount > 0 || status.nearbyPeers == 0) {
                    PathChip(PathKind.Queued, if (status.queuedCount > 0) "${status.queuedCount} queued" else stringResource(R.string.path_queued))
                }
            }
            if (chats.isEmpty()) {
                EmptyState(
                    title = stringResource(R.string.chats_empty_title),
                    body = stringResource(R.string.chats_empty_body),
                )
            } else {
                LazyColumn {
                    items(chats, key = { it.id }) { row ->
                        ChatRow(row, onClick = { onOpenChat(row.id) })
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalFoundationApi::class)
@Composable
fun ChatThreadScreen(
    vm: ChatThreadViewModel,
    onBack: () -> Unit,
    onSecurity: () -> Unit,
) {
    val all by vm.messages.collectAsState()
    val status by vm.status.collectAsState()
    val msgs = all[vm.contactId].orEmpty()
    val contact = vm.contact()
    val c = LocalOmcpColors.current
    var draft by remember { mutableStateOf("") }
    // Per-message 🔥 (long-press Send). Falls back to the chat's Burn-after-read default.
    var burnNext by remember { mutableStateOf(false) }
    var opened by remember { mutableStateOf<OpenedBurn?>(null) }
    LaunchedEffect(vm.contactId) {
        while (true) { vm.tickBurn(); delay(250) }
    }
    opened?.let { o ->
        val live = msgs.firstOrNull { it.id == o.msg.id } ?: o.msg
        BurnViewer(OpenedBurn(live, o.body), now = System::currentTimeMillis) {
            vm.closeBurn(o.msg.id); o.body.fill(0); opened = null
        }
    }
    val path = remember(status, msgs) { vm.path() }
    val ctx = LocalContext.current
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        var name = "file"
        var size = 0L
        ctx.contentResolver.query(uri, null, null, null, null)?.use { cur ->
            if (cur.moveToFirst()) {
                cur.getColumnIndex(OpenableColumns.DISPLAY_NAME).takeIf { it >= 0 }?.let { name = cur.getString(it) }
                cur.getColumnIndex(OpenableColumns.SIZE).takeIf { it >= 0 }?.let { size = cur.getLong(it) }
            }
        }
        vm.onFilePicked(name, size, ctx.contentResolver.getType(uri) ?: "application/octet-stream")
    }
    val confirm by vm.fileConfirm.collectAsState()
    confirm?.let { f ->
        val who = contact?.displayName ?: ""
        AlertDialog(
            onDismissRequest = vm::cancelFile,
            title = { Text(if (f.tooLarge) stringResource(R.string.file_too_large_title) else stringResource(R.string.file_send_title, who)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(f.name, style = MaterialTheme.typography.titleSmall)
                    if (f.tooLarge) {
                        Callout(stringResource(R.string.file_too_large_body, mb(f.sizeBytes), mb(f.maxBytes)), CalloutKind.Danger)
                    } else {
                        Text(stringResource(R.string.file_size_row, mb(f.sizeBytes), mb(f.maxBytes)))
                        Text(f.mime, color = c.ink2)
                        PathChip(f.route, if (f.route == PathKind.Queued) stringResource(R.string.path_queued) else stringResource(R.string.path_mesh))
                        Callout(
                            if (f.route == PathKind.Queued) stringResource(R.string.file_queue_hint, who)
                            else stringResource(R.string.file_mesh_hint, who),
                        )
                    }
                }
            },
            confirmButton = {
                if (f.tooLarge) {
                    TextButton(onClick = { vm.cancelFile(); picker.launch(arrayOf("*/*")) }) { Text(stringResource(R.string.file_choose_another)) }
                } else {
                    TextButton(onClick = vm::confirmFile) {
                        Text(if (f.route == PathKind.Queued) stringResource(R.string.file_queue) else stringResource(R.string.file_send))
                    }
                }
            },
            dismissButton = { TextButton(onClick = vm::cancelFile) { Text(stringResource(R.string.cancel)) } },
        )
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(contact?.displayName ?: "Chat")
                            if (contact?.verified == true) chat.omcp.app.ui.components.VerifiedBadge()
                        }
                        // Product decision: unverified contacts show no badge or label.
                        if (contact?.verified == true) {
                            Text(stringResource(R.string.verified_contact), style = MaterialTheme.typography.labelSmall, color = c.ok)
                        }
                    }
                },
                navigationIcon = {
                    IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back)) }
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
                IconButton(onClick = { picker.launch(arrayOf("*/*")) }) { Icon(Icons.Default.AttachFile, contentDescription = "Attach") }
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
                val burnOn = burnNext || vm.chatBurn() != null
                Box(
                    Modifier
                        .size(48.dp)
                        .combinedClickable(
                                                        onClick = {
                                if (draft.isNotBlank()) {
                                    vm.send(draft, if (burnNext) vm.chatBurn() ?: BurnSpec.onOpen() else null)
                                    draft = ""; burnNext = false
                                }
                            },
                            onLongClick = { burnNext = !burnNext },
                        ),
                    contentAlignment = Alignment.Center,
                ) {
                    if (burnOn) Text("🔥") else Icon(Icons.Default.Send, contentDescription = "Send")
                }
            }
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            // Path banner: mesh / gateway / mixed / queued only.
            PathBanner(
                kind = path,
                text = when (path) {
                    PathKind.Gateway -> "Sent via neighbour’s Internet · encrypted end-to-end"
                    PathKind.Queued -> "Waiting for mesh · will send when a path opens"
                    PathKind.Mixed -> "Mixed paths · encrypted end-to-end"
                    PathKind.Mesh -> "Via mesh · encrypted end-to-end"
                },
                modifier = Modifier.fillMaxWidth(),
            )
            LazyColumn(
                modifier = Modifier.weight(1f).fillMaxWidth(),
                contentPadding = PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                items(msgs, key = { it.id }) { m ->
                    val f = m.file
                    if (m.burn != null) {
                        BurnBubble(m) { vm.openBurn(m.id)?.let { body -> opened = OpenedBurn(vm.messages.value[vm.contactId]!!.first { it.id == m.id }, body) } }
                    } else if (f != null) {
                        FileBubble(m.mine, f)
                    } else {
                        val meta = time(m.timeMs) + " · " + when (m.status) {
                            ChatMessage.Status.Queued -> "Queued"
                            ChatMessage.Status.Sent -> "Sent · Mesh"
                            ChatMessage.Status.Received -> "Mesh"
                        }
                        Bubble(mine = m.mine, text = m.text, meta = meta)
                    }
                }
            }
        }
    }
}

/** File bubble (§5.24). Beta: transfers are queued only — chunk streaming is TODO(M5.1). */
@Composable
private fun FileBubble(mine: Boolean, f: FileAttachment) {
    val c = LocalOmcpColors.current
    Column(Modifier.fillMaxWidth(), horizontalAlignment = if (mine) Alignment.End else Alignment.Start) {
        Column(
            Modifier
                .fillMaxWidth(0.8f)
                .clip(RoundedCornerShape(14.dp))
                .border(1.dp, c.line, RoundedCornerShape(14.dp))
                .background(MaterialTheme.colorScheme.surface)
                .padding(12.dp),
        ) {
            Text(f.name, style = MaterialTheme.typography.titleSmall)
            Text(
                when (f.state) {
                    FileAttachment.State.Queued -> "Queued · ${mb(f.sizeBytes)} MB · next in line"
                    FileAttachment.State.Paused -> "Paused · out of range"
                    FileAttachment.State.Received -> "${mb(f.sizeBytes)} MB · Received"
                    FileAttachment.State.Failed -> "Couldn’t finish — will retry on mesh"
                    FileAttachment.State.IntegrityFailed -> "File didn’t match — ask them to resend"
                    else -> f.state.name
                },
                style = MaterialTheme.typography.bodySmall,
                color = c.ink2,
            )
            LinearProgressIndicator(progress = { 0f }, modifier = Modifier.fillMaxWidth().padding(top = 6.dp), color = c.pathQueue)
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
