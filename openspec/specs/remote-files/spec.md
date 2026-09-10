# remote-files Specification

## Purpose

Let users browse local and peer-owned checkout files efficiently using the existing native Files and preview surfaces.

## Requirements

### Requirement: Bounded directory browsing
Files SHALL list and paginate one requested directory on its owning device with bounded results, validated checkout-relative paths and explicit unsupported/offline/error states. Navigation MUST preserve expansion and scroll across refresh.

#### Scenario: Remote directory refresh
Test: unit + integration — directory RPC and reconciliation; none — BCU surface.
- **WHEN** a user expands a peer-owned directory and a filesystem update arrives
- **THEN** only affected directory data refreshes while expanded nodes and browsing position remain stable

### Requirement: Native file opening compatibility
Files SHALL open supported documents in the existing native preview. Existing absolute local file links outside the checkout SHALL continue to open from Chat. Older peers SHALL show an explicit compatibility state for unsupported Files requests.

#### Scenario: External local report
Test: integration — path resolution and RPC; none — BCU preview.
- **WHEN** a Chat link targets an absolute HTML or Markdown file in another directory
- **THEN** one click opens that file in the native preview without requiring it to belong to the Chat checkout
