## 1. Shared preparation

- [x] 1.1 Share compiled grammar configurations and Markdown blocks; verify concurrent highlighting and immutable snapshot regression tests.
- [x] 1.2 Borrow transcript entries, skip unrelated derivation and bound render preparation caches; verify invalidation and visible-state tests.

## 2. Input and interactions

- [x] 2.1 Cache composer shaping and notify only on final geometry; verify cache reuse and text/IME/style/width invalidation tests.
- [x] 2.2 Wrap staged/sent attachments, recover mounted focus and suspend following on selection start; verify focus/selection tests and native narrow-column behavior.

## 3. Acceptance

- [x] 3.1 Compare native baseline/candidate interaction behavior for streaming, sticky header, selection, blocks and HTML/Markdown previews; record obtained measurements and limitations.
- [x] 3.2 Run formatting, focused checks, workspace suite and build; update owning DOX, validate and archive the change.

Evidence and native automation limits: see `validation.md`.
