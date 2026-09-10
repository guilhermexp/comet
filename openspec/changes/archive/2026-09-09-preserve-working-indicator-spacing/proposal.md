# Preserve transcript spacing when docking activity

## Why
Only the indicator should move down; removing its old footprint also shifts the last content.

## What Changes
- Retain the previous in-flow line and padding as a blank reservation, sharing the existing visibility rules.
- Keep the visible indicator immediately above the composer; do not mount a second animated spinner in the reservation.

## Impact
Native transcript layout only.
