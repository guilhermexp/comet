## Why

The fork's last ancestry sync was upstream `v0.2.29` (`b3fa5187`, 2026-08-30); later upstream work (through `v0.2.72`) was only hand-ported by concept. Upstream (`zeronsh/zeron`, formerly `zeronsh/comet`) is now at `v0.2.83` (`d721f301`, 2026-09-22), ~630 commits ahead. The user asked to bring everything in while keeping every private fork change ("traz tudo mantendo nossas modificações privadas").

## What Changes

- Merge `refs/upstream/zeron-main@d721f301` into the fork on `sync/upstream-v0.2.83`, using the `v0.2.29` sync (`87123b50`) as the explicit merge base. Upstream re-signed its history (GPG only, trees identical), so a temporary `git replace --graft b3fa5187 87123b50` restored the real base for the merge and was removed right after.
- Take upstream additions: explicit agent installers, Antigravity ACP adapter, OpenCode 2.x agent selection/versioned replies, per-credential model catalogs, harness hardening, rich composer references/skills, shared message queue panel, sidebar sections/pins, Files explorer/editor, image zoom, staged review comment editing, per-file diff horizontal scroll, Project Actions, window geometry, About panel, sound signature, Linux CSD/browser, `zeron-mcp`, new CI test workflows, iOS outbox/residency/projectless work.
- Keep fork contracts where they conflict: Run/Steer steering (upstream "always queue" #284 rejected; the queue coexists with steering), Workers/OMP/Live Voice, trajectory, accounts/usage, fork visual design (monocode, frost blur, Material icons, tool presentation), fork Files/Details structure, right-pane terminal, fork release workflow and version `0.2.18`.
- Rename the syntax crate `comet-syntax` → `zeron-syntax` to match upstream and remove a recurring conflict.
- Drop upstream's scheduled `cursor-sdk-update` workflow (opens PRs daily) and keep the fork `release.yml`.

## Capabilities

### New Capabilities

- `upstream-v0-2-83-integration`: upstream v0.2.83 behavior available in the fork without regressing fork-owned contracts.

### Modified Capabilities

None. Existing OMP, Workers, Live Voice, steering, trajectory, theme, Files and release contracts remain binding.

## Impact

Whole Rust workspace (`proto`, `doc`, `sync`, `rpc`, `harness`, `engine`, `ui`, `preview`, `syntax`, new `mcp`), `apps/zeron`, `apps/ios`, `edge`, CI workflows, docs. No push, tag, deploy, release, TestFlight upload or promotion to `main` is authorized by this change.
