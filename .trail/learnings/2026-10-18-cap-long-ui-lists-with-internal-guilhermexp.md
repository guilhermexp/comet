---
title: Cap long UI lists with internal scroll
author: Guilhermexp
date: 2026-10-18
standing: true
---

## Decided
Long lists and panels in the UI (skills, servers, settings pages) keep a small fixed height and scroll inside themselves. They must not grow to full size.
The user wants only relevant items: skills of the current project, not global ones. Servers should be only dev servers the chat's agent started, not every open port.

## Watch out
When asked to fix scroll lag, don't change the component's size. The user said: "the size was right, only the scroll was bad."

_Told to Claude in 3 past sessions here, most recently on Oct 15._
