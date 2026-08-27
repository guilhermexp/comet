//
//  ActivityLog.swift
//  UnpeelNative
//
//  Persisted session-activity history: the data source for the "Recent"
//  panel and the always-visible titlebar bell. `activity-state.json` and
//  `last-hook-event.json` are current-state snapshots only, so this is the
//  one place that records *events over time* — session started, needed
//  input, finished, exited — durable across app restarts.
//

import Foundation

/// One entry in the persisted history feed. Title/project/command are
/// snapshotted at log time so an entry stays renderable after its session
/// (or even the project) is removed.
struct ActivityLogEntry: Codable, Identifiable, Equatable {
    enum Kind: String, Codable {
        case started
        case needsInput = "needs_input"
        case finished
        case exited
    }

    let id: String
    let sessionID: String
    let kind: Kind
    /// ms epoch
    let at: UInt64
    let title: String
    /// Launch command, for the CLI/provider icon.
    let command: String
    let projectID: String
    let projectName: String

    enum CodingKeys: String, CodingKey {
        case id
        case sessionID = "session_id"
        case kind
        case at
        case title
        case command
        case projectID = "project_id"
        case projectName = "project_name"
    }

    var date: Date { Date(timeIntervalSince1970: TimeInterval(at) / 1000) }
}

/// Append-only JSONL log at `<UNPEEL_HOME>/activity-log.jsonl` (same
/// per-home scoping as activity-state.json, so workspaces keep separate
/// feeds). Undecodable lines are skipped on load so the format can grow;
/// the file is compacted back to the in-memory tail whenever its line
/// count reaches double the entry cap.
@MainActor
final class ActivityLogStore {
    static let maxEntries = 300

    private(set) var entries: [ActivityLogEntry] = []
    private let fileURL: URL
    private var fileLineCount = 0

    init(fileURL: URL = LaunchConfig.activityLogFile) {
        self.fileURL = fileURL
        load()
    }

    /// Append, collapsing a same-kind repeat for the same session into one
    /// bumped entry (a TUI re-requesting permission, or back-to-back turn
    /// finishes, refresh a single feed row instead of stacking).
    func append(_ entry: ActivityLogEntry) {
        Self.appendCollapsing(entry, to: &entries)
        if entries.count > Self.maxEntries {
            entries.removeFirst(entries.count - Self.maxEntries)
        }
        guard let line = try? JSONEncoder().encode(entry) else { return }
        do {
            let fm = FileManager.default
            try fm.createDirectory(
                at: fileURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            if !fm.fileExists(atPath: fileURL.path) {
                fm.createFile(atPath: fileURL.path, contents: nil)
            }
            let handle = try FileHandle(forWritingTo: fileURL)
            defer { try? handle.close() }
            try handle.seekToEnd()
            try handle.write(contentsOf: line + Data("\n".utf8))
            fileLineCount += 1
            if fileLineCount >= Self.maxEntries * 2 { compact() }
        } catch {
            NSLog("[UnpeelNative] failed to append activity-log: \(error)")
        }
    }

    static func appendCollapsing(
        _ entry: ActivityLogEntry, to entries: inout [ActivityLogEntry]
    ) {
        if let last = entries.lastIndex(where: { $0.sessionID == entry.sessionID }),
           entries[last].kind == entry.kind {
            entries.remove(at: last)
        }
        entries.append(entry)
    }

    private func load() {
        guard let data = try? Data(contentsOf: fileURL),
              let text = String(data: data, encoding: .utf8)
        else { return }
        let decoder = JSONDecoder()
        let lines = text.split(separator: "\n", omittingEmptySubsequences: true)
        fileLineCount = lines.count
        var loaded: [ActivityLogEntry] = []
        for line in lines {
            guard let entry = try? decoder.decode(
                ActivityLogEntry.self, from: Data(line.utf8)
            ) else { continue }
            // Same collapse rule as append, so a reload renders the same
            // feed the previous run showed.
            Self.appendCollapsing(entry, to: &loaded)
        }
        entries = Array(loaded.suffix(Self.maxEntries))
        if fileLineCount >= Self.maxEntries * 2 { compact() }
    }

    /// Rewrite the file down to the in-memory tail.
    private func compact() {
        let encoder = JSONEncoder()
        var out = Data()
        for entry in entries {
            guard let line = try? encoder.encode(entry) else { continue }
            out.append(line)
            out.append(0x0A)
        }
        do {
            try out.write(to: fileURL, options: .atomic)
            fileLineCount = entries.count
        } catch {
            NSLog("[UnpeelNative] failed to compact activity-log: \(error)")
        }
    }
}
