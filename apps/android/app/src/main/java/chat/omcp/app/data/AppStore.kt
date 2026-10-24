package chat.omcp.app.data

import chat.omcp.app.core.Contact

/** Persistent app state. Android impl: [PrefsAppStore]; tests: [InMemoryAppStore]. */
interface AppStore {
    /** Recovery phrase (the identity master secret). Sealed with a Keystore key on device. */
    var recoveryPhrase: String?
    var displayName: String?
    var contacts: List<Contact>
    /** Global "Private notifications" (REQUIREMENTS Notifications v0.1.2). Off by default: sender + preview shown. */
    var privateNotifications: Boolean
    /** Chat ids with per-chat "Private notifications" on. */
    var privateChats: Set<String>
    /** Hide notification on the lock screen (VISIBILITY_SECRET). Off by default. */
    var hideOnLockScreen: Boolean
    /** Per-chat burn option: chatId -> -1 off, 0 on open, else timer seconds. */
    var burnChats: Map<String, Int>
    /** Per-chat unread TTL (seconds) for burn messages. */
    var burnUnreadTtl: Map<String, Int>
    var gatewayEnabled: Boolean
    /** Off = Wi-Fi only (default). Metered cellular warns, never blocks. */
    var gatewayAllowMobile: Boolean
    /** Completed backup methods: "drive", "written". */
    var backupMethods: Set<String>
    var lastCloudBackupMs: Long
    /** "Remind me later" deadline (24 h); 0 = not deferred. */
    var backupRemindDeadlineMs: Long
}

class InMemoryAppStore : AppStore {
    override var recoveryPhrase: String? = null
    override var displayName: String? = null
    override var contacts: List<Contact> = emptyList()
    override var privateNotifications = false
    override var privateChats: Set<String> = emptySet()
    override var hideOnLockScreen = false
    override var burnChats: Map<String, Int> = emptyMap()
    override var burnUnreadTtl: Map<String, Int> = emptyMap()
    override var gatewayEnabled = false
    override var gatewayAllowMobile = false
    override var backupMethods: Set<String> = emptySet()
    override var lastCloudBackupMs = 0L
    override var backupRemindDeadlineMs = 0L
}
