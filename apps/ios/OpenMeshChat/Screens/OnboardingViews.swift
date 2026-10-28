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

/// Auralis presentation over the unchanged onboarding state machine.
/// Welcome → Profile → Secure your account → Done (O1: 4 screens).
struct OnboardingView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c
    @StateObject private var vm = OnboardingVM()

    var body: some View {
        NavigationStack {
            Group {
                if vm.s.step == .welcome {
                    welcome
                } else {
                    AuralisSettingsPage {
                        onboardingHeading
                        content
                    }
                        .auralisScreen()
                        .navigationTitle(title)
                }
            }
            .disabled(vm.busy)
            .overlay {
                if vm.busy {
                    ProgressView().padding(24)
                        .background(c.surface, in: RoundedRectangle(cornerRadius: 20))
                }
            }
        }
        .onAppear { vm.attach(model) }
    }

    @ViewBuilder private var onboardingHeading: some View {
        VStack(alignment: .leading, spacing: 14) {
            AuralisWordmark()
            if let step = stepNumber {
                Text(L("auralis_onboarding_step", step, 4)).font(.caption.weight(.semibold)).foregroundStyle(c.brand)
                ProgressView(value: Double(step), total: 4).tint(c.brand)
            }
            Text(title).font(.largeTitle.weight(.bold)).tracking(-1)
        }
    }

    private var stepNumber: Int? {
        switch vm.s.step {
        case .welcome: return 1
        case .profile: return 2
        case .secure, .writeDown, .quiz: return 3
        case .done: return 4
        case .restorePrompt, .restorePhrase: return nil
        }
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
        case .welcome: EmptyView()
        case .profile: profile
        case .secure: secure
        case .writeDown: writeDown
        case .quiz: quiz
        case .done: done
        case .restorePrompt: restorePrompt
        case .restorePhrase: restorePhrase
        }
        if let e = vm.s.error {
            AuralisSettingsSection { Text(L("ob_error_" + e)).foregroundStyle(.red) }
        }
    }

    private var welcome: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                AuralisWordmark()
                AuralisMark().frame(width: 116, height: 116)
                    .padding(.top, 24)
                VStack(alignment: .leading, spacing: 16) {
                    Text(L("auralis_welcome_title"))
                        .font(.largeTitle.weight(.bold)).tracking(-1.2)
                        .fixedSize(horizontal: false, vertical: true)
                    Text(L("ob_welcome_body"))
                        .font(.body).foregroundStyle(c.ink2)
                        .fixedSize(horizontal: false, vertical: true)
                }
                VStack(alignment: .leading, spacing: 18) {
                    welcomeBenefit("point.3.connected.trianglepath.dotted", "auralis_benefit_mesh")
                    welcomeBenefit("lock.shield", "auralis_benefit_private")
                    welcomeBenefit("person.crop.circle.badge.checkmark", "auralis_benefit_identity")
                }
                .padding(22).frame(maxWidth: .infinity, alignment: .leading)
                .background(c.surface, in: RoundedRectangle(cornerRadius: 24, style: .continuous))
                VStack(spacing: 12) {
                    Button(L("ob_get_started")) { vm.now { $0.getStarted() } }
                        .buttonStyle(AuralisPrimaryButtonStyle())
                    Button(L("ob_restore_with_phrase")) { vm.now { $0.usePhraseInstead() } }
                        .font(.subheadline.weight(.semibold)).frame(minHeight: 44)
                }
                if let e = vm.s.error { Text(L("ob_error_" + e)).foregroundStyle(c.danger) }
            }
            .padding(28)
        }
        .auralisScreen()
        .toolbar(.hidden, for: .navigationBar)
    }

    private func welcomeBenefit(_ icon: String, _ key: String) -> some View {
        Label {
            Text(L(key)).font(.subheadline).foregroundStyle(c.ink)
        } icon: {
            Image(systemName: icon).foregroundStyle(c.brand).frame(width: 24)
        }
    }

    private var profile: some View {
        AuralisSettingsSection(footer: L("ob_profile_footer")) {
            TextField(L("ob_display_name"), text: Binding(get: { vm.s.displayName }, set: { v in vm.now { $0.setName(v) } }))
                .textContentType(.nickname)
            Button(L("ob_next")) { vm.run { $0.profileNext() } }.buttonStyle(AuralisPrimaryButtonStyle())
        }
    }

    @ViewBuilder private var secure: some View {
        AuralisSettingsSection(footer: L("ob_secure_honest")) {
            Text(L("ob_secure_body"))
        }
        AuralisSettingsSection(L("ob_icloud"), footer: L(vm.s.usePassword ? "ob_icloud_pw_footer" : "ob_icloud_nopw_footer")) {
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
        AuralisSettingsSection(L("ob_drive")) {
            if vm.s.driveDone {
                Label(L("ob_backup_done"), systemImage: "checkmark.circle.fill").foregroundStyle(.green)
            } else if !vm.s.driveAvailable {
                Text(L("ob_drive_unavailable")).foregroundStyle(.secondary)
            } else {
                passwordField
                Button(L("ob_backup_drive")) { vm.run { $0.backupToDrive() } }.disabled(!vm.s.passwordOk)
            }
        }
        AuralisSettingsSection(L("ob_write_down")) {
            if vm.s.writtenDone {
                Label(L("ob_backup_done"), systemImage: "checkmark.circle.fill").foregroundStyle(.green)
            } else {
                Button(L("ob_show_words")) { vm.now { $0.writeItDown() } }
            }
        }
        AuralisSettingsSection(footer: L("ob_remind_footer")) {
            Button(L("ob_continue")) { vm.now { _ = $0.secureNext() } }.buttonStyle(AuralisPrimaryButtonStyle()).disabled(!vm.s.anyBackupDone)
            Button(L("ob_remind_later")) { vm.now { $0.remindLater() } }
        }
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
        AuralisSettingsSection(footer: L("ob_words_footer")) {
            AuralisRecoveryWords(words: vm.s.words)
        }
        AuralisSettingsSection {
            Button(L("ob_wrote_it_down")) { vm.now { $0.startQuiz() } }.buttonStyle(AuralisPrimaryButtonStyle())
            Button(L("ob_back")) { vm.now { $0.backToSecure() } }
        }
    }

    @ViewBuilder private var quiz: some View {
        AuralisSettingsSection(L("ob_quiz_title")) {
            ForEach(Array(vm.s.quiz.enumerated()), id: \.offset) { i, f in
                VStack(alignment: .leading) {
                    TextField(L("ob_quiz_word", f.position + 1), text: Binding(get: { f.input }, set: { v in vm.now { $0.setQuizInput(i, v) } }))
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                    if f.error { Text(L("ob_quiz_wrong")).font(.caption).foregroundStyle(.red) }
                }
            }
        }
        AuralisSettingsSection {
            Button(L("ob_check")) { vm.now { _ = $0.submitQuiz() } }.buttonStyle(AuralisPrimaryButtonStyle())
            Button(L("ob_back")) { vm.now { $0.backToSecure() } }
        }
    }

    @ViewBuilder private var done: some View {
        AuralisSettingsSection(footer: L("my_qr_privacy")) {
            Text(L("ob_done_body"))
            if let qr = try? model.core.qrPayload() { QrCodeImage(payload: qr).frame(width: 220, height: 220).frame(maxWidth: .infinity) }
        }
        AuralisSettingsSection {
            Button(L("ob_start_chatting")) { vm.now { $0.finish() } }.buttonStyle(AuralisPrimaryButtonStyle())
        }
    }

    @ViewBuilder private var restorePrompt: some View {
        AuralisSettingsSection(footer: L("ob_restore_honest")) {
            Text(L("ob_restore_found", vm.s.foundMethod == backupICloud ? "iCloud" : "Google Drive"))
            if vm.s.foundBackup?.needsPassword == true {
                SecureField(L("ob_backup_password"), text: Binding(get: { vm.s.restorePassword }, set: { v in vm.now { $0.setRestorePassword(v) } }))
            }
            if vm.s.restoreLockMs > 0 { Text(L("ob_restore_locked", Int(vm.s.restoreLockMs / 1000))).foregroundStyle(.orange) }
            Button(L("ob_restore")) { vm.run { $0.restoreFromCloud() } }.buttonStyle(AuralisPrimaryButtonStyle())
            Button(L("ob_use_phrase")) { vm.now { $0.usePhraseInstead() } }
            Button(L("ob_start_fresh")) { vm.now { $0.backToWelcome() } }
        }
    }

    @ViewBuilder private var restorePhrase: some View {
        AuralisSettingsSection(footer: L("ob_phrase_footer")) {
            TextEditor(text: Binding(get: { vm.s.restorePhrase }, set: { v in vm.now { $0.setRestorePhrase(v) } }))
                .frame(minHeight: 120).textInputAutocapitalization(.never).autocorrectionDisabled()
            Button(L("ob_restore")) { vm.now { _ = $0.restoreFromPhrase() } }.buttonStyle(AuralisPrimaryButtonStyle())
            Button(L("ob_back")) { vm.now { $0.backToWelcome() } }
        }
    }
}

/// Persistent banner while "Remind me later" is running, then a blocking sheet (O6).
struct BackupReminderBanner: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c
    var body: some View {
        NavigationLink { BackupSettingsView() } label: {
            Label(L("backup_banner"), systemImage: "exclamationmark.shield").font(.footnote.weight(.medium))
                .foregroundStyle(c.warn)
                .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading).padding(14)
                .background(c.warnSoft, in: RoundedRectangle(cornerRadius: 18))
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
    @Environment(\.scenePhase) private var scenePhase
    @State private var password = ""
    @State private var usePassword = false
    @State private var phrase: [String] = []
    @State private var error: String?
    @State private var busy = false
    @State private var confirmDelete: String?
    @State private var isVisible = false
    @State private var revealRequest: UUID?

    private var settings: BackupSettings { model.makeBackupSettings() }

    var body: some View {
        let s = settings
        AuralisSettingsPage {
            AuralisSettingsSection(footer: L("ob_secure_honest")) {
                LabeledContent(L("backup_status"), value: s.methods.isEmpty ? L("backup_none") : s.methods.sorted().map { L("method_" + $0) }.joined(separator: ", "))
                if s.lastCloudBackupMs > 0 {
                    LabeledContent(L("backup_last"), value: Date(timeIntervalSince1970: Double(s.lastCloudBackupMs) / 1000).formatted(date: .abbreviated, time: .shortened))
                }
            }
            AuralisSettingsSection(L("backup_password_section")) {
                Toggle(L("ob_use_password"), isOn: $usePassword)
                if usePassword { SecureField(L("ob_backup_password"), text: $password).textContentType(.newPassword) }
            }
            ForEach([backupICloud, backupDrive], id: \.self) { m in
                AuralisSettingsSection(L("method_" + m)) {
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
            AuralisSettingsSection(L("ob_write_down")) {
                if phrase.isEmpty {
                    Button(L("backup_view_phrase")) { reveal() }
                } else {
                    AuralisRecoveryWords(words: phrase)
                    Button(L("ob_wrote_it_down")) { s.markWritten(); phrase = []; model.touch() }
                }
            }
            if let error { AuralisSettingsSection { Text(error).foregroundStyle(.red) } }
        }
        .disabled(busy)
        .overlay { if busy { ProgressView().padding(24).background(.regularMaterial, in: RoundedRectangle(cornerRadius: 20)) } }
        .navigationTitle(L("backup_title"))
        .onAppear { isVisible = true }
        .onDisappear { isVisible = false; phrase = []; revealRequest = nil }
        .onChange(of: scenePhase) { _, phase in
            if phase != .active { phrase = [] }
            // Face ID can briefly make the scene inactive. Only backgrounding cancels its result.
            if phase == .background { revealRequest = nil }
        }
        .confirmationDialog(L("backup_delete_confirm"), isPresented: Binding(get: { confirmDelete != nil }, set: { if !$0 { confirmDelete = nil } })) {
            Button(L("backup_delete"), role: .destructive) {
                if let m = confirmDelete { work { try s.deleteCloudBackup(m) } }
            }
        }
    }

    private func work(_ f: @escaping () throws -> Void) {
        guard !busy else { return }
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
        guard !busy, revealRequest == nil else { return }
        let request = UUID()
        revealRequest = request
        let ctx = LAContext()
        ctx.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: L("backup_view_phrase_reason")) { ok, _ in
            DispatchQueue.main.async {
                guard isVisible, revealRequest == request else { return }
                revealRequest = nil
                if ok { phrase = (try? settings.revealPhrase()) ?? [] }
            }
        }
    }
}
