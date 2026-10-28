import HoplaKit
import SwiftUI

struct SecurityCentreView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c
    let contact: Contact

    var body: some View {
        let _ = model.revision
        let ct = model.repo.contact(contact.id) ?? contact
        AuralisSettingsPage {
            AuralisSettingsSection {
                HStack(spacing: 16) {
                    AuralisAvatar(name: ct.displayName, size: 64)
                    Text(ct.displayName).font(.title2.weight(.semibold))
                    VerifiedBadge(contact: ct)
                }
                Text("omcp1:\(ct.id.prefix(4))…\(ct.id.suffix(4))").font(.footnote.monospaced()).foregroundStyle(c.ink2)
                Text(L(ct.verified ? "verified_contact" : "unverified")).foregroundStyle(ct.verified ? c.ok : c.ink2)
            }
            AuralisSettingsSection(L("sn_section"), footer: L("sn_compare", ct.displayName)) {
                SafetyNumberGrid(groups: ct.safetyNumber)
                Button(L(ct.verified ? "sn_unverify" : "sn_mark_verified")) { try? model.repo.setVerified(ct.id, !ct.verified) }
                    .buttonStyle(AuralisPrimaryButtonStyle())
            }
            AuralisSettingsSection(L("session")) {
                Text(model.core.suiteLabel).font(.footnote.monospaced())
            }
        }
        .navigationTitle(L("security_centre"))
    }
}
