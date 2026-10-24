package chat.omcp.app

import chat.omcp.app.backup.CloudUnavailableException
import chat.omcp.app.backup.DriveAppDataBackupStore
import chat.omcp.app.backup.DriveAppDataClient
import chat.omcp.app.backup.DriveHttpException
import chat.omcp.app.backup.HttpTransport
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Drive v3 appDataFolder calls against an in-memory fake of the REST API. */
class DriveAppDataClientTest {
    private class FakeDrive : HttpTransport {
        val calls = mutableListOf<String>()
        val auth = mutableListOf<String?>()
        var file: Pair<String, ByteArray>? = null
        var expireNext = false

        override fun request(method: String, url: String, headers: Map<String, String>, body: ByteArray?): HttpTransport.Response {
            val m = headers["X-HTTP-Method-Override"] ?: method
            calls += "$m ${url.substringBefore('?')}"
            auth += headers["Authorization"]
            if (expireNext) { expireNext = false; return HttpTransport.Response(401, ByteArray(0)) }
            val f = file
            return when {
                m == "GET" && url.contains("spaces=appDataFolder") -> {
                    assertTrue(url.contains("omcp-backup-v1.cbor"))
                    ok(if (f == null) """{"files":[]}""" else """{"files":[{"id":"${f.first}","modifiedTime":"x"}]}""")
                }
                m == "POST" && url.contains("uploadType=multipart") -> {
                    val text = String(body!!, Charsets.ISO_8859_1)
                    assertTrue(text.contains(""""parents":["appDataFolder"]"""))
                    assertTrue(headers["Content-Type"]!!.startsWith("multipart/related; boundary="))
                    val blob = text.substringAfter("application/octet-stream\r\n\r\n").substringBeforeLast("\r\n--").toByteArray(Charsets.ISO_8859_1)
                    file = "f1" to blob
                    ok("""{"id":"f1"}""")
                }
                m == "PATCH" && url.contains("uploadType=media") -> { file = f!!.first to body!!; ok("""{"id":"${f.first}"}""") }
                m == "GET" && url.contains("alt=media") -> HttpTransport.Response(200, f!!.second)
                m == "DELETE" -> { file = null; HttpTransport.Response(204, ByteArray(0)) }
                else -> HttpTransport.Response(500, ByteArray(0))
            }
        }

        private fun ok(s: String) = HttpTransport.Response(200, s.toByteArray())
    }

    @Test fun uploadCreatesThenOverwritesDownloadAndDelete() {
        val api = FakeDrive()
        var n = 0
        val d = DriveAppDataClient(api, { "tok${n++}" })
        assertNull(d.download())
        val blob = byteArrayOf(0, 1, 2, 0xFF.toByte(), 13, 10)
        d.upload(blob)
        assertArrayEquals(blob, d.download())
        d.upload(byteArrayOf(9, 9))
        assertArrayEquals(byteArrayOf(9, 9), d.download())
        assertTrue(api.calls.contains("PATCH https://www.googleapis.com/upload/drive/v3/files/f1"))
        d.delete()
        assertNull(d.download())
        assertTrue(api.auth.all { it!!.startsWith("Bearer tok") })
    }

    @Test fun expiredTokenIsRefreshedOnce() {
        val api = FakeDrive().apply { expireNext = true }
        val seen = mutableListOf<String>()
        var n = 0
        val d = DriveAppDataClient(api, { "tok${n++}" }, { seen += it })
        assertNull(d.findId())
        assertEquals(listOf("tok0"), seen)
        assertEquals("Bearer tok1", api.auth.last())
    }

    @Test fun httpErrorsSurface() {
        val d = DriveAppDataClient(object : HttpTransport {
            override fun request(method: String, url: String, headers: Map<String, String>, body: ByteArray?) =
                HttpTransport.Response(403, ByteArray(0))
        }, { "t" })
        try { d.findId(); error("expected") } catch (e: DriveHttpException) { assertEquals(403, e.code) }
    }

    @Test fun storeIsUnavailableWithoutClientId() {
        val s = DriveAppDataBackupStore("") { error("must not build a client") }
        assertFalse(s.available)
        assertNull(s.download())
        assertNull(s.downloadSilently())
        try { s.upload(ByteArray(1)); error("expected") } catch (_: CloudUnavailableException) {}
        val live = DriveAppDataBackupStore("123-abc.apps.googleusercontent.com") { DriveAppDataClient(FakeDrive(), { "t" }) }
        assertTrue(live.available)
        live.upload(byteArrayOf(4))
    }
}
