package chat.omcp.app.backup

import java.io.ByteArrayOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

/** Minimal HTTP seam so the Drive REST calls are unit-tested without a network. */
interface HttpTransport {
    class Response(val code: Int, val body: ByteArray) {
        val text: String get() = body.toString(Charsets.UTF_8)
    }
    fun request(method: String, url: String, headers: Map<String, String>, body: ByteArray?): Response
}

class DriveHttpException(val code: Int, msg: String) : Exception(msg)

/**
 * Google Drive v3 REST against the hidden, app-private `appDataFolder` (scope
 * `https://www.googleapis.com/auth/drive.appdata` only: the app can't see the user's other files
 * and the user can't browse this one; they can delete it under Drive › Settings › Manage apps).
 *
 * Exactly one file, [FILE_NAME], holding the **already-encrypted** CBOR backup blob from Rust.
 * [token] returns an OAuth access token (may block for user consent; never call on main).
 * On 401 the token is refreshed once via [invalidate].
 */
class DriveAppDataClient(
    private val http: HttpTransport,
    private val token: () -> String,
    private val invalidate: (String) -> Unit = {},
) {
    companion object {
        const val SCOPE = "https://www.googleapis.com/auth/drive.appdata"
        const val FILE_NAME = "omcp-backup-v1.cbor"
        private const val API = "https://www.googleapis.com/drive/v3/files"
        private const val UPLOAD = "https://www.googleapis.com/upload/drive/v3/files"
        private const val BOUNDARY = "omcp-backup-boundary"

        private val ID = Regex("\"id\"\\s*:\\s*\"([A-Za-z0-9_-]+)\"")
        private val NAME = Regex("\"name\"\\s*:\\s*\"([A-Za-z0-9._-]+)\"")
        private val NEXT = Regex("\"nextPageToken\"\\s*:\\s*\"([^\"]+)\"")
        fun firstId(json: String): String? = ID.find(json)?.groupValues?.get(1)
    }

    /** Id of our backup file in appDataFolder, or null. */
    fun findId(): String? = findId(FILE_NAME)

    /** Id of the appDataFolder file called [name], or null. */
    fun findId(name: String): String? {
        val q = URLEncoder.encode("name = '$name' and trashed = false", "UTF-8")
        val r = call("GET", "$API?spaces=appDataFolder&q=$q&fields=files(id,modifiedTime)&pageSize=10", emptyMap(), null)
        return firstId(r.text)
    }

    /** Names of appDataFolder files starting with [prefix] (chat-backup blobs). */
    fun listNames(prefix: String): List<String> {
        val q = URLEncoder.encode("name contains '$prefix' and trashed = false", "UTF-8")
        val names = mutableListOf<String>()
        var page: String? = null
        do {
            val tok = page?.let { "&pageToken=" + URLEncoder.encode(it, "UTF-8") } ?: ""
            val t = call("GET", "$API?spaces=appDataFolder&q=$q&fields=nextPageToken,files(name)&pageSize=1000$tok", emptyMap(), null).text
            NAME.findAll(t).mapTo(names) { it.groupValues[1] }
            page = NEXT.find(t)?.groupValues?.get(1)
        } while (page != null)
        return names.filter { it.startsWith(prefix) }.distinct()
    }

    /** Create or overwrite the backup file. */
    fun upload(blob: ByteArray) = upload(FILE_NAME, blob)

    /** Create or overwrite the appDataFolder file [name]. */
    fun upload(name: String, blob: ByteArray) {
        val id = findId(name)
        if (id != null) {
            // HttpURLConnection has no PATCH; Google APIs honour the override header.
            call(
                "POST", "$UPLOAD/$id?uploadType=media",
                mapOf("X-HTTP-Method-Override" to "PATCH", "Content-Type" to "application/octet-stream"), blob,
            )
            return
        }
        val meta = """{"name":"$name","parents":["appDataFolder"],"mimeType":"application/octet-stream"}"""
        val body = ByteArrayOutputStream().apply {
            write("--$BOUNDARY\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n$meta\r\n".toByteArray())
            write("--$BOUNDARY\r\nContent-Type: application/octet-stream\r\n\r\n".toByteArray())
            write(blob)
            write("\r\n--$BOUNDARY--\r\n".toByteArray())
        }.toByteArray()
        call("POST", "$UPLOAD?uploadType=multipart&fields=id", mapOf("Content-Type" to "multipart/related; boundary=$BOUNDARY"), body)
    }

    fun download(): ByteArray? = download(FILE_NAME)

    fun download(name: String): ByteArray? {
        val id = findId(name) ?: return null
        return call("GET", "$API/$id?alt=media", emptyMap(), null).body
    }

    /** Drive `about.storageQuota`: true when usage has reached the limit (unlimited → false). */
    fun storageFull(): Boolean {
        val t = call("GET", "https://www.googleapis.com/drive/v3/about?fields=storageQuota", emptyMap(), null).text
        val limit = Regex("\"limit\"\\s*:\\s*\"(\\d+)\"").find(t)?.groupValues?.get(1)?.toLongOrNull() ?: return false
        val usage = Regex("\"usage\"\\s*:\\s*\"(\\d+)\"").find(t)?.groupValues?.get(1)?.toLongOrNull() ?: return false
        return usage >= limit - 64 * 1024
    }

    fun delete() = delete(FILE_NAME)

    fun delete(name: String) {
        val id = findId(name) ?: return
        call("DELETE", "$API/$id", emptyMap(), null)
    }

    private fun call(method: String, url: String, headers: Map<String, String>, body: ByteArray?): HttpTransport.Response {
        var t = token()
        var r = http.request(method, url, headers + ("Authorization" to "Bearer $t"), body)
        if (r.code == 401) {
            invalidate(t)
            t = token()
            r = http.request(method, url, headers + ("Authorization" to "Bearer $t"), body)
        }
        if (r.code !in 200..299) throw DriveHttpException(r.code, "Google Drive error ${r.code}")
        return r
    }
}

/** java.net transport (no extra dependency). */
class UrlConnectionTransport(private val timeoutMs: Int = 30_000) : HttpTransport {
    override fun request(method: String, url: String, headers: Map<String, String>, body: ByteArray?): HttpTransport.Response {
        val c = URL(url).openConnection() as HttpURLConnection
        try {
            c.requestMethod = method
            c.connectTimeout = timeoutMs
            c.readTimeout = timeoutMs
            headers.forEach { (k, v) -> c.setRequestProperty(k, v) }
            if (body != null) {
                c.doOutput = true
                c.setFixedLengthStreamingMode(body.size)
                c.outputStream.use { it.write(body) }
            }
            val code = c.responseCode
            val stream = if (code in 200..299) c.inputStream else c.errorStream
            return HttpTransport.Response(code, stream?.use { it.readBytes() } ?: ByteArray(0))
        } finally {
            c.disconnect()
        }
    }
}
