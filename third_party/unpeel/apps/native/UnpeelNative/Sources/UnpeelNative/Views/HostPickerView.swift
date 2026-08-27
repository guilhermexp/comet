//
//  HostPickerView.swift
//  UnpeelNative
//
//  Host scope selection lives on the Remote settings screen: the section at
//  the top of that panel switches the app between Local and paired Hosts,
//  and the nearby-pairing sheet here adds a new one. Bonjour makes Hosts
//  discoverable; the one-time code remains the authority until the Host has
//  an explicit approve-this-Controller handshake.
//

import AppKit
import SwiftUI
import UnpeelShared

/// The "This App Controls" section pinned to the top of the Remote settings
/// panel — the one place Host scope is chosen (the sidebar footer only
/// reflects the choice via the labeled remote button).
struct HostScopeSection: View {
    @ObservedObject var store: UnpeelStore
    @ObservedObject var hosts: RemoteHostStore
    @State private var addHostPresented = false

    var body: some View {
        Section {
            hostRow(
                name: "Local",
                detail: "This Mac",
                icon: "desktopcomputer",
                selected: store.selectedHostScope == .local,
                select: { store.selectHost(nil) }
            )

            ForEach(hosts.records) { host in
                hostRow(
                    name: host.name,
                    detail: host.hostID,
                    icon: "server.rack",
                    selected: store.selectedHostScope.remoteHostID == host.hostID,
                    select: { store.selectHost(host.hostID) }
                )
                .contextMenu {
                    Button("Forget Host", role: .destructive) {
                        store.forgetHost(host.hostID)
                    }
                }
            }

            Button {
                addHostPresented = true
            } label: {
                Label("Add Host…", systemImage: "plus")
            }
            .sheet(isPresented: $addHostPresented) {
                AddHostSheet(store: store, hosts: hosts)
            }
        } header: {
            // Same header style as "Controls This Mac" below it — the two
            // sections read as a symmetric outbound/inbound pair.
            SettingsSectionHeader(
                title: "This App Controls",
                description: "Selecting a Host scopes the whole app to it — sessions in the sidebar live on that Host. Right-click a Host to forget it."
            )
        }
    }

    private func hostRow(
        name: String,
        detail: String,
        icon: String,
        selected: Bool,
        select: @escaping () -> Void
    ) -> some View {
        Button(action: select) {
            HStack(spacing: 10) {
                Image(systemName: icon)
                    .frame(width: 18)
                VStack(alignment: .leading, spacing: 1) {
                    Text(name)
                        .font(.system(size: 13))
                    Text(detail)
                        .font(.system(size: 10, design: .monospaced))
                        .foregroundStyle(Theme.mutedForeground)
                }
                Spacer()
                if selected {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(Theme.accent)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

struct AddHostSheet: View {
    @ObservedObject var store: UnpeelStore
    @ObservedObject var hosts: RemoteHostStore
    @StateObject private var browser: NearbyHostBrowser
    @Environment(\.dismiss) private var dismiss

    @State private var selectedCandidateID: String?
    @State private var pairingCode = ""
    @State private var pairing = false
    @State private var errorMessage: String?
    @State private var pairingTask: Task<Void, Never>?

    init(store: UnpeelStore, hosts: RemoteHostStore) {
        self.store = store
        self.hosts = hosts
        _browser = StateObject(
            wrappedValue: NearbyHostBrowser(excludingHostID: store.localHostID)
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Add a Host")
                        .font(.system(size: 20, weight: .semibold))
                    Text("Control an Unpeel TUI or another Mac on your network.")
                        .font(.system(size: 12))
                        .foregroundStyle(Theme.mutedForeground)
                }
                Spacer()
                Button("Cancel") { cancelAndDismiss() }
                    .keyboardShortcut(.cancelAction)
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("Nearby")
                    .font(.system(size: 12, weight: .semibold))
                if case .unavailable(let message) = browser.state {
                    VStack(alignment: .leading, spacing: 5) {
                        Text("Nearby discovery is unavailable")
                            .font(.system(size: 11, weight: .medium))
                        Text(message)
                            .font(.system(size: 10))
                            .foregroundStyle(Theme.mutedForeground)
                        Text("You can still paste the pairing code below.")
                            .font(.system(size: 10))
                            .foregroundStyle(Theme.mutedForeground)
                    }
                    .frame(maxWidth: .infinity, minHeight: 54, alignment: .leading)
                } else if browser.candidates.isEmpty {
                    HStack(spacing: 8) {
                        ProgressView().controlSize(.small)
                        Text("Looking for Unpeel Hosts…")
                            .foregroundStyle(Theme.mutedForeground)
                    }
                    .frame(maxWidth: .infinity, minHeight: 54, alignment: .leading)
                } else {
                    VStack(spacing: 4) {
                        Button {
                            selectedCandidateID = nil
                        } label: {
                            HStack {
                                Image(systemName: "qrcode")
                                Text("Use pairing code only")
                                Spacer()
                                if selectedCandidateID == nil {
                                    Image(systemName: "checkmark.circle.fill")
                                        .foregroundStyle(Theme.accent)
                                }
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .padding(9)
                        .background(
                            RoundedRectangle(cornerRadius: 8, style: .continuous)
                                .fill(
                                    selectedCandidateID == nil
                                        ? Theme.accent.opacity(0.10)
                                        : Theme.hoverRow
                                )
                        )

                        ForEach(browser.candidates) { candidate in
                            Button {
                                selectedCandidateID = candidate.hostID
                            } label: {
                                HStack {
                                    Image(systemName: "server.rack")
                                    VStack(alignment: .leading, spacing: 1) {
                                        Text(candidate.name)
                                        Text(candidate.hostID)
                                            .font(.system(size: 10, design: .monospaced))
                                            .foregroundStyle(Theme.mutedForeground)
                                    }
                                    Spacer()
                                    if selectedCandidateID == candidate.hostID {
                                        Image(systemName: "checkmark.circle.fill")
                                            .foregroundStyle(Theme.accent)
                                    }
                                }
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .padding(9)
                            .background(
                                RoundedRectangle(cornerRadius: 8, style: .continuous)
                                    .fill(
                                        selectedCandidateID == candidate.hostID
                                            ? Theme.accent.opacity(0.10)
                                            : Theme.hoverRow
                                    )
                            )
                        }
                    }
                }
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("Pairing code")
                    .font(.system(size: 12, weight: .semibold))
                Text(
                    "For a TUI Host, run `unpeel pair --serve`; it opens automatically "
                        + "after pairing. For another Mac app, copy the pairing code "
                        + "from its Settings ▸ Remote screen. Paste the one-time code here."
                )
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.mutedForeground)
                TextField("UNPEEL:1:…", text: $pairingCode)
                    .textFieldStyle(.roundedBorder)
                    .font(.system(size: 11, design: .monospaced))
            }

            if let errorMessage {
                Text(errorMessage)
                    .font(.system(size: 11))
                    .foregroundStyle(.red)
            }

            HStack {
                Text("The code is sealed to this Host and expires after one use.")
                    .font(.system(size: 10))
                    .foregroundStyle(Theme.mutedForeground)
                Spacer()
                Button {
                    completePairing()
                } label: {
                    if pairing {
                        ProgressView().controlSize(.small)
                    } else {
                        Text("Pair")
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(pairing || pairingCode.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(24)
        .frame(width: 520)
        .onAppear { browser.start() }
        .onDisappear {
            pairingTask?.cancel()
            pairingTask = nil
            browser.stop()
        }
        .onChange(of: browser.candidates) { candidates in
            if let selectedCandidateID,
               !candidates.contains(where: { $0.hostID == selectedCandidateID }) {
                self.selectedCandidateID = nil
            }
        }
    }

    private func completePairing() {
        guard !pairing else { return }
        pairing = true
        errorMessage = nil
        let code = pairingCode
        let expectedHostID = selectedCandidateID
        pairingTask = Task { @MainActor in
            defer { pairing = false }
            do {
                let record = try await hosts.pair(
                    code: code,
                    expectedHostID: expectedHostID
                )
                try Task.checkCancellation()
                // A successful re-pair may rotate the bearer/Link key for an
                // already-selected Host. Apply the new pairing explicitly;
                // ordinary clicks on the checked Host remain a no-op.
                store.selectHost(record.hostID, forceReconnect: true)
                dismiss()
            } catch is CancellationError {
                return
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    private func cancelAndDismiss() {
        pairingTask?.cancel()
        pairingTask = nil
        dismiss()
    }
}


