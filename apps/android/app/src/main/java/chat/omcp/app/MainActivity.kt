package chat.omcp.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import chat.omcp.app.ui.OmcpRoot
import chat.omcp.app.ui.theme.OmcpTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            OmcpTheme {
                OmcpRoot()
            }
        }
    }
}
