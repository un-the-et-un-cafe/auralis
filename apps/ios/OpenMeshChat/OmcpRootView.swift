import SwiftUI

struct OmcpRootView: View {
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        TabView {
            ChatsListView()
                .tabItem { Label(String(localized: "tab_chats"), systemImage: "bubble.left.and.bubble.right") }
            CommunitiesHomeView()
                .tabItem { Label(String(localized: "tab_communities"), systemImage: "person.3") }
            LiveTabView()
                .tabItem { Label(String(localized: "tab_live"), systemImage: "video") }
            YouView()
                .tabItem { Label(String(localized: "tab_you"), systemImage: "person.crop.circle") }
        }
        .tint(OmcpColors.current().brand)
        .environment(\.omcpColors, scheme == .dark ? .dark : .light)
    }
}

#Preview {
    OmcpRootView()
}
