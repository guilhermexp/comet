## 1. Regression coverage
- [x] 1.1 Add and observe failing focused tests for D1, D2 and D3.
## 2. Implementation
- [x] 2.1 Correct canvas and serialized geometry.
- [x] 2.2 Preserve stopped alternate-screen output without affecting live terminals.
- [x] 2.3 Correct shell divider coordinates and visible-width constraints; reproduce three failures before the fix and pass all 76 shell tests afterward.
## 3. Verification
- [x] 3.1 Run focused tests and build the current checkout.
- [x] 3.2 Verify native shared canvas and recovered stopped Worker; update owning DOX.
- [x] 3.3 Native divider validation: user confirmed dragging is now correct in CometDividerQA.
- [x] 3.4 Reproduce and fix delayed local grid resize, stopped alternate-screen crop, and narrow historical replay with autowrap disabled.
- [x] 3.5 Validate narrow → wide → narrow in the updated native build; original columns remain intact.
- [ ] 3.6 Resolve requested presentation choice for explicit TUI line breaks/boxes versus semantic document reflow, then archive. Original formatting still leaves unused width after a stopped terminal grows beyond the printed line lengths.
