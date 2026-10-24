package chat.omcp.app.backup

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import com.google.android.gms.auth.api.identity.AuthorizationRequest
import com.google.android.gms.auth.api.identity.Identity
import com.google.android.gms.common.api.Scope
import com.google.android.gms.tasks.Tasks
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

/**
 * Hands a Google consent screen from a background Drive call to the visible Activity and
 * waits for the answer. [MainActivity] collects [pending], launches it, and calls [complete].
 */
class DriveConsentBroker {
    private val _pending = MutableStateFlow<PendingIntent?>(null)
    val pending: StateFlow<PendingIntent?> = _pending.asStateFlow()
    @Volatile private var waiter: CompletableFuture<Intent?>? = null

    /** Blocking (background thread): show consent, return the result Intent or null if declined. */
    fun request(intent: PendingIntent, timeoutSec: Long = 180): Intent? {
        val f = CompletableFuture<Intent?>()
        waiter = f
        _pending.value = intent
        return try {
            f.get(timeoutSec, TimeUnit.SECONDS)
        } catch (_: Exception) {
            null
        } finally {
            _pending.value = null
            waiter = null
        }
    }

    fun complete(data: Intent?) {
        _pending.value = null
        waiter?.complete(data)
    }
}

/**
 * Access tokens for `drive.appdata` through Google Identity Services' AuthorizationClient.
 * On Android the OAuth client is matched by package name + signing-certificate SHA-1 (an
 * "Android" client in Google Cloud), so no secret is embedded; [clientId] only marks the build
 * as configured. Must run off the main thread.
 */
class GoogleDriveTokenProvider(
    private val context: Context,
    private val consent: DriveConsentBroker,
) {
    private val client get() = Identity.getAuthorizationClient(context)
    private val request = AuthorizationRequest.builder()
        .setRequestedScopes(listOf(Scope(DriveAppDataClient.SCOPE)))
        .build()

    /** [interactive] = false never shows a consent screen (throws instead). */
    fun token(interactive: Boolean = true): String {
        val result = Tasks.await(client.authorize(request), 60, TimeUnit.SECONDS)
        result.accessToken?.let { return it }
        if (!interactive) throw CloudUnavailableException("Google Drive consent needed")
        val pi = result.pendingIntent ?: throw CloudUnavailableException("Google sign-in unavailable")
        val data = consent.request(pi) ?: throw CloudUnavailableException("Google Drive access was not granted")
        return client.getAuthorizationResultFromIntent(data).accessToken
            ?: throw CloudUnavailableException("Google Drive access was not granted")
    }

    /** Expired/revoked token: the next authorize() call returns a fresh one. */
    fun invalidate(@Suppress("UNUSED_PARAMETER") token: String) = Unit
}
