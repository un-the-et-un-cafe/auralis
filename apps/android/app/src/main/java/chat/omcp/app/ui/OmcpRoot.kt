package chat.omcp.app.ui

import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Chat
import androidx.compose.material.icons.filled.Groups
import androidx.compose.material.icons.filled.LiveTv
import androidx.compose.material.icons.filled.Person
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import chat.omcp.app.R
import chat.omcp.app.ui.screens.ChatThreadScreen
import chat.omcp.app.ui.screens.ChatsListScreen
import chat.omcp.app.ui.screens.CommunitiesHomeScreen
import chat.omcp.app.ui.screens.ContactAddScreen
import chat.omcp.app.ui.screens.GatewaySettingsScreen
import chat.omcp.app.ui.screens.LiveTabScreen
import chat.omcp.app.ui.screens.MyQrScreen
import chat.omcp.app.ui.screens.SecurityCentreScreen
import chat.omcp.app.ui.screens.YouScreen

private object Routes {
    const val Chats = "chats"
    const val Communities = "communities"
    const val Live = "live"
    const val You = "you"
    const val Thread = "thread/{chatId}"
    const val Security = "security"
    const val Gateway = "gateway"
    const val ContactAdd = "contact-add"
    const val MyQr = "my-qr"

    fun thread(id: String) = "thread/$id"
}

@Composable
fun OmcpRoot() {
    val nav = rememberNavController()
    val backStack by nav.currentBackStackEntryAsState()
    val route = backStack?.destination?.route
    val showBar = route in setOf(Routes.Chats, Routes.Communities, Routes.Live, Routes.You)

    Scaffold(
        bottomBar = {
            if (showBar) {
                NavigationBar {
                    NavigationBarItem(
                        selected = route == Routes.Chats,
                        onClick = { nav.tab(Routes.Chats) },
                        icon = { Icon(Icons.Default.Chat, null) },
                        label = { Text(stringResource(R.string.tab_chats)) },
                    )
                    NavigationBarItem(
                        selected = route == Routes.Communities,
                        onClick = { nav.tab(Routes.Communities) },
                        icon = { Icon(Icons.Default.Groups, null) },
                        label = { Text(stringResource(R.string.tab_communities)) },
                    )
                    NavigationBarItem(
                        selected = route == Routes.Live,
                        onClick = { nav.tab(Routes.Live) },
                        icon = { Icon(Icons.Default.LiveTv, null) },
                        label = { Text(stringResource(R.string.tab_live)) },
                    )
                    NavigationBarItem(
                        selected = route == Routes.You,
                        onClick = { nav.tab(Routes.You) },
                        icon = { Icon(Icons.Default.Person, null) },
                        label = { Text(stringResource(R.string.tab_you)) },
                    )
                }
            }
        },
    ) { padding ->
        NavHost(
            navController = nav,
            startDestination = Routes.Chats,
            modifier = Modifier.padding(padding),
        ) {
            composable(Routes.Chats) {
                ChatsListScreen(
                    onOpenChat = { nav.navigate(Routes.thread(it)) },
                    onAddContact = { nav.navigate(Routes.ContactAdd) },
                )
            }
            composable(Routes.Communities) { CommunitiesHomeScreen() }
            composable(Routes.Live) { LiveTabScreen() }
            composable(Routes.You) {
                YouScreen(
                    onGateway = { nav.navigate(Routes.Gateway) },
                    onSecurity = { nav.navigate(Routes.Security) },
                )
            }
            composable(Routes.Thread) { entry ->
                val id = entry.arguments?.getString("chatId") ?: "1"
                ChatThreadScreen(
                    chatId = id,
                    onBack = { nav.popBackStack() },
                    onSecurity = { nav.navigate(Routes.Security) },
                )
            }
            composable(Routes.Security) {
                SecurityCentreScreen(onBack = { nav.popBackStack() })
            }
            composable(Routes.Gateway) {
                GatewaySettingsScreen(onBack = { nav.popBackStack() })
            }
            composable(Routes.ContactAdd) {
                ContactAddScreen(
                    onBack = { nav.popBackStack() },
                    onMyQr = { nav.navigate(Routes.MyQr) },
                )
            }
            composable(Routes.MyQr) {
                MyQrScreen(onBack = { nav.popBackStack() })
            }
        }
    }
}

private fun androidx.navigation.NavHostController.tab(route: String) {
    navigate(route) {
        popUpTo(graph.findStartDestination().id) { saveState = true }
        launchSingleTop = true
        restoreState = true
    }
}
