import HoplaKit
import Network
import SwiftUI

/// Product / design: 500 MB/day, Wi‑Fi-only default, warn (not hard-block) on metered cellular.
/// Cap meter always starts at **0 of 500 MB** — never ship a fake “200 of 500” fill.
/// iOS BLE pause callout lives here (You → Gateway), next to the Android companion recommendation —
/// not on the Chats/thread path banner.
struct GatewaySettingsView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c
    @StateObject private var net = NetworkWatcher()
    private var gatewayOn: Binding<Bool> {
        Binding(get: { model.store.gatewayEnabled }, set: { model.store.gatewayEnabled = $0; model.touch() })
    }
    private var allowMobile: Binding<Bool> {
        Binding(get: { model.store.gatewayAllowMobile }, set: { model.store.gatewayAllowMobile = $0; model.touch() })
    }
    @State private var showDisclosure = false
    @State private var showMeteredWarn = false

    /// Cap and Wi-Fi/metered decision come from the Rust core policy.
    private var gw: GatewayUIState {
        _ = model.revision
        return GatewayPolicy.state(store: model.store, core: model.core, network: net.type, metered: net.metered)
    }
    private var dailyCapMB: Int { Int(gw.dailyCapBytes / 1_000_000) }
    private var usedMB: Int { Int(gw.usedBytes / 1_000_000) } // Designy lock-in: 0 until real metering

    var body: some View {
        AuralisSettingsPage {
            VStack(alignment: .leading, spacing: 12) {
                Image(systemName: "antenna.radiowaves.left.and.right")
                    .font(.largeTitle).foregroundStyle(c.brand).accessibilityHidden(true)
                Text(L("auralis_gateway_heading")).font(.largeTitle.weight(.bold)).tracking(-1)
                Text(L("auralis_gateway_intro")).foregroundStyle(c.ink2)
            }
            AuralisSettingsSection(L("gateway_title")) {
                toggleRow(title: L("gateway_act_as"), desc: L("gateway_act_as_desc"), isOn: gatewayOn) { newValue in
                    if newValue { showDisclosure = true; return false }
                    return true
                }
                Divider().overlay(c.line)
                toggleRow(title: L("gateway_mobile_data"), desc: L("gateway_mobile_data_desc"), isOn: allowMobile) { newValue in
                    if newValue { showMeteredWarn = true; return false }
                    return true
                }
            }
            AuralisSettingsSection(L("gateway_cap")) {
                Text(L("auralis_gateway_usage", usedMB, dailyCapMB)).font(.headline).monospacedDigit()
                ProgressView(value: Double(usedMB), total: Double(max(dailyCapMB, 1))).tint(c.brand)
                if gatewayOn.wrappedValue {
                    Text(L(gw.decision.bridge ? "gateway_bridging" : "gateway_not_bridging")).font(.subheadline)
                    if gw.decision.meteredWarning {
                        Text(L("gateway_metered_warn")).font(.footnote).foregroundStyle(c.warn)
                    }
                }
            }
            AuralisSettingsSection(L("auralis_gateway_privacy")) {
                Label(L("auralis_gateway_disclosure"), systemImage: "lock.shield")
                    .font(.subheadline).foregroundStyle(c.ink2)
                Text(L("auralis_store_forward_unavailable")).font(.footnote).foregroundStyle(c.ink3)
            }
            AuralisSettingsSection(L("auralis_gateway_routes")) {
                legend(.mesh, L("auralis_route_mesh"), L("auralis_gateway_local"))
                legend(.gateway, L("auralis_route_gateway"), L("auralis_gateway_neighbor"))
                legend(.mixed, L("auralis_route_mixed"), L("auralis_gateway_mixed"))
                legend(.queued, L("auralis_route_queued"), L("auralis_gateway_waiting"))
            }
            AuralisSettingsSection(L("auralis_mesh_ios_title")) {
                Text(L("ios_companion_hint")).font(.subheadline).foregroundStyle(c.ink2)
                Text(L("ios_ble_pause_callout")).font(.footnote).foregroundStyle(c.ink2)
            }
        }
        .navigationTitle(L("gateway_title"))
        .alert(L("auralis_gateway_heading"), isPresented: $showDisclosure) {
            Button(L("auralis_gateway_enable")) { gatewayOn.wrappedValue = true }
            Button(L("cancel"), role: .cancel) {}
        } message: {
            Text(L("auralis_gateway_confirm"))
        }
        .alert(L("auralis_gateway_metered"), isPresented: $showMeteredWarn) {
            Button(L("ob_continue")) { allowMobile.wrappedValue = true }
            Button(L("auralis_gateway_wifi_only"), role: .cancel) {}
        } message: {
            Text(L("gateway_metered_warn"))
        }
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

    private func legend(_ kind: PathKind, _ label: String, _ meaning: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            PathChip(kind: kind, label: label)
            Text(meaning).font(.caption).foregroundStyle(c.ink2)
        }
    }
}

/// Current network for the core gateway policy.
final class NetworkWatcher: ObservableObject {
    @Published var type: NetworkType = .offline
    @Published var metered = false
    private let monitor = NWPathMonitor()
    init() {
        monitor.pathUpdateHandler = { [weak self] p in
            let t: NetworkType = p.status != .satisfied ? .offline : (p.usesInterfaceType(.cellular) ? .cellular : .wifi)
            DispatchQueue.main.async {
                self?.type = t
                self?.metered = p.isExpensive || p.isConstrained
            }
        }
        monitor.start(queue: DispatchQueue(label: "omcp.net"))
    }
    deinit { monitor.cancel() }
}
