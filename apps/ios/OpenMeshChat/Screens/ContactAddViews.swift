import SwiftUI

/// Minimal Contact-add chooser (Designy §5.5).
/// Includes **My QR** so You → Profile/QR can stay deferred.
struct ContactAddView: View {
    @Environment(\.omcpColors) private var c

    var body: some View {
        List {
            Section {
                Text(String(localized: "contact_add_subtitle"))
                    .font(.subheadline)
                    .foregroundStyle(c.ink2)
            }
            Section {
                Label {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(String(localized: "contact_add_scan_qr"))
                        Text(String(localized: "contact_add_scan_qr_desc"))
                            .font(.caption)
                            .foregroundStyle(c.ink2)
                    }
                } icon: {
                    Image(systemName: "qrcode.viewfinder")
                }
                NavigationLink {
                    MyQrView()
                } label: {
                    Label {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(String(localized: "contact_add_my_qr"))
                            Text(String(localized: "contact_add_my_qr_desc"))
                                .font(.caption)
                                .foregroundStyle(c.ink2)
                        }
                    } icon: {
                        Image(systemName: "qrcode")
                    }
                }
                Label {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(String(localized: "contact_add_nfc"))
                        Text(String(localized: "contact_add_nfc_desc"))
                            .font(.caption)
                            .foregroundStyle(c.ink2)
                    }
                } icon: {
                    Image(systemName: "wave.3.right")
                }
                Label {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(String(localized: "contact_add_peer_id"))
                        Text(String(localized: "contact_add_peer_id_desc"))
                            .font(.caption)
                            .foregroundStyle(c.ink2)
                    }
                } icon: {
                    Image(systemName: "number")
                }
            }
        }
        .navigationTitle(String(localized: "contact_add_title"))
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct MyQrView: View {
    @Environment(\.omcpColors) private var c

    var body: some View {
        VStack(spacing: 20) {
            Spacer()
            RoundedRectangle(cornerRadius: 16)
                .stroke(c.line, lineWidth: 1)
                .background(c.surface, in: RoundedRectangle(cornerRadius: 16))
                .aspectRatio(1, contentMode: .fit)
                .frame(maxWidth: 260)
                .overlay {
                    VStack(spacing: 12) {
                        Image(systemName: "qrcode")
                            .font(.system(size: 96))
                            .foregroundStyle(c.ink2)
                        Text(String(localized: "my_qr_placeholder"))
                            .font(.caption)
                            .foregroundStyle(c.ink3)
                            .multilineTextAlignment(.center)
                            .padding(.horizontal, 16)
                    }
                }
            Text("You").font(.title3.weight(.semibold))
            Text("omcp1…local-stub")
                .font(.system(.footnote, design: .monospaced))
                .foregroundStyle(c.ink2)
            Text(String(localized: "my_qr_hint"))
                .font(.caption)
                .foregroundStyle(c.ink3)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(c.bg)
        .navigationTitle(String(localized: "contact_add_my_qr"))
        .navigationBarTitleDisplayMode(.inline)
    }
}
