---
title: Keep settings and wire changes backward compatible
author: Guilhermexp
date: 2026-10-18
standing: true
---

## Decided
New settings fields get `#[serde(default)]` so old settings files keep working. Wire and schema changes are additive, and older hosts get a fallback (e.g. handle UnknownMethod). Old clients and old edge must work in either deploy order.

## Watch out
Adding a field to a struct means updating every test fixture that builds it.

_Flagged in review on 4 PRs here (#901, #530, #647, #777), most recently on Oct 14._
