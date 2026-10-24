package chat.omcp.app.data

import chat.omcp.app.core.Contact

/** Persistent app state. Android impl: [PrefsAppStore]; tests: [InMemoryAppStore]. */
interface AppStore {
    /** Recovery phrase (the identity master secret). Sealed with a Keystore key on device. */
    var recoveryPhrase: String?
    var displayName: String?
    var contacts: List<Contact>
    /** Product decision: notification previews off by default. */
    var notificationPreviews: Boolean
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
    override var notificationPreviews = false
    override var gatewayEnabled = false
    override var gatewayAllowMobile = false
    override var backupMethods: Set<String> = emptySet()
    override var lastCloudBackupMs = 0L
    override var backupRemindDeadlineMs = 0L
}
