//
//  PairingView.swift
//  UnpeelIOS
//
//  Pair this phone with a Mac running Unpeel. The Mac's Settings ▸ Mobile
//  section shows a QR with a compact pairing code (RemotePairingCode; a
//  one-time token, 5-minute TTL — legacy Macs encode the payload JSON);
//  scanning it — or pasting the same code, the simulator path — exchanges
//  it for a persistent device token via POST /mobile/pair.
//

import AVFoundation
import SwiftUI
import UnpeelShared
#if os(iOS)
import UIKit
#endif

struct PairingView: View {
    @ObservedObject var connection: RemoteConnectionStore
    @Environment(\.dismiss) private var dismiss
    @State private var pastedPayload = ""
    @State private var pairingInFlight = false
    @State private var errorMessage: String?
    @State private var scannerPaused = false
    @State private var addingMac = false
    @State private var unpairCandidate: PairedMacRecord?
    @ObservedObject private var push = PushManager.shared

    var body: some View {
        NavigationStack {
            ZStack {
                TerminalChrome.background.ignoresSafeArea()
                ScrollView {
                    VStack(spacing: 20) {
                        if connection.pairedMacs.isEmpty {
                            VStack(spacing: 12) {
                                UnpeelBrandLogo(size: 68)
                                Text("Unpeel")
                                    .font(.system(size: 22, weight: .semibold))
                                    .foregroundStyle(.white)
                            }
                            .frame(maxWidth: .infinity)
                            .padding(.top, 8)
                        }
                        connectionSection
                        if showPairingInputs {
                            if addingMac {
                                addMacHeader
                            }
                            scannerSection
                            pasteSection
                        }
                        if let errorMessage {
                            Text(errorMessage)
                                .font(.footnote.weight(.medium))
                                .foregroundStyle(.red.opacity(0.9))
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        notificationSection
                        securitySection
                        if #available(iOS 26.0, *) {
                            dictationSection
                        }
                        #if DEBUG
                        developerSection
                        #endif
                    }
                    .padding(20)
                }
            }
            .navigationTitle(navigationTitle)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .preferredColorScheme(.dark)
            .confirmationDialog(
                "Forget \(unpairCandidate?.macName ?? "this Mac")?",
                isPresented: Binding(
                    get: { unpairCandidate != nil },
                    set: { if !$0 { unpairCandidate = nil } }
                ),
                titleVisibility: .visible,
                presenting: unpairCandidate
            ) { record in
                Button("Forget \(record.macName)", role: .destructive) {
                    connection.unpair(macID: record.macID)
                }
                Button("Cancel", role: .cancel) {}
            } message: { _ in
                Text("This phone will no longer be able to control it. You can revoke its device entry in the Mac's Settings too.")
            }
        }
        // Mac list = little content, so a short sheet instead of full
        // height; the scanner state keeps the room it needs.
        .presentationDetents(showPairingInputs ? [.large] : [.medium, .large])
        .presentationDragIndicator(.visible)
    }

    private var navigationTitle: String {
        if connection.pairedMacs.isEmpty { return "Pair with your Mac" }
        return addingMac ? "Add a Device" : "Your Devices"
    }

    /// The scanner/paste inputs show when there is nothing paired yet, or
    /// when the user explicitly asked to add another Mac.
    private var showPairingInputs: Bool {
        connection.pairedMacs.isEmpty || addingMac
    }

    private var notificationSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Notifications")
                .font(.caption.weight(.semibold))
                .textCase(.uppercase)
                .foregroundStyle(.white.opacity(0.5))
                .frame(maxWidth: .infinity, alignment: .leading)

            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Image(systemName: push.registrationState.permissionWasDenied
                    ? "bell.slash.fill" : "bell.badge.fill")
                    .foregroundStyle(push.registrationState.permissionWasDenied
                        ? Color.orange : Color.cyan)
                Text(push.registrationState.diagnosticLabel)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(0.82))
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            if push.registrationState.permissionWasDenied {
                Button("Open iOS Settings") { openNotificationSettings() }
                    .font(.subheadline.weight(.semibold))
                    .tint(.cyan)
            } else if push.registrationState.canRetry {
                Button("Retry registration") { push.requestAndRegister() }
                    .font(.subheadline.weight(.semibold))
                    .tint(.cyan)
            }

            Text("A ready APNs token is sent to each reachable paired Host. "
                + "Unpeel Link then delivers needs-input and opted-in finished alerts.")
                .font(.caption)
                .foregroundStyle(.white.opacity(0.5))
        }
        .padding(14)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Color.white.opacity(0.06))
        )
    }

    private func openNotificationSettings() {
        #if os(iOS)
        guard let url = URL(string: UIApplication.openSettingsURLString) else { return }
        UIApplication.shared.open(url)
        #endif
    }

    private var addMacHeader: some View {
        HStack {
            Text("Pair the new device")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white.opacity(0.85))
            Spacer()
            Button("Cancel") { addingMac = false }
                .font(.subheadline)
                .tint(.cyan)
        }
    }

    /// App lock: require Face ID / Touch ID (with passcode fallback) to open
    /// the app. Enabling authenticates once up front so the toggle only arms
    /// when the method actually works.
    private var securitySection: some View {
        let capability = AppLockManager.capability()
        return VStack(alignment: .leading, spacing: 12) {
            Text("Security")
                .font(.caption.weight(.semibold))
                .textCase(.uppercase)
                .foregroundStyle(.white.opacity(0.5))
                .frame(maxWidth: .infinity, alignment: .leading)

            Toggle(isOn: Binding(
                get: { AppLockManager.shared.isEnabled },
                set: { enabled in
                    if enabled {
                        Task { _ = await AppLockManager.shared.enable() }
                    } else {
                        AppLockManager.shared.disable()
                    }
                }
            )) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Require \(AppLockManager.methodLabel())")
                        .foregroundStyle(.white)
                    Text("Locks Unpeel when you leave the app.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.5))
                }
            }
            .disabled(!capability.available)

            if !capability.available {
                Text("Set a device passcode (and enroll Face ID) in the iOS Settings app to use the app lock.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(0.5))
            }
        }
        .padding(14)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Color.white.opacity(0.06))
        )
    }

    /// Dictation: the optional Apple Intelligence cleanup pass over finished
    /// dictations (`DictationReflection.swift`). iOS 26+ only; on devices
    /// without Apple Intelligence the pass silently falls back to verbatim,
    /// so the toggle stays visible with the requirement noted below it.
    @available(iOS 26.0, *)
    private var dictationSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Dictation")
                .font(.caption.weight(.semibold))
                .textCase(.uppercase)
                .foregroundStyle(.white.opacity(0.5))
                .frame(maxWidth: .infinity, alignment: .leading)

            Toggle(isOn: Binding(
                get: { DictationSettings.shared.reflectionEnabled },
                set: { DictationSettings.shared.reflectionEnabled = $0 }
            )) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Polish with Apple Intelligence")
                        .foregroundStyle(.white)
                    Text("Cleans up punctuation and filler words on-device before dictated text is pasted. Requires Apple Intelligence.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.5))
                }
            }
        }
        .padding(14)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Color.white.opacity(0.06))
        )
    }

    #if DEBUG
    /// Dev-only toggles (DEBUG builds). Add flags to `DevSettings` and a Toggle
    /// here; read the flag wherever you need it.
    private var developerSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Developer")
                .font(.caption.weight(.semibold))
                .textCase(.uppercase)
                .foregroundStyle(.white.opacity(0.5))
                .frame(maxWidth: .infinity, alignment: .leading)

            Toggle(isOn: Binding(
                get: { DevSettings.shared.showTerminalBounds },
                set: { DevSettings.shared.showTerminalBounds = $0 }
            )) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Show terminal bounds")
                        .foregroundStyle(.white)
                    Text("Red outline around the terminal grid.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.5))
                }
            }
        }
        .padding(14)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Color.white.opacity(0.06))
        )
    }
    #endif

    // MARK: - Sections

    /// The paired-Mac list (tap to switch, minus to forget, footer to add),
    /// or the unpaired/dev-bridge guidance when nothing is paired yet.
    @ViewBuilder
    private var connectionSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            if !connection.pairedMacs.isEmpty {
                Text("Your Devices")
                    .font(.caption.weight(.semibold))
                    .textCase(.uppercase)
                    .foregroundStyle(.white.opacity(0.5))
                ForEach(connection.pairedMacs, id: \.macID) { record in
                    macRow(record)
                }
                Button {
                    addingMac = true
                } label: {
                    Label("Add a Device", systemImage: "plus.circle")
                        .font(.subheadline.weight(.semibold))
                }
                .tint(.cyan)
                .padding(.top, 4)
            } else if RemoteConnectionStore.devBridgeAvailable {
                Label("Using the local dev bridge (Simulator)", systemImage: "hammer")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(0.7))
                Text("Pair with a Mac to use the real connection, or keep the dev bridge for local development.")
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(0.5))
            } else {
                Label("Not paired", systemImage: "wifi.slash")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(0.7))
                Text("Open Unpeel on your Mac, go to Settings ▸ iPhone, and show the pairing code.")
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(0.5))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .background(.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    private func macRow(_ record: PairedMacRecord) -> some View {
        let isActive = record.macID == connection.activeMacID
        return HStack(spacing: 10) {
            Button {
                connection.switchTo(macID: record.macID)
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: "desktopcomputer")
                        .foregroundStyle(isActive ? .cyan : .white.opacity(0.55))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(record.macName)
                            .font(isActive ? .subheadline.weight(.semibold) : .subheadline)
                            .foregroundStyle(.white)
                        Text(
                            isActive && connection.usingRelay
                                ? "via Unpeel Remote"
                                : record.endpoint.absoluteString
                        )
                        .font(.caption.monospaced())
                        .foregroundStyle(.white.opacity(0.55))
                        .lineLimit(1)
                    }
                    Spacer(minLength: 0)
                    if isActive {
                        Image(systemName: "checkmark.circle.fill")
                            .foregroundStyle(.cyan)
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            Button {
                unpairCandidate = record
            } label: {
                Image(systemName: "minus.circle")
                    .foregroundStyle(.white.opacity(0.45))
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Forget \(record.macName)")
        }
        .padding(.vertical, 4)
    }

    @ViewBuilder
    private var scannerSection: some View {
        #if os(iOS) && !targetEnvironment(simulator)
        VStack(alignment: .leading, spacing: 10) {
            Text("Scan the pairing code")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white.opacity(0.85))
            PairingScannerView(paused: scannerPaused) { code in
                handlePayload(code)
            }
            .frame(height: 240)
            .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        }
        #else
        EmptyView()
        #endif
    }

    private var pasteSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Or paste the pairing code")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white.opacity(0.85))
            TextField("Pairing code", text: $pastedPayload, axis: .vertical)
                .font(.caption.monospaced())
                .lineLimit(3 ... 5)
                .textFieldStyle(.plain)
                .autocorrectionDisabled()
                #if os(iOS)
                .textInputAutocapitalization(.never)
                #endif
                .padding(10)
                .background(.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            Button {
                handlePayload(pastedPayload)
            } label: {
                if pairingInFlight {
                    ProgressView().frame(maxWidth: .infinity)
                } else {
                    Text("Connect")
                        .font(.subheadline.weight(.semibold))
                        .frame(maxWidth: .infinity)
                }
            }
            .buttonStyle(.borderedProminent)
            .tint(.cyan)
            .disabled(pairingInFlight || pastedPayload.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
    }

    // MARK: - Pairing

    private func handlePayload(_ raw: String) {
        guard !pairingInFlight else { return }
        guard let payload = RemotePairingCode.decode(raw) else {
            errorMessage = "That doesn't look like an Unpeel pairing code."
            return
        }
        pairingInFlight = true
        scannerPaused = true
        errorMessage = nil
        Task { @MainActor in
            do {
                try await connection.completePairing(with: payload)
                // The new Mac is upserted and active; land on it directly.
                addingMac = false
                dismiss()
            } catch let error as PairingError {
                errorMessage = error.message
            } catch {
                errorMessage = "Couldn't reach the Mac — make sure both devices are on the same network."
            }
            pairingInFlight = false
            scannerPaused = false
        }
    }
}

// MARK: - QR scanner (device only)

#if os(iOS) && !targetEnvironment(simulator)
private struct PairingScannerView: UIViewRepresentable {
    let paused: Bool
    let onCode: (String) -> Void

    func makeUIView(context: Context) -> ScannerPreview {
        let view = ScannerPreview()
        view.onCode = { code in
            DispatchQueue.main.async { onCode(code) }
        }
        view.start()
        return view
    }

    func updateUIView(_ view: ScannerPreview, context: Context) {
        view.setScanning(enabled: !paused)
    }

    static func dismantleUIView(_ view: ScannerPreview, coordinator: ()) {
        view.stop()
    }

    final class ScannerPreview: UIView, AVCaptureMetadataOutputObjectsDelegate {
        var onCode: ((String) -> Void)?
        private var scanningEnabled = true
        private let session = AVCaptureSession()
        private var lastCode: String?

        /// Pausing stops the capture session, not just the metadata gate —
        /// otherwise the camera (and its status indicator) stays hot for the
        /// whole pairing exchange.
        func setScanning(enabled: Bool) {
            guard scanningEnabled != enabled else { return }
            scanningEnabled = enabled
            let session = session
            DispatchQueue.global(qos: .userInitiated).async {
                if enabled {
                    if !session.isRunning { session.startRunning() }
                } else if session.isRunning {
                    session.stopRunning()
                }
            }
        }

        override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }
        private var previewLayer: AVCaptureVideoPreviewLayer {
            layer as! AVCaptureVideoPreviewLayer
        }

        func start() {
            backgroundColor = .black
            previewLayer.videoGravity = .resizeAspectFill
            AVCaptureDevice.requestAccess(for: .video) { [weak self] granted in
                guard granted else { return }
                DispatchQueue.main.async { self?.configureSession() }
            }
        }

        func stop() {
            let session = session
            DispatchQueue.global(qos: .userInitiated).async { session.stopRunning() }
        }

        private func configureSession() {
            guard session.inputs.isEmpty,
                  let device = AVCaptureDevice.default(for: .video),
                  let input = try? AVCaptureDeviceInput(device: device),
                  session.canAddInput(input)
            else { return }
            session.addInput(input)

            let output = AVCaptureMetadataOutput()
            guard session.canAddOutput(output) else { return }
            session.addOutput(output)
            output.setMetadataObjectsDelegate(self, queue: .main)
            output.metadataObjectTypes = [.qr]

            previewLayer.session = session
            // If a pairing exchange paused scanning while permission/config
            // was still in flight, stay stopped; setScanning restarts later.
            guard scanningEnabled else { return }
            let session = session
            DispatchQueue.global(qos: .userInitiated).async { session.startRunning() }
        }

        nonisolated func metadataOutput(
            _: AVCaptureMetadataOutput,
            didOutput objects: [AVMetadataObject],
            from _: AVCaptureConnection
        ) {
            // Extract before the actor hop: AVMetadataObject is not Sendable.
            let code = (objects.first as? AVMetadataMachineReadableCodeObject)?.stringValue
            guard let code else { return }
            // Delegate queue is .main (set in configureSession), so this is
            // a documented-safe assume, not a hope.
            MainActor.assumeIsolated {
                guard scanningEnabled, code != lastCode else { return }
                lastCode = code
                onCode?(code)
            }
        }
    }
}
#endif
