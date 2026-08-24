# Design: Workers subagent widget polish

The existing activity expansion map remains the sole local authority, but new
ids are inserted as collapsed rather than auto-opening the first id. The
existing status renderer is mounted after the subagent title so avatars remain
stable and the lifecycle indicator occupies a trailing fixed slot.

The Details To-dos view retains its data projection and adopts the inline
renderer's row height, horizontal padding and gap through the existing pure
layout descriptor. No shared renderer extraction is needed for this surgical
correction.
