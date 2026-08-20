# Compact empty composer implementation plan

1. Add a focused unit test for the rendered layout decision.
2. Confirm the test fails before the decision helper exists.
3. Route rendering through a helper that follows the measured compact/expanded mode instead of forcing new chats open.
4. Update stale comments that describe new chats as always expanded.
5. Run the focused test, UI detector, workspace checks, build, review, and native visual validation.

No commit is included in this change.
