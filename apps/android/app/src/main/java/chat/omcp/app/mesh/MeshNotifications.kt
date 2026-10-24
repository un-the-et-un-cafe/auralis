package chat.omcp.app.mesh

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.pm.PackageManager
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import chat.omcp.app.R
import chat.omcp.app.core.IncomingText
import chat.omcp.app.data.MeshRepository

/** Message notifications. Previews are **off by default**: generic "New Hopla message". */
object MeshNotifications {
    const val CHANNEL_MESH = "mesh"
    const val CHANNEL_MESSAGES = "messages"

    fun ensureChannels(ctx: Context) {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_MESH, ctx.getString(R.string.notif_channel_mesh), NotificationManager.IMPORTANCE_LOW),
        )
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_MESSAGES, ctx.getString(R.string.notif_channel_messages), NotificationManager.IMPORTANCE_DEFAULT),
        )
    }

    /** Pure: what the notification shows given the previews setting. */
    fun content(ctx: Context, previews: Boolean, sender: String, text: String): Pair<String, String> =
        if (previews) sender to text else ctx.getString(R.string.app_name) to ctx.getString(R.string.notif_new_message)

    fun onMessage(ctx: Context, repo: MeshRepository, msg: IncomingText) {
        if (ctx.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        val sender = repo.contact(msg.contactId)?.displayName ?: "Contact"
        val (title, body) = content(ctx, repo.appStore.notificationPreviews, sender, msg.text)
        val n = NotificationCompat.Builder(ctx, CHANNEL_MESSAGES)
            .setSmallIcon(R.drawable.ic_stat_mesh)
            .setContentTitle(title)
            .setContentText(body)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setAutoCancel(true)
            .build()
        NotificationManagerCompat.from(ctx).notify(msg.contactId.hashCode(), n)
    }
}
