package chat.omcp.app.ui

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Chat
import androidx.compose.material.icons.filled.Hub
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.ui.unit.dp
import chat.omcp.app.ui.screens.MeshHomeScreen
import androidx.compose.material.icons.filled.Groups
import androidx.compose.material.icons.filled.LiveTv
import androidx.compose.material.icons.filled.Person
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.delay
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import chat.omcp.app.R
import chat.omcp.app.notify.NotificationPolicy
import chat.omcp.app.backup.CloudBackupStore
import chat.omcp.app.data.BackupGate
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.mesh.MeshPermissions
import chat.omcp.app.mesh.MeshService
import chat.omcp.app.ui.screens.BackupSettingsScreen
import chat.omcp.app.ui.screens.StorageSettingsScreen
import chat.omcp.app.ui.screens.ChatThreadScreen
import chat.omcp.app.ui.screens.ChatsListScreen
import chat.omcp.app.ui.screens.CommunitiesHomeScreen
import chat.omcp.app.ui.screens.ContactAddScreen
import chat.omcp.app.ui.screens.GatewaySettingsScreen
import chat.omcp.app.ui.screens.LiveTabScreen
import chat.omcp.app.ui.screens.MyQrBlock
import chat.omcp.app.ui.screens.MyQrScreen
import chat.omcp.app.ui.screens.OnboardingFlow
import chat.omcp.app.ui.screens.SecurityCentreScreen
import chat.omcp.app.ui.screens.YouScreen
import chat.omcp.app.ui.vm.BackupSettingsViewModel
import chat.omcp.app.ui.vm.ChatThreadViewModel
import chat.omcp.app.ui.vm.ChatsViewModel
import chat.omcp.app.ui.vm.ContactAddViewModel
import chat.omcp.app.ui.vm.GatewayViewModel
import chat.omcp.app.ui.vm.OnboardingStep
import chat.omcp.app.ui.vm.OnboardingViewModel

private object Routes {
    const val Chats = "chats"
    const val Mesh = "mesh"
    const val Communities = "communities"
    const val Live = "live"
    const val You = "you"
    const val Thread = "thread/{chatId}"
    const val Security = "security/{chatId}"
    const val Gateway = "gateway"
    const val ContactAdd = "contact-add"
    const val MyQr = "my-qr"
    const val Backup = "backup"
    const val Storage = "storage"

    fun thread(id: String) = "thread/$id"
    fun security(id: String) = "security/$id"
}

@Composable
private inline fun <reified VM : androidx.lifecycle.ViewModel> vm(key: String? = null, crossinline make: () -> VM): VM =
    viewModel(key = key, factory = viewModelFactory { initializer { make() } })

@Composable
fun AuralisApp(
    repo: MeshRepository,
    cloud: CloudBackupStore,
    secureWindow: (Boolean) -> Unit,
    requestDeviceAuth: ((() -> Unit) -> Unit),
) {
    val ctx = LocalContext.current
    val storageBlocked by repo.storageBlocked.collectAsState()
    if (storageBlocked) {
        androidx.compose.foundation.layout.Column(
            modifier = Modifier.padding(24.dp),
            verticalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(16.dp),
        ) {
            Text(stringResource(R.string.store_blocked_title), style = MaterialTheme.typography.headlineSmall)
            Text(stringResource(R.string.store_blocked_body))
            androidx.compose.material3.Button(onClick = { if (repo.storageBlocked.value) repo.retryRestore() }) {
                Text(stringResource(R.string.restore_try_again))
            }
        }
        return
    }
    val onboarded by repo.onboarded.collectAsState()
    val contacts by repo.contacts.collectAsState()
    val meshStatus by repo.status.collectAsState()
    // Expiry must take effect without changing contacts or restarting the process. Recheck
    // on resume, at the deadline, and once a minute for changes to the device's wall clock.
    val gateTick = remember { mutableIntStateOf(0) }
    val gate = remember(onboarded, contacts, gateTick.intValue) { repo.backupGate() }
    val lifecycleOwner = LocalLifecycleOwner.current
    LaunchedEffect(repo, onboarded, lifecycleOwner) {
        if (onboarded) lifecycleOwner.lifecycle.repeatOnLifecycle(Lifecycle.State.RESUMED) {
            while (true) {
                gateTick.intValue++
                val remaining = repo.appStore.backupRemindDeadlineMs - System.currentTimeMillis()
                delay(if (remaining > 0) remaining.coerceAtMost(60_000L) else 60_000L)
            }
        }
    }

    val perms = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        if (MeshPermissions.hasBle(ctx)) MeshService.start(ctx)
    }
    val requestMesh = { perms.launch(MeshPermissions.runtimePermissions()) }
    LaunchedEffect(onboarded) { if (onboarded && MeshPermissions.hasBle(ctx)) MeshService.start(ctx) }

    if (!onboarded || gate == BackupGate.Required) {
        // O6: after 24 h without a backup method, the Secure step is mandatory.
        val app = ctx.applicationContext as? chat.omcp.app.OmcpApplication
        val ovm = vm {
            if (app != null) {
                OnboardingViewModel(
                    repo, cloud,
                    deviceHint = android.os.Build.MODEL ?: "Android",
                    restore = app.restore,
                    runner = chat.omcp.app.restore.RestoreWorker.runner(ctx.applicationContext),
                )
            } else {
                OnboardingViewModel(repo, cloud)
            }
        }
        if (onboarded && gate == BackupGate.Required) {
            LaunchedEffect(ovm) { ovm.resumeRequiredBackup() }
        }
        OnboardingFlow(
            vm = ovm,
            myQr = {
                val id = repo.core.identity()
                if (id != null) MyQrBlock(repo.core.qrPayload(), id.displayName, id.peerIdHex)
            },
            onFindNearby = requestMesh,
            secureWindow = secureWindow,
        )
        val st by ovm.state.collectAsState()
        LaunchedEffect(st.step) { if (st.step == OnboardingStep.Done) gateTick.intValue++ }
        return
    }

    val nav = rememberNavController()
    val backStack by nav.currentBackStackEntryAsState()
    val route = backStack?.destination?.route
    val showBar = route in setOf(Routes.Chats, Routes.Mesh, Routes.You)

    Scaffold(
        contentWindowInsets = WindowInsets(0, 0, 0, 0),
        bottomBar = {
            if (showBar) {
                NavigationBar(containerColor = MaterialTheme.colorScheme.surface, tonalElevation = 0.dp) {
                    val itemColors = NavigationBarItemDefaults.colors(
                        selectedIconColor = MaterialTheme.colorScheme.primary,
                        selectedTextColor = MaterialTheme.colorScheme.primary,
                        indicatorColor = MaterialTheme.colorScheme.primaryContainer,
                        unselectedIconColor = MaterialTheme.colorScheme.onSurfaceVariant,
                        unselectedTextColor = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    NavigationBarItem(route == Routes.Chats, { nav.tab(Routes.Chats) }, { Icon(Icons.Default.Chat, null) }, label = { Text(stringResource(R.string.tab_chats)) }, colors = itemColors)
                    NavigationBarItem(route == Routes.Mesh, { nav.tab(Routes.Mesh) }, { Icon(Icons.Default.Hub, null) }, label = { Text(stringResource(R.string.tab_mesh)) }, colors = itemColors)
                    NavigationBarItem(route == Routes.You, { nav.tab(Routes.You) }, { Icon(Icons.Default.Person, null) }, label = { Text(stringResource(R.string.tab_you)) }, colors = itemColors)
                }
            }
        },
    ) { padding ->
        NavHost(navController = nav, startDestination = Routes.Chats, modifier = Modifier.padding(padding).consumeWindowInsets(padding)) {
            composable(Routes.Chats) {
                ChatsListScreen(
                    vm = vm { ChatsViewModel(repo) },
                    backupGate = gate,
                    onOpenChat = { nav.navigate(Routes.thread(it)) },
                    onAddContact = { nav.navigate(Routes.ContactAdd) },
                    onBackup = { nav.navigate(Routes.Backup) },
                    onMesh = { nav.tab(Routes.Mesh) },
                )
            }
            composable(Routes.Mesh) {
                MeshHomeScreen(
                    status = meshStatus,
                    onFindNearby = requestMesh,
                    onAddContact = { nav.navigate(Routes.ContactAdd) },
                    onGateway = { nav.navigate(Routes.Gateway) },
                    onCommunities = { nav.navigate(Routes.Communities) },
                    onLive = { nav.navigate(Routes.Live) },
                )
            }
            composable(Routes.Communities) { CommunitiesHomeScreen(onBack = { nav.popBackStack() }) }
            composable(Routes.Live) { LiveTabScreen(onBack = { nav.popBackStack() }) }
            composable(Routes.You) {
                YouScreen(
                    identity = repo.core.identity(),
                    suiteLabel = repo.core.suiteLabel,
                    privateNotifications = repo.appStore.privateNotifications,
                    onSetPrivateNotifications = { repo.appStore.privateNotifications = it },
                    hideOnLockScreen = repo.appStore.hideOnLockScreen,
                    onSetHideOnLockScreen = { repo.appStore.hideOnLockScreen = it },
                    onGateway = { nav.navigate(Routes.Gateway) },
                    contacts = contacts,
                    onSecurity = { id -> nav.navigate(Routes.security(id)) },
                    onBackup = { nav.navigate(Routes.Backup) },
                    onStorage = { nav.navigate(Routes.Storage) },
                    onMyQr = { nav.navigate(Routes.MyQr) },
                )
            }
            composable(Routes.Thread) { entry ->
                val id = entry.arguments?.getString("chatId") ?: return@composable
                ChatThreadScreen(
                    vm = vm(key = "thread-$id") { ChatThreadViewModel(repo, id) },
                    onBack = { nav.popBackStack() },
                    onSecurity = { nav.navigate(Routes.security(id)) },
                )
            }
            composable(Routes.Security) { entry ->
                val id = entry.arguments?.getString("chatId")
                SecurityCentreScreen(
                    contact = contacts.firstOrNull { it.id == id },
                    suiteLabel = repo.core.suiteLabel,
                    onSetVerified = { v -> id?.let { repo.setVerified(it, v) } },
                    privateNotifications = id != null && NotificationPolicy.isPrivate(repo.appStore, id),
                    onSetPrivateNotifications = { v -> id?.let { NotificationPolicy.setChatPrivate(repo.appStore, it, v) } },
                    burnOption = id?.let { repo.appStore.burnChats[it] } ?: -1,
                    burnUnreadTtl = id?.let { repo.appStore.burnUnreadTtl[it] } ?: chat.omcp.app.burn.BurnSpec.UNREAD_7D,
                    onSetBurn = { o, t -> id?.let { repo.setChatBurn(it, o, t) } },
                    onBack = { nav.popBackStack() },
                )
            }
            composable(Routes.Gateway) {
                GatewaySettingsScreen(vm = vm { GatewayViewModel(repo) }, onBack = { nav.popBackStack() })
            }
            composable(Routes.ContactAdd) {
                ContactAddScreen(
                    vm = vm { ContactAddViewModel(repo) },
                    onBack = { nav.popBackStack() },
                    onMyQr = { nav.navigate(Routes.MyQr) },
                    onOpenChat = { id ->
                        nav.navigate(Routes.thread(id)) { popUpTo(Routes.Chats) }
                        if (!MeshPermissions.hasBle(ctx)) requestMesh()
                    },
                )
            }
            composable(Routes.MyQr) {
                MyQrScreen(
                    vm = vm { ContactAddViewModel(repo) },
                    onBack = { nav.popBackStack() },
                    onScanTheirs = { nav.navigate(Routes.ContactAdd) { popUpTo(Routes.Chats) } },
                )
            }
            composable(Routes.Storage) {
                StorageSettingsScreen(vm = vm { chat.omcp.app.ui.vm.StorageViewModel(repo.core) }, onBack = { nav.popBackStack() })
            }
            composable(Routes.Backup) {
                BackupSettingsScreen(
                    vm = vm { BackupSettingsViewModel(repo, cloud) },
                    requestDeviceAuth = requestDeviceAuth,
                    secureWindow = secureWindow,
                    onBack = { gateTick.intValue++; nav.popBackStack() },
                )
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
