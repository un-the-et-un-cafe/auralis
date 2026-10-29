package chat.omcp.app

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import chat.omcp.app.ui.components.LocalFileRepairAction
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class LocalFileRepairActionTest {
    @get:Rule val compose = createComposeRule()
    @Test fun repairRequiresConfirmationAndCarriesCapturedGenerationAcrossRecomposition() {
        val token = mutableStateOf("old-generation")
        val requests = mutableListOf<Pair<String, String>>()
        compose.setContent { MaterialTheme { LocalFileRepairAction("transfer", token.value) { id, value -> requests += id to value } } }
        compose.runOnIdle { assertTrue(requests.isEmpty()) }
        compose.onNodeWithTag("repair-local-open").performClick()
        compose.runOnIdle { assertTrue(requests.isEmpty()) }
        compose.onNodeWithTag("repair-local-dismiss").performClick()
        compose.onNodeWithTag("repair-local-confirm").assertDoesNotExist()
        compose.runOnIdle { assertTrue(requests.isEmpty()) }
        compose.onNodeWithTag("repair-local-open").performClick()
        compose.runOnIdle { token.value = "new-generation" }
        compose.onNodeWithTag("repair-local-confirm").performClick()
        compose.runOnIdle { assertEquals(listOf("transfer" to "old-generation"), requests) }
        compose.onNodeWithTag("repair-local-confirm").assertDoesNotExist()
    }
}
