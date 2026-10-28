import AVFoundation
import CoreImage.CIFilterBuiltins
import HoplaKit
import SwiftUI

/// Contact add (design pack v0.1.1 §5.19–5.22): chooser, Scan QR, My QR, preview + safety number.
struct ContactAddView: View {
    @Environment(\.omcpColors) private var c
    var body: some View {
        AuralisSettingsPage {
            VStack(alignment: .leading, spacing: 12) {
                AuralisMark().frame(width: 64, height: 64)
                Text(L("auralis_contact_heading")).font(.largeTitle.weight(.bold)).tracking(-1)
                Text(L("contact_add_subtitle")).foregroundStyle(c.ink2)
            }
            AuralisCard {
                NavigationLink { QrScanView() } label: {
                    AuralisActionRow(icon: "qrcode.viewfinder", title: L("contact_add_scan_qr"), detail: L("contact_add_scan_qr_desc"))
                }
                Divider().overlay(c.line)
                NavigationLink { MyQrView() } label: {
                    AuralisActionRow(icon: "qrcode", title: L("contact_add_my_qr"), detail: L("contact_add_my_qr_desc"))
                }
            }
            Label(L("my_qr_privacy"), systemImage: "hand.raised")
                .font(.footnote).foregroundStyle(c.ink2)
        }
        .navigationTitle(L("contact_add_title"))
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
        AuralisSettingsPage {
            AuralisWordmark()
            AuralisCard {
                VStack(spacing: 18) {
                    Text(id?.displayName ?? "").font(.title2.weight(.semibold))
                    if let qr = try? model.core.qrPayload() {
                        QrCodeImage(payload: qr).frame(maxWidth: 260).aspectRatio(1, contentMode: .fit)
                            .clipShape(RoundedRectangle(cornerRadius: 18))
                    }
                    if let p = id?.peerIdHex {
                        Text("omcp1:\(p.prefix(4))…\(p.suffix(4))").font(.footnote.monospaced())
                        Button(L("my_qr_copy")) { UIPasteboard.general.string = p }.frame(minHeight: 44)
                    }
                }
                .frame(maxWidth: .infinity)
            }
            Text(L("my_qr_brightness")).font(.subheadline)
            Label(L("my_qr_privacy"), systemImage: "lock.shield")
                .font(.footnote).foregroundStyle(.secondary)
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
    @State private var staleQr = false
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
                RoundedRectangle(cornerRadius: 28).stroke(notOmcp ? Color.red : Color(hex: 0x84E4CA), lineWidth: 3)
                    .frame(width: 240, height: 240).frame(maxWidth: .infinity, maxHeight: .infinity)
                    .accessibilityLabel(L("viewfinder_a11y"))
                Text(L("scan_hint")).foregroundStyle(.white).padding().background(.black.opacity(0.5), in: Capsule()).padding(.top, 24)
            }
            if staleQr {
                Text(L("qr_stale")).font(.footnote).padding().background(Color.orange.opacity(0.9), in: RoundedRectangle(cornerRadius: 12)).foregroundStyle(.white).padding()
                    .onTapGesture { staleQr = false }
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
        let c: Contact
        do { c = try model.repo.addContact(qr: payload, action: action) } catch CoreError.verificationFailed {
            // RS2: a pre-restore QR no longer verifies.
            preview = nil
            staleQr = true
            return
        } catch { return }
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
            AuralisSettingsPage {
                AuralisCard {
                    HStack(spacing: 16) {
                        AuralisAvatar(name: ct.displayName, size: 64)
                        VStack(alignment: .leading, spacing: 8) {
                            Text(ct.displayName).font(.title2.weight(.semibold))
                            Text("omcp1:\(ct.id.prefix(4))…\(ct.id.suffix(4))")
                                .font(.footnote.monospaced()).foregroundStyle(c.ink2)
                            Text(L("unverified")).font(.caption.weight(.semibold)).foregroundStyle(c.warn)
                        }
                    }
                }
                if let old = preview.mismatchWith {
                    VStack(alignment: .leading, spacing: 10) {
                        Label(L("sn_mismatch_title"), systemImage: "exclamationmark.shield").font(.headline)
                        Text(L("sn_mismatch_body", old.displayName)).font(.subheadline)
                    }
                    .padding(22).frame(maxWidth: .infinity, alignment: .leading)
                    .foregroundStyle(c.danger).background(c.dangerSoft, in: RoundedRectangle(cornerRadius: 24))
                }
                AuralisSettingsSection(L("sn_section"), footer: L("sn_compare", ct.displayName)) {
                    SafetyNumberGrid(groups: ct.safetyNumber, differs: preview.mismatchWith.map {
                        Set(zip($0.safetyNumber, ct.safetyNumber).enumerated()
                            .filter { $0.element.0 != $0.element.1 }.map(\.offset))
                    } ?? [])
                }
                VStack(spacing: 12) {
                    if preview.mismatchWith == nil {
                        Button(L("sn_verify_chat")) { onAction(.verifyAndChat) }
                            .buttonStyle(AuralisPrimaryButtonStyle())
                        Button(L("sn_chat_unverified")) { onAction(.chatWithoutVerifying) }
                            .frame(maxWidth: .infinity, minHeight: 44)
                    } else {
                        Button(L("sn_scan_again"), action: onCancel).buttonStyle(AuralisPrimaryButtonStyle())
                        Button(L("sn_dont_trust"), role: .destructive, action: onCancel).frame(minHeight: 44)
                    }
                    Button(L("cancel"), action: onCancel).frame(maxWidth: .infinity, minHeight: 44)
                }
            }
            .navigationTitle(L("contact_add_title"))
        }
    }
}

struct SafetyNumberGrid: View {
    @Environment(\.omcpColors) private var c
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    let groups: [String]
    var differs: Set<Int> = []
    var body: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible()), count: dynamicTypeSize.isAccessibilitySize ? 1 : 3), spacing: 8) {
            ForEach(Array(groups.enumerated()), id: \.offset) { i, g in
                Text(g).font(.body.monospaced())
                    .underline(differs.contains(i)).foregroundStyle(differs.contains(i) ? c.danger : c.ink)
                    .padding(.vertical, 12).frame(maxWidth: .infinity)
                    .background(c.surface2, in: RoundedRectangle(cornerRadius: 10))
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
        private let sessionQueue = DispatchQueue(label: "auralis.camera")
        private var configured = false
        private var isVisible = false
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
            configured = true
            if isVisible { setRunning(true) }
        }

        private func setRunning(_ running: Bool) {
            sessionQueue.async {
                if running && !self.session.isRunning { self.session.startRunning() }
                else if !running && self.session.isRunning { self.session.stopRunning() }
            }
        }

        override func viewWillAppear(_ animated: Bool) {
            super.viewWillAppear(animated)
            isVisible = true
            if configured { setRunning(true) }
        }

        override func viewDidLayoutSubviews() {
            super.viewDidLayoutSubviews()
            layer?.frame = view.bounds
        }

        override func viewWillDisappear(_ animated: Bool) {
            super.viewWillDisappear(animated)
            isVisible = false
            setRunning(false)
        }

        func metadataOutput(_ o: AVCaptureMetadataOutput, didOutput objs: [AVMetadataObject], from c: AVCaptureConnection) {
            if let s = (objs.first as? AVMetadataMachineReadableCodeObject)?.stringValue { onCode?(s) }
        }
    }
}
