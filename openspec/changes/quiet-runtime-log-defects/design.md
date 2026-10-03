## Context

All fixes come from one long-running headed log; each was traced to code before editing (see proposal). The guiding rule: a loop that fails the same way again must cost less each time, and say so once.

## Decisions

- **D1 Cursor certification is live, not captured.** `ChatPersistence` read `cursor_verified` once at construction; the warm handle outlives client rotations, so every re-admission saw the stale `false`. The constructor already discards an unverified cursor (starts at 0), and the cursor only advances through contiguous `applied`/checkpoint/ack paths, so any successful `save_verified_snapshot_with_cursor` makes the in-memory flag true. Returning `true` unconditionally was rejected: it would skip the repair on the very first admission of a legacy row.
- **D2 Usage backoff escalates per entry.** `UsageEntry.failures` (serde default, skipped when zero, so old cache files load) doubles the base backoff for transient classes only, capped at 30 min, never below the base (a longer Retry-After wins). Credential-bound failures keep their fixed wait and are still lifted by a credential change. Logging moved from the probe to the fold, where the previous class is known.
- **D3 `host_offline` is evidence, not a blip.** The relay answering `host_offline` proves the host is absent. Such a failure gets `offline_cooldown` (5 min) and survives online broadcasts and token refreshes; `reset_cooldown` (fresh presence) lifts it; sign-out still clears everything (native-runtime-integrity peer revocation requirement). Ordinary failures keep the 5 s → 60 s curve, which now actually escalates because the online broadcast no longer wipes it. The link-down reason can land a moment after the probe call fails, so the dial waits up to 250 ms for it.
- **D4 Edge quota meters admitted traffic.** The quota exists to contain runaway loops (its own comment). Rejected pushes no longer extend the window; limits sized for a 1 MiB-row reconnect flush. Client behavior (head-probe one batch per grant) is unchanged; only its logging collapses to one warning per episode.
- **D5 Discovery reason, not `Option` debug.** `None` from the 100 ms disk-serve deadline is normal and stays at debug. Shim ranking is a stable sort before the version comparison, so a strictly newer shim still wins and a lone shim still resolves; only the `cmux-cli-shims` path component is recognized, to avoid demoting test fixtures that live in tempdirs.
- **D6 Drain before quit.** GPUI's 200 ms `SHUTDOWN_TIMEOUT` lives in vendored zui (not editable). `quit_after_save` hides the app and runs `EngineHandle::shutdown` under an 8 s budget on Tokio, then calls the original quit path. A second quit during the drain is absorbed; the `on_app_quit` handler is a no-op after a completed drain and remains the fallback for paths that bypass the gate. Remote engines quit immediately as before.
- **D7 Fix the prober, not the warning.** The native-runtime-integrity spec keeps complete-but-invalid IPC handshakes at warning level. The real defect was another Zeron's preview scanner probing this IPC port; the scanner now skips listeners whose program is `zeron`.
- **D8 Attach command entries whole.** Loro copies a detached container's state on attach, so the entry becomes visible complete. Peers on older builds can still be observed mid-write; an identified entry without payload is classified in-flight (debug), anything else stays a warning.

## Risks / Trade-offs

- [A peer comes back while its offline cooldown runs and presence is unavailable] → worst case 5 min until redial; fresh presence lifts it immediately.
- [Larger edge quota admits a runaway client longer] → still bounded per minute; rows remain ≤1 MiB.
- [Drain hides windows up to 8 s before exit] → previous behavior silently skipped the flushes instead.
- [Edge change is a deploy] → not pushed by this change; publishing is the user's call.
