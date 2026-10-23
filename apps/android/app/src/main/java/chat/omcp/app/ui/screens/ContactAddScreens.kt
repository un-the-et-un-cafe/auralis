package chat.omcp.app.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Contactless
import androidx.compose.material.icons.filled.QrCode
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material.icons.filled.Tag
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.ui.theme.LocalOmcpColors

/**
 * Minimal Contact-add chooser (Designy §5.5).
 * Includes **My QR** so You → Profile/QR can stay deferred.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ContactAddScreen(
    onBack: () -> Unit,
    onMyQr: () -> Unit,
) {
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.contact_add_title)) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            Text(
                stringResource(R.string.contact_add_subtitle),
                style = MaterialTheme.typography.bodyMedium,
                color = LocalOmcpColors.current.ink2,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
            )
            HorizontalDivider()
            ListItem(
                headlineContent = { Text(stringResource(R.string.contact_add_scan_qr)) },
                supportingContent = { Text(stringResource(R.string.contact_add_scan_qr_desc)) },
                leadingContent = { Icon(Icons.Default.QrCodeScanner, contentDescription = null) },
                trailingContent = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null) },
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable { /* camera shell — deferred */ },
            )
            ListItem(
                headlineContent = { Text(stringResource(R.string.contact_add_my_qr)) },
                supportingContent = { Text(stringResource(R.string.contact_add_my_qr_desc)) },
                leadingContent = { Icon(Icons.Default.QrCode, contentDescription = null) },
                trailingContent = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null) },
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable(onClick = onMyQr),
            )
            ListItem(
                headlineContent = { Text(stringResource(R.string.contact_add_nfc)) },
                supportingContent = { Text(stringResource(R.string.contact_add_nfc_desc)) },
                leadingContent = { Icon(Icons.Default.Contactless, contentDescription = null) },
                trailingContent = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null) },
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable { /* NFC shell — deferred */ },
            )
            ListItem(
                headlineContent = { Text(stringResource(R.string.contact_add_peer_id)) },
                supportingContent = { Text(stringResource(R.string.contact_add_peer_id_desc)) },
                leadingContent = { Icon(Icons.Default.Tag, contentDescription = null) },
                trailingContent = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null) },
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable { /* advanced — deferred */ },
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MyQrScreen(onBack: () -> Unit) {
    val c = LocalOmcpColors.current
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.contact_add_my_qr)) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            Box(
                Modifier
                    .fillMaxWidth(0.72f)
                    .aspectRatio(1f)
                    .clip(RoundedCornerShape(16.dp))
                    .border(1.dp, c.line, RoundedCornerShape(16.dp))
                    .background(MaterialTheme.colorScheme.surface),
                contentAlignment = Alignment.Center,
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Icon(
                        Icons.Default.QrCode,
                        contentDescription = null,
                        modifier = Modifier.size(96.dp),
                        tint = c.ink2,
                    )
                    Spacer(Modifier.height(12.dp))
                    Text(
                        stringResource(R.string.my_qr_placeholder),
                        style = MaterialTheme.typography.bodySmall,
                        color = c.ink3,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.padding(horizontal = 16.dp),
                    )
                }
            }
            Spacer(Modifier.height(20.dp))
            Text("You", style = MaterialTheme.typography.titleMedium)
            Text(
                "omcp1…local-stub",
                fontFamily = FontFamily.Monospace,
                color = c.ink2,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.padding(top = 4.dp),
            )
            Text(
                stringResource(R.string.my_qr_hint),
                color = c.ink3,
                style = MaterialTheme.typography.bodySmall,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(top = 16.dp),
            )
        }
    }
}
