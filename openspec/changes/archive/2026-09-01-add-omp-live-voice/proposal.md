# Change: Add local OMP Live Voice

## Why

OMP already owns a private, subscription-backed Codex Live implementation, including OAuth, DeviceCheck, WebRTC, Opus, microphone capture, and playback. Comet can run OMP through JSONL RPC but currently has no desktop control for this Live surface because the RPC protocol does not expose it.

This change adds Live Voice to locally hosted OMP Chats on macOS. Comet owns the native control, transient status, and lifecycle policy; an ephemeral OMP child continues to own all media. When Live delegates coding work, Comet queues an ordinary durable Run command and executes it through the existing OmpHarness backend path so the command ledger, Run Journal, Chat Transcript, and session folding remain authoritative.

## Scope

- Offer Live Voice only for eligible OMP Chats hosted on the current Mac.
- Probe the additive OMP `liveVoice` RPC capability instead of inferring support from a version string.
- Keep phases, levels, casual transcripts, errors, and all audio device-local and transient.
- Convert each accepted Live delegation into exactly one durable Run command.
- Expose local engine controls and a native gpui Live surface.
- Declare microphone usage in the packaged macOS application.

## Non-goals

- Live Voice for non-OMP harnesses.
- Remote voice control or DeviceRoom relay.
- Relaying, storing, uploading, or logging audio or casual Live transcripts.
- Calling the public OpenAI Realtime API.
- Implementing microphone capture, playback, WebRTC, or Opus inside Comet.
- Adding retries, a second backend event normalizer, or a parallel agent-run pipeline.
