import SwiftUI

struct SecurityCentreView: View {
    @Environment(\.omcpColors) private var c
    var contactName: String = "Alex Rivera"

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                card {
                    Text(contactName).font(.title2.weight(.semibold))
                    Text("PeerID · omcp1…stub")
                        .font(.system(.footnote, design: .monospaced))
                        .foregroundStyle(c.ink2)
                    Text(String(localized: "verified_contact"))
                        .foregroundStyle(c.ok)
                        .font(.subheadline.weight(.semibold))
                        .padding(.top, 4)
                }
                card {
                    Text("Safety number").font(.headline)
                    Text("12345 67890 12345 67890\n12345 67890 12345 67890\n12345 67890 12345 67890")
                        .font(.system(.body, design: .monospaced))
                        .padding(.top, 8)
                    HStack {
                        Button("Mark verified") {}
                        Button("Scan QR") {}
                    }
                    .buttonStyle(.bordered)
                    .padding(.top, 8)
                }
                card {
                    Text("Session").font(.headline)
                    Text(OmcpCoreStub.encryptionSummary())
                        .foregroundStyle(c.ink2)
                        .padding(.top, 6)
                    Button("Reset session") {}
                        .buttonStyle(.bordered)
                        .padding(.top, 8)
                }
                Button(role: .destructive) {} label: {
                    Text("Block contact")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .tint(c.danger)
            }
            .padding(16)
        }
        .background(c.bg)
        .navigationTitle(String(localized: "security_centre"))
    }

    @ViewBuilder
    private func card<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            content()
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(c.surface, in: RoundedRectangle(cornerRadius: 14))
    }
}
