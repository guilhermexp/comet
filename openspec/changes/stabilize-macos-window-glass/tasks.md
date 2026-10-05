# Tasks

## 1. Native backing behavior

- [ ] 1.1 Prepare a separate zui source checkout with the exact tracked snapshot and add a native regression that fails when declared-radius backing is absent; record RED output.
- [ ] 1.2 Retain independent reference-configured backing, reuse it on apply/radius change, and clean it up on material/opaque transitions; pass native regression and existing window blur selector tests.
- [ ] 1.3 Update zui source documentation and provenance, copy the reviewed snapshot delta, and verify source/snapshot identity plus both line-wrap closing-punctuation rules.

## 2. Downstream integration

- [ ] 2.1 Build the downstream desktop app and perform a focused headed glass comparison without changing user settings; record separately any unproven visual outcome.
- [ ] 2.2 Complete independent review and affected DOX/design documentation; pass formatting, diff checks and strict OpenSpec validation, then archive when required work is complete.
