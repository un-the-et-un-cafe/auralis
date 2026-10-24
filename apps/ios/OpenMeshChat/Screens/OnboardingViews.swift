import HoplaKit
import LocalAuthentication
import SwiftUI

/// Observable wrapper over the platform-independent `OnboardingFlow` (HoplaKit). Cloud work runs
/// off the main thread (CloudKit / Drive calls block).
@MainActor
final class OnboardingVM: ObservableObject {
    @Published private(set) var s = OnboardingState()
    @Published var busy = false
    private(set) var flow: OnboardingFlow?

    func attach(_ model: AppModel) {
        guard flow == nil else { return }
        let f = model.makeOnboarding()
        f.onChange = { [weak self] st in Task { @MainActor in self?.s = st } }
        flow = f
        s = f.state
        run { $0.detectBackup() }
    }

    /// Run a (possibly blocking) flow step in the background.
    func run(_ step: @escaping (OnboardingFlow) -> Void) {
        guard let flow, !busy else { return }
        busy = true
        DispatchQueue.global(qos: .userInitiated).async {
            step(flow)
            DispatchQueue.main.async { self.s = flow.state; self.busy = false }
        }
    }

    func now(_ step: (OnboardingFlow) -> Void) {
        guard let flow else { return }
        step(flow)
        s = flow.state
    }
}

/// Plain layouts on purpose: the v0.1.2 onboarding design is not approved yet.
/// Welcome → Profile → Secure your account → Done (O1: 4 screens).
struct OnboardingView: View {
    @EnvironmentObject private var model: AppModel
    @StateObject private var vm = OnboardingVM()

    var body: some View {
        NavigationStack {
            Form { content }
                .disabled(vm.busy)
                .overlay { if vm.busy { ProgressView() } }
                .navigationTitle(title)
        }
        .onAppear { vm.attach(model) }
    }

    private var title: String {
        switch vm.s.step {
        case .welcome: return L("app_name")
        case .profile: return L("ob_profile_title")
        case .secure, .writeDown, .quiz: return L("ob_secure_title")
        case .done: return L("ob_done_title")
        case .restorePrompt, .restorePhrase: return L("ob_restore_title")
        }
    }

    @ViewBuilder private var content: some View {
        switch vm.s.step {
        case .welcome: welcome
        case .profile: profile
        case .secure: secure
        case .writeDown: writeDown
        case .quiz: quiz
        case .done: done
        case .restorePrompt: restorePrompt
        case .restorePhrase: restorePhrase
        }
        if let e = vm.s.error {
            Section { Text(L("ob_error_" + e)).foregroundStyle(.red) }
        }
    }

    private var welcome: some View {
        Section {
            Text(L("ob_welcome_body"))
            Button(L("ob_get_started")) { vm.now { $0.getStarted() } }.bold()
            Button(L("ob_restore_with_phrase")) { vm.now { $0.usePhraseInstead() } }
        }
    }

    private var profile: some View {
        Section(footer: Text(L("ob_profile_footer"))) {
            TextField(L("ob_display_name"), text: Binding(get: { vm.s.displayName }, set: { v in vm.now { $0.setName(v) } }))
                .textContentType(.nickname)
            Button(L("ob_next")) { vm.run { $0.profileNext() } }.bold()
        }
    }

    @ViewBuilder private var secure: some View {
        Section(footer: Text(L("ob_secure_honest"))) {
            Text(L("ob_secure_body"))
        }
        Section(header: Text(L("ob_icloud")), footer: Text(L(vm.s.usePassword ? "ob_icloud_pw_footer" : "ob_icloud_nopw_footer"))) {
            if vm.s.iCloudDone {
                Label(L("ob_backup_done"), systemImage: "checkmark.circle.fill").foregroundStyle(.green)
            } else if !vm.s.iCloudAvailable {
                Text(L("ob_icloud_unavailable")).foregroundStyle(.secondary)
            } else {
                Toggle(L("ob_use_password"), isOn: Binding(get: { vm.s.usePassword }, set: { v in vm.now { $0.setUsePassword(v) } }))
                if vm.s.usePassword { passwordField }
                Button(L("ob_backup_icloud")) { vm.run { $0.backupToICloud() } }
                    .disabled(vm.s.usePassword && !vm.s.passwordOk)
            }
        }
        Section(header: Text(L("ob_drive"))) {
            if vm.s.driveDone {
                Label(L("ob_backup_done"), systemImage: "checkmark.circle.fill").foregroundStyle(.green)
            } else if !vm.s.driveAvailable {
                Text(L("ob_drive_unavailable")).foregroundStyle(.secondary)
            } else {
                passwordField
                Button(L("ob_backup_drive")) { vm.run { $0.backupToDrive() } }.disabled(!vm.s.passwordOk)
            }
        }
        Section(header: Text(L("ob_write_down"))) {
            if vm.s.writtenDone {
                Label(L("ob_backup_done"), systemImage: "checkmark.circle.fill").foregroundStyle(.green)
            } else {
                Button(L("ob_show_words")) { vm.now { $0.writeItDown() } }
            }
        }
        Section {
            Button(L("ob_continue")) { vm.now { _ = $0.secureNext() } }.bold().disabled(!vm.s.anyBackupDone)
            Button(L("ob_remind_later")) { vm.now { $0.remindLater() } }
        } footer: { Text(L("ob_remind_footer")) }
    }

    private var passwordField: some View {
        VStack(alignment: .leading) {
            SecureField(L("ob_backup_password"), text: Binding(get: { vm.s.password }, set: { v in vm.now { $0.setPassword(v) } }))
                .textContentType(.newPassword)
            if vm.s.passwordTooShort { Text(L("ob_pw_too_short")).font(.caption).foregroundStyle(.red) }
            else if !vm.s.password.isEmpty && !vm.s.passwordOk { Text(L("ob_pw_weak")).font(.caption).foregroundStyle(.orange) }
        }
    }

    @ViewBuilder private var writeDown: some View {
        Section(footer: Text(L("ob_words_footer"))) {
            ForEach(Array(vm.s.words.enumerated()), id: \.offset) { i, w in
                Text("\(i + 1). \(w)").font(.body.monospaced())
            }
        }
        Section {
            Button(L("ob_wrote_it_down")) { vm.now { $0.startQuiz() } }.bold()
            Button(L("ob_back")) { vm.now { $0.backToSecure() } }
        }
    }

    @ViewBuilder private var quiz: some View {
        Section(header: Text(L("ob_quiz_title"))) {
            ForEach(Array(vm.s.quiz.enumerated()), id: \.offset) { i, f in
                VStack(alignment: .leading) {
                    TextField(L("ob_quiz_word", f.position + 1), text: Binding(get: { f.input }, set: { v in vm.now { $0.setQuizInput(i, v) } }))
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                    if f.error { Text(L("ob_quiz_wrong")).font(.caption).foregroundStyle(.red) }
                }
            }
        }
        Section {
            Button(L("ob_check")) { vm.now { _ = $0.submitQuiz() } }.bold()
            Button(L("ob_back")) { vm.now { $0.backToSecure() } }
        }
    }

    @ViewBuilder private var done: some View {
        Section(footer: Text(L("my_qr_privacy"))) {
            Text(L("ob_done_body"))
            if let qr = try? model.core.qrPayload() { QrCodeImage(payload: qr).frame(width: 220, height: 220).frame(maxWidth: .infinity) }
        }
        Section {
            Button(L("ob_start_chatting")) { vm.now { $0.finish() } }.bold()
        }
    }

    @ViewBuilder private var restorePrompt: some View {
        Section(footer: Text(L("ob_restore_honest"))) {
            Text(L("ob_restore_found", vm.s.foundMethod == backupICloud ? "iCloud" : "Google Drive"))
            if vm.s.foundBackup?.needsPassword == true {
                SecureField(L("ob_backup_password"), text: Binding(get: { vm.s.restorePassword }, set: { v in vm.now { $0.setRestorePassword(v) } }))
            }
            if vm.s.restoreLockMs > 0 { Text(L("ob_restore_locked", Int(vm.s.restoreLockMs / 1000))).foregroundStyle(.orange) }
            Button(L("ob_restore")) { vm.run { $0.restoreFromCloud() } }.bold()
            Button(L("ob_use_phrase")) { vm.now { $0.usePhraseInstead() } }
            Button(L("ob_start_fresh")) { vm.now { $0.backToWelcome() } }
        }
    }

    @ViewBuilder private var restorePhrase: some View {
        Section(footer: Text(L("ob_phrase_footer"))) {
            TextEditor(text: Binding(get: { vm.s.restorePhrase }, set: { v in vm.now { $0.setRestorePhrase(v) } }))
                .frame(minHeight: 120).textInputAutocapitalization(.never).autocorrectionDisabled()
            Button(L("ob_restore")) { vm.now { _ = $0.restoreFromPhrase() } }.bold()
            Button(L("ob_back")) { vm.now { $0.backToWelcome() } }
        }
    }
}

/// Persistent banner while "Remind me later" is running, then a blocking sheet (O6).
struct BackupReminderBanner: View {
    @EnvironmentObject private var model: AppModel
    var body: some View {
        NavigationLink { BackupSettingsView() } label: {
            Label(L("backup_banner"), systemImage: "exclamationmark.shield").font(.footnote)
                .frame(maxWidth: .infinity, alignment: .leading).padding(10)
                .background(Color.orange.opacity(0.15))
        }
    }
}

struct BackupRequiredView: View {
    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                Text(L("backup_required_body")).padding()
                BackupSettingsView()
            }
            .navigationTitle(L("backup_required_title"))
        }
        .interactiveDismissDisabled()
    }
}

/// Settings › Backup and restore.
struct BackupSettingsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var password = ""
    @State private var usePassword = false
    @State private var phrase: [String] = []
    @State private var error: String?
    @State private var busy = false
    @State private var confirmDelete: String?

    private var settings: BackupSettings { model.makeBackupSettings() }

    var body: some View {
        let s = settings
        Form {
            Section(footer: Text(L("ob_secure_honest"))) {
                LabeledContent(L("backup_status"), value: s.methods.isEmpty ? L("backup_none") : s.methods.sorted().map { L("method_" + $0) }.joined(separator: ", "))
                if s.lastCloudBackupMs > 0 {
                    LabeledContent(L("backup_last"), value: Date(timeIntervalSince1970: Double(s.lastCloudBackupMs) / 1000).formatted(date: .abbreviated, time: .shortened))
                }
            }
            Section(header: Text(L("backup_password_section"))) {
                Toggle(L("ob_use_password"), isOn: $usePassword)
                if usePassword { SecureField(L("ob_backup_password"), text: $password).textContentType(.newPassword) }
            }
            ForEach([backupICloud, backupDrive], id: \.self) { m in
                Section(header: Text(L("method_" + m))) {
                    if !s.available(m) {
                        Text(L(m == backupICloud ? "ob_icloud_unavailable" : "ob_drive_unavailable")).foregroundStyle(.secondary)
                    } else {
                        Button(L(s.methods.contains(m) ? "backup_now_change" : "backup_add")) { backUp(m) }
                            .disabled(busy || ((usePassword || m == backupDrive) && !model.core.checkBackupPassword(password).ok))
                        if s.methods.contains(m) {
                            Button(L("backup_delete"), role: .destructive) { confirmDelete = m }
                        }
                    }
                }
            }
            Section(header: Text(L("ob_write_down"))) {
                if phrase.isEmpty {
                    Button(L("backup_view_phrase")) { reveal() }
                } else {
                    ForEach(Array(phrase.enumerated()), id: \.offset) { i, w in Text("\(i + 1). \(w)").font(.body.monospaced()) }
                    Button(L("ob_wrote_it_down")) { s.markWritten(); phrase = []; model.touch() }
                }
            }
            if let error { Section { Text(error).foregroundStyle(.red) } }
        }
        .navigationTitle(L("backup_title"))
        .confirmationDialog(L("backup_delete_confirm"), isPresented: Binding(get: { confirmDelete != nil }, set: { if !$0 { confirmDelete = nil } })) {
            Button(L("backup_delete"), role: .destructive) {
                if let m = confirmDelete { work { try s.deleteCloudBackup(m) } }
            }
        }
    }

    private func work(_ f: @escaping () throws -> Void) {
        busy = true
        DispatchQueue.global().async {
            let e: String? = (try? f()) == nil ? L("backup_failed") : nil
            DispatchQueue.main.async { busy = false; error = e; password = ""; model.touch() }
        }
    }

    private func backUp(_ m: String) {
        let pw = (usePassword || m == backupDrive) ? password : nil
        let s = settings
        work { try s.backUp(m, password: pw) }
    }

    /// Biometric or device passcode required.
    private func reveal() {
        let ctx = LAContext()
        ctx.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: L("backup_view_phrase_reason")) { ok, _ in
            DispatchQueue.main.async { if ok { phrase = (try? settings.revealPhrase()) ?? [] } }
        }
    }
}
