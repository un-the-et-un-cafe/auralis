package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import chat.omcp.app.R
import chat.omcp.app.ui.components.EmptyState

/** Planned protocol capabilities stay discoverable in Mesh, never masquerade as live features. */
@Composable
fun CommunitiesHomeScreen(onBack: () -> Unit) = RoadmapScreen(R.string.tab_communities, R.string.communities_empty_body, onBack)

@Composable
fun LiveTabScreen(onBack: () -> Unit) = RoadmapScreen(R.string.tab_live, R.string.live_empty_body, onBack)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun RoadmapScreen(title: Int, body: Int, onBack: () -> Unit) {
    Scaffold(topBar = {
        TopAppBar(
            title = { Text(stringResource(title)) },
            navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) } },
        )
    }) { padding ->
        EmptyState(
            title = stringResource(R.string.auralis_on_horizon),
            body = stringResource(body),
            modifier = Modifier.padding(padding),
            actionLabel = stringResource(R.string.auralis_back_to_mesh),
            onAction = onBack,
        )
    }
}
