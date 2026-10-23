import SwiftUI

/// Jakob: keep Communities + Live tabs visible with Coming in v0.2 empty states.
struct CommunitiesHomeView: View {
    var body: some View {
        NavigationStack {
            EmptyStateView(
                title: String(localized: "coming_v02_title"),
                bodyText: String(localized: "communities_empty_body")
            )
            .navigationTitle(String(localized: "tab_communities"))
        }
    }
}

struct LiveTabView: View {
    var body: some View {
        NavigationStack {
            EmptyStateView(
                title: String(localized: "coming_v02_title"),
                bodyText: String(localized: "live_empty_body")
            )
            .navigationTitle(String(localized: "tab_live"))
        }
    }
}
