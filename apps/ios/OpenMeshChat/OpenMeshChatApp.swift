import SwiftUI

@main
struct OpenMeshChatApp: App {
    var body: some Scene {
        WindowGroup {
            OmcpRootView()
                .environment(\.omcpColors, OmcpColors.current())
        }
    }
}
