package chat.omcp.app

import android.os.Bundle
import chat.omcp.app.security.ScreenSecurity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import chat.omcp.app.ui.OmcpRoot
import chat.omcp.app.ui.theme.OmcpTheme

class MainActivity : FragmentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        ScreenSecurity.secure(this)
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val app = application as OmcpApplication
        setContent {
            OmcpTheme {
                OmcpRoot(
                    repo = app.repository,
                    cloud = app.cloudBackup,
                    secureWindow = ::setSecure,
                    requestDeviceAuth = ::deviceAuth,
                )
            }
        }
    }

    /** FLAG_SECURE is always on app-wide (SS1); screens may only re-assert it, never clear it. */
    @Suppress("UNUSED_PARAMETER")
    private fun setSecure(on: Boolean) = ScreenSecurity.secure(window)

    /** Biometric or device PIN before showing the phrase again. */
    private fun deviceAuth(onSuccess: () -> Unit) {
        val prompt = BiometricPrompt(
            this, ContextCompat.getMainExecutor(this),
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) = onSuccess()
            },
        )
        prompt.authenticate(
            BiometricPrompt.PromptInfo.Builder()
                .setTitle(getString(R.string.backup_view_phrase))
                .setAllowedAuthenticators(BIOMETRIC_STRONG or DEVICE_CREDENTIAL)
                .build(),
        )
    }
}
