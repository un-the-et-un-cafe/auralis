package chat.omcp.app

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import chat.omcp.app.ui.components.CancelledFileRequestAction
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class CancelledFileRequestActionTest {
    @get:Rule val compose = createComposeRule()
    @Test fun requestRequiresConfirmationAndCarriesCapturedGenerationAcrossRecomposition() {
        val token = mutableStateOf("old-generation")
        val requests = mutableListOf<Pair<String, String>>()
        compose.setContent { MaterialTheme { CancelledFileRequestAction("transfer", token.value) { id, value -> requests += id to value } } }
        compose.runOnIdle { assertTrue(requests.isEmpty()) }
        compose.onNodeWithTag("request-again-open").performClick()
        compose.runOnIdle { assertTrue(requests.isEmpty()) }
        compose.onNodeWithTag("request-again-dismiss").performClick()
        compose.onNodeWithTag("request-again-confirm").assertDoesNotExist()
        compose.runOnIdle { assertTrue(requests.isEmpty()) }
        compose.onNodeWithTag("request-again-open").performClick()
        compose.runOnIdle { token.value = "new-generation" }
        compose.onNodeWithTag("request-again-confirm").performClick()
        compose.runOnIdle { assertEquals(listOf("transfer" to "old-generation"), requests) }
        compose.onNodeWithTag("request-again-confirm").assertDoesNotExist()
    }
}
