## Why
The reference ThinkingTool uses a fixed Thought label instead of exposing reasoning in its header. Tool action and detail currently share a tone.

## What Changes
Use Thinking while active and Thought when completed. Preserve the full body behind its disclosure, including short text. Use stronger action and quieter detail tokens in common tool and file headers; retain semantic failure colors.

Replace the thinking sparkle with the native reference SpiralLoader at 24px; animate only active reasoning, honor reduced motion and avoid a duplicate spinner.

## Capabilities
### Modified Capabilities
- `turn-step-tool-groups`: reasoning labels and tool token hierarchy.

## Impact
Native UI only; no events, provider state or durable content changes.
