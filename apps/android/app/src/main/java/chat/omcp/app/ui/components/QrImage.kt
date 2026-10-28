package chat.omcp.app.ui.components

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.res.stringResource
import chat.omcp.app.R
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel

/** Black-on-white QR (even in dark mode), quiet zone 4, ECC H (DESIGN_SPEC §5.21). */
fun qrBitmap(payload: String, size: Int = 512): Bitmap {
    val m = QRCodeWriter().encode(
        payload, BarcodeFormat.QR_CODE, size, size,
        mapOf(EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.H, EncodeHintType.MARGIN to 4),
    )
    val px = IntArray(size * size) { i -> if (m[i % size, i / size]) 0xFF000000.toInt() else 0xFFFFFFFF.toInt() }
    return Bitmap.createBitmap(px, size, size, Bitmap.Config.ARGB_8888)
}

@Composable
fun QrImage(payload: String, modifier: Modifier = Modifier) {
    val label = stringResource(R.string.auralis_qr_a11y)
    val bmp = remember(payload) { qrBitmap(payload).asImageBitmap() }
    Image(
        bmp, null,
        filterQuality = FilterQuality.None,
        modifier = modifier
            .fillMaxWidth(0.75f)
            .aspectRatio(1f)
            .background(Color.White)
            .padding(4.dp)
            .semantics { contentDescription = label },
    )
}
