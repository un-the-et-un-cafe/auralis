package chat.omcp.app.restore

import chat.omcp.app.backup.CloudUnavailableException
import chat.omcp.app.backup.DriveHttpException
import chat.omcp.app.core.BackupTooNewException
import java.util.concurrent.ExecutionException

/** Restore error screens (DESIGN_SPEC §5.88). Shown only after the user chose Restore. */
enum class RestoreErrorKind { Offline, PermissionRevoked, QuotaFull, Corrupted, TooNew }

object RestoreErrors {
    /** Maps a Drive / core failure to the screen to show. Backups never use mesh or gateways. */
    fun classify(t: Throwable): RestoreErrorKind {
        val e = if (t is ExecutionException && t.cause != null) t.cause!! else t
        return when (e) {
            is BackupTooNewException -> RestoreErrorKind.TooNew
            is java.io.IOException -> RestoreErrorKind.Offline
            is DriveHttpException -> when (e.code) {
                401, 403 -> RestoreErrorKind.PermissionRevoked
                else -> RestoreErrorKind.Offline
            }
            is CloudUnavailableException ->
                if (e.message.orEmpty().contains("NETWORK_ERROR") || e.message.orEmpty().contains("timed out")) {
                    RestoreErrorKind.Offline
                } else {
                    RestoreErrorKind.PermissionRevoked
                }
            else -> RestoreErrorKind.Corrupted
        }
    }
}
