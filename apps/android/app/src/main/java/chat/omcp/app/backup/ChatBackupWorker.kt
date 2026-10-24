package chat.omcp.app.backup

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import chat.omcp.app.OmcpApplication
import java.util.concurrent.TimeUnit

/**
 * Hourly chat-backup pass (text at most hourly, Wi-Fi only by default) followed by the daily media
 * pass when it is on (CB5). The Rust core decides what is due; this only provides the schedule.
 */
class ChatBackupWorker(ctx: Context, params: WorkerParameters) : CoroutineWorker(ctx, params) {
    override suspend fun doWork(): Result {
        val app = applicationContext as OmcpApplication
        val sync = app.repository.chatBackup ?: return Result.success()
        if (!app.repository.onboarded.value) return Result.success()
        val wifi = unmetered(applicationContext)
        return runCatching {
            sync.runText(onWifi = wifi)
            val prefs = app.repository.core.mediaBackupSettings()
            sync.runMedia(onWifi = wifi || prefs.mobileData)
        }.fold({ Result.success() }, { if (it is CloudUnavailableException) Result.success() else Result.retry() })
    }

    companion object {
        private const val WORK = "omcp-chat-backup"

        fun unmetered(ctx: Context): Boolean {
            val cm = ctx.getSystemService(ConnectivityManager::class.java) ?: return false
            val caps = cm.getNetworkCapabilities(cm.activeNetwork) ?: return false
            return caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
        }

        fun schedule(ctx: Context) {
            val req = PeriodicWorkRequestBuilder<ChatBackupWorker>(1, TimeUnit.HOURS)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .build()
            WorkManager.getInstance(ctx).enqueueUniquePeriodicWork(WORK, ExistingPeriodicWorkPolicy.KEEP, req)
        }
    }
}
