package chat.omcp.app.mesh

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build

/** Android 12+ "Nearby devices" runtime permissions (SCAN is neverForLocation). */
object MeshPermissions {
    val BLE = arrayOf(
        Manifest.permission.BLUETOOTH_SCAN,
        Manifest.permission.BLUETOOTH_CONNECT,
        Manifest.permission.BLUETOOTH_ADVERTISE,
    )

    fun hasBle(ctx: Context) = BLE.all { ctx.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED }

    /** Notification consent is independent of BLE and does not exist on Android 12/12L. */
    fun runtimePermissions(sdkInt: Int = Build.VERSION.SDK_INT): Array<String> =
        if (sdkInt >= Build.VERSION_CODES.TIRAMISU) BLE + Manifest.permission.POST_NOTIFICATIONS
        else BLE.copyOf()
}
