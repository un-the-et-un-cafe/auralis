import Foundation

/// Designy lock-in: sample chat rows are **not** in release/default UI.
/// Empty-state only until beta has real contacts.
///
/// Set [showSampleChatsForDesignReview] to `true` only for design-review builds
/// that need placeholder rows. Off by default.
enum DesignReviewFlags {
    static let showSampleChatsForDesignReview = false
}
