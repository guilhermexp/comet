---
status: accepted
---

# Chat Transcript Export reads the Chat Transcript, never the Run Journal

A Chat Transcript Export carries exactly what the Chat Transcript carries: the same
privacy filter, the same bounded tool output. The Run Journal was rejected as a source
even though it is the complete record — it deliberately keeps the raw tool inputs that
the transcript strips before anything is displayed or synced, so exporting it would hand
out, in a file meant to be pasted elsewhere, the very payloads the transcript exists to
withhold. Resolving the sidecar blobs behind large outputs was rejected too: it costs a
fetch per tool chip and still cannot recover the stripped inputs, so it buys size and
latency without buying completeness.

The cost is accepted and permanent: an export can say a command ran without saying what
the command was. Anyone reaching for the journal to "make the export complete" is
undoing this decision, not fixing an oversight.
