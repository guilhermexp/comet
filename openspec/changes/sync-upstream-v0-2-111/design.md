## Integration decisions

Use a real merge in an isolated worktree, after merging PR #21 and formatting the fork. Freeze upstream at the PR #20 head; do not replace it during this attempt. Preserve fork version/identity and path gpui dependencies. Resolve each conflict semantically against ancestor/fork/upstream, rather than choosing a side globally.

Source control retains authorized Worker checkout roots, bounded Git process execution and fork commit-message generation. New search and PR RPCs belong in the existing method registry, including forwarding/deadlines. The upstream session-information toggle coexists with the fork Details sidebar; it does not erase Device, Workers, usage or worked-project totals. Deleted legacy Details Files and Live Voice OMP surfaces remain deleted.

## Verification plan

1. Inspect PR #21's existing Linux proof and CI failures. The current CI logs identify the documented Linux file-link menu test failure, while 2263 UI tests passed; this does not prove the merged revision.
2. After conflict resolution, run formatting, manifest/lock reconciliation and API compilation. Use one local cargo-verify batch for focused tests of engine search/source control, RPC registry, sidebar/PR UI and worked-project subscription lifecycle. Broad regression stays in CI when available.
3. Build the macOS release using `scripts/package-macos.sh`, inspect the installed bundle/executable and archive, and confirm the script does not launch it. Do not exercise provider writes or real PR mutations as a smoke test.
4. Inspect native isolated UI using the existing demo/fixture recipe for Details totals, navigation/search and panels where practical. Record unavailable live paths explicitly.
5. Verify both requested heads are ancestors of delivered main, no unresolved paths, clean checkout, exact remote publication when authorized, and archive the integration OpenSpec only with recorded acceptance/limitations.
