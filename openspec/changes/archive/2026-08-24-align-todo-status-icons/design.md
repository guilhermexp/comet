# Design: To-dos status alignment

The existing 15 px circle becomes a `flex_none` two-axis-centered container for
the existing 9 px check or arrow. Row height, padding, gap, colors, and widget
clipping remain unchanged. A pure layout descriptor pins the geometry without
introducing a render harness.
