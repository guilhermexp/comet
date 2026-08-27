Unpeel is provider-agnostic: every command can run in a durable terminal, and
supported agent runtimes add only the integration claims their provider can
actually satisfy. Built-in runtimes now have one discoverable source package
under `runtimes/<slug>`.

This is currently a source-contribution workflow. Adding or changing a runtime
requires building Unpeel; installing an arbitrary third-party executable
adapter at runtime is a future capability.

## What a runtime package owns

```text
runtimes/<slug>/
├── runtime.toml
├── adapter/
│   ├── mod.rs
│   ├── setup.rs          # optional hooks, wrappers, config and MCP setup
│   ├── resume.rs         # optional resume, fresh, fork and launch identity
│   ├── context.rs        # optional append-only system context
│   └── transcript.rs     # optional transcript discovery and parsing
├── assets/
│   ├── icon.svg          # optional client-embedded runtime mark
│   └── hooks/            # optional provider scripts and plugins
└── fixtures/             # provider-owned behavior samples
```

The build discovers `runtime.toml` automatically. The descriptor owns stable
identity, legacy compatibility slug, labels and colors, install metadata,
command/process recognition, suggested presets, lifecycle authority, and
declared capabilities. Generated metadata keeps the Mac, iPhone, iPad, and
terminal UI from needing a new provider switch for presentation or presets.

To ship a custom mark, set `display.icon_asset = "assets/icon.svg"`. Also
record `display.icon_source` (an upstream HTTPS URL or explicit `internal:`
generation/migration marker) and `display.icon_license`. Monochrome SVGs are
template-tinted by default; set `display.icon_template = false` only for an
asset such as a two-tone logo whose authored fills must remain intact. Without
an asset, every client renders the generic agent fallback.

Provider-specific compiled behavior stays in that same package. Shared safety
mechanisms stay in the Host: terminal process ownership, hook ordering,
configuration locks, MCP authorization, transcript path validation, activity,
and the remote protocol.

## Choose the honest integration level

You do not need to imitate Claude Code to add a useful runtime:

| Level | What it adds |
| --- | --- |
| Preset | A convenient command in a durable terminal |
| Detection | Live name, logo, and tint while its foreground process runs |
| Managed setup | Provider hooks, wrapper/config preparation, or automatic MCP where supported |
| Conversation | Verified resume identity and safe Resume Agent behavior after the runtime returns to its shell |
| Extended capabilities | Provider-backed Fresh, Fork, append context, transcripts, or reliable notifications |

Detection is deliberately presentation-only. If someone types a recognized
agent into a blank Terminal, Unpeel must not invent its original environment,
flags, conversation, hooks, or resume recipe.

## Before implementing

Verify the provider's behavior from primary sources and fixtures:

- executable names, wrappers, package paths, and false positives;
- lifecycle event schema and whether completion is reliable;
- conversation ID or storage, exact resume, fresh, and fork semantics;
- additive MCP registration and environment forwarding;
- transcript roots, file formats, and live-write behavior; and
- official installation/configuration paths and version differences.

Declare only capabilities you can prove. Continue-last or a provider picker
can still be useful, but it should not be presented as exact resume.

## Hooks and setup

Keep provider scripts and plugins as files under `assets/hooks`, not embedded
in a central source file. Installation must be idempotent, atomically preserve
user configuration, and remove only entries owned by Unpeel.

Every Unpeel-owned lifecycle reporter carries the numeric runtime generation
in both its HTTP event and durable seed. That is what lets the Host reject a
late Stop event from the agent that Resume Agent just relaunched. Reporters
must safely do nothing outside a hosted Unpeel Session.

Keep controller output behavior in the descriptor too. The lifecycle fields
`anchor_start_event_to_output` and `attention_clears_on_output` default to
`true`; `distrust_stops_while_output_grows` defaults to `false`. Override them
only when fixtures prove a CLI's hook or terminal behavior needs it. Both the
Mac app and terminal UI consume these fields, so a new exception does not
require client-specific provider code.

Automatic MCP configuration is separate from authorization. Record a domain
as automatically registered only when the provider was actually configured
for that launch; a saved Sessions, Browser, or Computer grant is not evidence
of injection. Users may still [register Unpeel MCP manually](/docs/agent-runtimes#manual-mcp-registration).

## Resume and transcripts

A safe Resume Agent recipe comes from the managed launch and a verified
provider identity—not the currently visible process name. Offer it only after
that managed runtime has exited or crashed and the shared Host proves its
original interactive login shell owns the foreground. Retained expected jobs,
stopped/background jobs, different recognized runtimes, and incomplete process
inspection must all fail closed; never turn Resume Agent into a way to
interrupt an active process. Preserve the user's semantic command and bind
actions to the current runtime generation. The shared
`runtime_launch_pending`/`runtimeLaunchPending` latch must cover the final PTY
submission so two Controllers cannot launch the recipe twice.

Transcript adapters return normalized conversation blocks and provider path
claims. The shared Host remains responsible for canonicalizing trusted roots,
rejecting traversal and symlink escapes, bounding searches and reads, and
serving the same transcript contract to desktop, terminal, MCP, and phone
clients.

## Build and test

The repository's [`runtimes/README.md`](https://github.com/unpeel-com/unpeel/blob/main/runtimes/README.md)
is the exact contributor checklist and schema reference. Run
`bun run generate:runtimes` followed by `bun run validate:runtimes`, then the
Rust and Swift suites, and add a real hosted-terminal test for any deep
integration.

At minimum, prove both sides of the boundary:

- a managed launch receives only its declared setup and actions; and
- the same executable typed into a blank Terminal may be recognized, but does
  not gain hooks, MCP injection, transcript ownership, or Resume Agent merely
  from detection;
- an active, stopped/background, renamed, different, or unverifiable job cannot
  receive the relaunch input; and
- two simultaneous Resume Agent requests produce one launch while the pending
  request is rejected.
