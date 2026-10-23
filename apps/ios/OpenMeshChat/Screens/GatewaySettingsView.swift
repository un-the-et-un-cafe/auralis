import SwiftUI

/// Jakob / Designy: 500 MB/day, Wi‑Fi-only default, warn (not hard-block) on metered cellular.
/// Cap meter always starts at **0 of 500 MB** — never ship a fake “200 of 500” fill.
/// iOS BLE pause callout lives here (You → Gateway), next to the Android companion recommendation —
/// not on the Chats/thread path banner.
struct GatewaySettingsView: View {
    @Environment(\.omcpColors) private var c
    @State private var gatewayOn = false
    @State private var allowMobile = false
    @State private var storeForward = false
    @State private var showDisclosure = false
    @State private var showMeteredWarn = false

    private let dailyCapMB = 500
    private let usedMB = 0 // real metering not wired yet; Designy lock-in: 0 used

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Help nearby peers reach the Internet").font(.headline)
                    Text("Forwards encrypted envelopes only. Battery and data may increase.")
                        .font(.subheadline)
                        .foregroundStyle(c.ink2)
                }
                .padding(14)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(c.warnSoft, in: RoundedRectangle(cornerRadius: 14))

                card {
                    toggleRow(
                        title: String(localized: "gateway_act_as"),
                        desc: String(localized: "gateway_act_as_desc"),
                        isOn: $gatewayOn
                    ) { newValue in
                        if newValue { showDisclosure = true; return false }
                        return true
                    }
                    toggleRow(
                        title: String(localized: "gateway_mobile_data"),
                        desc: String(localized: "gateway_mobile_data_desc"),
                        isOn: $allowMobile
                    ) { newValue in
                        if newValue { showMeteredWarn = true; return false }
                        return true
                    }
                    Text(String(localized: "gateway_cap")).font(.subheadline.weight(.semibold))
                    Text("\(usedMB) MB used of \(dailyCapMB) MB today")
                        .font(.caption)
                        .foregroundStyle(c.ink2)
                    ProgressView(value: Double(usedMB), total: Double(dailyCapMB))
                        .tint(c.brand)
                }

                card {
                    Text("Battery & privacy").font(.headline)
                    Text("Gateway mode keeps BLE active. Relays see encrypted envelopes and timing — not names or text.")
                        .font(.subheadline)
                        .foregroundStyle(c.ink2)
                        .padding(.top, 4)
                    Toggle(isOn: $storeForward) {
                        VStack(alignment: .leading) {
                            Text("Store-and-forward")
                            Text("Hold encrypted envelopes up to 24 h. Off by default on mobile.")
                                .font(.caption)
                                .foregroundStyle(c.ink2)
                        }
                    }
                    .padding(.top, 8)
                }

                card {
                    Text("Path indicators").font(.headline)
                    VStack(alignment: .leading, spacing: 10) {
                        legend(.mesh, "Mesh", "Local BLE only")
                        legend(.gateway, "Gateway", "Via a neighbour’s Internet")
                        legend(.mixed, "Mixed", "Some via mesh, some via gateway")
                        legend(.queued, "Queued", "Waiting for a path")
                    }
                    .padding(.top, 8)
                }

                // Designy: iOS BLE pause + Android companion — together on You → Gateway.
                VStack(alignment: .leading, spacing: 8) {
                    Text(String(localized: "ios_companion_hint"))
                        .font(.caption)
                        .foregroundStyle(c.ink2)
                    Text(String(localized: "ios_ble_pause_callout"))
                        .font(.caption)
                        .foregroundStyle(c.ink2)
                }
                .padding(14)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(c.warnSoft.opacity(0.65), in: RoundedRectangle(cornerRadius: 14))
            }
            .padding(16)
        }
        .background(c.bg)
        .navigationTitle(String(localized: "gateway_title"))
        .alert("Help nearby peers reach the Internet", isPresented: $showDisclosure) {
            Button("Enable gateway") { gatewayOn = true }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("When on, your phone forwards only encrypted OMCP envelopes. You never see message content. Uses battery and may use mobile data.")
        }
        .alert("Metered cellular", isPresented: $showMeteredWarn) {
            Button("Continue") { allowMobile = true }
            Button("Keep Wi‑Fi only", role: .cancel) {}
        } message: {
            Text(String(localized: "gateway_metered_warn"))
        }
    }

    @ViewBuilder
    private func card<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 12) { content() }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(c.surface, in: RoundedRectangle(cornerRadius: 14))
    }

    private func toggleRow(
        title: String,
        desc: String,
        isOn: Binding<Bool>,
        gate: @escaping (Bool) -> Bool
    ) -> some View {
        Toggle(isOn: Binding(
            get: { isOn.wrappedValue },
            set: { newValue in
                if gate(newValue) { isOn.wrappedValue = newValue }
            }
        )) {
            VStack(alignment: .leading) {
                Text(title)
                Text(desc).font(.caption).foregroundStyle(c.ink2)
            }
        }
    }

    private func legend(_ kind: OmcpCoreStub.PathKind, _ label: String, _ meaning: String) -> some View {
        HStack(spacing: 10) {
            PathChip(kind: kind, label: label)
            Text(meaning).font(.caption).foregroundStyle(c.ink2)
        }
    }
}
