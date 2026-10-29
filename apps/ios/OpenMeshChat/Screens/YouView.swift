import HoplaKit
import SwiftUI

struct YouView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c

    var body: some View {
        let _ = model.revision
        let id = model.core.identity()
        NavigationStack {
            AuralisSettingsPage {
                if model.repo.backupGate() == .banner { BackupReminderBanner() }
                AuralisCard {
                    HStack(alignment: .top, spacing: 18) {
                        AuralisAvatar(name: id?.displayName ?? "", size: 70)
                        VStack(alignment: .leading, spacing: 8) {
                            Text(id?.displayName ?? "").font(.title2.weight(.semibold))
                            Text(L("auralis_your_identity")).font(.subheadline).foregroundStyle(c.ink2)
                            if let p = id?.peerIdHex {
                                Text("omcp1:\(p.prefix(4))…\(p.suffix(4))")
                                    .font(.footnote.monospaced()).foregroundStyle(c.ink2)
                            }
                        }
                    }
                    Divider().overlay(c.line)
                    NavigationLink { MyQrView() } label: {
                        AuralisActionRow(icon: "qrcode", title: L("my_qr_title"), detail: L("auralis_identity_qr_detail"))
                    }
                    .accessibilityIdentifier("you.myQR")
                }
                AuralisSettingsSection(L("auralis_your_control")) {
                    NavigationLink { BackupSettingsView() } label: {
                        AuralisActionRow(icon: "arrow.triangle.2.circlepath", title: L("backup_title"), detail: L("backup_row_desc"))
                    }
                    .accessibilityIdentifier("you.backup")
                    Divider().overlay(c.line)
                    NavigationLink { PrivacySettingsView() } label: {
                        AuralisActionRow(icon: "hand.raised", title: L("privacy_title"), detail: L("privacy_row_desc"))
                    }
                    .accessibilityIdentifier("you.privacy")
                    Divider().overlay(c.line)
                    NavigationLink { NotificationSettingsView() } label: {
                        AuralisActionRow(icon: "bell", title: L("notif_title"), detail: L("notif_row_desc"))
                    }
                    .accessibilityIdentifier("you.notifications")
                    Divider().overlay(c.line)
                    NavigationLink { StorageSettingsView() } label: {
                        AuralisActionRow(icon: "externaldrive", title: L("storage_title"), detail: L("storage_keep_title"))
                    }
                    .accessibilityIdentifier("you.storage")
                    Divider().overlay(c.line)
                    NavigationLink { GatewaySettingsView() } label: {
                        AuralisActionRow(icon: "antenna.radiowaves.left.and.right", title: L("gateway_title"), detail: L("gateway_row_desc"))
                    }
                    .accessibilityIdentifier("you.gateway")
                }
                AuralisCard {
                    AuralisWordmark()
                    Text(L("about_desc")).font(.footnote).foregroundStyle(c.ink2)
                    Text(model.core.suiteLabel).font(.caption.monospaced()).foregroundStyle(c.ink3)
                }
            }
            .navigationTitle(L("tab_you"))
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
        AuralisSettingsPage {
            AuralisSettingsSection(footer: L("notif_private_footer")) {
                Toggle(L("notif_private"), isOn: Binding(get: { s.privateNotifications }, set: { s.privateNotifications = $0; model.touch() }))
                    .accessibilityIdentifier("settings.privateNotifications")
                Toggle(L("notif_hide_lock"), isOn: Binding(get: { s.hideOnLockScreen }, set: { s.hideOnLockScreen = $0; model.touch() }))
            }
            AuralisSettingsSection(L("notif_preview")) {
                let p = NotificationPolicy.content(s, chatId: "", sender: "Ana", text: L("notif_sample_text"), genericBody: L("notif_new_message"))
                VStack(alignment: .leading) { Text(p.title).bold(); Text(p.body) }
            }
        }
        .navigationTitle(L("notif_title"))
    }
}

/// Settings › Privacy: supported covers and explicit screenshot/camera limitations.
struct PrivacySettingsView: View {
    var body: some View {
        AuralisSettingsPage {
            AuralisSettingsSection(footer: L("privacy_second_camera")) {
                Label(L("privacy_screen_limits"), systemImage: "eye.slash")
            }
        }
        .navigationTitle(L("privacy_title"))
    }
}

/// Settings > Storage > Keep sent media (REQUIREMENTS "Keep sent media"); limits live in Rust.
struct StorageSettingsView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c
    @State private var prefs = KeptMediaPrefs()
    @State private var usage = KeptMediaUsage(count: 0, bytes: 0)

    private func reload() { prefs = model.core.keptMediaSettings(); usage = model.core.keptMediaUsage() }
    private func apply(_ p: KeptMediaPrefs) {
        _ = try? model.core.setKeptMediaSettings(p, nowMs: Int64(Date().timeIntervalSince1970 * 1000))
        reload()
    }

    var body: some View {
        AuralisSettingsPage {
            AuralisSettingsSection(L("storage_keep_title"), footer: L("storage_keep_body")) {
                Picker(L("storage_keep_days"), selection: Binding(get: { prefs.maxDays }, set: { var p = prefs; p.maxDays = $0; apply(p) })) {
                    ForEach(StorageSettings.dayOptions, id: \.self) { Text(L("storage_days", $0)).tag($0) }
                }
                Picker(L("storage_keep_size"), selection: Binding(get: { prefs.maxBytes }, set: { var p = prefs; p.maxBytes = $0; apply(p) })) {
                    ForEach(StorageSettings.sizeOptions, id: \.self) { Text(StorageSettings.formatBytes($0)).tag($0) }
                }
                Text(L("storage_usage", usage.count, StorageSettings.formatBytes(usage.bytes))).font(.caption).foregroundStyle(c.ink2)
            }
        }
        .navigationTitle(L("storage_title"))
        .onAppear(perform: reload)
    }
}
