import SwiftUI

struct ChatRowModel: Identifiable {
    let id: String
    let title: String
    let preview: String
    let time: String
    let initials: String
    let verified: Bool
    var unread: Int = 0
    var path: OmcpCoreStub.PathKind? = nil
    var isGroup: Bool = false
}

/// Designy lock-in: sample rows only when [DesignReviewFlags.showSampleChatsForDesignReview]
/// is on (off by default). Release/default UI is empty-state only.
enum DesignReviewSampleData {
    static let chats: [ChatRowModel] = [
        .init(id: "1", title: "Alex Rivera", preview: "See you at the park entrance", time: "9:38", initials: "AL", verified: true, path: .mesh),
        .init(id: "2", title: "Neighbourhood watch", preview: "Maya: Power’s back on our street", time: "9:12", initials: "N", verified: false, unread: 3, isGroup: true),
        .init(id: "3", title: "Jules Bernard", preview: "Waiting for mesh… photo.pdf", time: "8:55", initials: "JB", verified: false, path: .queued),
        .init(id: "4", title: "Sam Kim", preview: "Sent via neighbour’s Internet", time: "Yesterday", initials: "SK", verified: true, path: .gateway),
    ]

    static var visibleChats: [ChatRowModel] {
        DesignReviewFlags.showSampleChatsForDesignReview ? chats : []
    }
}

struct ChatsListView: View {
    @Environment(\.omcpColors) private var c
    private let status = OmcpCoreStub.meshStatus()
    private var chats: [ChatRowModel] { DesignReviewSampleData.visibleChats }

    var body: some View {
        NavigationStack {
            Group {
                if chats.isEmpty {
                    VStack(spacing: 0) {
                        statusStrip
                        Spacer()
                        VStack(spacing: 12) {
                            Text(String(localized: "chats_empty_title"))
                                .font(.title2.weight(.semibold))
                            Text(String(localized: "chats_empty_body"))
                                .font(.subheadline)
                                .foregroundStyle(c.ink2)
                                .multilineTextAlignment(.center)
                                .padding(.horizontal, 32)
                        }
                        Spacer()
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(c.bg)
                } else {
                    List {
                        Section {
                            statusStrip
                                .listRowBackground(Color.clear)
                                .listRowInsets(EdgeInsets(top: 8, leading: 16, bottom: 4, trailing: 16))
                        }
                        ForEach(chats) { row in
                            NavigationLink(value: row.id) {
                                ChatRowView(model: row)
                            }
                        }
                    }
                    .listStyle(.plain)
                    .background(c.bg)
                }
            }
            .navigationTitle(String(localized: "tab_chats"))
            .toolbar {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    Button(action: {}) { Image(systemName: "magnifyingglass") }
                    NavigationLink {
                        ContactAddView()
                    } label: {
                        Image(systemName: "plus")
                    }
                    .accessibilityLabel(String(localized: "contact_add_title"))
                }
            }
            .navigationDestination(for: String.self) { id in
                ChatThreadView(chatId: id)
            }
        }
    }

    private var statusStrip: some View {
        HStack(spacing: 8) {
            PathChip(kind: .mesh, label: status.nearbyPeers > 0 ? "Mesh · \(status.nearbyPeers) nearby" : "Mesh · placeholder")
            PathChip(kind: .queued, label: OmcpCoreStub.coreLinked ? "\(status.queuedCount) queued" : "Core stub")
        }
    }
}

struct ChatRowView: View {
    @Environment(\.omcpColors) private var c
    let model: ChatRowModel

    var body: some View {
        HStack(spacing: 12) {
            Text(model.initials)
                .font(.headline)
                .frame(width: 48, height: 48)
                .background(c.surface2)
                .clipShape(model.isGroup ? AnyShape(RoundedRectangle(cornerRadius: 12)) : AnyShape(Circle()))
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 4) {
                    Text(model.title).font(.headline)
                    if model.verified {
                        Image(systemName: "checkmark.shield.fill")
                            .foregroundStyle(c.ok)
                            .font(.caption)
                            .accessibilityLabel(String(localized: "verified_contact"))
                    }
                }
                Text(model.preview).font(.subheadline).foregroundStyle(c.ink2).lineLimit(1)
            }
            Spacer(minLength: 0)
            VStack(alignment: .trailing, spacing: 4) {
                Text(model.time).font(.caption).foregroundStyle(c.ink3)
                if model.unread > 0 {
                    Text("\(model.unread)")
                        .font(.caption2.weight(.bold))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 7)
                        .padding(.vertical, 2)
                        .background(c.brand, in: Capsule())
                } else if let path = model.path, path != .mesh {
                    PathChip(kind: path, label: path.rawValue.capitalized, micro: true)
                }
            }
        }
        .padding(.vertical, 4)
        .frame(minHeight: 56)
    }
}

/// Type-erased shape helper for avatar clip.
private struct AnyShape: Shape {
    private let pathBuilder: (CGRect) -> Path
    init<S: Shape>(_ shape: S) { pathBuilder = { shape.path(in: $0) } }
    func path(in rect: CGRect) -> Path { pathBuilder(rect) }
}

struct ChatThreadView: View {
    @Environment(\.omcpColors) private var c
    let chatId: String
    @State private var draft = ""

    private var chat: ChatRowModel {
        DesignReviewSampleData.visibleChats.first { $0.id == chatId }
            ?? ChatRowModel(id: chatId, title: "Chat", preview: "", time: "", initials: "?", verified: false, path: .mesh)
    }

    /// Path banner copy only — mesh/gateway/mixed/queued. No iOS BLE callout (Designy: that lives on You → Gateway).
    private var bannerText: String {
        switch chat.path {
        case .gateway: return "Sent via neighbour’s Internet · encrypted end-to-end"
        case .queued: return "Waiting for mesh · will send when a path opens"
        case .mixed: return "Mixed paths · encrypted end-to-end"
        default: return "Via mesh · placeholder hops · encrypted end-to-end"
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            PathBanner(kind: chat.path ?? .mesh, text: bannerText)
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    if !chat.preview.isEmpty {
                        bubble(mine: false, text: chat.preview, meta: "9:30 · Delivered · Mesh")
                        bubble(mine: true, text: "Placeholder reply — mesh not linked yet.", meta: "9:32 · Queued")
                    }
                }
                .padding(16)
            }
            HStack {
                Button(action: {}) { Image(systemName: "paperclip") }
                TextField("Message", text: $draft)
                    .padding(10)
                    .background(c.surface2, in: RoundedRectangle(cornerRadius: 10))
                Button(action: { draft = "" }) { Image(systemName: "paperplane.fill") }
            }
            .padding(8)
            .background(c.surface)
        }
        .background(c.bg)
        .navigationTitle(chat.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                NavigationLink {
                    SecurityCentreView(contactName: chat.title)
                } label: {
                    Image(systemName: "shield")
                }
                .accessibilityLabel(String(localized: "security_centre"))
            }
        }
    }

    @ViewBuilder
    private func bubble(mine: Bool, text: String, meta: String) -> some View {
        VStack(alignment: mine ? .trailing : .leading, spacing: 4) {
            Text(text)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .background(mine ? c.brandSoft : c.surface)
                .overlay {
                    if !mine {
                        RoundedRectangle(cornerRadius: 14).stroke(c.line, lineWidth: 1)
                    }
                }
                .clipShape(RoundedRectangle(cornerRadius: 14))
            Text(meta).font(.caption2).foregroundStyle(c.ink3)
        }
        .frame(maxWidth: .infinity, alignment: mine ? .trailing : .leading)
    }
}
