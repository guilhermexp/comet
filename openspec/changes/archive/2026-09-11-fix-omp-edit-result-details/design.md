## Context

See proposal.md. OMP keeps complete edit snapshots inside result details. The existing document fold already replaces the call path and derives a bounded preview from ToolDiff; the journal-backed input RPC already prefers authoritative snapshots when expanding.

## Decisions

Read the vendor envelope in the OMP normalizer and reuse that existing pipeline, rather than adding OMP-specific parsing to the UI. Accept a single per-file result because it fits ToolDiff's single-file contract; do not collapse a batch into its first file. Keep the legacy top-level representation compatible.

## Risks / Trade-offs

Pruned snapshots and multi-file batches cannot be represented as a complete single-file ToolDiff; preserve the existing no-diff behavior rather than fabricating content. Previously persisted records lack this metadata and are not migrated. Rollback is the normalizer change only; no schema migration is involved.
