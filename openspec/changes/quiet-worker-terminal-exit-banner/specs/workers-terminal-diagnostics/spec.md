## ADDED Requirements

### Requirement: A finished session is not a disconnect

The Worker terminal SHALL treat a `session has exited` failure as the expected
end of the session and SHALL NOT render it as a disconnect banner, while every
other failure SHALL still reach the banner.

#### Scenario: The exit status stays off the grid

Test: `a_finished_worker_is_not_reported_as_a_disconnect`

- **WHEN** a request against the session fails with `409: session has exited`
- **THEN** no disconnect banner renders over the terminal grid
- **AND** the surface keeps reporting the ended session in its footer

#### Scenario: Real failures still surface

Test: `a_finished_worker_is_not_reported_as_a_disconnect`

- **WHEN** a request fails with any other status or a connection error
- **THEN** the disconnect banner renders with that failure
- **AND** both the poll error and the resize error take the same path
