- [x] 1. Reproduce redundant generic prefixes in a presentation regression.
- [x] 2. Remove generic verbs, preserve names and lifecycle, and verify tests/build.
- [x] 3. Update owner contract and validate/archive.

Evidence: regression reproduced Running tool + ask (`/tmp/comet-tool-name-red.log`); 1,205 UI tests passed (`/tmp/comet-tool-name-ui.log`), build passed (`/tmp/comet-tool-name-build.log`), fmt/diff checks passed. The existing header renderer, icon, payload and lifecycle paths are retained; no native screenshot claim for this copy-only change.
