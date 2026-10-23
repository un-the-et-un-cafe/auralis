import SwiftUI

struct OmcpRootView: View {
    @Environment(\.colorScheme) private var scheme
    /// Optional first-launch tip; after dismiss, Gateway owns the BLE/companion callout.
    @AppStorage("omcp.iosBleTipDismissed") private var iosBleTipDismissed = false
    @State private var showIosBleTip = false

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
        .onAppear {
            if !iosBleTipDismissed {
                showIosBleTip = true
            }
        }
        .alert(
            String(localized: "ios_ble_tip_title"),
            isPresented: $showIosBleTip
        ) {
            Button(String(localized: "ios_ble_tip_dismiss")) {
                iosBleTipDismissed = true
                showIosBleTip = false
            }
        } message: {
            Text(String(localized: "ios_ble_tip_body"))
        }
    }
}

#Preview {
    OmcpRootView()
}
