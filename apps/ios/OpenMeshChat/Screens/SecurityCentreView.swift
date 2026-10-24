import HoplaKit
import SwiftUI

struct SecurityCentreView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c
    let contact: Contact

    var body: some View {
        let _ = model.revision
        let ct = model.repo.contact(contact.id) ?? contact
        Form {
            Section {
                HStack { Text(ct.displayName).font(.title3.bold()); VerifiedBadge(contact: ct) }
                Text("omcp1:\(ct.id.prefix(4))…\(ct.id.suffix(4))").font(.footnote.monospaced()).foregroundStyle(c.ink2)
                Text(L(ct.verified ? "verified_contact" : "unverified")).foregroundStyle(ct.verified ? c.ok : c.ink2)
            }
            Section(header: Text(L("sn_section")), footer: Text(L("sn_compare", ct.displayName))) {
                SafetyNumberGrid(groups: ct.safetyNumber)
                Button(L(ct.verified ? "sn_unverify" : "sn_mark_verified")) { try? model.repo.setVerified(ct.id, !ct.verified) }
            }
            Section(header: Text(L("session"))) {
                Text(model.core.suiteLabel).font(.footnote.monospaced())
            }
        }
        .navigationTitle(L("security_centre"))
    }
}
