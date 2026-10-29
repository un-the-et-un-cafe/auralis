package chat.omcp.app.ui.components

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import chat.omcp.app.R

/** Confirmation captures the immutable eligibility action, not merely the message ID. No repair on appearance/retry. */
@Composable
internal fun LocalFileRepairAction(transferId: String, actionId: String, onRepair: (String, String) -> Unit) {
    var confirmation by remember { mutableStateOf<Pair<String, String>?>(null) }
    TextButton(onClick = { confirmation = transferId to actionId }, modifier = Modifier.testTag("repair-local-open")) {
        Text(stringResource(R.string.file_repair_local))
    }
    confirmation?.let { (id, capturedAction) ->
        AlertDialog(
            onDismissRequest = { confirmation = null },
            title = { Text(stringResource(R.string.file_repair_local)) },
            text = { Text(stringResource(R.string.file_repair_confirm)) },
            confirmButton = { TextButton(onClick = {
                confirmation = null
                onRepair(id, capturedAction)
            }, modifier = Modifier.testTag("repair-local-confirm")) { Text(stringResource(R.string.file_repair_local)) } },
            dismissButton = { TextButton(onClick = { confirmation = null }, modifier = Modifier.testTag("repair-local-dismiss")) {
                Text(stringResource(android.R.string.cancel))
            } },
        )
    }
}
