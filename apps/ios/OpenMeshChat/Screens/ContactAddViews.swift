import AVFoundation
import CoreImage.CIFilterBuiltins
import HoplaKit
import SwiftUI

/// Contact add (design pack v0.1.1 §5.19–5.22): chooser, Scan QR, My QR, preview + safety number.
struct ContactAddView: View {
    @Environment(\.omcpColors) private var c
    var body: some View {
        List {
            Section { Text(L("contact_add_subtitle")).font(.subheadline).foregroundStyle(c.ink2) }
            Section {
                NavigationLink { QrScanView() } label: {
                    Label { row("contact_add_scan_qr", "contact_add_scan_qr_desc") } icon: { Image(systemName: "qrcode.viewfinder") }
                }
                NavigationLink { MyQrView() } label: {
                    Label { row("contact_add_my_qr", "contact_add_my_qr_desc") } icon: { Image(systemName: "qrcode") }
                }
            }
        }
        .navigationTitle(L("contact_add_title"))
    }
    private func row(_ t: String, _ d: String) -> some View {
        VStack(alignment: .leading, spacing: 2) { Text(L(t)); Text(L(d)).font(.caption).foregroundStyle(c.ink2) }
    }
}

/// QR image: black on white always, ECC H, quiet zone.
struct QrCodeImage: View {
    let payload: String
    var body: some View {
        let f = CIFilter.qrCodeGenerator()
        f.message = Data(payload.utf8)
        f.correctionLevel = "H"
        let ctx = CIContext()
        if let out = f.outputImage?.transformed(by: CGAffineTransform(scaleX: 10, y: 10)),
           let cg = ctx.createCGImage(out, from: out.extent) {
            return AnyView(Image(decorative: cg, scale: 1).interpolation(.none).resizable().scaledToFit()
                .padding(16).background(Color.white).accessibilityLabel(L("my_qr_a11y")))
        }
        return AnyView(Color.white)
    }
}

struct MyQrView: View {
    @EnvironmentObject private var model: AppModel
    @State private var savedBrightness: CGFloat = 0.5
    var body: some View {
        let id = model.core.identity()
        ScrollView {
            VStack(spacing: 16) {
                if let qr = try? model.core.qrPayload() { QrCodeImage(payload: qr).frame(width: 260, height: 260) }
                Text(id?.displayName ?? "").font(.title3.bold())
                if let p = id?.peerIdHex {
                    Text("omcp1:\(p.prefix(4))…\(p.suffix(4))").font(.footnote.monospaced())
                    Button(L("my_qr_copy")) { UIPasteboard.general.string = p }
                }
                Text(L("my_qr_brightness")).font(.footnote).multilineTextAlignment(.center)
                Text(L("my_qr_privacy")).font(.footnote).foregroundStyle(.secondary).multilineTextAlignment(.center)
            }.padding()
        }
        .navigationTitle(L("my_qr_title"))
        .onAppear { savedBrightness = UIScreen.main.brightness; UIScreen.main.brightness = 1 }
        .onDisappear { UIScreen.main.brightness = savedBrightness }
    }
}

struct QrScanView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var preview: ContactPreview?
    @State private var payload = ""
    @State private var notOmcp = false
    @State private var denied = false
    @State private var openChat: String?

    var body: some View {
        ZStack(alignment: .top) {
            if denied {
                VStack(spacing: 12) {
                    Text(L("camera_off_title")).font(.headline)
                    Button(L("open_settings")) { UIApplication.shared.open(URL(string: UIApplication.openSettingsURLString)!) }
                    NavigationLink(L("show_my_qr_instead")) { MyQrView() }
                }.frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                QrCameraView(onCode: handle, onDenied: { denied = true }).ignoresSafeArea()
                RoundedRectangle(cornerRadius: 16).stroke(notOmcp ? Color.red : .white, lineWidth: 3)
                    .frame(width: 240, height: 240).frame(maxWidth: .infinity, maxHeight: .infinity)
                    .accessibilityLabel(L("viewfinder_a11y"))
                Text(L("scan_hint")).foregroundStyle(.white).padding().background(.black.opacity(0.5), in: Capsule()).padding(.top, 24)
            }
            if notOmcp {
                VStack(alignment: .leading) {
                    Text(L("scan_not_omcp_title")).bold()
                    Text(L("scan_not_omcp_body")).font(.footnote)
                }.padding().background(Color.red.opacity(0.9), in: RoundedRectangle(cornerRadius: 12)).foregroundStyle(.white).padding()
            }
        }
        .navigationTitle(L("contact_add_scan_qr"))
        .navigationDestination(item: $openChat) { ChatThreadView(contactId: $0) }
        .sheet(item: Binding(get: { preview.map(IdentifiedPreview.init) }, set: { preview = $0?.p })) { ip in
            ContactPreviewSheet(preview: ip.p, onAction: { action in add(action) }, onCancel: { preview = nil })
                .presentationDetents([.large])
        }
    }

    /// Never opens URLs from scanned codes; anything that isn't an OMCP code is an error.
    private func handle(_ code: String) {
        guard preview == nil else { return }
        guard let p = try? model.core.previewContact(code) else {
            notOmcp = true
            DispatchQueue.main.asyncAfter(deadline: .now() + 2) { notOmcp = false }
            return
        }
        UINotificationFeedbackGenerator().notificationOccurred(.success)
        payload = code
        preview = p
    }

    private func add(_ action: ContactAddAction) {
        guard let c = try? model.repo.addContact(qr: payload, action: action) else { return }
        preview = nil
        openChat = c.id
    }
}

private struct IdentifiedPreview: Identifiable {
    let p: ContactPreview
    var id: String { p.contact.id }
}

/// §5.22: safety number 4×3 grid; Verify & chat / Chat without verifying / Cancel.
/// Mismatch: no chat action.
struct ContactPreviewSheet: View {
    @Environment(\.omcpColors) private var c
    let preview: ContactPreview
    let onAction: (ContactAddAction) -> Void
    let onCancel: () -> Void

    var body: some View {
        let ct = preview.contact
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text(ct.displayName).font(.title2.bold())
                    Text("omcp1:\(ct.id.prefix(4))…\(ct.id.suffix(4))").font(.footnote.monospaced()).foregroundStyle(c.ink2)
                    if let old = preview.mismatchWith {
                        VStack(alignment: .leading, spacing: 6) {
                            Text(L("sn_mismatch_title")).bold()
                            Text(L("sn_mismatch_body", old.displayName)).font(.footnote)
                        }.padding().background(c.danger.opacity(0.12), in: RoundedRectangle(cornerRadius: 12))
                    } else {
                        Text(L("unverified")).font(.caption.bold()).padding(6).background(c.warnSoft, in: Capsule())
                    }
                    Text(L("sn_section")).font(.headline)
                    SafetyNumberGrid(groups: ct.safetyNumber, differs: preview.mismatchWith.map { Set(zip($0.safetyNumber, ct.safetyNumber).enumerated().filter { $0.element.0 != $0.element.1 }.map(\.offset)) } ?? [])
                    Text(L("sn_compare", ct.displayName)).font(.footnote).foregroundStyle(c.ink2)
                    if preview.mismatchWith == nil {
                        Button { onAction(.verifyAndChat) } label: { Text(L("sn_verify_chat")).frame(maxWidth: .infinity) }
                            .buttonStyle(.borderedProminent)
                        Button { onAction(.chatWithoutVerifying) } label: { Text(L("sn_chat_unverified")).frame(maxWidth: .infinity) }
                            .buttonStyle(.bordered)
                    } else {
                        Button(L("sn_scan_again"), action: onCancel).buttonStyle(.borderedProminent)
                        Button(L("sn_dont_trust"), role: .destructive, action: onCancel)
                    }
                    Button(L("cancel"), action: onCancel).frame(maxWidth: .infinity)
                }.padding()
            }
        }
    }
}

struct SafetyNumberGrid: View {
    @Environment(\.omcpColors) private var c
    let groups: [String]
    var differs: Set<Int> = []
    var body: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible()), count: 3), spacing: 8) {
            ForEach(Array(groups.enumerated()), id: \.offset) { i, g in
                Text(g).font(.body.monospaced())
                    .underline(differs.contains(i)).foregroundStyle(differs.contains(i) ? c.danger : c.ink)
                    .accessibilityLabel(L("sn_group_a11y", i + 1, g.map(String.init).joined(separator: " ")))
            }
        }
    }
}

/// AVCaptureMetadataOutput `.qr` scanner.
struct QrCameraView: UIViewControllerRepresentable {
    let onCode: (String) -> Void
    let onDenied: () -> Void

    func makeUIViewController(context: Context) -> ScannerVC {
        let vc = ScannerVC()
        vc.onCode = onCode
        vc.onDenied = onDenied
        return vc
    }
    func updateUIViewController(_ vc: ScannerVC, context: Context) {}

    final class ScannerVC: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
        var onCode: ((String) -> Void)?
        var onDenied: (() -> Void)?
        private let session = AVCaptureSession()
        private var layer: AVCaptureVideoPreviewLayer?

        override func viewDidLoad() {
            super.viewDidLoad()
            view.backgroundColor = .black
            AVCaptureDevice.requestAccess(for: .video) { ok in
                DispatchQueue.main.async { ok ? self.setup() : self.onDenied?() }
            }
        }

        private func setup() {
            guard let dev = AVCaptureDevice.default(for: .video), let input = try? AVCaptureDeviceInput(device: dev),
                  session.canAddInput(input) else { return }
            session.addInput(input)
            let out = AVCaptureMetadataOutput()
            guard session.canAddOutput(out) else { return }
            session.addOutput(out)
            out.setMetadataObjectsDelegate(self, queue: .main)
            out.metadataObjectTypes = [.qr]
            let l = AVCaptureVideoPreviewLayer(session: session)
            l.videoGravity = .resizeAspectFill
            l.frame = view.bounds
            view.layer.addSublayer(l)
            layer = l
            DispatchQueue.global().async { self.session.startRunning() }
        }

        override func viewDidLayoutSubviews() {
            super.viewDidLayoutSubviews()
            layer?.frame = view.bounds
        }

        override func viewWillDisappear(_ animated: Bool) {
            super.viewWillDisappear(animated)
            session.stopRunning()
        }

        func metadataOutput(_ o: AVCaptureMetadataOutput, didOutput objs: [AVMetadataObject], from c: AVCaptureConnection) {
            if let s = (objs.first as? AVMetadataMachineReadableCodeObject)?.stringValue { onCode?(s) }
        }
    }
}
