import HoplaKit
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

struct ChatsListView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c

    var body: some View {
        let _ = model.revision
        let contacts = model.repo.contacts
        let status = model.core.pathStatus()
        NavigationStack {
            VStack(spacing: 0) {
                if model.repo.backupGate() == .banner { BackupReminderBanner() }
                HStack(spacing: 8) {
                    PathChip(kind: status.aggregatePath, label: L("status_nearby", status.nearbyPeers))
                    if status.queuedCount > 0 { PathChip(kind: .queued, label: L("status_queued", status.queuedCount)) }
                }
                .padding(.horizontal, 16).padding(.vertical, 8)
                .frame(maxWidth: .infinity, alignment: .leading)
                if contacts.isEmpty {
                    Spacer()
                    VStack(spacing: 12) {
                        Text(L("chats_empty_title")).font(.title2.weight(.semibold))
                        Text(L("chats_empty_body")).font(.subheadline).foregroundStyle(c.ink2).multilineTextAlignment(.center)
                    }.padding(.horizontal, 32)
                    Spacer()
                } else {
                    List(contacts) { contact in
                        NavigationLink(value: contact.id) { ChatRowView(contact: contact, last: model.repo.messages[contact.id]?.last) }
                    }
                    .listStyle(.plain)
                }
            }
            .background(c.bg)
            .navigationTitle(L("tab_chats"))
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    NavigationLink { ContactAddView() } label: { Image(systemName: "plus") }
                        .accessibilityLabel(L("contact_add_title"))
                }
            }
            .navigationDestination(for: String.self) { ChatThreadView(contactId: $0) }
        }
    }
}

/// Verified seal only for verified contacts; unverified rows show nothing (design §5C).
struct VerifiedBadge: View {
    @Environment(\.omcpColors) private var c
    let contact: Contact
    var body: some View {
        if contact.showsVerifiedBadge {
            Image(systemName: "checkmark.seal.fill").foregroundStyle(c.ok).font(.caption)
                .accessibilityLabel(L("verified_contact"))
        }
    }
}

struct ChatRowView: View {
    @Environment(\.omcpColors) private var c
    let contact: Contact
    let last: ChatMessage?

    private var preview: String {
        guard let last else { return "" }
        if last.burn != nil { return L("burn_preview") }
        if let f = last.file { return f.name }
        return last.text
    }

    var body: some View {
        HStack(spacing: 12) {
            Text(String(contact.displayName.prefix(2)).uppercased())
                .font(.headline).frame(width: 48, height: 48).background(c.surface2).clipShape(Circle())
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 4) { Text(contact.displayName).font(.headline); VerifiedBadge(contact: contact) }
                Text(preview).font(.subheadline).foregroundStyle(c.ink2).lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .frame(minHeight: 56)
    }
}

struct ChatThreadView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c
    let contactId: String
    @State private var draft = ""
    @State private var burnNext = false
    @State private var viewing: (id: String, content: BurnContent)?
    @State private var fileCandidate: (name: String, size: UInt64, mime: String)?
    @State private var showImporter = false
    @State private var error: String?

    var body: some View {
        let _ = model.revision
        let contact = model.repo.contact(contactId)
        let msgs = model.repo.messages[contactId] ?? []
        let queued = model.core.queuedFor(contactId)
        VStack(spacing: 0) {
            PathBanner(kind: queued > 0 ? .queued : .mesh, text: L(queued > 0 ? "path_queued" : "path_mesh"))
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 10) {
                    ForEach(msgs) { m in bubble(m) }
                }.padding(16)
            }
            if let error { Text(error).font(.caption).foregroundStyle(c.danger).padding(.horizontal) }
            composer
        }
        .background(c.bg)
        .navigationTitle(contact?.displayName ?? "")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                HStack(spacing: 4) { Text(contact?.displayName ?? "").font(.headline); if let contact { VerifiedBadge(contact: contact) } }
            }
            ToolbarItem(placement: .topBarTrailing) {
                NavigationLink { ChatInfoView(contactId: contactId) } label: { Image(systemName: "info.circle") }
                    .accessibilityLabel(L("chat_info"))
            }
        }
        .fileImporter(isPresented: $showImporter, allowedContentTypes: [.item]) { r in
            guard case .success(let url) = r else { return }
            let ok = url.startAccessingSecurityScopedResource()
            defer { if ok { url.stopAccessingSecurityScopedResource() } }
            let size = (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize).map(UInt64.init) ?? 0
            let mime = UTType(filenameExtension: url.pathExtension)?.preferredMIMEType ?? "application/octet-stream"
            fileCandidate = (url.lastPathComponent, size, mime)
        }
        .sheet(isPresented: Binding(get: { fileCandidate != nil }, set: { if !$0 { fileCandidate = nil } })) {
            if let f = fileCandidate {
                FileConfirmSheet(name: f.name, size: f.size, contactName: contact?.displayName ?? "", queued: queued > 0,
                                 onSend: { send(file: f) }, onChooseAnother: { fileCandidate = nil; showImporter = true })
                    .presentationDetents([.medium, .large])
            }
        }
        .fullScreenCover(isPresented: Binding(get: { viewing != nil }, set: { if !$0 { closeViewer() } })) {
            if let v = viewing { BurnViewer(content: v.content, onClose: closeViewer) }
        }
    }

    private var composer: some View {
        HStack {
            Button { showImporter = true } label: { Image(systemName: "paperclip") }
                .accessibilityLabel(L("attach_file"))
            TextField(L("composer_placeholder"), text: $draft)
                .padding(10).background(c.surface2, in: RoundedRectangle(cornerRadius: 10))
            Button { send() } label: { Image(systemName: burnNext ? "flame.fill" : "paperplane.fill") }
                .foregroundStyle(burnNext ? .orange : c.brand)
                .contextMenu {
                    Button { burnNext.toggle() } label: { Label(L(burnNext ? "burn_toggle_off" : "burn_toggle_on"), systemImage: "flame") }
                }
                .accessibilityLabel(L(burnNext ? "send_burn" : "send"))
                .accessibilityHint(L("burn_long_press_hint"))
        }
        .padding(8).background(c.surface)
    }

    private func send() {
        do {
            try model.repo.send(contactId, text: draft, burnOnce: burnNext ? .onOpen() : nil)
            draft = ""
            burnNext = false
            error = nil
        } catch { self.error = L("send_failed") }
    }

    private func send(file f: (name: String, size: UInt64, mime: String)) {
        do { try model.repo.queueFile(contactId, name: f.name, size: f.size, mime: f.mime) } catch { self.error = L("file_too_large_title") }
        fileCandidate = nil
    }

    private func closeViewer() {
        if let v = viewing { try? model.repo.closeBurn(contactId, msgId: v.id) }
        viewing = nil
    }

    @ViewBuilder private func bubble(_ m: ChatMessage) -> some View {
        VStack(alignment: m.mine ? .trailing : .leading, spacing: 4) {
            Group {
                if m.burn != nil { burnBubble(m) }
                else if let f = m.file { Label("\(f.name) · \(FilePolicy.label(f.sizeBytes))", systemImage: "doc") }
                else { Text(m.text) }
            }
            .padding(.horizontal, 12).padding(.vertical, 8)
            .background(m.mine ? c.brandSoft : c.surface)
            .clipShape(RoundedRectangle(cornerRadius: 14))
            Text(L(m.mine ? (m.status == .queued ? "msg_queued" : "msg_sent") : "msg_received")).font(.caption2).foregroundStyle(c.ink3)
        }
        .frame(maxWidth: .infinity, alignment: m.mine ? .trailing : .leading)
        // No copy / share / forward / quote for burn content (B2).
        .contextMenu { if m.canShare && !m.text.isEmpty { Button(L("copy")) { UIPasteboard.general.string = m.text } } }
    }

    @ViewBuilder private func burnBubble(_ m: ChatMessage) -> some View {
        switch m.burnState {
        case .opened: Text(L("burn_photo_opened"))
        case .open where !m.mine:
            if let at = m.burnAtMs {
                TimelineView(.periodic(from: .now, by: 1)) { _ in
                    Text(L("burn_countdown", max(0, Int((at - model.repo.now()) / 1000))))
                }
            } else { Text(L("burn_open")) }
        default:
            if m.mine { Label(L("burn_sent"), systemImage: "flame") }
            else {
                Button {
                    if let content = try? model.repo.openBurn(contactId, msgId: m.id) { viewing = (m.id, content) }
                } label: { Label(L(m.isViewOnceMedia ? "burn_tap_view_once" : "burn_tap_to_read"), systemImage: "flame") }
                    .blur(radius: 0)
            }
        }
    }
}

/// Full-screen burn viewer: shown once; closing burns it. Secure layer is window-wide.
struct BurnViewer: View {
    let content: BurnContent
    let onClose: () -> Void
    var body: some View {
        VStack(spacing: 16) {
            HStack { Spacer(); Button(L("close"), action: onClose) }.padding()
            Spacer()
            if content.mime?.hasPrefix("image/") == true, let img = UIImage(data: content.bytes) {
                Image(uiImage: img).resizable().scaledToFit()
            } else {
                Text(content.text).font(.title3).padding()
            }
            if let at = content.burnAtMs {
                TimelineView(.periodic(from: .now, by: 1)) { _ in
                    let left = max(0, Int((Double(at) / 1000 - Date().timeIntervalSince1970).rounded(.up)))
                    Text(L("burn_countdown", left)).font(.caption).onChange(of: left) { _, v in if v == 0 { onClose() } }
                }
            }
            Spacer()
            Text(L("burn_honest")).font(.caption).foregroundStyle(.secondary).padding()
        }
    }
}

/// Design §5.23 file-picker-confirm (+ too-large). The cap comes from the Rust core.
struct FileConfirmSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c
    let name: String
    let size: UInt64
    let contactName: String
    let queued: Bool
    let onSend: () -> Void
    let onChooseAnother: () -> Void

    var body: some View {
        let max = model.core.maxFileBytes()
        let check = FilePolicy.check(size: size, core: model.core)
        let mb = String(format: "%.1f", Double(size) / 1_000_000)
        NavigationStack {
            Form {
                Section {
                    Label(name, systemImage: "doc").foregroundStyle(check == .ok ? c.ink : c.danger)
                    LabeledContent(L("file_size"), value: L("file_size_of_max", mb, FilePolicy.label(max)))
                    LabeledContent(L("file_route")) { PathChip(kind: queued ? .queued : .mesh, label: L(queued ? "path_queued" : "path_mesh")) }
                }
                if check == .ok {
                    Section { Text(queued ? L("file_queue_hint", contactName) : L("file_mesh_hint", contactName)).font(.footnote) }
                    Button(L(queued ? "file_queue" : "send"), action: onSend).bold()
                } else {
                    Section(header: Text(L("file_too_large_title"))) {
                        Text(L("file_too_large_body", mb, FilePolicy.label(max), FilePolicy.label(max))).font(.footnote)
                    }
                    Button(L("file_choose_another"), action: onChooseAnother).bold()
                }
            }
            .navigationTitle(L("file_send_title", contactName))
            .navigationBarTitleDisplayMode(.inline)
        }
    }
}

/// Chat info: verification, per-chat Private notifications override (both ways), burn after read.
struct ChatInfoView: View {
    @EnvironmentObject private var model: AppModel
    let contactId: String

    var body: some View {
        let _ = model.revision
        let store = model.store
        Form {
            if let contact = model.repo.contact(contactId) {
                Section { NavigationLink(L("security_centre")) { SecurityCentreView(contact: contact) } }
            }
            Section(header: Text(L("notif_title")), footer: Text(L("notif_private_footer"))) {
                Picker(L("notif_private"), selection: Binding<Int>(
                    get: { store.chatPrivateOverride[contactId].map { $0 ? 1 : 0 } ?? -1 },
                    set: { v in NotificationPolicy.setChatPrivate(store, chatId: contactId, v == -1 ? nil : v == 1); model.touch() })) {
                    Text(L(store.privateNotifications ? "notif_follow_global_on" : "notif_follow_global_off")).tag(-1)
                    Text(L("notif_private_on")).tag(1)
                    Text(L("notif_private_off")).tag(0)
                }
            }
            Section(header: Text(L("burn_title")), footer: Text(L("burn_honest"))) {
                Picker(L("burn_title"), selection: Binding<Int>(
                    get: { store.burnChats[contactId] ?? -1 },
                    set: { v in try? model.repo.setChatBurn(contactId, option: v, unreadTtl: store.burnUnreadTtl[contactId] ?? BurnSpec.unread7d) })) {
                    ForEach(BurnSpec.chatOptions, id: \.self) { Text(L("burn_opt_\($0)")).tag($0) }
                }
                Picker(L("burn_unread"), selection: Binding<Int>(
                    get: { store.burnUnreadTtl[contactId] ?? BurnSpec.unread7d },
                    set: { v in try? model.repo.setChatBurn(contactId, option: store.burnChats[contactId] ?? -1, unreadTtl: v) })) {
                    Text(L("burn_ttl_1h")).tag(BurnSpec.unread1h)
                    Text(L("burn_ttl_24h")).tag(BurnSpec.unread24h)
                    Text(L("burn_ttl_7d")).tag(BurnSpec.unread7d)
                }
            }
        }
        .navigationTitle(L("chat_info"))
    }
}
