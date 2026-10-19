---
title: Check upstream first; keep our fork changes
author: Guilhermexp
date: 2026-10-18
standing: true
---

## Decided
When something looks wrong in the fork, compare with upstream before changing anything. The user often suspects the fork introduced the bug.
If it's our bug, match upstream with a small, surgical fix. Keep the private fork modifications when syncing.
For the stopped-worker terminal, the user said to copy upstream and stop piling on tweaks.

## Watch out
The user doesn't want to open new changes or add new mechanisms when a revert or alignment is enough.

_Told to Claude in 3 past sessions here, most recently on Oct 15._
