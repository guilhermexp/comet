---
status: accepted
---

# Refresh Kimi OAuth credentials in Comet

Comet reads Kimi Code's permission-restricted local OAuth credential, refreshes it through the official Kimi OAuth endpoint under a cross-process lock, and persists rotations atomically. Access and refresh tokens never enter logs, RPC snapshots, UI state, or Loro/edge data. Read-only token access was rejected because an expired token would hide Usage until Kimi CLI ran; parsing a hidden `/usage` TUI was rejected as version-fragile.
