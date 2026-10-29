package chat.omcp.app

import chat.omcp.app.files.TrustedReceivedPaths
import java.io.File
import org.junit.Assert.*
import org.junit.Test

class TrustedReceivedPathsTest {
    private val paths = TrustedReceivedPaths(File("/data/user/0/chat.auralis.app/files"), File("/data/data/chat.auralis.app/files"))
    @Test fun mapsOnlyExactCurrentTrustedRootAndPreservesSuffixWithoutResolvingIt() {
        val current = "/data/data/chat.auralis.app/files/received/file.bin"
        assertEquals(current, paths.map(current))
        assertEquals(current, paths.map("/data/user/0/chat.auralis.app/files/received/file.bin"))
        assertEquals("/data/data/chat.auralis.app/files/received/sub/file.bin", paths.map("/data/user/0/chat.auralis.app/files/received/sub/file.bin"))
    }
    @Test fun rejectsTraversalLookalikesForeignContainersAndEmptySuffix() {
        for (path in listOf(
            "/data/user/0/chat.auralis.app/files-other/received/file.bin",
            "/data/user/0/other.app/files/received/file.bin",
            "/data/user/10/chat.auralis.app/files/received/file.bin",
            "/data/user/0/chat.auralis.app/files/received/",
            "/data/user/0/chat.auralis.app/files/received/../file.bin",
            "/data/user/0/chat.auralis.app/files/received/sub/../../file.bin",
            "/data/user/0/chat.auralis.app/files/received/./file.bin",
            "/data/user/0/chat.auralis.app/files/received//file.bin",
            "data/user/0/chat.auralis.app/files/received/file.bin",
            "/data/user/0/chat.auralis.app/files/received/file\u0000.bin",
        )) assertNull(path, paths.map(path))
    }
}
