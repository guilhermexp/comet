# Spec Delta

## ADDED Requirements

### Requirement: Voice helper discovery is independent of Chat CLI installation
Local host and desktop client voice SHALL prefer the Chat CLI's compatible native package, then active official standalone and app-server-daemon packages in the device's Codex home. Discovery SHALL validate identity, layout, platform and supported version, exclude plugin/vendor caches and inactive releases, and leave the Chat executable unchanged.

#### Scenario: npm CLI with an active native package
Test: unit — voice-media resolver fixtures and harness host adapter fixture.
- **WHEN** the Chat CLI is an npm launcher and a compatible active official package exists outside PATH
- **THEN** voice resolves that package's helper
- **AND** the Chat CLI selection is unchanged

#### Scenario: Invalid or missing native installation
Test: unit — voice-media package validation and absence fixtures.
- **WHEN** no compatible active package is available, or candidates have invalid identity, layout, target, version or redirected helper paths
- **THEN** voice reports the existing unavailable or unsupported rejection
- **AND** it neither installs a runtime nor searches plugin/vendor caches or inactive releases

### Requirement: Explicit voice runtime override remains authoritative
An explicit development voice runtime override SHALL take precedence over automatic discovery. Its existing manifest, target, build commit, protocol and digest checks SHALL remain mandatory; failure SHALL NOT fall back to another installed runtime. Discovery SHALL NOT weaken helper process isolation or voice authorization.

#### Scenario: Valid development runtime override
Test: unit — voice-media override precedence fixture.
- **WHEN** a valid development override and an installed native package both exist
- **THEN** voice uses the override

#### Scenario: Invalid development runtime override
Test: unit — voice-media invalid override and integrity fixtures.
- **WHEN** the explicit override is missing, malformed, incompatible or tampered
- **THEN** voice rejects it even if a compatible installed package exists
