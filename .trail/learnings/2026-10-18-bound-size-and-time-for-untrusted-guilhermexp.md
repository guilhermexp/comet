---
title: Bound size and time for untrusted content
author: Guilhermexp
date: 2026-10-18
standing: true
---

## Decided
Content rendered or loaded from messages, PR text or RPC input needs hard limits on size, time and concurrency. Use the app's bounded decoder for images and clamp counts. Untrusted content must not freeze the app or exhaust memory.

_Flagged in review on 3 PRs here (#912, #895, #484), most recently on Oct 18._
