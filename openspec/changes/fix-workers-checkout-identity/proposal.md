## Why

Workers fail before spawning with `unknown project id` even though their projects appear in `list_projects`. The two affected Comet checkouts have persisted identity conflicts because the Git common directory's Unix device number changed while its canonical path and inode remained unchanged. The executable catalog filters the conflict without explaining it.

## What Changes

- Use a macOS directory identity that remains stable when the device number changes, while detecting replacement of the Git common directory.
- Preserve conservative handling of ambiguous legacy identities and provide explicit, compare-and-swap revalidation for a selected repository and its verified checkouts.
- Report a known checkout's launch blocker consistently through the Workers MCP and backend; keep unknown IDs distinct.
- Cover identity migration, real replacement, recovery, and launch behavior with isolated regressions.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workers-repository-identity`: stable local identity, explicit recovery of obsolete fingerprints, and actionable launch failures.

## Impact

`crates/workers-unpeel` identity, launch validation and controller MCP; the vendored Unpeel create catalog and its provenance; existing Workers tests and documentation. State edits preserve IDs, sessions, presets and unrelated fields. iOS dictation and its worktree are outside the code scope.
