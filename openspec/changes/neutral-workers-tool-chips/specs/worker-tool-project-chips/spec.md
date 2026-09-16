## ADDED Requirements

### Requirement: Workers chips use the neutral chip fill
The `@project` chip and the Worker/preset identity chip on a Workers tool row
SHALL use the same neutral fill as the transcript's file-path chip, derived from
the text color and not from the user's accent. Changing the accent SHALL NOT
change them. Their text tone, runtime icon, truncation, maximum width and
geometry SHALL be unchanged, and a failed row SHALL keep its danger text tone.

#### Scenario: A colored accent is selected
Test: unit — chip fill against the accent across accent presets.
- **WHEN** a Workers row with a resolved project or identity renders under a colored accent
- **THEN** its chips use the neutral fill rather than an accent wash
- **AND** they match the file-path chip rendered on neighbouring rows

#### Scenario: The call failed
Test: none — native GPUI visual acceptance.
- **WHEN** the Workers call failed
- **THEN** its chip keeps the neutral fill and the danger text tone
