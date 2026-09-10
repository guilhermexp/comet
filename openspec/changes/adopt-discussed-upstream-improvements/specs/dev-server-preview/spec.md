## Purpose

Expose running development servers through native browser tabs with stable URLs and authenticated cross-device access.

## ADDED Requirements

### Requirement: Local discovery and native browsing
The app SHALL discover running project development servers and open their stable preview URLs in native browser tabs with navigation, resize and live reload support.

#### Scenario: Open a development server
Test: unit + integration — discovery and proxy; none — native BCU browser.
- **WHEN** a project starts an HTTP development server
- **THEN** the preview becomes discoverable and opens in the browser with working navigation and HMR

### Requirement: Peer preview access
A signed-in peer SHALL access an authorized device preview through the preview pairing transport. Pairing failures, unavailable hosts and unsupported peers SHALL be visible; a failed connection MUST NOT imply successful page loading.

#### Scenario: Open preview hosted by another device
Test: integration — pairing, authorization and transport; none — live peer validation.
- **WHEN** an authorized peer selects a preview belonging to an online host
- **THEN** the native browser connects to that preview and relays HTTP and live-update traffic within the authenticated pairing
