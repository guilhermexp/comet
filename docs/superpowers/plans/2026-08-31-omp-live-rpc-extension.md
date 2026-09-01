# OMP Live RPC Extension Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose OMP's existing Codex Live media stack as a headless RPC voice frontend that delegates coding work to its host and accepts backend progress/results as text context.

**Architecture:** `LiveSessionController` gains an explicit `host` delegation mode. In that mode it still owns Codex OAuth, DeviceCheck, WebRTC/Opus, microphone capture, playback, and realtime conversation, but it does not trigger its internal `AgentSession`. `rpc-mode.ts` converts controller callbacks to additive JSONL events and exposes start, mute, context-append, and stop commands. Existing interactive Live remains in the default `session` mode.

**Tech Stack:** Bun, TypeScript, OMP JSONL RPC v2, existing `LiveSessionController`, existing `CodexLiveTransport`, existing Rust/N-API `LiveWebRtcPeer`.

**Spec:** `docs/plans/2026-08-31-omp-live-voice-design.md`

## Global Constraints

- Execute this plan in the `can1357/oh-my-pi` repository, not in the Comet repository that stores the plan.
- Keep the extension additive: current RPC v1/v2 clients and interactive `/live` behavior remain compatible.
- Default `LiveSessionController` delegation mode remains `session`.
- Host mode permits exactly one unresolved delegation.
- `live_append_context` must match the active `delegationId`; `kind: "final"` closes that delegation.
- Do not send PCM, Opus, `output_audio.delta`, OAuth tokens, attestation payloads, or casual transcripts to logs.
- Use OMP's effective `live.voice`; do not introduce a second RPC voice setting.
- One RPC process owns at most one Live controller; stop and disposal are idempotent.
- Do not add a public Realtime API fallback.

---

## File Structure

- Modify `packages/coding-agent/src/live/controller.ts`: add host-delegation mode and public progress/final context methods.
- Modify `packages/coding-agent/src/modes/rpc/rpc-types.ts`: additive capability, commands, and event wire types.
- Modify `packages/coding-agent/src/modes/rpc/rpc-mode.ts`: own one controller, dispatch Live commands, emit presentation events, and dispose safely.
- Modify `packages/coding-agent/test/modes/controllers/live-command-controller.test.ts`: preserve default session-mode behavior and test host-mode delegation/context.
- Create `packages/coding-agent/test/rpc-live.test.ts`: wire and RPC lifecycle tests with an injected fake controller.
- Modify `docs/rpc.md`: canonical Live RPC reference.
- Modify `packages/coding-agent/CHANGELOG.md`: user-visible extension entry following the existing format.

### Task 1: Split Live delegation from media ownership

**Files:**
- Modify: `packages/coding-agent/src/live/controller.ts`
- Test: `packages/coding-agent/test/modes/controllers/live-command-controller.test.ts`

**Interfaces:**
- Produces: `LiveDelegationMode`, `LiveSessionCallbacks.onDelegation`, `appendDelegationProgress`, and `appendDelegationFinal`.
- Preserves: default internal `AgentSession.sendCustomMessage` behavior for interactive `/live`.

- [ ] **Step 1: Write the failing host-mode tests**

Extend the existing Live command-controller test setup. Reuse its fake session and transport; do not create a second transport mock stack.

Add a test proving host mode emits the request and never starts the internal agent:

```ts
const delegations: Array<{ id: string; request: string }> = [];
const controller = createController({
	delegationMode: "host",
	callbacks: {
		...callbacks,
		onDelegation: (id, request) => delegations.push({ id, request }),
	},
});

transport.emit({
	type: "delegation.created",
	item: {
		type: "delegation",
		target: "client",
		id: "del-1",
		content: [{ type: "input_text", text: "Inspect auth" }],
	},
});

expect(delegations).toEqual([{ id: "del-1", request: "Inspect auth" }]);
expect(session.sendCustomMessage).not.toHaveBeenCalled();
expect(controller.phase).toBe("working");
```

Add a second test proving context correlation and final settlement:

```ts
await controller.appendDelegationProgress("del-1", "Inspecting call sites");
await expect(controller.appendDelegationFinal("wrong", "Done")).rejects.toThrow("active delegation");
await controller.appendDelegationFinal("del-1", "Fixed and tested");
expect(controller.phase).toBe("listening");
```

Assert the fake transport received `buildDelegationContextAppend` output with `commentary` for progress and the existing final-message prompt wrapping for final.

- [ ] **Step 2: Run the focused test and confirm RED**

```bash
bun test packages/coding-agent/test/modes/controllers/live-command-controller.test.ts
```

Expected: FAIL because host mode, `onDelegation`, and public context methods do not exist.

- [ ] **Step 3: Add explicit delegation mode without changing the default**

In `controller.ts` add:

```ts
export type LiveDelegationMode = "session" | "host";

export interface LiveSessionCallbacks {
	onPhase(phase: LivePhase): void;
	onLevels(input: number, output: number): void;
	onTranscript(transcript: LiveTranscript | undefined): void;
	onDelegation?(delegationId: string, request: string): void;
	onTerminal(error?: Error): void;
}
```

Extend options and controller state:

```ts
export interface LiveSessionControllerOptions {
	session: AgentSession;
	callbacks: LiveSessionCallbacks;
	extractAssistantText(message: AssistantMessage): string;
	voice?: string;
	delegationMode?: LiveDelegationMode;
}
```

Store `options.delegationMode ?? "session"`. During `start()`, subscribe to `AgentSession` events only in `session` mode.

- [ ] **Step 4: Branch the delegation handler at the ownership boundary**

After extracting and validating the request:

```ts
if (this.#activeDelegationId) {
	this.#reportFailure(new Error("Live already has an active delegation"));
	return;
}
this.#activeDelegationId = event.item.id;
this.#emitPhase("working");
if (this.#delegationMode === "host") {
	this.#callbacks.onDelegation?.(event.item.id, request);
	return;
}
void this.#session.sendCustomMessage(/* existing message and triggerTurn options */)
	.catch(cause => this.#reportFailure(errorFrom(cause)));
```

Do not invoke both paths.

- [ ] **Step 5: Add correlated public context methods**

Use the existing `chunkLiveContext`, `buildDelegationContextAppend`, and `agent-final-message.md` behavior:

```ts
async appendDelegationProgress(delegationId: string, text: string): Promise<void>
async appendDelegationFinal(delegationId: string, text: string): Promise<void>
```

Both methods must:

1. reject unless mode is `host`;
2. reject unless `delegationId === #activeDelegationId`;
3. trim and reject empty text;
4. enqueue all bounded context chunks on `#sendChain`;
5. await the resulting send chain so an RPC response means the append reached the transport queue.

`appendDelegationFinal` must apply the existing final-message template, clear `#activeDelegationId` only after successful queueing, and refresh phase.

Refactor `#appendProgress` and `#appendFinalResponse` to share private chunk-sending helpers rather than duplicate final-message formatting.

- [ ] **Step 6: Prove default session mode is unchanged**

Add/retain an assertion that a controller created without `delegationMode` calls `session.sendCustomMessage` once and does not require `onDelegation`.

Run:

```bash
bun test packages/coding-agent/test/modes/controllers/live-command-controller.test.ts packages/coding-agent/test/live/protocol.test.ts
```

Expected: PASS.

- [ ] **Step 7: Commit the controller boundary**

```bash
git add packages/coding-agent/src/live/controller.ts packages/coding-agent/test/modes/controllers/live-command-controller.test.ts
git commit -m "feat(live): support host-owned delegations"
```

### Task 2: Define the additive Live RPC wire contract

**Files:**
- Modify: `packages/coding-agent/src/modes/rpc/rpc-types.ts`
- Create: `packages/coding-agent/test/rpc-live.test.ts`

**Interfaces:**
- Produces: ready capability, four Live commands, and five transient event variants.

- [ ] **Step 1: Write the failing wire-contract test**

Create `rpc-live.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import type { RpcCommand, RpcLiveFrame, RpcReadyFrame } from "../src/modes/rpc/rpc-types";

const command = (value: RpcCommand) => value;
const frame = (value: RpcLiveFrame) => value;

describe("RPC Live wire contract", () => {
	test("defines additive capability, commands, and events", () => {
		const ready: RpcReadyFrame = {
			type: "ready",
			protocolVersion: 1,
			supportedProtocolVersions: [1, 2],
			maxFrameBytes: 1_048_576,
			maxReassembledFrameBytes: 67_108_864,
			capabilities: { liveVoice: 1 },
		};
		expect(ready.capabilities?.liveVoice).toBe(1);
		expect(command({ id: "1", type: "live_start", delegationMode: "host" }).type).toBe("live_start");
		expect(command({ id: "2", type: "live_set_muted", muted: true }).type).toBe("live_set_muted");
		expect(command({ id: "3", type: "live_append_context", delegationId: "d", kind: "final", text: "done" }).type).toBe("live_append_context");
		expect(command({ id: "4", type: "live_stop" }).type).toBe("live_stop");
		expect(frame({ type: "live_phase", phase: "listening" }).type).toBe("live_phase");
	});
});
```

- [ ] **Step 2: Run the test and confirm missing types fail**

```bash
bun test packages/coding-agent/test/rpc-live.test.ts
```

Expected: FAIL.

- [ ] **Step 3: Add the minimal wire types**

Add to `RpcCommand`:

```ts
| { id?: string; type: "live_start"; delegationMode: "host" }
| { id?: string; type: "live_set_muted"; muted: boolean }
| {
	id?: string;
	type: "live_append_context";
	delegationId: string;
	kind: "progress" | "final";
	text: string;
  }
| { id?: string; type: "live_stop" }
```

Add:

```ts
export interface RpcCapabilities { liveVoice?: 1 }
export type RpcLivePhase = "connecting" | "listening" | "speaking" | "working" | "muted" | "error";
export type RpcLiveFrame =
	| { type: "live_phase"; phase: RpcLivePhase }
	| { type: "live_levels"; input: number; output: number }
	| { type: "live_transcript"; role: "user" | "assistant"; turn: number; text: string; final: boolean }
	| { type: "live_delegation_created"; delegationId: string; request: string }
	| { type: "live_ended"; error: string | null };
```

Extend `RpcReadyFrame` with optional `capabilities?: RpcCapabilities`.

- [ ] **Step 4: Run and commit**

```bash
bun test packages/coding-agent/test/rpc-live.test.ts
git add packages/coding-agent/src/modes/rpc/rpc-types.ts packages/coding-agent/test/rpc-live.test.ts
git commit -m "feat(rpc): define live voice wire contract"
```

Expected: PASS, then commit succeeds.

### Task 3: Add a testable RPC Live lifecycle

**Files:**
- Modify: `packages/coding-agent/src/modes/rpc/rpc-mode.ts`
- Test: `packages/coding-agent/test/rpc-live.test.ts`

**Interfaces:**
- Produces: one active controller, deterministic start/mute/context/stop transitions, and callback-to-frame mapping.

- [ ] **Step 1: Add failing lifecycle tests with a fake controller**

Define the structural fake in the test:

```ts
class FakeLiveController {
	started = 0;
	stopped = 0;
	muted = false;
	appends: Array<[string, string, string]> = [];
	async start() { this.started += 1; }
	async stop() { this.stopped += 1; }
	toggleMute() { this.muted = !this.muted; }
	async appendDelegationProgress(id: string, text: string) { this.appends.push([id, "progress", text]); }
	async appendDelegationFinal(id: string, text: string) { this.appends.push([id, "final", text]); }
}
```

Assert:

- first start succeeds;
- duplicate start rejects;
- setting the current mute value is a no-op;
- context kind selects the exact controller method;
- stop twice calls controller stop once;
- failed start clears active state.

- [ ] **Step 2: Run the focused test and confirm RED**

```bash
bun test packages/coding-agent/test/rpc-live.test.ts
```

Expected: FAIL because the lifecycle helper does not exist.

- [ ] **Step 3: Export a small structural lifecycle helper**

In `rpc-mode.ts`, define `RpcLiveControllerLike` and `createRpcLiveLifecycle(factory)`. The returned object exposes:

```ts
readonly active: boolean;
start(): Promise<void>;
setMuted(muted: boolean): Promise<void>;
appendContext(delegationId: string, kind: "progress" | "final", text: string): Promise<void>;
stop(): Promise<void>;
```

Set the controller before awaiting `start()` to close duplicate-start races. Clear it if start fails. In `stop()`, take and clear the controller before awaiting its idempotent stop.

- [ ] **Step 4: Add and test the pure callback mapper**

Export `createRpcLiveCallbacks(output)` and assert exact frames:

```ts
callbacks.onPhase("listening");
callbacks.onLevels(0.2, 0.4);
callbacks.onTranscript({ role: "user", turn: 1, text: "Inspect auth", final: true });
callbacks.onDelegation?.("del-1", "Inspect auth");
callbacks.onTerminal();
```

Expected frames are `live_phase`, `live_levels`, `live_transcript`, `live_delegation_created`, and `live_ended` in that order. An undefined transcript emits nothing.

- [ ] **Step 5: Run and commit**

```bash
bun test packages/coding-agent/test/rpc-live.test.ts
git add packages/coding-agent/src/modes/rpc/rpc-mode.ts packages/coding-agent/test/rpc-live.test.ts
git commit -m "feat(rpc): add live voice lifecycle"
```

Expected: PASS.

### Task 4: Wire production commands, capability, and cleanup

**Files:**
- Modify: `packages/coding-agent/src/modes/rpc/rpc-mode.ts`
- Test: `packages/coding-agent/test/rpc-live.test.ts`

**Interfaces:**
- Consumes: current RPC `AgentSession` only for Live auth/session identity, effective `live.voice`, and the Task 3 lifecycle.
- Produces: correlated RPC responses and transient Live frames.

- [ ] **Step 1: Add failing command-dispatch tests**

Use the current RPC test's stdin/stdout fixture or extract a command dispatcher only if the existing integration fixture cannot inject the controller factory. Assert exact response correlation for:

```json
{"id":"1","type":"live_start","delegationMode":"host"}
{"id":"2","type":"live_set_muted","muted":true}
{"id":"3","type":"live_append_context","delegationId":"del-1","kind":"progress","text":"Inspecting"}
{"id":"4","type":"live_append_context","delegationId":"del-1","kind":"final","text":"Fixed"}
{"id":"5","type":"live_stop"}
```

Also assert `live_start` with any mode other than `host` is rejected by parsing or dispatch.

- [ ] **Step 2: Run the focused test and confirm RED**

```bash
bun test packages/coding-agent/test/rpc-live.test.ts
```

Expected: FAIL because production dispatch is not wired.

- [ ] **Step 3: Construct the production controller**

The lifecycle factory creates:

```ts
new LiveSessionController({
	session,
	callbacks: createRpcLiveCallbacks(output),
	extractAssistantText: /* reuse the established assistant text extraction helper */,
	voice: session.settings.get("live.voice"),
	delegationMode: "host",
})
```

Do not resume/switch sessions or register tools inside the Live command. RPC process startup still creates the ordinary session object needed by Codex auth storage and a realtime session identity, but host mode never triggers it.

- [ ] **Step 4: Add command switch arms through the existing error boundary**

Each command awaits the lifecycle method and returns through the existing success response helper. Do not emit success after a thrown transport or correlation error.

Use response data:

```ts
{ active: true }
{ muted: command.muted }
{ delegationId: command.delegationId, kind: command.kind }
{ active: false }
```

- [ ] **Step 5: Advertise capability and close Live on every disposal path**

Add to the existing ready frame near `supportedProtocolVersions`:

```ts
capabilities: { liveVoice: 1 },
```

Call `await liveLifecycle.stop()` before every `session.dispose()` path and on stdin EOF. Multiple cleanup calls are safe by contract.

- [ ] **Step 6: Run the focused RPC regression set**

```bash
bun test packages/coding-agent/test/rpc-live.test.ts packages/coding-agent/test/rpc-input-frame.test.ts packages/coding-agent/test/rpc-malformed-input.test.ts packages/coding-agent/test/rpc.test.ts
```

Expected: PASS.

- [ ] **Step 7: Commit production integration**

```bash
git add packages/coding-agent/src/modes/rpc/rpc-mode.ts packages/coding-agent/test/rpc-live.test.ts
git commit -m "feat(rpc): expose OMP live voice frontend"
```

### Task 5: Document and verify the OMP extension

**Files:**
- Modify: `docs/rpc.md`
- Modify: `packages/coding-agent/CHANGELOG.md`

- [ ] **Step 1: Add the canonical RPC reference**

Document:

- `capabilities.liveVoice: 1` in the ready example;
- the four command schemas and five event schemas;
- `delegationMode: "host"` semantics;
- one-active-delegation correlation;
- progress versus final context behavior;
- absence of audio frames;
- stop and stdin/disposal cleanup.

- [ ] **Step 2: Add the changelog entry**

Following the current unreleased-section format, record that headless RPC hosts can use Codex Live as a voice frontend while retaining ownership of delegated backend execution.

- [ ] **Step 3: Run formatting and focused tests**

```bash
bun run format
bun test packages/coding-agent/test/rpc-live.test.ts packages/coding-agent/test/modes/controllers/live-command-controller.test.ts packages/coding-agent/test/rpc.test.ts
```

Expected: formatter exits 0; tests PASS.

- [ ] **Step 4: Smoke the non-media JSONL contract**

Start `omp --mode rpc-ui`, read one frame, and verify `capabilities.liveVoice === 1`. Send `live_stop` before start and confirm an idempotent correlated success. This automated smoke must not require microphone permission or Codex credentials.

- [ ] **Step 5: Commit docs**

```bash
git add docs/rpc.md packages/coding-agent/CHANGELOG.md
git commit -m "docs(rpc): document live voice frontend"
```

## OMP Plan Completion Gate

- Interactive `/live` still defaults to internal session execution.
- RPC host mode never calls `AgentSession.sendCustomMessage`.
- Ready capability, commands, and events are additive.
- Fake-controller tests cover duplicate start, exact mute, context correlation, failed start cleanup, and idempotent stop.
- Host mode permits one unresolved delegation and final context returns it to listening.
- stdin close and session disposal release capture/playback.
- No media bytes, casual transcripts, or secrets are logged.
- Comet implementation may start only after its OMP binary/fixture exposes this exact contract.
