# Spec Delta

## REMOVED Requirements

### Requirement: Native OMP title ownership
**Reason**: OMP 18.10 made `/rename` generation asynchronous; the native path silently degraded every OMP Chat title to the prompt fallback, and the owner chose upstream's isolated title generation over tracking the CLI's async contract.
**Migration**: OMP Chats use the isolated title harness (Settings → titles, else Claude Code or Codex), as every other runtime does.
