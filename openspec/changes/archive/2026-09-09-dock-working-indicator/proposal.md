# Dock working indicator above composer

## Why
The working indicator remains too far above the input when attached to transcript content.

## What Changes
- Place the main Chat working indicator in the existing reserved status strip immediately above the composer.
- Reuse the existing presenter for timing, sending, queued and retry states; do not duplicate the indicator in transcript rows.
- Subagent transcript panes retain their local indicator.

## Impact
Native shell/transcript presentation only.
