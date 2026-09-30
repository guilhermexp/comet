# Tasks

## 1. Lifecycle recovery

- [x] 1.1 Add deterministic missed-Stop and ordering regressions; capture the behavioral RED before changing activity behavior.
- [x] 1.2 Recover newer disk events once while preserving generation/output guards; prove GREEN in shared activity and Comet derivation tests and document the poll contract.

## 2. Listener preservation

- [x] 2.1 Add seventeen-listener regressions for both Rust registry writers; record RED before removing age truncation.
- [x] 2.2 Preserve entries in Rust and Swift registry writers; prove register/unregister isolation and update vendor provenance and DOX.

## 3. Integrated validation

- [ ] 3.1 Run affected downstream tests, formatting and OpenSpec validation; independent review has no unresolved findings.
- [ ] 3.2 Build the app and exercise the spinner transition in an isolated profile; preserve existing app/Worker sessions and report any destination reload still required.

Implementation evidence: `.tmp/verify/spinner-stop-20260930/report.md`. All 289 unit tests of the actual adapter library passed after compiling current source with compatible cached dependencies; the focused subset passed 48/48. Canonical activity tests passed independently 17/17; both registry tests passed; extracted production derivation plus indicator mapping passed 18/18. Formatting, strict OpenSpec and all three Test stamps passed; review has no unresolved finding in the documented scope.

Integrated validation remains pending: after waiting for the unrelated global Cargo lock, the mandatory wrapper refused at 57.5 GiB free (60 GiB minimum). Cargo integration suite, full app build and headed/native observation were not run. No installed binary was replaced and no existing app/Worker process was restarted. The change stays open until those checks can be completed; code integration on main does not imply runtime publication.
