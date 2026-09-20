# Fix native identity for macOS development launches

## Why
On this Mac, cmux 0.64.25 identifies the bare `cargo run` GUI process as cmux and terminates it through its duplicate-instance observer. A temporary signal diagnostic confirmed SIGTERM sender PID 1159 was cmux. Removing __CFBundleIdentifier did not prevent the termination. Upstream uses the same GPUI lifecycle and does not supply a development bundle. Blocking SIGTERM hides the symptom and breaks normal shutdown.

## What Changes
- A macOS Cargo runner places the freshly built headed executable in a local Zeron.app with the existing distribution identity and icon, then execs that executable.
- Keep plain cargo run, the selected profile/target directory, arguments, environment, cwd, stdio, exit status, and normal signals.
- Pass CLI commands and other Cargo targets through directly. No Rust lifecycle changes, cmux modifications, or OMP overrides.

## Impact
Only .cargo/config.toml, development scripts, targeted integration tests, and documentation. Generated bundles live beside the Cargo executable, outside source. No release, commit, push, or deployment.
