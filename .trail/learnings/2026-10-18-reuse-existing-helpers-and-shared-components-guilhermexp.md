---
title: Reuse existing helpers and shared components
author: Guilhermexp
date: 2026-10-18
standing: true
---

## Decided
Before writing new UI or navigation logic, find the existing helper and use it. Examples are `harness_brand_icon`, `navigate_back`/`navigate_forward`, the `tab_chip` in `surface_chrome`, and the existing mutations. This keeps behavior identical across surfaces.

_Flagged in review on 3 PRs here (#861, #725, #872), most recently on Oct 14._
