package chat.omcp.app.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

data class OmcpExtraColors(
    val ink2: Color,
    val ink3: Color,
    val surface2: Color,
    val line: Color,
    val brandSoft: Color,
    val ok: Color,
    val okSoft: Color,
    val warn: Color,
    val warnSoft: Color,
    val danger: Color,
    val dangerSoft: Color,
    val pathMesh: Color,
    val pathMeshSoft: Color,
    val pathGateway: Color,
    val pathGatewaySoft: Color,
    val pathMixed: Color,
    val pathMixedSoft: Color,
    val pathQueue: Color,
    val pathQueueSoft: Color,
)

val LocalOmcpColors = staticCompositionLocalOf {
    OmcpExtraColors(
        ink2 = OmcpLight.Ink2,
        ink3 = OmcpLight.Ink3,
        surface2 = OmcpLight.Surface2,
        line = OmcpLight.Line,
        brandSoft = OmcpLight.BrandSoft,
        ok = OmcpLight.Ok,
        okSoft = OmcpLight.OkSoft,
        warn = OmcpLight.Warn,
        warnSoft = OmcpLight.WarnSoft,
        danger = OmcpLight.Danger,
        dangerSoft = OmcpLight.DangerSoft,
        pathMesh = OmcpLight.PathMesh,
        pathMeshSoft = OmcpLight.PathMeshSoft,
        pathGateway = OmcpLight.PathGateway,
        pathGatewaySoft = OmcpLight.PathGatewaySoft,
        pathMixed = OmcpLight.PathMixed,
        pathMixedSoft = OmcpLight.PathMixedSoft,
        pathQueue = OmcpLight.PathQueue,
        pathQueueSoft = OmcpLight.PathQueueSoft,
    )
}

private val LightScheme = lightColorScheme(
    primary = OmcpLight.Brand,
    onPrimary = Color.White,
    primaryContainer = OmcpLight.BrandSoft,
    onPrimaryContainer = OmcpLight.Brand,
    background = OmcpLight.Bg,
    onBackground = OmcpLight.Ink,
    surface = OmcpLight.Surface,
    onSurface = OmcpLight.Ink,
    surfaceVariant = OmcpLight.Surface2,
    onSurfaceVariant = OmcpLight.Ink2,
    outline = OmcpLight.Line,
    error = OmcpLight.Danger,
    onError = Color.White,
)

private val DarkScheme = darkColorScheme(
    primary = OmcpDark.Brand,
    onPrimary = Color(0xFF062624),
    primaryContainer = OmcpDark.BrandSoft,
    onPrimaryContainer = OmcpDark.Brand,
    background = OmcpDark.Bg,
    onBackground = OmcpDark.Ink,
    surface = OmcpDark.Surface,
    onSurface = OmcpDark.Ink,
    surfaceVariant = OmcpDark.Surface2,
    onSurfaceVariant = OmcpDark.Ink2,
    outline = OmcpDark.Line,
    error = OmcpDark.Danger,
    onError = Color(0xFF3A1820),
)

private val LightExtra = OmcpExtraColors(
    ink2 = OmcpLight.Ink2,
    ink3 = OmcpLight.Ink3,
    surface2 = OmcpLight.Surface2,
    line = OmcpLight.Line,
    brandSoft = OmcpLight.BrandSoft,
    ok = OmcpLight.Ok,
    okSoft = OmcpLight.OkSoft,
    warn = OmcpLight.Warn,
    warnSoft = OmcpLight.WarnSoft,
    danger = OmcpLight.Danger,
    dangerSoft = OmcpLight.DangerSoft,
    pathMesh = OmcpLight.PathMesh,
    pathMeshSoft = OmcpLight.PathMeshSoft,
    pathGateway = OmcpLight.PathGateway,
    pathGatewaySoft = OmcpLight.PathGatewaySoft,
    pathMixed = OmcpLight.PathMixed,
    pathMixedSoft = OmcpLight.PathMixedSoft,
    pathQueue = OmcpLight.PathQueue,
    pathQueueSoft = OmcpLight.PathQueueSoft,
)

private val DarkExtra = OmcpExtraColors(
    ink2 = OmcpDark.Ink2,
    ink3 = OmcpDark.Ink3,
    surface2 = OmcpDark.Surface2,
    line = OmcpDark.Line,
    brandSoft = OmcpDark.BrandSoft,
    ok = OmcpDark.Ok,
    okSoft = OmcpDark.OkSoft,
    warn = OmcpDark.Warn,
    warnSoft = OmcpDark.WarnSoft,
    danger = OmcpDark.Danger,
    dangerSoft = OmcpDark.DangerSoft,
    pathMesh = OmcpDark.PathMesh,
    pathMeshSoft = OmcpDark.PathMeshSoft,
    pathGateway = OmcpDark.PathGateway,
    pathGatewaySoft = OmcpDark.PathGatewaySoft,
    pathMixed = OmcpDark.PathMixed,
    pathMixedSoft = OmcpDark.PathMixedSoft,
    pathQueue = OmcpDark.PathQueue,
    pathQueueSoft = OmcpDark.PathQueueSoft,
)

@Composable
fun OmcpTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    val scheme = if (darkTheme) DarkScheme else LightScheme
    val extra = if (darkTheme) DarkExtra else LightExtra
    CompositionLocalProvider(LocalOmcpColors provides extra) {
        MaterialTheme(colorScheme = scheme, content = content)
    }
}
