package chat.omcp.app

import androidx.compose.ui.graphics.Color
import chat.omcp.app.ui.theme.OmcpDark
import chat.omcp.app.ui.theme.OmcpLight
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow

/** Contrast and localisation are part of the shipped design, not screenshot-only checks. */
class AuralisDesignTest {
    private fun luminance(color: Color): Double {
        fun linear(value: Float): Double = if (value <= 0.04045f) value / 12.92 else ((value + 0.055) / 1.055).pow(2.4)
        return 0.2126 * linear(color.red) + 0.7152 * linear(color.green) + 0.0722 * linear(color.blue)
    }
    private fun contrast(foreground: Color, background: Color): Double {
        val a = luminance(foreground)
        val b = luminance(background)
        return (max(a, b) + 0.05) / (min(a, b) + 0.05)
    }

    @Test fun `small text and delivery labels meet AA in both themes`() {
        val pairs = listOf(
            OmcpLight.Ink to OmcpLight.Bg, OmcpLight.Ink2 to OmcpLight.Surface,
            OmcpLight.Ink3 to OmcpLight.Surface2, OmcpLight.Brand to Color.White,
            OmcpLight.Brand to OmcpLight.BrandSoft, OmcpLight.Ok to OmcpLight.OkSoft,
            OmcpLight.Warn to OmcpLight.WarnSoft, OmcpLight.Danger to OmcpLight.DangerSoft,
            OmcpLight.PathGateway to OmcpLight.PathGatewaySoft, OmcpLight.PathMixed to OmcpLight.PathMixedSoft,
            OmcpDark.Ink to OmcpDark.Bg, OmcpDark.Ink2 to OmcpDark.Surface,
            OmcpDark.Ink3 to OmcpDark.Surface2, OmcpDark.Brand to OmcpDark.Bg,
            OmcpDark.Brand to OmcpDark.BrandSoft, OmcpDark.Ok to OmcpDark.OkSoft,
            OmcpDark.Warn to OmcpDark.WarnSoft, OmcpDark.Danger to OmcpDark.DangerSoft,
            OmcpDark.PathGateway to OmcpDark.PathGatewaySoft, OmcpDark.PathMixed to OmcpDark.PathMixedSoft,
        )
        pairs.forEach { (foreground, background) ->
            assertTrue("Contrast must be at least 4.5:1: $foreground on $background", contrast(foreground, background) >= 4.5)
        }
    }

    private fun strings(file: File): Map<String, String> {
        val doc = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(file)
        val elements = doc.getElementsByTagName("string")
        return (0 until elements.length).associate { index ->
            val element = elements.item(index) as org.w3c.dom.Element
            element.getAttribute("name") to element.textContent
        }
    }

    @Test fun `every new Auralis string has a French translation with the same placeholders`() {
        val placeholders = Regex("%[0-9]+\\$[ds]")
        listOf("auralis.xml", "auralis_settings.xml").forEach { name ->
            val en = strings(File("src/main/res/values/$name"))
            val fr = strings(File("src/main/res/values-fr/$name"))
            assertEquals(en.keys, fr.keys)
            en.forEach { (key, value) ->
                assertTrue("$key must not be empty", value.isNotBlank() && fr.getValue(key).isNotBlank())
                assertEquals("$key placeholders", placeholders.findAll(value).map { it.value }.toList(), placeholders.findAll(fr.getValue(key)).map { it.value }.toList())
            }
        }
    }
}
