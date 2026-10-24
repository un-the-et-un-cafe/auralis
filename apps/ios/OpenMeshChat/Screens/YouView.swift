import HoplaKit
import SwiftUI

struct YouView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c

    var body: some View {
        let _ = model.revision
        let id = model.core.identity()
        NavigationStack {
            List {
                if model.repo.backupGate() == .banner { BackupReminderBanner().listRowInsets(EdgeInsets()) }
                Section {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(id?.displayName ?? "").font(.title2.weight(.semibold))
                        if let p = id?.peerIdHex { Text("omcp1:\(p.prefix(4))…\(p.suffix(4))").font(.footnote.monospaced()).foregroundStyle(c.ink2) }
                        Text(model.core.suiteLabel).font(.caption).foregroundStyle(c.ink3)
                    }
                    NavigationLink(L("my_qr_title")) { MyQrView() }
                }
                Section {
                    NavigationLink { BackupSettingsView() } label: { row(L("backup_title"), L("backup_row_desc")) }
                    NavigationLink { NotificationSettingsView() } label: { row(L("notif_title"), L("notif_row_desc")) }
                    NavigationLink { PrivacySettingsView() } label: { row(L("privacy_title"), L("privacy_row_desc")) }
                    NavigationLink { GatewaySettingsView() } label: { row(L("gateway_title"), L("gateway_row_desc")) }
                }
                Section { row(L("about"), L("about_desc")) }
            }
            .navigationTitle(L("tab_you"))
        }
    }

    private func row(_ title: String, _ support: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
            Text(support).font(.caption).foregroundStyle(c.ink2)
        }
    }
}

/// Settings › Notifications: on by default with sender + preview; global Private notifications;
/// Hide on lock screen. Per-chat overrides live in Chat info.
struct NotificationSettingsView: View {
    @EnvironmentObject private var model: AppModel
    var body: some View {
        let _ = model.revision
        let s = model.store
        Form {
            Section(footer: Text(L("notif_private_footer"))) {
                Toggle(L("notif_private"), isOn: Binding(get: { s.privateNotifications }, set: { s.privateNotifications = $0; model.touch() }))
                Toggle(L("notif_hide_lock"), isOn: Binding(get: { s.hideOnLockScreen }, set: { s.hideOnLockScreen = $0; model.touch() }))
            }
            Section(header: Text(L("notif_preview"))) {
                let p = NotificationPolicy.content(s, chatId: "", sender: "Ana", text: L("notif_sample_text"), genericBody: L("notif_new_message"))
                VStack(alignment: .leading) { Text(p.title).bold(); Text(p.body) }
            }
        }
        .navigationTitle(L("notif_title"))
    }
}

/// Settings › Privacy: screenshots are always blocked; honest copy about second cameras.
struct PrivacySettingsView: View {
    var body: some View {
        Form {
            Section(footer: Text(L("privacy_second_camera"))) {
                Label(L("privacy_screens_blocked"), systemImage: "eye.slash")
            }
        }
        .navigationTitle(L("privacy_title"))
    }
}
