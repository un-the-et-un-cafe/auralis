import HoplaKit
import SwiftUI

/// A network-first workspace. Every number comes from the core; no simulated peers or traffic.
struct AuralisMeshView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c

    var body: some View {
        let _ = model.revision
        let status = model.core.pathStatus()
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    VStack(alignment: .leading, spacing: 10) {
                        AuralisWordmark()
                        Text(L("auralis_mesh_heading")).font(.largeTitle.weight(.bold)).tracking(-1)
                        Text(L("auralis_mesh_intro")).foregroundStyle(c.ink2)
                    }
                    AuralisCard {
                        PathChip(kind: status.aggregatePath, label: L("auralis_route_" + status.aggregatePath.rawValue))
                        ViewThatFits(in: .horizontal) {
                            HStack(spacing: 28) {
                                metric(status.nearbyPeers, "auralis_nearby_peers")
                                metric(status.queuedCount, "auralis_waiting_messages")
                            }
                            VStack(alignment: .leading, spacing: 20) {
                                metric(status.nearbyPeers, "auralis_nearby_peers")
                                metric(status.queuedCount, "auralis_waiting_messages")
                            }
                        }
                        Text(L(status.nearbyPeers == 0 ? "auralis_mesh_no_peers" : "auralis_mesh_has_peers"))
                            .font(.subheadline).foregroundStyle(c.ink2)
                    }
                    AuralisCard {
                        NavigationLink { ContactAddView() } label: {
                            AuralisActionRow(icon: "qrcode.viewfinder", title: L("contact_add_title"), detail: L("auralis_mesh_add_detail"))
                        }
                        Divider().overlay(c.line)
                        NavigationLink { GatewaySettingsView() } label: {
                            AuralisActionRow(icon: "antenna.radiowaves.left.and.right", title: L("gateway_title"), detail: L("gateway_row_desc"))
                        }
                    }
                    AuralisCard {
                        Label(L("auralis_mesh_ios_title"), systemImage: "iphone").font(.headline)
                        Text(L("ios_companion_hint")).font(.subheadline).foregroundStyle(c.ink2)
                        Text(L("ios_ble_pause_callout")).font(.footnote).foregroundStyle(c.ink2)
                    }
                    AuralisCard {
                        Text(L("auralis_next_chapter")).font(.caption.weight(.semibold)).foregroundStyle(c.ink3)
                        NavigationLink { CommunitiesHomeView() } label: {
                            AuralisActionRow(icon: "person.3", title: L("tab_communities"), detail: L("coming_v02_title"))
                        }
                        Divider().overlay(c.line)
                        NavigationLink { LiveTabView() } label: {
                            AuralisActionRow(icon: "video", title: L("tab_live"), detail: L("coming_v02_title"))
                        }
                    }
                }
                .padding(24)
            }
            .auralisScreen()
            .navigationTitle(L("auralis_tab_mesh"))
            .navigationBarTitleDisplayMode(.inline)
        }
    }

    private func metric(_ value: Int, _ key: String) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(value, format: .number).font(.largeTitle.weight(.semibold)).monospacedDigit()
            Text(L(key)).font(.caption.weight(.medium)).foregroundStyle(c.ink2)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}
