package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import chat.omcp.app.R
import chat.omcp.app.ui.components.EmptyState

/** Jakob: keep Communities + Live tabs visible with Coming in v0.2 empty states. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CommunitiesHomeScreen() {
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.tab_communities)) }) }) { padding ->
        EmptyState(
            title = stringResource(R.string.coming_v02_title),
            body = stringResource(R.string.communities_empty_body),
            modifier = Modifier.padding(padding),
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LiveTabScreen() {
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.tab_live)) }) }) { padding ->
        EmptyState(
            title = stringResource(R.string.coming_v02_title),
            body = stringResource(R.string.live_empty_body),
            modifier = Modifier.padding(padding),
        )
    }
}
