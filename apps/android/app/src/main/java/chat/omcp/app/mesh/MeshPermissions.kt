package chat.omcp.app.mesh

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager

/** Android 12+ "Nearby devices" runtime permissions (SCAN is neverForLocation). */
object MeshPermissions {
    val BLE = arrayOf(
        Manifest.permission.BLUETOOTH_SCAN,
        Manifest.permission.BLUETOOTH_CONNECT,
        Manifest.permission.BLUETOOTH_ADVERTISE,
    )

    fun hasBle(ctx: Context) = BLE.all { ctx.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED }
}
