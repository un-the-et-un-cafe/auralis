package chat.omcp.app.ui.debug

import chat.omcp.app.BuildConfig

/**
 * Designy lock-in: sample chat rows are **not** in release/default UI.
 * Empty-state only until beta has real contacts.
 *
 * Flip [BuildConfig.SHOW_SAMPLE_CHATS_FOR_DESIGN_REVIEW] (see app/build.gradle.kts)
 * for design-review builds that need placeholder rows. Off by default.
 */
object DesignReviewFlags {
    val showSampleChatsForDesignReview: Boolean
        get() = BuildConfig.SHOW_SAMPLE_CHATS_FOR_DESIGN_REVIEW
}
