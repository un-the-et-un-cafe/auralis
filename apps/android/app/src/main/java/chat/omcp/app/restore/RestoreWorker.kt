package chat.omcp.app.restore

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import androidx.core.app.NotificationCompat
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import chat.omcp.app.OmcpApplication
import chat.omcp.app.R

/**
 * Expedited WorkManager job that finishes a restore while the user may leave the app
 * (DESIGN_SPEC §5.86), with an ongoing notification "Restoring Auralis · step n of 3".
 */
class RestoreWorker(ctx: Context, params: WorkerParameters) : CoroutineWorker(ctx, params) {
    override suspend fun doWork(): Result {
        val app = applicationContext as OmcpApplication
        val nm = applicationContext.getSystemService(NotificationManager::class.java)
        ensureChannel(applicationContext)
        val r = app.restore.run { stage -> runCatching { nm.notify(NOTIF_ID, notification(applicationContext, stage.ordinal + 1)) } }
        nm.cancel(NOTIF_ID)
        if (r is RestoreProgress.Done) {
            runCatching {
                nm.notify(
                    NOTIF_DONE_ID,
                    NotificationCompat.Builder(applicationContext, CHANNEL)
                        .setSmallIcon(R.drawable.ic_stat_mesh)
                        .setContentTitle(applicationContext.getString(R.string.restore_notif_done))
                        .setAutoCancel(true)
                        .build(),
                )
            }
        }
        return if (r is RestoreProgress.Done) Result.success() else Result.failure()
    }

    override suspend fun getForegroundInfo(): ForegroundInfo {
        ensureChannel(applicationContext)
        return ForegroundInfo(NOTIF_ID, notification(applicationContext, 1))
    }

    companion object {
        const val CHANNEL = "restore"
        const val NOTIF_ID = 4201
        const val NOTIF_DONE_ID = 4202
        private const val WORK = "hopla-restore"

        fun ensureChannel(ctx: Context) {
            ctx.getSystemService(NotificationManager::class.java).createNotificationChannel(
                NotificationChannel(CHANNEL, ctx.getString(R.string.restore_notif_channel), NotificationManager.IMPORTANCE_LOW),
            )
        }

        fun notification(ctx: Context, step: Int) = NotificationCompat.Builder(ctx, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_mesh)
            .setContentTitle(ctx.getString(R.string.restore_notif_title))
            .setContentText(ctx.getString(R.string.restore_notif_step, step, 3))
            .setProgress(3, step, false)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .build()

        fun runner(ctx: Context) = RestoreRunner {
            val req = OneTimeWorkRequestBuilder<RestoreWorker>()
                .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
                .build()
            WorkManager.getInstance(ctx).enqueueUniqueWork(WORK, ExistingWorkPolicy.REPLACE, req)
        }
    }
}
