# Why

OMP startup waits for protocol negotiation before the consumer can drain events. A burst larger than the bounded event channel can block stdout parsing before the negotiation response, causing a false timeout. Current installed OMP starts promptly; this change addresses a separately identified deterministic deadlock, without claiming to explain the historical zero-stdout handshake timeouts.

# What Changes

- Keep startup event delivery from blocking protocol response parsing throughout negotiation.
- Preserve event ordering, transport limits and cancellation behavior.
- Add a subprocess regression that emits more startup events than the live channel capacity.

# Capabilities

## New Capabilities

- `omp-startup`: startup protocol completion under extension event bursts.

## Modified Capabilities

None.

# Impact

OMP subprocess transport and harness fixtures. No extension disabling, arbitrary timeout increase, or wire changes.
