---
title: Treat synced and external data as hostile
author: Guilhermexp
date: 2026-10-18
standing: true
---

## Decided
Data from other devices, git, or PR text can be malformed or adversarial. Don't unwrap or slice it. Validate ids and keys, and fall back safely. Parse git output in machine-safe form (e.g. `-z`) so paths with odd characters can't inject entries.

## Watch out
Ids that reach routes or SQL must be restricted and parameterized.

_Flagged in review on 4 PRs here (#903, #909, #890, #777), most recently on Oct 17._
