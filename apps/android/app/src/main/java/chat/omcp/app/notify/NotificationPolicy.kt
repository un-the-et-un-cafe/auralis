package chat.omcp.app.notify

import chat.omcp.app.data.AppStore

/** Lock-screen visibility, mapped to NotificationCompat.VISIBILITY_* by the Android layer. */
enum class LockVisibility { PRIVATE, SECRET }

/**
 * What a message notification shows. When [private] is true there is no sender,
 * no preview and no avatar: title = app name, body = generic "New message".
 * [publicVersion] is what the lock screen shows for VISIBILITY_PRIVATE.
 */
data class NotificationContent(
    val title: String,
    val body: String,
    val showAvatar: Boolean,
    val private: Boolean,
    val visibility: LockVisibility,
    val publicTitle: String,
    val publicBody: String,
)

/**
 * Pure notification policy (REQUIREMENTS "Notifications (v0.1.2)", PROTOCOL §privacy 8).
 * Default: sender + preview. Private (global or per chat): generic only.
 */
object NotificationPolicy {
    fun isPrivate(store: AppStore, chatId: String): Boolean =
        store.privateNotifications || chatId in store.privateChats

    fun content(
        store: AppStore,
        chatId: String,
        sender: String,
        text: String,
        appName: String,
        genericBody: String,
    ): NotificationContent {
        val private = isPrivate(store, chatId)
        val vis = if (store.hideOnLockScreen) LockVisibility.SECRET else LockVisibility.PRIVATE
        return if (private) {
            NotificationContent(appName, genericBody, false, true, vis, appName, genericBody)
        } else {
            NotificationContent(sender, text, true, false, vis, appName, genericBody)
        }
    }

    fun setChatPrivate(store: AppStore, chatId: String, on: Boolean) {
        store.privateChats = if (on) store.privateChats + chatId else store.privateChats - chatId
    }
}
