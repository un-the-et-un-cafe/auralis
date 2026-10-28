import HoplaKit
import SwiftUI

/// Fresh Auralis navigation: people, the live mesh, and your identity.
struct AuralisRootView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.colorScheme) private var scheme
    /// Optional first-launch tip; after dismiss, Gateway owns the BLE/companion callout.
    @AppStorage("omcp.iosBleTipDismissed") private var iosBleTipDismissed = false
    @State private var showIosBleTip = false

    private var gate: BackupGate { model.backupGate }

    var body: some View {
        TabView {
            ChatsListView()
                .tabItem { Label(L("tab_chats"), systemImage: "bubble.left.and.bubble.right") }
            AuralisMeshView()
                .tabItem { Label(L("auralis_tab_mesh"), systemImage: "point.3.connected.trianglepath.dotted") }
            YouView()
                .tabItem { Label(L("tab_you"), systemImage: "person.crop.circle") }
        }
        .tint(scheme == .dark ? OmcpColors.dark.brand : OmcpColors.light.brand)
        .toolbarBackground(scheme == .dark ? OmcpColors.dark.surface : OmcpColors.light.surface, for: .tabBar)
        .toolbarBackground(.visible, for: .tabBar)
        .environment(\.omcpColors, scheme == .dark ? .dark : .light)
        .onAppear { if !iosBleTipDismissed && gate != .required { showIosBleTip = true } }
        .onChange(of: gate) { _, newGate in if newGate == .required { showIosBleTip = false } }
        // O6: after the 24 h "Remind me later" window, Auralis can't be used until a backup exists.
        .fullScreenCover(isPresented: .constant(gate == .required)) { BackupRequiredView().environmentObject(model) }
        .alert(L("ios_ble_tip_title"), isPresented: $showIosBleTip) {
            Button(L("ios_ble_tip_dismiss")) { iosBleTipDismissed = true; showIosBleTip = false }
        } message: {
            Text(L("ios_ble_tip_body"))
        }
    }
}
