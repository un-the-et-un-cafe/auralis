import SwiftUI
import HoplaKit

struct PathChip: View {
    @Environment(\.omcpColors) private var c
    let kind: PathKind
    let label: String
    var micro: Bool = false

    private var pair: (Color, Color) {
        switch kind {
        case .mesh: return (c.pathMesh, c.pathMeshSoft)
        case .gateway: return (c.pathGateway, c.pathGatewaySoft)
        case .mixed: return (c.pathMixed, c.pathMixedSoft)
        case .queued: return (c.pathQueue, c.pathQueueSoft)
        }
    }

    var body: some View {
        let (fg, bg) = pair
        HStack(spacing: 6) {
            Circle().fill(fg).frame(width: micro ? 5 : 7, height: micro ? 5 : 7)
            Text(label)
                .font(.system(size: micro ? 10 : 12, weight: .semibold))
                .foregroundStyle(fg)
        }
        .padding(.horizontal, micro ? 7 : 10)
        .padding(.vertical, micro ? 2 : 5)
        .background(bg, in: Capsule())
        .accessibilityLabel("Delivery path: \(label)")
    }
}

struct PathBanner: View {
    @Environment(\.omcpColors) private var c
    let kind: PathKind
    let text: String

    private var pair: (Color, Color) {
        switch kind {
        case .mesh: return (c.pathMesh, c.pathMeshSoft)
        case .gateway: return (c.pathGateway, c.pathGatewaySoft)
        case .mixed: return (c.pathMixed, c.pathMixedSoft)
        case .queued: return (c.pathQueue, c.pathQueueSoft)
        }
    }

    var body: some View {
        let (fg, bg) = pair
        HStack(spacing: 8) {
            Circle().fill(fg).frame(width: 8, height: 8)
            Text(text).font(.subheadline).foregroundStyle(fg)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(bg)
        .accessibilityLabel("Delivery path: \(text)")
    }
}

struct EmptyStateView: View {
    @Environment(\.omcpColors) private var c
    let title: String
    let bodyText: String

    var body: some View {
        VStack(spacing: 12) {
            Text(title).font(.title2.weight(.bold)).multilineTextAlignment(.center)
            Text(bodyText)
                .font(.body)
                .foregroundStyle(c.ink2)
                .multilineTextAlignment(.center)
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
