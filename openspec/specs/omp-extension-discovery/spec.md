# omp-extension-discovery Specification

## Purpose
OMP Sessions launched by Comet discover user and project extensions by default, so installed extension commands are available without an explicit `--extension` flag.

## Requirements

### Requirement: Session launch uses default extension discovery

The system SHALL start each OMP Session without `--no-extensions`, so the installed OMP performs its normal user and project extension discovery. The Session launch SHALL still include `--mode rpc-ui`, `--auto-approve`, and `--allow-home`. The system SHALL NOT add a discovery toggle, allowlist, or explicit `--extension` argument as a substitute.

#### Scenario: New Session omits the discovery-off flag
- **Test:** harness integration

- **WHEN** Comet starts an OMP Session
- **THEN** the child argv SHALL NOT contain `--no-extensions`
- **AND** the child argv SHALL contain `--mode rpc-ui`, `--auto-approve`, and `--allow-home`

#### Scenario: Installed extensions appear in the command catalog
- **Test:** harness integration

- **WHEN** a new OMP Session starts in a cwd that has installed extensions
- **THEN** `get_available_commands` SHALL include commands provided by those extensions

### Requirement: Skill overlay stays independent of extension discovery

The system SHALL keep the skill-scope overlay that disables the four user-level skill roots (`~/.claude`, `~/.agents`, `~/.codex`, `~/.pi`) and leaves project-level skill roots enabled. Enabling extension discovery SHALL NOT change that overlay, approval policy, RPC framing, host Workers bridge, cwd, or Session resume semantics.

#### Scenario: User skill roots remain off
- **Test:** harness integration

- **WHEN** an OMP Session starts after extension discovery is enabled
- **THEN** the launch still passes `--config` with every user-level skill root disabled
- **AND** project-level skill roots remain enabled
