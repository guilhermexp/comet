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

What the filter actually removes is narrower than "tool inputs": file contents on a
write, the before/after strings of an edit, a web fetch's prompt, and the free-form
input of MCP and unrecognized tools. A shell command, a read path, a search pattern and
a query all survive. So an export can say precisely which command ran and which file was
edited — it just cannot show what was written into that file.

The cost is accepted and permanent. Anyone reaching for the journal to "make the export
complete" is undoing this decision, not fixing an oversight.
