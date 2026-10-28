import SwiftUI

/// Planned capabilities remain discoverable from Mesh, never presented as working channels.
struct CommunitiesHomeView: View {
    var body: some View {
        EmptyStateView(title: L("coming_v02_title"), bodyText: L("communities_empty_body"), icon: "person.3")
            .navigationTitle(L("tab_communities"))
    }
}

struct LiveTabView: View {
    var body: some View {
        EmptyStateView(title: L("coming_v02_title"), bodyText: L("live_empty_body"), icon: "video")
            .navigationTitle(L("tab_live"))
    }
}
