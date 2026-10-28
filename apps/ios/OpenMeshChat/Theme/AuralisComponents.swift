import SwiftUI

/// The app follows the system appearance, including changes while onboarding or in a sheet.
private struct AuralisTheme: ViewModifier {
    @Environment(\.colorScheme) private var scheme

    func body(content: Content) -> some View {
        let colors: OmcpColors = scheme == .dark ? .dark : .light
        content
            .environment(\.omcpColors, colors)
            .tint(colors.brand)
            .foregroundStyle(colors.ink)
            .background(colors.bg.ignoresSafeArea())
    }
}

private struct AuralisScreen: ViewModifier {
    @Environment(\.omcpColors) private var c

    func body(content: Content) -> some View {
        content
            .scrollContentBackground(.hidden)
            .background(c.bg)
            .toolbarBackground(c.bg, for: .navigationBar)
    }
}

extension View {
    func auralisTheme() -> some View { modifier(AuralisTheme()) }
    func auralisScreen() -> some View { modifier(AuralisScreen()) }
}

/// Three rising signals, one shared space. Pure vector artwork; no animation or network work.
struct AuralisMark: View {
    @Environment(\.omcpColors) private var c

    var body: some View {
        GeometryReader { proxy in
            let side = min(proxy.size.width, proxy.size.height)
            ZStack {
                RoundedRectangle(cornerRadius: side * 0.3, style: .continuous)
                    .fill(c.brandSoft)
                AuralisSignal()
                    .stroke(c.brand, style: StrokeStyle(lineWidth: side * 7 / 108, lineCap: .round))
            }
            .frame(width: side, height: side)
        }
        .accessibilityHidden(true)
    }
}

/// Matches design/brand/auralis/icon.svg exactly; the launcher and native mark share geometry.
private struct AuralisSignal: Shape {
    func path(in rect: CGRect) -> Path {
        func point(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
            CGPoint(x: rect.minX + x * rect.width / 108, y: rect.minY + y * rect.height / 108)
        }
        var path = Path()
        path.move(to: point(30, 72))
        path.addQuadCurve(to: point(54, 31), control: point(40, 49))
        path.move(to: point(47, 72))
        path.addQuadCurve(to: point(64, 44), control: point(55, 55))
        path.move(to: point(64, 72))
        path.addQuadCurve(to: point(76, 57), control: point(69, 63))
        return path
    }
}

struct AuralisCard<Content: View>: View {
    @Environment(\.omcpColors) private var c
    private let content: Content

    init(@ViewBuilder content: () -> Content) { self.content = content() }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) { content }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(22)
            .background(c.surface, in: RoundedRectangle(cornerRadius: 24, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 24, style: .continuous).stroke(c.line.opacity(0.6), lineWidth: 1))
    }
}

/// Shared settings layout keeps controls native while separating decisions into readable cards.
struct AuralisSettingsPage<Content: View>: View {
    private let content: Content
    init(@ViewBuilder content: () -> Content) { self.content = content() }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) { content }
                .padding(24)
        }
        .auralisScreen()
        .pickerStyle(.menu)
        .textFieldStyle(.roundedBorder)
        .buttonStyle(AuralisTapTargetStyle())
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct AuralisSettingsSection<Content: View>: View {
    @Environment(\.omcpColors) private var c
    let title: String?
    let footer: String?
    private let content: Content

    init(_ title: String? = nil, footer: String? = nil, @ViewBuilder content: () -> Content) {
        self.title = title
        self.footer = footer
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let title { Text(title).font(.headline).foregroundStyle(c.ink) }
            AuralisCard { content }
            if let footer {
                Text(footer).font(.footnote).foregroundStyle(c.ink2)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

struct AuralisActionRow: View {
    @Environment(\.omcpColors) private var c
    let icon: String
    let title: String
    let detail: String

    var body: some View {
        HStack(spacing: 16) {
            Image(systemName: icon).font(.title3).foregroundStyle(c.brand)
                .frame(width: 44, height: 44)
                .background(c.brandSoft, in: RoundedRectangle(cornerRadius: 14))
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.headline).foregroundStyle(c.ink)
                Text(detail).font(.subheadline).foregroundStyle(c.ink2)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
            Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(c.ink3)
                .accessibilityHidden(true)
        }
        .frame(maxWidth: .infinity, minHeight: 52, alignment: .leading)
        .contentShape(Rectangle())
    }
}

struct AuralisWordmark: View {
    @Environment(\.omcpColors) private var c

    var body: some View {
        HStack(spacing: 10) {
            AuralisMark().frame(width: 34, height: 34)
            Text(L("app_name"))
                .font(.title3.weight(.semibold))
                .tracking(-0.5)
                .foregroundStyle(c.ink)
        }
        .accessibilityElement(children: .combine)
    }
}

struct AuralisPrimaryButtonStyle: ButtonStyle {
    @Environment(\.omcpColors) private var c
    @Environment(\.isEnabled) private var enabled
    @Environment(\.colorScheme) private var scheme

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.headline)
            .padding(.horizontal, 16).padding(.vertical, 12)
            .frame(maxWidth: .infinity, minHeight: 52)
            .foregroundStyle(scheme == .dark ? OmcpColors.dark.bg : Color.white)
            .background(c.brand, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            .opacity(enabled ? (configuration.isPressed ? 0.8 : 1) : 0.45)
    }
}

private struct AuralisTapTargetStyle: ButtonStyle {
    @Environment(\.omcpColors) private var c
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .frame(minHeight: 44)
            .contentShape(Rectangle())
            .foregroundStyle(configuration.role == .destructive ? c.danger : c.brand)
            .opacity(enabled ? (configuration.isPressed ? 0.7 : 1) : 0.45)
    }
}

struct AuralisAvatar: View {
    @Environment(\.omcpColors) private var c
    let name: String
    var size: CGFloat = 54

    var body: some View {
        Text(String(name.prefix(2)).uppercased())
            .font(.headline)
            .foregroundStyle(c.brand)
            .frame(width: size, height: size)
            .background(c.brandSoft, in: RoundedRectangle(cornerRadius: size * 0.35, style: .continuous))
            .accessibilityHidden(true)
    }
}

/// Recovery material is never copied, logged, or rendered outside the secure app window.
struct AuralisRecoveryWords: View {
    @Environment(\.omcpColors) private var c
    @ScaledMetric(relativeTo: .body) private var columnWidth: CGFloat = 125
    let words: [String]

    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: columnWidth), alignment: .leading)], spacing: 10) {
            ForEach(Array(words.enumerated()), id: \.offset) { index, word in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(index + 1, format: .number).font(.caption.monospacedDigit()).foregroundStyle(c.ink3)
                    Text(word).font(.body.monospaced()).foregroundStyle(c.ink)
                }
                .padding(10).frame(maxWidth: .infinity, alignment: .leading)
                .background(c.surface2, in: RoundedRectangle(cornerRadius: 12))
                .accessibilityElement(children: .combine)
            }
        }
    }
}
