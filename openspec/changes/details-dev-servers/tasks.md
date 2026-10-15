## 1. Implementation

- [x] 1.1 `zeron-preview::servers::{list, stop}` over the existing listener scan.
- [x] 1.2 Proto `DevServer`, `ListDevServersParams`, `StopDevServerParams`, `StopDevServerReply`; RPCs `ListDevServers`, `StopDevServer` (forwardable).
- [x] 1.3 Engine handlers on `spawn_blocking`.
- [x] 1.4 Details Servers section with hover Stop and 5 s poll.

## 2. Tests

- [x] 2.1 `servers::tests` (roots, dedupe, exclusions, boundaries, refusal).
- [x] 2.2 `dev_servers::tests::labels_drop_paths_and_flags`.
- [ ] 2.3 Manual: a background `next dev` shows in Details and Stop ends it.

## 3. Docs

- [x] 3.1 `crates/preview/AGENTS.md`, `crates/ui/AGENTS.md`.
