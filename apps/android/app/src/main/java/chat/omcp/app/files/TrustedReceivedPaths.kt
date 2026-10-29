package chat.omcp.app.files

import java.io.File

/** Only the app wrapper supplies these current OS storage anchors; never a persisted path. */
internal class TrustedReceivedPaths(trustedLexicalRoot: File, physicalRoot: File) {
    private val lexical = trustedLexicalRoot.absolutePath.trimEnd('/') + "/received/"
    private val physical = physicalRoot.absolutePath.trimEnd('/') + "/received/"

    /** Map an exact legacy OS-root prefix, preserving suffix for subsequent no-follow checks. */
    fun map(path: String): String? {
        if (path.contains('\u0000')) return null
        val suffix = when {
            path.startsWith(physical) -> path.removePrefix(physical)
            path.startsWith(lexical) -> path.removePrefix(lexical)
            else -> return null
        }
        if (suffix.isEmpty() || suffix.split('/').any { it.isEmpty() || it == "." || it == ".." }) return null
        return physical + suffix
    }
}
