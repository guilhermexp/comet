---
title: Keep heavy work out of per-frame render paths
author: Guilhermexp
date: 2026-10-18
standing: true
---

## Decided
Render functions in zeron-ui run on every notify or frame. Don't re-derive whole lists, orders or pairwise comparisons there. Compute once at the owner (e.g. reconcile in the sidebar pane, not again in the shell) and keep per-row work cheap.

## Watch out
Quadratic checks over a selection stall on large ranges. Streams or timers should stop while their UI is hidden.

_Flagged in review on 3 PRs here (#895, #903, #774), most recently on Oct 17._
