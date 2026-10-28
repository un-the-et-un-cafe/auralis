package chat.omcp.app.mesh

import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import chat.omcp.app.MainActivity
import chat.omcp.app.OmcpApplication
import chat.omcp.app.R

/** Foreground service for mesh participation (PROTOCOL §13.3), persistent notification. */
class MeshService : Service() {
    private var transport: BleMeshTransport? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopSelf()
            return START_NOT_STICKY
        }
        // A sticky restart can follow a permission revocation. Android 14+ validates the
        // connectedDevice prerequisites in startForeground(), not only when BLE is used.
        if (!MeshPermissions.hasBle(this)) {
            stopSelf()
            return START_NOT_STICKY
        }
        MeshNotifications.ensureChannels(this)
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        val n = NotificationCompat.Builder(this, MeshNotifications.CHANNEL_MESH)
            .setSmallIcon(R.drawable.ic_stat_mesh)
            .setContentTitle(getString(R.string.mesh_fg_title))
            .setContentText(getString(R.string.mesh_fg_body))
            .setOngoing(true)
            .setContentIntent(open)
            .build()
        try {
            ServiceCompat.startForeground(this, 1, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
        } catch (_: SecurityException) {
            // The grant may have changed between the check above and the framework call.
            stopSelf()
            return START_NOT_STICKY
        }
        val repo = (application as OmcpApplication).repository
        if (transport == null && repo.core.identity() != null) {
            transport = BleMeshTransport(this, repo).also { it.start() }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        transport?.stop()
        transport = null
        super.onDestroy()
    }

    companion object {
        private const val ACTION_STOP = "chat.omcp.app.STOP_MESH"
        fun start(ctx: Context) = ctx.startForegroundService(Intent(ctx, MeshService::class.java))
        fun stop(ctx: Context) = ctx.startService(Intent(ctx, MeshService::class.java).setAction(ACTION_STOP))
    }
}
