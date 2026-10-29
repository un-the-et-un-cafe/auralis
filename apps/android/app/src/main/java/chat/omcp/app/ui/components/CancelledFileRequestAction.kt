package chat.omcp.app.ui.components

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import chat.omcp.app.R

/** Confirmation captures the immutable eligibility action, not merely the message ID. No network request on appearance/retry. */
@Composable
internal fun CancelledFileRequestAction(transferId: String, actionId: String, onRequest: (String, String) -> Unit) {
    var confirmation by remember { mutableStateOf<Pair<String, String>?>(null) }
    TextButton(onClick = { confirmation = transferId to actionId }, modifier = Modifier.testTag("request-again-open")) {
        Text(stringResource(R.string.file_request_again))
    }
    confirmation?.let { (id, capturedAction) ->
        AlertDialog(
            onDismissRequest = { confirmation = null },
            title = { Text(stringResource(R.string.file_request_again)) },
            text = { Text(stringResource(R.string.file_request_again_confirm)) },
            confirmButton = { TextButton(onClick = {
                confirmation = null
                onRequest(id, capturedAction)
            }, modifier = Modifier.testTag("request-again-confirm")) { Text(stringResource(R.string.file_request_again)) } },
            dismissButton = { TextButton(onClick = { confirmation = null }, modifier = Modifier.testTag("request-again-dismiss")) {
                Text(stringResource(android.R.string.cancel))
            } },
        )
    }
}
