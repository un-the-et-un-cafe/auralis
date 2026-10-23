import SwiftUI

struct YouView: View {
    @Environment(\.omcpColors) private var c

    var body: some View {
        NavigationStack {
            List {
                Section {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("You").font(.title2.weight(.semibold))
                        Text("PeerID · omcp1…local-stub")
                            .font(.system(.footnote, design: .monospaced))
                            .foregroundStyle(c.ink2)
                        Text(String(localized: "mesh_placeholder"))
                            .font(.caption)
                            .foregroundStyle(c.ink3)
                        Text("Suite: \(OmcpCoreStub.suiteLabel)")
                            .font(.caption)
                            .foregroundStyle(c.ink3)
                    }
                    .padding(.vertical, 4)
                }
                Section {
                    NavigationLink {
                        GatewaySettingsView()
                    } label: {
                        row(String(localized: "gateway_title"), support: "Wi‑Fi-only default · 500 MB/day cap")
                    }
                    NavigationLink {
                        SecurityCentreView(contactName: "You")
                    } label: {
                        row(String(localized: "security_centre"), support: "Safety numbers, session, block list")
                    }
                    row("Notifications", support: "Previews off by default")
                    row("Backup / restore", support: "Encrypted local backup — deferred wiring")
                    row("About", support: "Open Mesh Chat · MIT · PROTOCOL.md")
                }
            }
            .navigationTitle(String(localized: "tab_you"))
        }
    }

    private func row(_ title: String, support: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
            Text(support).font(.caption).foregroundStyle(c.ink2)
        }
    }
}
