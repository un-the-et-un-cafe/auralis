import HoplaKit
import PhotosUI
import QuickLook
import SwiftUI
import UniformTypeIdentifiers

/// Auralis people-first inbox; names and previews are repository-backed, never demo content.
struct ChatsListView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c
    @State private var search = ""

    var body: some View {
        let _ = model.revision
        let contacts = model.repo.contacts
        let visible = contacts.filter { search.isEmpty || $0.displayName.localizedCaseInsensitiveContains(search) }
        let status = model.core.pathStatus()
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    if model.repo.backupGate() == .banner { BackupReminderBanner() }
                    VStack(alignment: .leading, spacing: 14) {
                        AuralisWordmark()
                        Text(L("auralis_chats_heading")).font(.largeTitle.weight(.bold)).tracking(-1)
                        ViewThatFits(in: .horizontal) {
                            HStack(spacing: 8) { statusChips(status) }
                            VStack(alignment: .leading, spacing: 8) { statusChips(status) }
                        }
                    }
                    if contacts.isEmpty {
                        AuralisCard {
                            Image(systemName: "bubble.left.and.bubble.right")
                                .font(.largeTitle).foregroundStyle(c.brand).accessibilityHidden(true)
                            Text(L("chats_empty_title")).font(.title2.weight(.semibold))
                            Text(L("chats_empty_body")).foregroundStyle(c.ink2)
                            NavigationLink { ContactAddView() } label: {
                                Label(L("contact_add_title"), systemImage: "plus")
                            }
                            .buttonStyle(AuralisPrimaryButtonStyle())
                        }
                    } else {
                        HStack(spacing: 10) {
                            Image(systemName: "magnifyingglass").foregroundStyle(c.ink3).accessibilityHidden(true)
                            TextField(L("auralis_find_person"), text: $search)
                                .textInputAutocapitalization(.never).autocorrectionDisabled()
                        }
                        .padding(16).background(c.surface, in: RoundedRectangle(cornerRadius: 18))
                        if visible.isEmpty {
                            Text(L("auralis_search_empty")).foregroundStyle(c.ink2).padding(.vertical, 24)
                        }
                        LazyVStack(spacing: 10) {
                            ForEach(visible) { contact in
                                NavigationLink(value: contact.id) {
                                    ChatRowView(contact: contact, last: model.repo.messages[contact.id]?.last)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
                .padding(24)
            }
            .auralisScreen()
            .navigationTitle(L("tab_chats"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    NavigationLink { ContactAddView() } label: {
                        Image(systemName: "square.and.pencil").frame(minWidth: 44, minHeight: 44)
                    }
                    .accessibilityLabel(L("contact_add_title"))
                    .accessibilityIdentifier("chats.addContact")
                }
            }
            .navigationDestination(for: String.self) { ChatThreadView(contactId: $0) }
        }
    }

    @ViewBuilder private func statusChips(_ status: MeshStatus) -> some View {
        PathChip(kind: status.aggregatePath, label: L("status_nearby", status.nearbyPeers))
        if status.queuedCount > 0 { PathChip(kind: .queued, label: L("status_queued", status.queuedCount)) }
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
        } else if contact.restoredUnverified {
            // RS2 (§5.98): neutral pill with a sync glyph until re-verified in person.
            Label(L("restored_pill"), systemImage: "arrow.triangle.2.circlepath")
                .font(.caption2).foregroundStyle(c.ink2)
                .padding(.horizontal, 6).padding(.vertical, 2)
                .background(c.surface2).clipShape(Capsule())
                .overlay(Capsule().stroke(c.line))
        }
    }
}

struct ChatRowView: View {
    @Environment(\.omcpColors) private var c
    let contact: Contact
    let last: ChatMessage?

    private var preview: String {
        guard let last else { return L("auralis_start_conversation") }
        if last.status == .system { return contact.restoredUnverified ? L("restored_preview", contact.displayName) : "" }
        if last.burn != nil { return L("burn_preview") }
        if let f = last.file { return f.name }
        return last.text
    }

    var body: some View {
        HStack(spacing: 16) {
            AuralisAvatar(name: contact.displayName)
            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(contact.displayName).font(.headline).foregroundStyle(c.ink)
                    VerifiedBadge(contact: contact)
                    Spacer(minLength: 0)
                    if let last {
                        Text(Date(timeIntervalSince1970: Double(last.timeMs) / 1000), style: .time)
                            .font(.caption2).foregroundStyle(c.ink3)
                    }
                }
                Text(preview).font(.subheadline).foregroundStyle(c.ink2).lineLimit(2)
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, minHeight: 86, alignment: .leading)
        .background(c.surface, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .contentShape(RoundedRectangle(cornerRadius: 22))
    }
}

struct ChatThreadView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c
    let contactId: String
    @State private var draft = ""
    @State private var burnNext = false
    @State private var viewing: (id: String, content: BurnContent)?
    @State private var fileCandidate: (name: String, size: UInt64, mime: String, url: URL)?
    @State private var previewURL: URL?
    @State private var showImporter = false
    @State private var chooseAnotherAfterDismiss = false
    @State private var error: String?

    var body: some View {
        let _ = model.revision
        let contact = model.repo.contact(contactId)
        let msgs = model.repo.messages[contactId] ?? []
        let queued = model.core.queuedFor(contactId)
        VStack(spacing: 0) {
            PathBanner(kind: queued > 0 ? .queued : model.core.pathStatus().aggregatePath,
                       text: L("auralis_route_" + (queued > 0 ? PathKind.queued : model.core.pathStatus().aggregatePath).rawValue))
            restoreBanners(contact: contact, msgs: msgs)
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 16) {
                        if msgs.isEmpty {
                            Label(L("auralis_thread_empty"), systemImage: "lock.shield")
                                .font(.footnote).foregroundStyle(c.ink2)
                                .frame(maxWidth: .infinity).padding(.vertical, 24)
                        }
                        ForEach(msgs) { m in bubble(m).id(m.id) }
                    }.padding(20)
                }
                .defaultScrollAnchor(.bottom)
                .onChange(of: msgs.count) { previousCount, newCount in
                    // Never pull someone out of older history for an inbound message or burn expiry.
                    if newCount > previousCount, let last = msgs.last, last.mine {
                        proxy.scrollTo(last.id, anchor: .bottom)
                    }
                }
            }
            if !model.repo.receiveFinalizationErrors.isEmpty {
                HStack {
                    Text(L("file_storage_pending")).font(.caption)
                    Spacer()
                    Button(L("store_retry")) { model.repo.retryReceivedFiles() }
                }.foregroundStyle(c.warn).padding(.horizontal)
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
            fileCandidate = (url.lastPathComponent, size, mime, url)
        }
        .sheet(isPresented: Binding(get: { fileCandidate != nil }, set: { if !$0 { fileCandidate = nil } }), onDismiss: {
            if chooseAnotherAfterDismiss { chooseAnotherAfterDismiss = false; showImporter = true }
        }) {
            if let f = fileCandidate {
                FileConfirmSheet(name: f.name, size: f.size, contactName: contact?.displayName ?? "", queued: queued > 0,
                                 onSend: { send(file: f) }, onChooseAnother: { chooseAnotherAfterDismiss = true; fileCandidate = nil })
                    .presentationDetents([.medium, .large])
            }
        }
        .quickLookPreview($previewURL)
        .fullScreenCover(isPresented: Binding(get: { viewing != nil }, set: { if !$0 { closeViewer() } })) {
            if let v = viewing { BurnViewer(content: v.content, onClose: closeViewer) }
        }
    }

    private var composer: some View {
        VStack(alignment: .leading, spacing: 10) {
            if burnNext {
                Label(L("send_burn"), systemImage: "flame.fill")
                    .font(.caption.weight(.semibold)).foregroundStyle(c.warn)
            }
            HStack(alignment: .bottom, spacing: 8) {
                Button { showImporter = true } label: {
                    Image(systemName: "plus").font(.title3).frame(width: 44, height: 44)
                }
                .accessibilityLabel(L("attach_file"))
                TextField(L("composer_placeholder"), text: $draft, axis: .vertical)
                    .lineLimit(1...5).padding(.vertical, 12).padding(.horizontal, 14)
                    .background(c.surface2, in: RoundedRectangle(cornerRadius: 20))
                Button { burnNext.toggle() } label: {
                    Image(systemName: burnNext ? "flame.fill" : "flame")
                        .foregroundStyle(burnNext ? c.warn : c.ink2).frame(width: 44, height: 44)
                }
                .accessibilityLabel(L(burnNext ? "burn_toggle_off" : "burn_toggle_on"))
                .accessibilityAddTraits(burnNext ? .isSelected : [])
                Button { send() } label: {
                    Image(systemName: "arrow.up").font(.headline)
                        .foregroundStyle(c.bg).frame(width: 44, height: 44)
                        .background(c.brand, in: Circle())
                }
                .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .opacity(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? 0.45 : 1)
                .contextMenu {
                    Button { burnNext.toggle() } label: {
                        Label(L(burnNext ? "burn_toggle_off" : "burn_toggle_on"), systemImage: "flame")
                    }
                }
                .accessibilityLabel(L(burnNext ? "send_burn" : "send"))
            }
        }
        .padding(12).background(c.surface)
        .overlay(alignment: .top) { Rectangle().fill(c.line).frame(height: 1) }
    }

    private func send() {
        do {
            try model.repo.send(contactId, text: draft, burnOnce: burnNext ? .onOpen() : nil)
            draft = ""
            burnNext = false
            error = nil
        } catch { self.error = L("send_failed") }
    }

    /// Size is checked against the Rust cap before the file is read or encrypted (§5.23);
    /// reading happens off the main thread, encryption into 64 KiB chunks in Rust.
    private func send(file f: (name: String, size: UInt64, mime: String, url: URL)) {
        fileCandidate = nil
        do { try model.repo.checkFile(size: f.size) } catch { self.error = L("file_too_large_title"); return }
        let repo = model.repo, cid = contactId
        let maxBytes = model.core.maxFileBytes()
        Task.detached(priority: .userInitiated) {
            let ok = f.url.startAccessingSecurityScopedResource()
            defer { if ok { f.url.stopAccessingSecurityScopedResource() } }
            let result = Result { try BoundedFileReader.read(f.url, maxBytes: maxBytes) }
            await MainActor.run {
                do {
                    let data = try result.get()
                    try repo.sendFile(cid, name: f.name, mime: f.mime, bytes: data)
                    self.error = nil
                } catch CoreError.fileTooLarge {
                    self.error = L("file_too_large_title")
                } catch { self.error = L("file_read_error") }
            }
        }
    }

    private func closeViewer() {
        if let v = viewing { try? model.repo.closeBurn(contactId, msgId: v.id) }
        viewing = nil
    }

    @State private var hideSelfBanner = false
    @State private var hideMediaBanner = false
    @State private var resendFor: ChatMessage?
    @State private var resendText = ""
    @State private var resendSent: Set<String> = []

    @ViewBuilder private func restoreBanners(contact: Contact?, msgs: [ChatMessage]) -> some View {
        let peer = contact?.displayName ?? ""
        if model.repo.restoredAtMs != nil && !hideSelfBanner {
            Text(L("restored_self_banner")).font(.footnote).padding(10).frame(maxWidth: .infinity, alignment: .leading)
                .background(c.surface2).onTapGesture { hideSelfBanner = true }
                .accessibilityAddTraits(.updatesFrequently)
        }
        let waiting = msgs.filter { $0.file?.isPlaceholder == true }.count
        if waiting > 0 && !hideMediaBanner {
            Text(L("media_banner", waiting, peer)).font(.footnote).padding(10).frame(maxWidth: .infinity, alignment: .leading)
                .background(c.surface2).onTapGesture { hideMediaBanner = true }
        }
        ForEach(model.repo.resendAsks.filter { $0.contactId == contactId }, id: \.msgId) { a in
            Text(a.note.map { L("media_resend_asked_note", peer, a.name, $0) } ?? L("media_resend_asked", peer, a.name)).font(.footnote).padding(10).frame(maxWidth: .infinity, alignment: .leading)
                .background(c.surface2).onTapGesture { model.repo.dismissResendAsk(a.msgId) }
        }
    }

    /// RS1 notice (§5.95/§5.98): centred, neutral; tap → Security centre to verify by QR.
    @ViewBuilder private func systemNotice(_ m: ChatMessage, peer: String) -> some View {
        if let at = m.restoredAtMs {
            let date = Date(timeIntervalSince1970: Double(at > 0 ? at : m.timeMs) / 1000).formatted(date: .abbreviated, time: .omitted)
            NavigationLink { ChatInfoView(contactId: contactId) } label: {
                VStack(spacing: 2) {
                    Text(L("restored_notice", peer, date)).font(.caption).foregroundStyle(c.ink2)
                    Text(L("restored_notice_link")).font(.caption.bold()).foregroundStyle(c.brand)
                }
                .padding(.horizontal, 14).padding(.vertical, 8).background(c.surface2).clipShape(RoundedRectangle(cornerRadius: 12))
            }
            .frame(maxWidth: .infinity)
        }
    }

    /// Missing-media placeholder (§5.92): no red, "NOT RESTORED YET", Request again / Ask to resend.
    @ViewBuilder private func placeholder(_ m: ChatMessage, _ f: FileAttachment, peer: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(L("media_not_restored")).font(.caption2.bold()).foregroundStyle(c.ink2)
            Text(f.name).font(.subheadline.bold())
            Text(m.id.isEmpty ? "" : (resendSent.contains(m.id) ? L("media_request_sent", peer)
                 : f.state == .requested ? L("media_requested")
                 : f.state == .unavailable ? L("media_gone", peer) : L("media_waiting", peer)))
                .font(.caption).foregroundStyle(c.ink2)
            if !resendSent.contains(m.id) {
                if f.state == .missing {
                    Button(L("media_request_again")) { model.repo.requestMedia(contactId, msgId: m.id) }.font(.caption.bold())
                } else if f.state == .unavailable {
                    Button(L("media_ask_resend")) { resendText = ""; resendFor = m }.font(.caption.bold())
                }
            }
        }
        .padding(12).background(c.surface2).clipShape(RoundedRectangle(cornerRadius: 14))
        .alert(L("media_resend_title", peer), isPresented: Binding(get: { resendFor?.id == m.id }, set: { if !$0 { resendFor = nil } })) {
            TextField(L("media_resend_label"), text: $resendText)
            Button(L("media_resend_send")) {
                try? model.repo.askToResend(contactId, msgId: m.id, note: resendText)
                resendSent.insert(m.id); resendFor = nil
            }
            Button(L("cancel_action"), role: .cancel) { resendFor = nil }
        } message: { Text(L("media_resend_note")) }
    }

    @ViewBuilder private func bubble(_ m: ChatMessage) -> some View {
        let peer = model.repo.contact(contactId)?.displayName ?? ""
        if m.status == .system { systemNotice(m, peer: peer) }
        else if let f = m.file, f.isPlaceholder {
            placeholder(m, f, peer: peer).frame(maxWidth: .infinity, alignment: m.mine ? .trailing : .leading)
        }
        else { regularBubble(m) }
    }

    @ViewBuilder private func regularBubble(_ m: ChatMessage) -> some View {
        VStack(alignment: m.mine ? .trailing : .leading, spacing: 4) {
            Group {
                if m.burn != nil { burnBubble(m) }
                else if let f = m.file {
                    let repair = model.repo.localReceiptRepairs[m.id]
                    let requestAgain = model.repo.cancelledReceiptRequests[m.id]
                    FileBubble(file: f, mine: m.mine, peerName: model.repo.contact(contactId)?.displayName ?? "",
                               onCancel: { model.repo.cancelFile(contactId, transferId: m.id) },
                               onRetry: { model.repo.retryReceivedFiles() },
                               onRequestAgain: requestAgain.map { action in { model.repo.requestCancelledFileAgain(action) } },
                               onRepair: repair.map { action in { model.repo.repairLocalReceivedFile(action) } },
                               onOpen: { if let p = f.localPath {
                                   model.repo.prepareReceivedFileForOpening(m.id, path: p) { safe in
                                       if let safe { previewURL = URL(fileURLWithPath: safe) }
                                   }
                               } })
                        .onAppear { if f.state == .failed { model.repo.showCancelledFileRequest(contactId, msgId: m.id) } }
                        .onChange(of: f.state) { state in
                            if state == .failed { model.repo.showCancelledFileRequest(contactId, msgId: m.id) }
                            else { model.repo.hideCancelledFileRequest(m.id) }
                        }
                        .onChange(of: model.repo.cancelledRequestRevision) { _ in
                            if f.state == .failed { model.repo.showCancelledFileRequest(contactId, msgId: m.id) }
                        }
                        .onDisappear { model.repo.hideCancelledFileRequest(m.id) }
                }
                else { Text(m.text) }
            }
            .padding(.horizontal, 16).padding(.vertical, 12)
            .background(m.mine ? c.brandSoft : c.surface)
            .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
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

/// File bubble (§5.24, Designy v0.1.1 `file-transfer`): queued / sending / receiving / paused /
/// sent / received (integrity checked in Rust) / failed / integrity fail, with Cancel, Open, Delete.
struct FileBubble: View {
    let file: FileAttachment
    let mine: Bool
    let peerName: String
    let onCancel: () -> Void
    let onRetry: () -> Void
    let onRequestAgain: (() -> Void)?
    let onRepair: (() -> Void)?
    @State private var confirmingRequest = false
    @State private var requestConfirmation = CancelledFileRequestConfirmation()
    @State private var confirmingRepair = false
    @State private var repairSnapshot: (() -> Void)?
    let onOpen: () -> Void
    @Environment(\.omcpColors) private var c

    private func mb(_ b: UInt64) -> String { String(format: "%.1f", Double(b) / 1_000_000) }

    private var line: String {
        let done = mb(file.doneBytes), total = mb(file.sizeBytes)
        switch file.state {
        case .queued: return L("file_state_queued", total)
        case .sending: return L("file_state_sending", done, total)
        case .sent: return L("file_state_sent", total)
        case .receiving: return L("file_state_receiving", done, total)
        case .paused: return L("file_state_paused", done, total)
        case .received: return L("file_state_received", total)
        case .finalizing: return L("file_state_finalizing")
        case .failed: return L("file_state_failed")
        case .integrityFailed: return L("file_state_integrity", peerName)
        case .missing, .requested, .unavailable: return L("media_waiting", peerName)
        }
    }

    private var tint: Color {
        switch file.state {
        case .paused, .queued: return c.pathQueue
        case .failed, .integrityFailed: return c.danger
        default: return c.brand
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                Image(systemName: "doc.fill").font(.title2).foregroundStyle(tint).accessibilityHidden(true)
                Text(file.name).font(.subheadline.bold()).lineLimit(1)
                Spacer()
                switch file.state {
                case .queued, .sending, .paused, .receiving:
                    Button(action: onCancel) { Image(systemName: "xmark.circle").frame(minWidth: 44, minHeight: 44) }
                        .accessibilityLabel(L(mine ? "file_cancel_a11y" : "file_cancel_receive_a11y", file.name))
                case .finalizing:
                    Button(action: onCancel) { Image(systemName: "xmark.circle").frame(minWidth: 44, minHeight: 44) }
                        .accessibilityLabel(L("file_cancel_receive_a11y", file.name))
                case .integrityFailed:
                    Button(L("file_delete"), role: .destructive, action: onCancel)
                case .received where file.localPath != nil:
                    Button(action: onOpen) { Image(systemName: "arrow.up.forward.square").frame(minWidth: 44, minHeight: 44) }
                        .accessibilityLabel(L("file_open_a11y", file.name))
                default:
                    EmptyView()
                }
            }
            Text(line).font(.caption).foregroundStyle(file.state == .failed || file.state == .integrityFailed ? c.danger : c.ink2)
            if file.state == .finalizing {
                VStack(alignment: .leading, spacing: 4) {
                    Button(L("file_retry_finalizing"), action: onRetry).frame(minHeight: 44)
                    if let onRepair {
                        Button(L("file_repair_local")) { repairSnapshot = onRepair; confirmingRepair = true }
                            .frame(minHeight: 44)
                    }
                }
            }
            if file.state == .failed, let onRequestAgain {
                Button(L("file_request_again")) { requestConfirmation.present(onRequestAgain); confirmingRequest = true }
                    .frame(minHeight: 44)
            }
            ProgressView(value: file.fraction).tint(tint)
                .accessibilityValue(L("file_progress_a11y", Int(file.fraction * 100)))
            if file.state == .received { Text(L("file_integrity_checked")).font(.caption2).foregroundStyle(c.ink3) }
            if file.state == .paused { Text(L("file_paused_meta")).font(.caption2).foregroundStyle(c.ink3) }
        }
        .frame(maxWidth: 300, alignment: .leading)
        .contentShape(Rectangle())
        .onTapGesture { if file.state == .received { onOpen() } }
        .confirmationDialog(L("file_request_again"), isPresented: $confirmingRequest, titleVisibility: .visible) {
            Button(L("file_request_again")) { requestConfirmation.confirm() }
            Button(L("cancel"), role: .cancel) { requestConfirmation.dismiss() }
        } message: { Text(L("file_request_again_explanation")) }
        .onChange(of: confirmingRequest) { shown in if !shown { requestConfirmation.dismiss() } }
        .onDisappear { requestConfirmation.dismiss(); confirmingRequest = false }
        .confirmationDialog(L("file_repair_local"), isPresented: $confirmingRepair, titleVisibility: .visible) {
            Button(L("file_repair_confirm"), role: .destructive) { let action = repairSnapshot; repairSnapshot = nil; action?() }
            Button(L("cancel"), role: .cancel) { repairSnapshot = nil }
        } message: { Text(L("file_repair_explanation")) }
    }
}

/// Full-screen burn viewer: shown once; closing burns it. Secure layer is window-wide.
struct BurnViewer: View {
    @Environment(\.omcpColors) private var c
    let content: BurnContent
    let onClose: () -> Void

    var body: some View {
        VStack(spacing: 24) {
            HStack {
                Label(L("burn_title"), systemImage: "flame.fill").font(.headline).foregroundStyle(c.warn)
                Spacer()
                Button(L("close"), action: onClose).frame(minWidth: 44, minHeight: 44)
            }
            Spacer(minLength: 0)
            ScrollView {
                if content.mime?.hasPrefix("image/") == true, let img = UIImage(data: content.bytes) {
                    Image(uiImage: img).resizable().scaledToFit().clipShape(RoundedRectangle(cornerRadius: 24))
                } else {
                    Text(content.text).font(.title3).frame(maxWidth: .infinity, alignment: .leading)
                        .padding(24).background(c.surface, in: RoundedRectangle(cornerRadius: 24))
                }
            }
            Spacer(minLength: 0)
            if let at = content.burnAtMs {
                TimelineView(.periodic(from: .now, by: 1)) { _ in
                    let left = max(0, Int((Double(at) / 1000 - Date().timeIntervalSince1970).rounded(.up)))
                    Text(L("burn_countdown", left)).font(.headline.monospacedDigit()).foregroundStyle(c.warn)
                        .onChange(of: left) { _, v in if v == 0 { onClose() } }
                }
            }
            Text(L("burn_honest")).font(.footnote).foregroundStyle(c.ink2)
        }
        .padding(24).background(c.bg.ignoresSafeArea())
    }
}

/// Design §5.23 file-picker-confirm (+ too-large). The cap comes from the Rust core.
struct FileConfirmSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
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
            AuralisSettingsPage {
                AuralisSettingsSection {
                    Label(name, systemImage: "doc").foregroundStyle(check == .ok ? c.ink : c.danger)
                    LabeledContent(L("file_size"), value: L("file_size_of_max", mb, FilePolicy.label(max)))
                    LabeledContent(L("file_route")) { PathChip(kind: queued ? .queued : .mesh, label: L(queued ? "path_queued" : "path_mesh")) }
                }
                if check == .ok {
                    AuralisSettingsSection { Text(queued ? L("file_queue_hint", contactName) : L("file_mesh_hint", contactName)).font(.footnote) }
                    Button(L(queued ? "file_queue" : "send"), action: onSend).buttonStyle(AuralisPrimaryButtonStyle())
                } else {
                    AuralisSettingsSection(L("file_too_large_title")) {
                        Text(L("file_too_large_body", mb, FilePolicy.label(max), FilePolicy.label(max))).font(.footnote)
                    }
                    Button(L("file_choose_another"), action: onChooseAnother).buttonStyle(AuralisPrimaryButtonStyle())
                }
            }
            .navigationTitle(L("file_send_title", contactName))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) { Button(L("cancel")) { dismiss() } }
            }
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
        AuralisSettingsPage {
            if let contact = model.repo.contact(contactId) {
                AuralisSettingsSection { NavigationLink(L("security_centre")) { SecurityCentreView(contact: contact) } }
            }
            AuralisSettingsSection(L("notif_title"), footer: L("notif_private_footer")) {
                Picker(L("notif_private"), selection: Binding<Int>(
                    get: { store.chatPrivateOverride[contactId].map { $0 ? 1 : 0 } ?? -1 },
                    set: { v in NotificationPolicy.setChatPrivate(store, chatId: contactId, v == -1 ? nil : v == 1); model.touch() })) {
                    Text(L(store.privateNotifications ? "notif_follow_global_on" : "notif_follow_global_off")).tag(-1)
                    Text(L("notif_private_on")).tag(1)
                    Text(L("notif_private_off")).tag(0)
                }
            }
            AuralisSettingsSection(L("burn_title"), footer: L("burn_honest")) {
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
