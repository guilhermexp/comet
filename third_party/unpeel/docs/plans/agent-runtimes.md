# Agent Runtimes — Host-owned detection and extensible integrations

> **Status (2026-08-15): Live observation, same-PTY Resume Agent, and the first
> in-repository runtime-package surface are implemented on this branch.** On
> macOS and Linux the Host recognizes current built-in agents in its owned PTY
> foreground job, persists `runtime.currentObservation`, and publishes additive
> `activeRuntimeID` presentation to native, TUI, and remote Controllers. Thus a
> user can open a blank terminal and later run `claude` without changing the
> Session's saved blank launch command. After a managed runtime exits or
> crashes back to its still-live shell, Resume Agent can continue it
> generation-safely inside the same Host/PTY. An active runtime has no such
> action, and passive observation never grants it; both retain generic
> screen/output activity.
>
> Thirteen built-ins now live under one discovered `runtimes/<slug>` package.
> Strict TOML descriptors generate the Rust integration/transcript registries
> and client presentation/setup catalog; provider setup/assets,
> resume/fresh/fork/context recipes, and transcript resolution/parsing live
> beside each descriptor. Adding an in-repository built-in no longer requires a
> central provider list or matching client metadata edits. Stable launch-binding
> records, normalized active-conversation generations, Host-advertised reasoned
> action objects, screen-rule manifests, and installable external adapters
> below remain unbuilt. The source package schema is v1 for this compiled
> built-in boundary; it is not yet an external plugin ABI.
>
> This is adjacent open-source/contributor preparation, not a new numbered
> phase in `docs/plans/master-plan-next.md` and not a blocker for its remote
> Controller work. `docs/agents/providers.md` remains authoritative for shipped
> provider behavior until each migration phase below lands.

## Outcome

Any command can be saved and launched as a preset. Unpeel always gives it a
generic hosted terminal. When the selected Host recognizes a supported agent
runtime, that Session gains only the capabilities that the runtime implements
and that the particular Session can safely use:

```text
flat preset list: any command
             |
             v
      Host launches generic PTY
             |
     +-------+------------------+
     |                          |
unknown command          recognized runtime
generic terminal         identity + capabilities
still fully usable       hooks / resume / transcript / context / status
```

The open-source contribution bar is:

> A basic runtime lands as one descriptor directory plus fixtures. A complex
> runtime adds one Host-side adapter. It never requires matching edits in
> Rust, Swift, iOS, TUI, and remote-controller switch statements.

Installable third-party executable adapters are a later trust/distribution
layer. The first goal is a clean, public in-repository contribution surface,
not a plugin marketplace.

## Product model and terminology

These are separate concepts:

| Concept | Meaning |
| --- | --- |
| **Preset** | A user-owned command entry with its current project association, order, enabled state, and quick-launch choice. It may launch any CLI. Working directory and launch environment remain Session/Host concerns rather than new preset fields in this plan. |
| **Agent runtime** | A recognized agent CLI such as Claude, Codex, Gemini, or a community agent. Recognition enriches a Session; it never decides whether the command may run. |
| **Runtime descriptor** | Data describing stable identity, command/process matches, suggested presets, display metadata, declared capabilities, lifecycle authority, and optional screen rules. |
| **Runtime adapter** | Optional trusted Host-side behavior for launch preparation, hook installation, exact resume/fork, appended context, and transcript resolution/parsing. |
| **Runtime binding** | The Host's stable prelaunch conclusion plus its current live-process observation. Relaunch capabilities attach to the stable launch binding; display/status may follow the active runtime. |
| **Unpeel App** | A standalone-first non-agent CLI/UI using RoomStore and optional semantic rendering. Agent runtime integrations are not Unpeel Apps and use a separate contract. |

Runtime packages may contribute suggested presets, but automatic seeding runs
only while creating a genuinely fresh empty preset list. After that, adding a
suggestion is an explicit import that shows its source and full command. There
is no periodic auto-add/update, so a package cannot reorder user presets or
resurrect one the user deleted or disabled. `app-state.json` remains the single
flat preset truth. Quick launch must eventually be valid for any enabled
preset, not only commands recognized by the built-in integration registry.

A preset may carry an optional `runtime_hint`; automatic detection remains the
default. A hint disambiguates a trusted wrapper but is not permission to run an
untrusted adapter or edit provider configuration.

## Current ground truth

The hosted-terminal foundation is already runtime-agnostic. The limitation is
how provider knowledge is resolved and duplicated around it.

| Area | Current behavior | Problem this plan resolves |
| --- | --- | --- |
| Launch recognition | Managed command identity and foreground process aliases/path signatures come from the generated runtime catalog; wrapper handling remains conservative Host logic. | Stable launch bindings still need to preserve stronger evidence than a legacy command-derived slug. |
| Rust integration | `runtimes/<slug>/adapter/` owns provider setup, launch, resume/context, and transcript callbacks; the build generates deterministic registries. | The callback boundary is compiled source contribution, not yet a version-pinned external adapter protocol. |
| Presets/setup | Suggested presets, install metadata, aliases, lifecycle, capabilities, label/tint/icon keys come from `runtime.toml`; client-safe Swift metadata is generated and drift-checked. | User import/update UX and external package provenance remain future work. |
| Session identity | `HostedSessionManifest` stores `provider_session_id` and `provider_transcript_path`, and `provider-session.json` is written by whichever frontend receives a hook. | The marker does not bind the identity to a runtime or generation, and identity can be lost while no frontend is listening. |
| Resume/fork/fresh | Runtime-owned adapters feed canonical `resume.rs` dispatch and `session_ops::relaunch_command`; same-PTY Resume Agent is generation-bound and available only after the managed runtime returns to its shell. | Relaunch is still selected from the saved command/legacy slug rather than a pinned launch binding and adapter revision. |
| Appended context | Runtime `context.rs` recipes plug into provider-neutral locked marker staging and relaunch dispatch. | Context still needs the future stable binding/action contract for adapters that change after Session creation. |
| Activity | Descriptor lifecycle policy selects hook vs output authority; owned hook events/seeds carry runtime generation and stale events are rejected. | Screen-manifest fallback and richer partial/identity-only effective authority remain unbuilt. |
| Transcripts | Ten generated package adapters own provider roots, discovery, identity parsing, and normalized parsing; core retains canonical path security, bounds, read modes, and DTOs. | Transcript locators are not yet bound to the future active conversation-generation record. |
| Controllers | Generated catalog metadata supplies built-in presentation/setup; remote summaries retain additive legacy `providerID`, `activeRuntimeID`, and Boolean capabilities. | Host-advertised effective action objects with reasons and adapter revisions remain future protocol work. |

The current behavior is valuable and must not be flattened into a lowest-common-
denominator plugin API. In particular, migration must retain minted IDs,
hook-captured IDs, Pi storage pinning, fresh/fork semantics, appended-context
merging, four transcript read modes, activity latches, and path validation.

## Prior-art decision: learn from Herdr, do not inherit its ceiling

Herdr validates the useful observation pipeline: identify a known runtime from
the pane's foreground process/argv, then evaluate that runtime's live parsed
terminal screen when hooks are incomplete. Its wrapper normalization, bounded
screen regions, hysteresis, and `agent explain` evidence are good prior art.
It also provides a useful normalized-ingress precedent: integrations report a
pane-scoped source, semantic lifecycle state, monotonic sequence, and optional
native session ID/path; central arbitration rejects stale/cross-talk reports,
lets complete hooks suppress screen fallback, persists one official session
reference, and can synthesize a provider-specific resume command after cold
server restore. Arbitrary custom agents may cooperatively report lifecycle
without becoming compiled automatic agent kinds. Its live-agent facade is also
useful API prior art: [`agent.start`](https://herdr.dev/docs/agent-automation/)
submits a canonical known-agent command to an existing pane at its idle shell,
then `agent.prompt`/`agent.wait` provide server-owned readiness and state waits.
That is orchestration over Herdr's built-in kinds, not third-party runtime
registration.

Those strengths do not remove Herdr's extensibility ceiling. Automatic process
identity, screen-manifest membership, official session ownership, and resume
planning remain compiled; adding Qwen still required changes across its enums,
recognition, authority tables, installer/assets, resume planner, UI, and tests.
Plugin v1 cannot register runtime actions, and a detection manifest can only
adjust screen rules for a kind the binary already knows.
Its persisted model is one current pane occupant plus one hard-coded resume
reference, not a lossless launch binding, launch/active generations,
transcripts, context/fork semantics, or a Host-advertised per-Session action
contract. Unpeel adopts the process-observation, ordering, authority, and
diagnostic patterns while preserving that deeper contract. Research baseline:
Herdr [commit `ddffb6e`](https://github.com/herdrdev/herdr/commit/ddffb6e1d79efb517a92034ed18b75c388a36e55),
its current [detection source](https://github.com/herdrdev/herdr/blob/master/src/detect/mod.rs),
[resume planner](https://github.com/herdrdev/herdr/blob/master/src/agent_resume.rs),
and [agent detection documentation](https://herdr.dev/docs/agents/).

## Load-bearing invariants

1. **Every new command launches generically.** Missing, invalid, disabled, or
   failing runtime support never prevents an untouched original command from
   starting in a generic hosted PTY. Resume, fork, appended-context, and other
   deep-operation failures abort honestly; they never fall through to a fresh
   generic conversation.
2. **The agent stays a standalone CLI.** A runtime integration teaches Unpeel
   about a user-installed command; it does not embed, proxy, or become the
   vendor runtime. Reviewed setup metadata may offer the vendor's official
   install path.
3. **The Host owns runtime truth.** It resolves commands and live processes,
   installs trusted support, derives Session capabilities, persists runtime
   identity, and performs runtime operations. Controllers never scan PATH,
   inspect processes, load adapters, or parse provider commands.
4. **Recognition is not capability.** “This is Codex” does not imply that this
   Session has an exact conversation ID, a readable transcript, or a safe
   relaunch recipe.
5. **Launch and active runtime are distinct.** A shell may run several agents.
   A late foreground-process match may update active display/status without
   rewriting the stable launch recipe or stealing another runtime's
   conversation identity.
6. **Capabilities are Session-specific.** Advertise both implementation support
   and current availability, precision, local resume scope, canonical handoff
   strategy, and an optional reason for absence.
7. **Unpeel and runtime identities stay distinct.** The Unpeel Session ID
   targets the hosted Session and Sessions MCP. The runtime conversation ID
   targets the agent provider's conversation and may survive several Unpeel
   Session processes.
8. **Exactness is honest.** Never call a `continue-last` guess exact, never use
   it for cross-Host handoff, and never infer handoff support merely from having
   an ID.
9. **Detection never grants trust.** Seeing a matching executable cannot
   authorize third-party code execution, configuration changes, hook
   installation, or transcript access.
10. **Hooks and screen evidence have declared authority.** Screen matching is a
   fallback/cross-check after runtime identity, never an input-driving or
   destructive automation mechanism.
11. **One activity and notification model.** Runtime signals feed the existing
   derived activity engine and existing push/attention policies; they do not
   create a second notification channel.
12. **Remote scope remains a pure client.** Runtime resolution and adapter work
    occur on the selected Host. A Controller scoped to a remote Host installs
    nothing and starts no local Session.
13. **One Host contract.** Native Mac and headless Mac/Linux Hosts advertise
    the same additive runtime/session fields and operations over the existing
    LAN, SSH, and Link/Relay transports. No Host-kind branches or 404 probes.
14. **No central daemon and no bundled Node.** Built-ins live in shared Rust;
    later executable adapters are bounded subprocesses, not a permanent
    runtime service or dynamic-library ABI.
15. **Cloud services receive no runtime content.** Descriptors, conversation
    IDs, adapter state, hooks, transcripts, and screen evidence remain on the
    user-owned Host. Relay remains opaque transport.
16. **Outer supervisor environments stay contained.** Preserve `HERDR_*`
    removal at both generic Host/provider launch boundaries for built-ins and
    third-party adapters alike.

## Host-owned runtime model

The first implementation introduces shared Rust types; exact names may change,
but the distinctions may not:

```text
RuntimeDescriptor
|- stable id + descriptor version
|- label, tint, generic icon key + optional package-local SVG/provenance
|- supported platforms
|- command matchers + live-process matchers
|- optional runtime variants + bounded CLI-version probe
|- suggested presets
|- declared implementation capabilities
|- lifecycle authority/fallback policy
`- optional adapter id

LaunchRecipe
|- original user-authored shell command
|- effective decorated shell command
|- parse class: simple_exec | wrapper | compound | opaque
`- adapter-owned edits with provenance

LaunchRuntimeBinding
|- runtime id + descriptor/adapter version
|- optional runtime variant + vendor CLI version
|- detection source + confidence + matched evidence
|- support-install revision/status
`- stable LaunchRecipe captured before spawn

ActiveRuntimeObservation
|- optional current runtime id
|- optional runtime variant + vendor CLI version
|- foreground process identity + start time
|- observation source/confidence/time
`- safe display/activity capabilities only

RuntimeConversationRef
|- runtime id
|- conversation id, if known
|- identity generation
|- capture source + captured time
|- transcript locator, if known
|- resume precision
`- Master Plan handoff strategy

SessionConversationBindings
|- launch conversation: pinned to the relaunch binding
`- active conversation: optional/volatile and keyed to the active binding epoch

RuntimeImplementationCapabilities
`- what the selected descriptor/adapter knows how to do

EffectiveSessionActions
`- Host-computed actions: target binding, available, strategy, reason unavailable
```

### Two identity domains and two runtime bindings

One runtime conversation may span multiple hosted Unpeel Sessions, but
same-PTY Resume Agent does not replace its Session:

```text
Unpeel Session A
  runtime generation 1 --Resume Agent--> runtime generation 2
             \                               /
              +-- runtime conversation XYZ -+

Unpeel Session A --handoff / fork / stopped Resume--> Unpeel Session B
             \                                      /
              +------ runtime lineage/identity -----+
```

`Copy Unpeel session ID` remains a universal core action and should be named
unambiguously. Runtime conversation references are keyed to either the stable
launch binding or the current active binding. Resume/fresh/fork always name the
launch binding. Copy conversation ID and transcript actions name their target
runtime explicitly; when launch and active differ, the UI must not pair one
runtime's label with the other's ID. The active reference is cleared when that
runtime's exit/return-to-shell is confirmed and never overwrites the launch
reference.

A separate `Copy <runtime> conversation ID` action is available only when the
target binding has captured one; controller summaries advertise availability
and target, with a scoped Host action returning the value on demand.

### Persistence

Add optional, backward-compatible runtime fields to `manifest.json`. Keep the
stable launch binding separate from a nullable current observation. An exited
agent's last observation may remain in local diagnostics, but it is not
advertised as current. Consumers treat a persisted observation as current only
while the manifest/Host heartbeat and process identity are valid:

```json
{
  "runtime": {
    "launch": {
      "id": "com.anthropic.claude-code",
      "descriptorVersion": "1",
      "adapterVersion": "builtin-1",
      "variant": "current",
      "cliVersion": "1.2.3",
      "detectionSource": "prelaunch-command",
      "confidence": "exact",
      "supportRevision": "...",
      "capabilitySnapshot": {},
      "launchRecipe": {
        "originalCommand": "claude",
        "effectiveCommand": "claude",
        "parseClass": "simple_exec",
        "ownedEdits": []
      }
    },
    "currentObservation": {
      "id": "com.anthropic.claude-code",
      "detectionSource": "live-process",
      "confidence": "exact"
    }
  }
}
```

The first blank-terminal slice writes only the nested `currentObservation`
member and uses transitional existing integration IDs such as `claude` and
`codex`; it deliberately does not fabricate `launch`. The descriptor migration
must map those values to the frozen IDs without changing the nesting or making
old observations relaunch-capable. The in-place Resume Agent slice also writes
transitional top-level `runtime_launch_generation`, `runtime_launched_at`, and
`runtime_launch_output_offset` fields. They reset prior-generation lifecycle
evidence and anchor resume-failure scanning without pretending that the
display-only observation is a launch binding; the eventual nested launch model
must preserve those semantics during migration.

Do not rename, remove, or silently change the shape of
`provider-session.json`; old apps and archived Sessions rely on its
`provider_session_id` and `provider_transcript_path` keys. Introduce a separate
canonical `runtime-conversations.json` while continuing to mirror the **launch
conversation only** into those legacy keys for old readers. New readers prefer
the canonical record and fall back to the old marker:

```json
{
  "version": 1,
  "launch": {
    "generation": 3,
    "runtime_id": "com.anthropic.claude-code",
    "conversation_id": "XYZ",
    "capture_source": "hook",
    "resume_precision": "exact_id",
    "handoff_strategy": "unsupported",
    "transcript_locator": {},
    "captured_at": 1786460000000
  },
  "active": null
}
```

Entering `/resume`, `/clear`, or similar text does not itself advance identity.
A generation advances only when a subsequent validated capture establishes a
different conversation identity, including a provider-confirmed in-tool
switch. The ID and transcript locator update together; never merge a new ID
with a stale path from a previous generation. The marker remains crash-safe and
atomic. One core Host/helper is the sole read-modify-write owner of the
canonical marker, using `<marker>.lock`, unique temporary files, and atomic
rename. Hook scripts, app, and TUI submit observations; they do not independently
increment generations. Conversation identity must be persisted by that path
even while no frontend is running. In a generic shell, a conversation captured
for the active runtime may enable targeted copy/transcript features without
making that runtime the Session's relaunch target; Resume remains attached to a
compatible stable launch binding and lossless launch recipe.

Legacy manifests and markers continue to decode. A runtime may be inferred for
an old Session only when the built-in matcher is unambiguous. Ambiguous legacy
Sessions stay generic rather than having an ID interpreted by the wrong
adapter. During the compatibility window, an old frontend may still update the
legacy marker; the Host imports such a change only when it can bind it to the
stable runtime safely, and never lets a legacy path overwrite a newer canonical
identity generation.

## Presets and runtime resolution

### Presets remain arbitrary commands

Presets keep their current flat, user-ordered storage. The runtime system adds
at most:

```text
runtime_hint: optional stable runtime id
```

`runtime_hint` is an escape hatch for a wrapper the automatic matcher cannot
see. It does not make the preset dependent on the runtime package.

Runtime availability scanning, fresh-install seeding, and later explicit
suggestion import happen on the Host that will run the command. An import shows
the exact command and source before writing through the normal locked
`app-state.json` path. A remote Controller consumes that Host's runtime/preset
catalog and never assumes its own PATH.

Arbitrary quick-launch presets also need a generic grouping rule. Use the
resolved runtime ID when available; otherwise use a stable normalized launcher
key derived without executing the command. Render one generic-tint/icon chip
per key, with the existing dropdown when two or more starred presets share it.
Do not require a `QuickPresetTool`/`SetupTool` enum case merely to render an
unknown favorite.

### Two-stage detection

Runtime resolution uses both stages because they solve different problems.

#### Stage 1: prelaunch command resolution

This runs before hook/config/MCP setup and produces the initial binding and
launch plan. It understands, without evaluating arbitrary shell code:

- absolute and symlinked executable paths;
- safe leading environment assignments such as `FOO=value claude`, retained
  without evaluating expansions;
- `env` and `command` prefixes;
- `npx` and `bunx` package invocations;
- Node/Bun script and package paths;
- versioned Python executables, modules, and scripts;
- direct wrapper signatures declared by a runtime;
- a trusted explicit preset hint.

It retains original spelling and quoting. It does not run command
substitutions, expand arbitrary variables, or pretend a compound shell command
or pipeline is safely rewriteable. Ambiguous commands launch generically.

#### Stage 2: live foreground-process observation

After spawn, the Host observes the PTY foreground process group and normalized
argv on macOS and Linux. It rechecks on foreground-group changes and at a
throttled recovery interval, so it can recognize:

- wrappers whose child executable becomes visible only after launch;
- an agent started later inside a blank shell;
- a foreground runtime switch inside the same terminal;
- a runtime that exits back to the shell.

The Host already owns the PTY/process group for safe termination; the observer
must reuse that ownership and the existing PID/start-time safety rules. A
foreground process group alone is insufficient because an agent may give a tool
subprocess temporary foreground ownership. Detection therefore follows the
PTY-anchored process ancestry, retains a still-live recognized agent across
short-lived descendants, and uses hysteresis/repeated misses before clearing or
switching. Clear current observation and its active conversation only after
confirmed agent exit or return to the shell.

Process observation may correct active display identity, screen rules, and
safe read-only capabilities. It does not overwrite the stable launch binding.
It must not reconstruct a lossy launch command, promise Resume, or
execute/install a newly discovered third-party adapter. Agents hidden behind a
remote `ssh` process, container/VM boundary, nested multiplexer, or inaccessible
process namespace remain unknown unless an explicit trusted hint or cooperative
report identifies them. Any future promotion from observation to a
relaunch-capable binding needs an adapter-validated lossless launch recipe and
explicit conformance coverage.

Screen text alone never identifies a runtime. It is too easy for ordinary
output or conversation content to resemble an agent UI.

### Diagnostics

Ship local user/operator diagnostics with the first detector:

```sh
unpeel runtime list
unpeel runtime explain --command 'npx @openai/codex'
unpeel runtime explain --session <unpeel-session-id>
unpeel runtime validate <installed-manifest-or-path>
```

Source contributors use repository tooling whose fixtures are not required in
release artifacts:

```sh
cargo xtask runtime validate <runtime-id>
cargo xtask runtime test <runtime-id>
```

`explain` reports the selected Host, match candidates, winning evidence,
confidence, active process, support-install state, conversation identity
source, and why each capability is or is not available. It never prints
transcript content, hook payloads, credentials, or unrestricted environment
values.

## Runtime capabilities and Session verbs

Keep three layers distinct:

1. **Host operation:** whether this Host/protocol can execute an operation.
2. **Runtime implementation:** whether the adapter knows the provider
   behavior.
3. **Session availability:** whether this Session captured the data and launch
   recipe needed to perform it safely.

The user-visible action is the intersection of all three.

| Action | Owner and availability rule |
| --- | --- |
| Copy Unpeel Session ID | Universal core action. |
| Rename, pin, remove, terminal input | Core Session actions, subject to existing lifecycle rules. |
| Archive | Effective Session action; preserve the current rule that archiving is offered only when the Session can be resumed safely (including the blank-shell exception). |
| Copy agent conversation ID | Runtime action; available only when the named launch/active binding has a conversation ID. |
| Resume conversation | Runtime action; requires a safe relaunch plan and the strategy-specific identity/state. |
| Fresh conversation | Runtime action; strips prior resume state and mints/isolates a new identity where required. |
| Fork conversation | Runtime action; requires a native provider fork primitive and a supported source strategy. Exact-ID and provider-last/same-Host forks are reported with different precision. |
| Append system context | Runtime action; applies on the next Resume Agent or replacement Resume through an append-mode provider mechanism. |
| Copy/read transcript | Runtime action; requires a validated resolver/parser and a transcript source for this Session. |
| Notify when done | Intersection of reliable runtime completion and a Host notification/push capability. |

Keep runtime implementation support internal and expose one Host-computed
effective action object rather than a second competing capability source:

```json
{
  "resume": {
    "available": false,
    "precision": "exact_id",
    "resumeScope": "same_host",
    "handoffStrategy": "unsupported",
    "targetBinding": "launch",
    "reasonUnavailable": "conversation_id_not_captured"
  }
}
```

Old Boolean fields remain during the protocol compatibility window and are
derived from the new structure.

## Conversation identity, resume, fresh, and fork

The adapter contract preserves every current resume tier:

| Strategy | Meaning |
| --- | --- |
| `exact_id` | Unpeel minted or captured an ID that targets one provider conversation. Existence verification is a separate optional state because not every provider exposes it. |
| `pinned_storage` | Provider storage was isolated/pinned so a continue operation is exact for this Session. |
| `continue_last` | Provider can only continue the latest conversation; preserve for current local compatibility but label it non-exact. |
| `picker` | Provider exposes an explicit history picker rather than an exact automatic ID. |
| `unsupported` | No honest continuation mechanism. |

Availability records local resume scope separately from the canonical Master
Plan Phase 7 handoff strategy: `exact_portable_id`,
`scoped_state_transfer`, `explicit_context_continuation`, or `unsupported`.
Cross-Host handoff never uses `continue_last` and never infers portability from
local resume scope alone.

Fork precision is similarly explicit: `exact_id` when the provider targets a
specific source conversation, or `provider_last_same_host` for shipped
provider primitives such as “fork last.” The latter remains supported for
local compatibility but is non-portable and must not be presented as exact.

The runtime adapter receives one complete relaunch request so ordering remains
canonical:

```text
plan_relaunch
|- original launch command/recipe
|- captured runtime binding and adapter version
|- conversation reference
|- has_been_written_to
|- mode: resume | fresh | fork
|- pending appended system context
`- current Host capabilities
```

It returns a new `LaunchRecipe` plus the expected next-conversation behavior.
The persisted command remains a shell command, so the adapter cannot flatten an
opaque/compound command into argv without changing semantics. Only a losslessly
parsed simple exec/wrapper may receive structured edits; other recipes remain
generic or fail the requested deep operation. Rewrites preserve the original
command and may strip/replace only adapter-owned edits recorded with
provenance—not coincidentally similar flags supplied by the user. The Host
remains responsible for spawning, Session ID minting, manifests, worktrees,
MCP grants, approvals, and liveness.

Identity transitions are explicit:

- **Resume Agent / resume in place:** after the runtime has returned to its
  live shell, the same hosted Unpeel Session retains the launch conversation
  reference, advances its runtime launch generation,
  clears prior-generation lifecycle evidence, and lets a hook/report advance
  the new capture generation;
- **Restore or handoff with resume:** a replacement Host/Session receives the
  same launch conversation reference, subject to the operation's scope and
  precision contract;
- **Fresh:** old conversation/transcript state is not inherited; mint or create
  a new pending launch reference where supported;
- **Fork:** create a new pending launch conversation with source lineage but
  never copy the source transcript locator or claim the same conversation ID;
- **Late active runtime:** update only the active reference and its targeted
  read/copy actions; never replace the launch conversation.

Required compatibility details include:

- do not resume an unused minted ID that never became a provider conversation;
- run an existence check before resume where a provider offers one, without
  pretending that all exact-ID providers can verify storage first;
- fresh removes old resume markers and mints/isolates where supported;
- provider-specific fork syntax and flag stripping remain exact;
- Pi-style storage pinning remains available;
- provider-ID changes trigger transcript-based auto-title only from a trusted
  exact identity/path, never a cwd-only heuristic;
- provider-specific resume-failure markers and the existing force-fresh
  recovery surface remain intact;
- runtime-owned cleanup, including Pi's isolated Session storage, remains
  scoped to the correct fresh/fork/remove lifecycle;
- an agent detected late inside a generic shell does not gain Resume unless
  the Host captured a safe launch recipe and required identity;
- adapter failure never destroys the old Session or consumes its identity.

## Append system context

Appended system context is a runtime capability, never a preset field. Preserve
the shipped contract:

- text is saved as a per-Session marker;
- it applies on the next Resume Agent after the managed runtime returns to its
  shell, never by interrupting an active agent;
- it appends to provider/base instructions and never replaces them;
- repeated appends merge into one canonical provider option;
- resume/fresh/fork is derived before context is applied;
- the existing restart-recommendation path surfaces the pending change;
- relaunch planning and support-install failure happen before PTY submission
  and leave the Session, shell, and marker intact;
- on Resume Agent, the Host verifies that the managed runtime is no longer
  active and the existing owned shell has the PTY, stages the marker, submits
  the rewritten command, then advances the runtime generation and
  removes the marker. The Session id, Host pid, socket, output log, artifacts,
  and grants do not change;
- marker consumption is an exact-snapshot transaction: Rust and Swift writers
  attach a fresh revision and share a stable per-Session flock; the Host holds
  that lock across final compare, staging, and PTY submission. A context (even
  identical text) or clear published concurrently is preserved as the next
  intent, and a pre-submit failure restores the consumed snapshot first;
- on Fork, the text is applied to the fork command while the source Session and
  its marker remain unchanged.

Terminal reload, stopped Resume, archived Restore & Resume, handoff, Fresh,
and Fork remain separate lifecycle operations. They must not be disguised as
Resume Agent or authorized from a display-only process observation. A provider
crash that returns to the live shell can expose Resume Agent; a Host crash
stops the terminal and exposes ordinary replacement Resume instead.

Simple runtimes may declare a safe append flag. Complex behavior such as
Codex's typed configuration override stays adapter code. Controllers submit
text to the Host and never learn provider flags.

## Hooks, lifecycle authority, and visible-state fallback

Replace `uses_hook_port: bool` with an explicit lifecycle contract:

```text
source: hooks | self_report | screen | output
authority: complete | partial | identity_only | none
fallback: screen | output | none
completion_reliable: bool
attention_reliable: bool
anchor_start_event_to_output: bool = true
attention_clears_on_output: bool = true
distrust_stops_while_output_grows: bool = false
```

Meanings:

- **complete:** lifecycle events are authoritative while support is installed
  and reporting;
- **partial:** hooks own the transitions they cover; declared screen rules
  cover missing cancellation, interrupt, or permission states;
- **identity_only:** hooks capture conversation identity but do not suppress
  normal activity derivation;
- **none:** use generic viewport/output behavior.

Support-install status is part of the Session binding. If hook installation
fails, the runtime follows its declared fallback instead of silently entering
a hook-owned state that will never receive events.

Provider-specific hooks normalize into the existing Unpeel lifecycle
vocabulary (`Start`, `UserPromptSubmit`, `Stop`, `StopFailure`,
`PermissionRequest`, plus the hook-seen latch). Preserve the first-event latch,
five-minute output-rearmed timeout, durable seed semantics, provider-specific
attention clearing, and generic menu-prompt override until a separately tested
shared activity derivation replaces them.

Every normalized hook/identity observation carries a runtime ID, integration
revision, Session ID, sequence/order evidence, and event source. A hook format
that cannot name its runtime enters through adapter-specific support already
bound to the stable launch runtime. The Host validates that binding, rejects or
quarantines conflicts, and never lets an event from a late active runtime
overwrite the launch conversation used for relaunch. The same single core
writer described above serializes conversation-marker changes; scripts and
frontends remain compatibility transports, not competing authorities.

Conversation identity and lifecycle durability are separate. The existing
`last-hook-event.json` keeps activity recoverable while no frontend is open;
the new Host-side conversation marker must likewise capture provider identity
without depending on a frontend receiving the HTTP broadcast.

### Screen rules

A runtime may include bounded rules over the Host's current libghostty-vt
screen grid. V1 does not depend on OSC title/progress because the current
viewport wrapper does not expose them; adding bounded sanitized OSC capture is
a separate tested extension. Rules run only after identity is known and may
produce `busy`, `attention`, `candidate_idle`, or `no_change`. They never send
input, approve a prompt, start an adapter, or directly trigger a destructive
action.

Activity carries evidence provenance. `candidate_idle` cannot be the sole
completion signal for notify-when-done, auto-stop, archive, or another
destructive/terminal transition; it needs a reliable lifecycle completion or
the existing output-quiescence policy as corroboration. This prevents a stale
or newly changed agent screen from stopping a still-working Session.

Screen samples are optional test fixtures, not runtime state:

```text
fixtures/screens/
|- permission-prompt.txt
|- working-spinner.txt
|- normal-prompt.txt
`- transcript-viewer.txt
```

Each fixture declares the expected state, viewport dimensions/region, and
attention provenance. Screen-derived attention is display-only and does not
send a push in v1; notification eligibility continues to require a reliable
lifecycle event. Sanitize prompts, paths, user content, and provider IDs before
committing them. Include narrow/wide layouts, plausible negative matches, and
stale/transcript screens. Regexes need size/time bounds.

The hot activity-engine migration follows `docs/plans/shared-core.md`; this
plan does not bypass its shipped-release gate or add a parallel activity
engine.

## Transcripts

Runtime adapters plug into the existing normalized transcript model; they do
not return an arbitrary Markdown blob as the only representation. Preserve:

- `snapshot`, `stream`, `history`, and `markdown` read modes;
- normalized text, reasoning, tool, result, file-change, usage, and provider
  metadata blocks;
- exact captured-path and conversation-ID resolution before bounded CWD/time
  heuristics;
- partial-record/offset semantics for live streams;
- shared rendering settings used by MCP, desktop, TUI, and phone.

An adapter-returned path is a claim, not authority. Core canonicalizes both
the path and allowed root, rejects relative/traversal/symlink escapes, enforces
runtime-specific root/extension/file-name rules, and bounds searches and
reads. Ambiguous transcript discovery cannot establish exact resume identity
or drive destructive behavior.

The conversation marker binds transcript locators to the runtime and identity
generation so a new provider ID cannot inherit a stale path.

## Descriptor and adapter boundary

### In-repository contribution shape

The compiled built-in source layout is now one discoverable runtime directory:

```text
runtimes/<slug>/
|- runtime.toml
|- adapter/                    # optional reviewed built-in module tree
|- assets/                     # optional icon + owned hook/plugin/setup assets
`- fixtures/
   |- commands.toml
   |- processes.toml
   |- lifecycle.ndjson
   |- resume.toml
   |- transcripts/             # optional sanitized provider records
   `- screens/                 # optional rendered-screen test cases
```

`runtime.toml` uses a stable reverse-DNS runtime ID (for example,
`com.anthropic.claude-code`) and a separately versioned schema. It also maps
that identity to the existing legacy provider slug while old DTOs ship; the
mapping is explicit rather than mechanically derived. It can declare:

- label, tint, platforms, and official install URL; reviewed built-ins may also
  carry the vendor's sanctioned install recipe;
- optional contributed `assets/icon.svg` plus required source/license
  provenance; built-in client catalogs embed it at build time and fall back to
  the generic agent icon rather than exposing a Host-local path;
- legacy provider slug used by old Controller DTOs;
- command/process matchers and explicit false-positive cases;
- suggested presets;
- lifecycle authority and safe screen rules;
- implemented capabilities and the ID of optional trusted built-in behavior.

Every contributed logo, hook/plugin asset, or vendored binary records its
source and license; generated/binary artifacts follow the open-source plan's
provenance gate.

Descriptor version, adapter version, runtime variant, and vendor CLI version
are independent values. A CLI-version/variant probe runs only when a reviewed
descriptor declares a bounded, side-effect-free probe, with a timeout and
output cap; never infer the vendor version from the adapter package version.
This distinction covers providers such as Kimi whose current and legacy
generations share a command name but need different setup behavior.

Simple descriptors must be discoverable without editing a central registry.
Complex provider logic may add one Host-side adapter module but no frontend
code. A build-time registry generator or equivalent validated discovery step
must fail on duplicate IDs, invalid capabilities, missing fixtures, and
unsupported schema versions.

### Cooperative runtime protocol

Agents willing to integrate directly should get a dedicated, versioned local
sidechannel, conceptually `UNPEEL_AGENT_CONTEXT_FD` with protocol
`unpeel.agent/1`. Do not use PTY stdout or an inherited reusable bearer token.
The channel may report normalized lifecycle, conversation identity,
capabilities, and transcript locators. The Host scopes it to one Session and
owns sequence/order validation and persistence.

Existing provider hooks remain supported and may emit the same canonical
events through their current local transport during migration.

### External executable adapters

Only add executable adapters after real contributors demonstrate behavior that
cannot be expressed safely as data. Use a versioned JSON/MessagePack subprocess
protocol, not a Rust dylib ABI. Coarse operations include:

```text
probe
prepare_launch
install_support
plan_relaunch
capture_conversation
append_context
transcript_resolve
transcript_read/follow
```

Calls are short-lived and bounded; a supervised transcript follower is the
only justified longer-lived operation. Adapters return plans/data. They never
own the PTY, Session manifest, liveness, state bus, Host router, remote
transport, credentials, or notification delivery.

## Host/Controller contract

Extend existing Session summaries/details additively:

```json
{
  "runtime": {
    "launch": {
      "id": "com.anthropic.claude-code",
      "variant": "current",
      "cliVersion": "1.2.3",
      "label": "Claude Code",
      "tint": "...",
      "icon": "generic-agent"
    },
    "active": {
      "id": "com.openai.codex",
      "label": "Codex",
      "tint": "...",
      "icon": "generic-agent",
      "detectionSource": "live-process"
    }
  },
  "conversations": {
    "launch": {
      "idAvailable": true,
      "resumePrecision": "exact_id",
      "handoffStrategy": "unsupported"
    },
    "active": {
      "idAvailable": true,
      "resumePrecision": "exact_id",
      "handoffStrategy": "unsupported"
    }
  },
  "sessionActions": {
    "resume": { "available": true, "targetBinding": "launch" },
    "fresh": { "available": true, "targetBinding": "launch" },
    "fork": { "available": true, "targetBinding": "launch" },
    "appendSystemContext": { "available": true, "targetBinding": "launch" },
    "copyAgentConversationID": {
      "targets": {
        "launch": {
          "available": true,
          "runtimeID": "com.anthropic.claude-code"
        },
        "active": {
          "available": true,
          "runtimeID": "com.openai.codex"
        }
      }
    },
    "transcript": {
      "targets": {
        "launch": { "available": true },
        "active": { "available": true }
      }
    },
    "archive": { "available": true },
    "notifyWhenDone": { "available": false,
      "reasonUnavailable": "host_push_unavailable" }
  }
}
```

The stable Host capability ledger continues to advertise operations. Every new
remote verb receives a stable operation ID in
`protocol/host-capabilities-v1.json` and valid/invalid cases against both Host
adapters in `protocol/host-conformance-v1.json`. Effective `sessionActions` are
the second, per-Session gate—not a replacement capability source. They are the
Host-computed intersection of Host operation support, runtime implementation,
and current Session state.

Old `providerID` and Boolean capability fields remain derived for old
Controllers during the compatibility window. New Controllers against old Hosts
degrade to generic/legacy behavior when the additive runtime object is absent.

All matching, support installation, identity lookup, relaunch planning, and
transcript parsing run on the selected Host. A Mac/iPhone/TUI Controller renders
the Host-provided label, tint, generic icon, availability, binding target, and
reason. Presentation metadata is carried for each non-null binding, and
read/copy actions expose a Host-resolved target map so a Controller can present
both launch and active conversations without pairing one runtime's identity
with the other's label. Host-local icon paths are never placed on the wire. It
does not branch on native versus headless Host.

## Trust, permissions, and distribution

Use three trust classes:

1. **Reviewed in-repository integrations.** Compiled/shipped with Unpeel and
   covered by the release tests. This is the first community contribution path.
2. **External data-only descriptors.** May add detection/display/suggested
   presets and bounded screen rules. They may show an attributed URL or
   instructions but not an auto-executed install recipe. They cannot grant
   filesystem roots, install hooks, rewrite commands, or execute code merely
   by being present.
3. **External executable adapters.** Explicitly installed and enabled by the
   user on each Host, with declared permissions and provenance.

Reviewed built-ins retain today's idempotent install/refresh-at-spawn behavior
for their owned hook assets and configuration entries. That shipped trust does
not transfer to an external package merely because it uses the same runtime ID.

An executable adapter is same-user code; subprocess RPC is isolation and
versioning, not a security sandbox. Before this tier ships, define:

- stable package ID/version and adapter protocol negotiation;
- exact executable/hash/source provenance and update ownership;
- explicit grants such as configuration write, hook installation, provider
  history read, and launch rewrite;
- minimal sanitized environment, removing Link/Relay/pairing credentials and
  all inherited `HERDR_*` values;
- deadlines, response-size limits, stderr capture bounds, crash handling, and
  malformed-output behavior;
- transactional/idempotent config merging with locks, atomic writes, ownership
  markers, and preservation of unrelated user settings;
- cleanup limited to explicitly owned files under validated paths;
- local diagnostics and an emergency disable path.

Detection, PATH scanning, a downloaded manifest, or a runtime hint never
constitutes enablement. There is no automatic cloud registry or marketplace in
this plan. The entire Host/client/adapter contract remains in the open-source
boundary; local/direct support has no entitlement gate.

Descriptor/adapter install, link, update, enable, disable, and remove writes use
the workspace-resolved Unpeel home, existing lock/atomic-write conventions, and
the existing state bus; one-shot CLI commands flush before exit. A running or
archived Session keeps its launch binding, descriptor/adapter version, owned
edits, and capability snapshot. Package changes affect new launches by default.
If a live Session needs refreshed support, surface it only through the existing
`restartRecommendations` API—never hot-swap relaunch behavior or add a second
banner path.

## Migration and compatibility

Migration is incremental and preserves a release fallback where another plan
already requires one.

- Capture current behavior in fixtures before moving ownership.
- Introduce the Rust runtime model behind existing provider outputs first; no
  visible behavior change.
- Add only optional manifest/marker/DTO fields. Missing fields keep old
  Sessions visible and generic/legacy behavior available.
- Keep `provider_session_id`, `provider_transcript_path`, `providerID`, and old
  Boolean capabilities during the compatibility window.
- Bind new conversation markers to runtime and generation while continuing to
  read old markers.
- Pin deep behavior to the Session's launch snapshot. Descriptor/adapter
  updates do not silently reinterpret a running or archived command.
- Retain built-in adapter compatibility for archived Sessions. If a current
  adapter cannot interpret an old snapshot, report Resume unavailable with a
  reason; never guess or destroy the archive.
- Native/TUI/phone consume the generated runtime catalog for built-in
  presentation and declared capabilities. Provider launch, resume, context,
  storage, and failure recipes are Host-owned; legacy DTO fields remain only
  as compatibility projections for older Controllers.
- State-file migrations use existing locks, atomic rename, and state-bus rules.
- Detection/support-install/prepare failure on a new launch may run the
  untouched original command generically. Resume, fork, context planning,
  transcript, or any operation where fallback would change meaning fails
  explicitly before teardown and retains pending context, identity markers,
  and the source Session for diagnosis.

`docs/agents/providers.md` and `runtimes/README.md` now document the compiled
one-directory contribution boundary. They must continue to distinguish that
shipped source workflow from the still-unbuilt installable external adapter
protocol.

## Implementation phases

Each phase is independently shippable. This plan is open-source preparation
adjacent to the Master Plan; it does not renumber or delay its critical path.

### Phase 0 — record the risky behavior first

**Goal:** create enough evidence to refactor safely without turning exhaustive
fixture capture into a months-long prerequisite.

Deliver:

1. An inventory matrix for every shipped provider covering detection, setup,
   launch rewriting, hook authority, conversation capture, resume/fresh/fork,
   appended context, transcript modes, auto-title, cleanup, and effective
   actions; missing coverage stays explicit.
2. Shared high-risk fixtures for unused minted IDs, hook IDs, Pi storage
   pinning/cleanup, continue-last, picker behavior, fork-last, repeated context,
   resume-failure/force-fresh recovery, trusted-path-only auto-title, transcript
   attacks, and old/new Controller shapes.
3. Complete vertical fixtures for the two Phase 1 pilot providers—one mostly
   declarative and one with complex launch/resume/transcript behavior.
4. A rule that each remaining provider gains its provider-specific fixtures
   immediately before it migrates in Phase 2, not as a global Phase 0 gate.

**Exit:** the common loss-of-conversation/security risks and both pilot
providers are test-pinned; the inventory identifies the remaining migration
work honestly.

### Phase 1 — private two-provider vertical slice

**Goal:** exercise the whole proposed boundary before freezing or advertising a
public contribution format.

Deliver:

1. Provisional `RuntimeDescriptor`, `LaunchRecipe`, launch/active bindings,
   launch/active conversation references, runtime implementation capabilities,
   and effective Session action types.
2. A minimal Host-owned normalized hook/identity ingress with validated runtime
   ID, ordering, one locked canonical marker writer, no-frontend durability,
   and a legacy provider-marker mirror. Activity-authority policy remains
   unchanged for now.
3. Optional manifest/runtime/conversation fields and additive Host DTOs with
   old-reader fallbacks.
4. Port one simple and one complex provider end-to-end through detection,
   setup/assets, launch recipe, identity, resume/fresh/fork as applicable,
   appended context, transcript security, auto-title, cleanup, and effective
   action derivation.
5. Make existing controllers generically render the provisional runtime label,
   tint, generic icon, and effective actions while retaining legacy behavior
   when additive fields are absent.
6. Run both pilots through native/headless Host and old/new Controller
   conformance, then revise the shapes based on the result.

No external descriptor loading and no one-directory contributor claim ships in
this phase.

**Exit:** two materially different providers reproduce shipped behavior through
one Host-owned vertical path, and the descriptor/adapter/session-action v1 shape
is ready to freeze rather than merely plausible on paper.

### Phase 2 — freeze v1 and migrate every built-in

**Goal:** make the Host the sole provider authority and retire duplicated
provider decisions through a compatibility window.

Deliver:

1. Freeze runtime descriptor/adapter/session-action v1 after the pilot review.
2. Migrate each remaining built-in, adding its risk-based fixtures first;
   preserve setup assets, launch transforms, lifecycle policy, exact/non-exact
   resume, failure recovery, context, transcript, auto-title, and cleanup.
3. Add a Host runtime catalog for availability/PATH status, variants, official
   setup metadata, suggested presets, and capabilities; migrate native setup
   and first-run consumers to it.
4. Make Host-derived effective actions the source for native, TUI, phone, and
   remote summaries while deriving legacy `providerID`/Booleans for old clients.
5. Pin running/archived bindings and route support updates through the existing
   restart-recommendation path.
6. Delete Swift/TUI provider decision tables only after the shipped fallback
   window required by `shared-core.md`.

**Exit:** all shipped providers use the v1 Host model; adding another built-in
requires no frontend provider switch, and archived/resumed Sessions retain
their current behavior.

### Phase 3 — source contribution surface and prelaunch detection

**Goal:** make an in-repository runtime contribution genuinely small and
testable without yet installing third-party code.

Deliver:

1. The one-directory source layout, v1 manifest/schema, generated discovery,
   scaffold, and repository validator/test runner.
2. Non-evaluating prelaunch resolution for fixture-covered direct commands,
   safe assignments, package runners, script runtimes, symlinks, and declared
   wrappers; ambiguous/compound commands stay generic.
3. Shipped `runtime list/explain/validate` diagnostics and separate source-only
   `cargo xtask runtime validate/test` commands.
4. Host-catalog-backed explicit suggestion import and fresh-only automatic
   seeding, with full command/source display and no resurrection.
5. Arbitrary quick-launch grouping with generic tint/icon plus optional trusted
   runtime hints.
6. A public “Adding an Agent Runtime” guide and one new community-style example
   that requires no frontend edits.

**Exit:** a basic in-repository runtime lands as one descriptor directory and
fixtures; a complex one adds one Host adapter/assets tree. Unknown commands and
uncovered wrappers behave exactly as generic terminals.

### Phase 4 — live observation, lifecycle authority, and cooperation

**Goal:** add Herdr-style runtime observation and honest hook/screen fallback on
top of the already-proven runtime contract.

**Early slice (2026-08-15):** the PTY-owned macOS/Linux observer, transition
hysteresis, nested current observation, and local/remote presentation path have
landed early to support agents started from a blank shell. It currently matches
the built-in integration IDs and reuses the generic activity heuristic (the
legacy hook latch cannot safely distinguish sequential agents in one shell).
The same slice now includes generation-bound **Resume Agent** for a stable
managed launch after it returns to the shell: the Host revalidates that the
owned shell has the PTY and resumes the launch there without stopping an active
runtime. A runtime merely observed after
being typed into a blank terminal remains presentation-only and cannot enable
that action. This proves the user-visible path and safe operation split, but
does not satisfy this phase's descriptor, authority, screen-rule, or
cooperative-protocol exit.

Deliver:

1. macOS/Linux PTY-anchored process-tree observation with PID/start-time
   safety, ancestry preference, hysteresis, runtime transitions, and explicit
   SSH/container/VM/nested-multiplexer limitations.
2. Replace hook-capable Booleans with complete/partial/identity-only/none
   authority plus support-install status and fallback, reusing the canonical
   Phase 1 event/identity ingress.
3. Add bounded screen-grid rules, evidence provenance, sanitized fixtures, and
   the no-screen-only-completion safety gate.
4. Introduce and conformance-test `unpeel.agent/1` over a Session-scoped
   inherited channel.
5. Feed the one shared activity derivation according to the sequencing gate in
   `shared-core.md`; do not create a second engine.

**Exit:** shell-started/fixture-covered visible runtimes are recognized;
lifecycle state remains safe when hooks are complete, partial, missing, or fail
to install; a cooperative agent needs no vendor-config hook.

### Phase 5 — external data-only integrations

**Goal:** let users add safe recognition/status support without rebuilding
Unpeel or executing third-party adapter code.

Deliver:

1. Versioned installed descriptor location under the workspace-resolved Unpeel
   home (`app_paths::unpeel_home()`; `~/.unpeel` only by default), precedence,
   and conflict rules. Built-in runtime IDs are reserved; shadowing one is
   rejected except through an explicit developer override.
2. Explicit install/link/remove/list/validate commands and diagnostics.
3. Strict validation and bounds for matches, metadata, suggested presets, and
   screen rules. External suggestions are never materialized automatically;
   import shows the source and complete command first.
4. Compatibility and rollback behavior when an installed descriptor is newer,
   invalid, removed, attempts to shadow a built-in, or uses an explicit
   developer override.

**Exit:** an external data-only runtime can be recognized and observed, but
cannot gain filesystem/config/launch authority through its manifest.

### Phase 6 — trusted executable adapters

**Goal:** support third-party deep integrations only after the declarative and
in-repository paths prove insufficient.

Deliver:

1. Versioned subprocess protocol and permission model.
2. Explicit per-Host install/enable/update/disable UX with provenance.
3. Deadlines, output bounds, sanitized environment, transactional setup, and
   new-launch-only generic fallback behavior.
4. Conformance SDK/fixtures usable from more than one implementation language;
   never require a bundled Node runtime.
5. One real external adapter that exercises conversation identity, exact
   resume, hooks, appended context, and transcript security.

**Exit:** disabling/removing/crashing the adapter cannot lose a Session or stop
generic terminal access; the same Host advertises honest reduced capabilities.

## Contributor conformance

The runtime harness must cover, as applicable:

- positive and negative direct-command matches;
- absolute paths, symlinks, leading assignments, wrappers, quoting, package
  runners, LaunchRecipe edit ownership, and refusal of unsafe compound-command
  rewrites;
- macOS/Linux foreground-process observations, agent/tool ancestry, hysteresis,
  confirmed clear, runtime transitions, and hidden SSH/container cases;
- detection precedence, confidence, ambiguous hints, and false positives;
- runtime variant/vendor-version probes, timeouts, and adapter-version
  independence;
- launch-plan snapshots and malformed/timed-out adapter responses;
- hook normalization, install failure, durable activity, and no-frontend
  conversation capture;
- hook runtime-ID conflicts, ordering, one-writer locking, and legacy marker
  mirroring;
- complete, partial, identity-only, and no-hook lifecycle traces;
- screen regions/layout variants, plausible negatives, regex bounds, evidence
  provenance, and proof that screen-only idle cannot notify/auto-stop/archive;
- minted ID, hook ID, pinned storage, continue-last, picker, and unsupported
  resume strategies;
- unused minted conversations, in-tool conversation switches, and identity
  generations;
- separate launch/active conversation targets and confirmed active clear;
- Resume/Fresh/Fork identity transitions, idempotence, lineage, transcript-path
  non-inheritance, and provider-specific flag stripping;
- provider-ID auto-title trust, resume-failure/force-fresh recovery, and
  runtime-owned cleanup;
- repeated appended context without duplicate flags or base-prompt replacement;
- transcript snapshot/stream/history/markdown parity;
- transcript traversal, symlink, extension, filename, root, search, and size
  attacks;
- legacy manifests/markers plus old/new Controller/Host skew;
- native and headless Host summary/operation conformance;
- Host runtime catalog/setup parity, arbitrary favorite grouping, and explicit
  suggestion import without resurrection;
- descriptor update state-bus delivery, CLI flush, pinned running bindings, and
  sole restart-recommendation behavior;
- generic fallback for untouched new launches when detection/setup fails, plus
  explicit no-fallback failures for resume/fork/context/transcript operations;
- no local runtime work while a Controller is scoped to a remote Host;
- no `HERDR_*` leakage into adapters or provider processes;
- clean macOS and Linux builds.

Repository-wide gates remain those in `AGENTS.md`: Rust tests, native Swift
build/tests, attach smoke, TUI real-PTY cases for shared behavior, and Host
protocol conformance whenever DTO/operation shapes change.

## What not to build yet

- No runtime marketplace, automatic cloud registry, ratings, or paid runtime
  catalog.
- No arbitrary executable adapter activation from PATH detection.
- No Rust dylib ABI, embedded scripting VM, permanent adapter daemon, or
  bundled Node runtime.
- No runtime parsing or adapter loading in Mac, iPhone/iPad, or TUI Controller
  presentation code.
- No second preset system, runtime-only session type, or failure mode where an
  unknown CLI cannot launch.
- No screen rule that sends keys, approves prompts, or becomes a security
  decision.
- No replacement of base system instructions under an “append” label.
- No assumption that conversation identity implies transcript access or a
  supported cross-Host handoff strategy.
- No new Host protocol or transport; extend the existing capability-ledgered
  contract.
- No Link entitlement gate around local/direct runtimes or adapters.
- No semantic chat/code editor, file tree, diff, or other IDE surface.

## Related

- `docs/agents/providers.md` — authoritative shipped provider behavior and
  current contribution checklist
- `docs/agents/presets.md` — flat preset model and shared storage
- `docs/agents/session-model.md` — hosted Sessions, archive, restart, and
  provider conversation identity
- `docs/agents/session-activity.md` — hook latch, durable seed, timeout, and
  viewport-menu behavior
- `docs/agents/transcripts.md` — normalized provider transcript API and path
  trust boundary
- `docs/plans/shared-core.md` — Rust ownership, compatibility window, and
  activity migration sequencing
- `docs/plans/master-plan-next.md` — canonical cross-project order and
  cross-Host handoff portability gate
- `docs/plans/host-controller-transports.md` — one Host contract over direct,
  SSH, and Link/Relay transports
- `docs/plans/open-source.md` — provider integrations as a public contribution
  surface
- `docs/plans/unpeel-apps.md` — separate standalone App/Room contract
- `docs/plans/herdr-integration.md` — aggregate Herdr projection and inherited
  environment containment
