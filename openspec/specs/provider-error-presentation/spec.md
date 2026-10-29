# provider-error-presentation Specification

## Purpose
Make provider failures understandable in desktop Chats while preserving diagnostics and offering recovery actions only when appropriate.

## Requirements

### Requirement: Readable provider limits
The desktop SHALL present recognized transport quota/rate-limit errors as short English messages consistently in the transcript and composer surfaces, without raw JSON or request identifiers by default. Existing readable provider reset explanations SHALL remain intact. Original diagnostics SHALL remain in existing stored errors/logs; unrecognized failures SHALL remain diagnostic.

#### Scenario: Anthropic error through OMP
- **WHEN** OMP reports an Anthropic 429 rate_limit_error with request metadata
- **THEN** the transcript and composer show a short provider-limit explanation without duplicating raw JSON
- **Test:** unit

#### Scenario: Unknown error
- **WHEN** a failure has no recognized provider-limit signal
- **THEN** its diagnostic text remains visible
- **Test:** unit

### Requirement: Honest retry timing
The desktop SHALL show a valid provider-reported retry interval in human units, rounded up, as a reported delay rather than a live countdown or promised reset. Missing, malformed, zero, negative or overflowing intervals SHALL NOT produce an invented deadline.

#### Scenario: Reported milliseconds
- **WHEN** a limit error includes retry-after-ms=8564000
- **THEN** the message reports a delay of 2h 23m
- **Test:** unit

#### Scenario: Unknown delay
- **WHEN** the limit error has no valid retry interval
- **THEN** the message asks the user to try again later without a duration
- **Test:** unit

### Requirement: Authentication recovery stays distinct
The desktop transcript SHALL offer Log in only for authentication failure, never quota/rate limits. Presentation SHALL NOT initiate retries, change accounts or select another model.

#### Scenario: Quota with a valid login
- **WHEN** the transcript displays a provider limit error
- **THEN** it has no Log in action and does not start recovery automatically
- **Test:** unit

#### Scenario: Genuine signed-out failure
- **WHEN** the transcript displays a signed-out error for a Chat with a login handler
- **THEN** Log in remains available and invokes that handler only on user click
- **Test:** unit
