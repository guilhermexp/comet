## Why
A native stress probe of a 1,000-section Markdown report spends about 22ms per repeated frame (p95 23.24ms). FilePreview renders every block and repeatedly flattens text without the existing RenderCache. HTML uses an independent WebKit host and needs separate lifecycle measurements.
## What Changes
Virtualize Markdown preview blocks and reuse the existing text cache; preserve document content, wrapping, per-file scroll isolation and fresh loading. Audit nearby chat/panel repaint and native preview lifecycle paths, changing only demonstrated bottlenecks.
## Impact
Native report previews and reproducible performance validation. No protocol or dependency changes.
