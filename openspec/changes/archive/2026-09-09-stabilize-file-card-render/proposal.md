## Why
Collapsed file cards reuse fetched full input and construct every line after the first expansion. Their measurement callback also schedules a frame on every paint and invalidates all sticky geometry whenever measured content changes, causing unnecessary render work and sticky suppression.
## What Changes
Use only the bounded durable preview while collapsed, retaining full input for reopening. Measure after child prepaint without a recurring next-frame callback. Invalidate only geometry after the changed card, retaining the current turn header.
## Impact
Native file card rendering and pure preview selection only. Existing streaming, wrapping and error behavior remain.
