package chat.omcp.app

import android.Manifest
import chat.omcp.app.mesh.MeshPermissions
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.*
import org.junit.Test
import org.w3c.dom.Element

class AndroidPlatformPolicyTest {
    @Test fun `Android 12 and 12L request only Nearby devices permissions`() {
        for (sdk in listOf(31, 32)) {
            assertArrayEquals(MeshPermissions.BLE, MeshPermissions.runtimePermissions(sdk))
        }
    }

    @Test fun `Android 13 and later request notification consent independently of BLE`() {
        for (sdk in listOf(33, 34, 35)) {
            assertEquals(MeshPermissions.BLE.toSet() + Manifest.permission.POST_NOTIFICATIONS,
                MeshPermissions.runtimePermissions(sdk).toSet())
        }
    }

    @Test fun `OS backup and device migration exclude all app storage domains`() {
        val parser = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }.newDocumentBuilder()
        val manifest = parser.parse(File("src/main/AndroidManifest.xml"))
        val app = manifest.getElementsByTagName("application").item(0) as Element
        val android = "http://schemas.android.com/apk/res/android"
        assertEquals("false", app.getAttributeNS(android, "allowBackup"))
        assertEquals("@xml/data_extraction_rules", app.getAttributeNS(android, "dataExtractionRules"))
        val policy = parser.parse(File("src/main/res/xml/data_extraction_rules.xml"))
        val domains = setOf("root", "file", "database", "sharedpref", "external",
            "device_root", "device_file", "device_database", "device_sharedpref")
        for (mode in listOf("cloud-backup", "device-transfer")) {
            val section = policy.getElementsByTagName(mode).item(0) as Element
            assertEquals(0, section.getElementsByTagName("include").length)
            val exclusions = section.getElementsByTagName("exclude")
            val actual = (0 until exclusions.length).map { i ->
                val exclusion = exclusions.item(i) as Element
                assertEquals(".", exclusion.getAttribute("path"))
                exclusion.getAttribute("domain")
            }.toSet()
            assertEquals(domains, actual)
        }
    }
}
