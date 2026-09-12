# Report preview performance — 2026-09-09

## Findings and change

F1. `FilePreview` rebuilt the entire Markdown document on every repaint and did
not attach the existing Markdown `RenderCache`. A 1,000-section report exceeded
a 16.7 ms p95 frame budget in the native probe. The preview now uses GPUI's
variable-height `ListState` for visible top-level blocks plus 200 px overscan,
and reuses prepared text. Parsing and file reads remain on the background
executor. No dependencies or wire formats changed.

F2. HTML follows a separate WebKit path. Its steady repaint probe was already
within budget. Native host creation measured 37.57 ms initially and 3.89 ms on
the following creation; these measurements exclude asynchronous page completion.
The multi-second delay was not reproduced in this HTML fixture, so the WebKit
implementation was preserved.

F3. The wider review covered transcript notifications/layout, file-card
virtualization, shell/panel width transitions, the Files sidebar and preview
loading/lifecycle. An idle native sample spent 1,396 of 1,430 main-thread samples
waiting in `mach_msg2_trap` (97.6%): no sustained idle repaint loop was observed.
A separate five-second sample during mock streaming with a report open showed
native layout and sidebar/transcript work, without establishing another isolated
multi-second blocker. This is sampling evidence, not per-click latency.

## Controlled measurements

Debug build, macOS, native 800 × 900 GPUI window, same report and machine.
The probe requests repeated repaints, ignores the first 20 frames, then measures
120 frame intervals including display scheduling and UI work.

| Case | Median | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Markdown before (red budget gate) | 22.27 ms | 23.33 ms | 31.98 ms |
| Markdown after, first run | 8.33 ms | 9.75 ms | 22.23 ms |
| Markdown after, final source | 8.33 ms | 9.22 ms | 21.48 ms |
| HTML before, already loaded | 8.33 ms | 9.08 ms | 23.17 ms |

The final Markdown p95 improved about 60%; occasional longer frames still exist.
The opt-in 16.7 ms gate failed before and passed after. This does not measure
cold-open latency or prove a constant frame rate on every device.

## Reproduce

Generate the Markdown fixture:

```sh
python3 - <<'PY'
from pathlib import Path
p = Path('/tmp/comet-preview-perf')
p.mkdir(exist_ok=True)
intro = 'Relatório **de desempenho** com acentuação, links e resultados. '
body = 'Texto de relatório que precisa quebrar corretamente na coluna. ' * 8
(p / 'report.md').write_text(''.join(
    f'## Seção {i}\n\n{intro}{body}\n\n' for i in range(1000)
))
(p / 'report.html').write_text(
    '<!doctype html><meta charset="utf-8">'
    '<style>body{font:16px sans-serif;padding:24px}'
    'article{padding:16px;border-bottom:1px solid #ddd}</style>'
    '<h1>Relatório HTML</h1>' + ''.join(
        f'<article><h2>Seção {i}</h2><p>Resultado com acentuação e tabela.</p></article>'
        for i in range(1000)
    )
)
PY
PREVIEW_FRAME_BUDGET_MS=16.7 cargo run -p zeron-ui --example preview_probe -- /tmp/comet-preview-perf/report.md
cargo run -p zeron-ui --example preview_probe -- /tmp/comet-preview-perf/report.html
```

The budget is optional and machine dependent. The probe opens an isolated
native window and exits; it does not connect to a daemon or mutate Chat data.

## Native acceptance

Used an isolated local mock Chat and actual production file-link routing.
Temporary mock fixture text was removed from source after seeding the Chat.

| Flow | Observed result |
| --- | --- |
| Inline Markdown link → side preview | Report opened and wrapped in the narrow column |
| Inline HTML link → side preview | Native HTML document appeared |
| Files tree → both report types | Both opened |
| Markdown scroll → HTML tab → Markdown tab | Section 2 and Section 3 returned at the same vertical coordinates |
| Window resized from 1320 to 1500 logical pixels | Markdown reflowed; sections remained readable |
| Change section heading on disk, switch away/back | `Atualizado QA 2` appeared at the retained viewport; no stale prepared text |
| Hide HTML pane → focus composer → type | `QA preview focus` appeared; test draft then cleared without sending |
| WebKit first responder integration | Hide, descendant, repeated hide, Drop and unrelated responder passed |

Session evidence is local and temporary: `/tmp/comet-preview-inline-markdown.png`,
`/tmp/comet-preview-inline-html.png`, `/tmp/comet-preview-scrolled-before.png`,
`/tmp/comet-preview-scrolled-restored.png`, `/tmp/comet-preview-resized.png`,
`/tmp/comet-preview-refreshed.png`, `/tmp/comet-preview-composer-focus.png`.
Measurements: `/tmp/comet-preview-red.log`, `/tmp/comet-preview-green.log`,
`/tmp/comet-preview-final-probe.log`, `/tmp/comet-preview-focus.log`.

## Gates and limits

- `cargo test -p zeron-ui`: 1,215 passed, zero failed.
- Final focused rerun `cargo test -p zeron-ui file_preview`: 24 passed.
- `ZERON_NATIVE_PREVIEW_FOCUS_TEST=1 cargo test -p zeron-ui --test native_preview_focus`: passed.
- `cargo build`: passed after restoring the temporary mock fixture.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- DOX updated in `crates/ui/AGENTS.md`; parent ownership/index unchanged.

Validation was on macOS in a debug build, using local reports and a mock
stream. It does not establish compatibility across all operating systems,
remote files, all HTML content or every frontend interaction. Virtualization is
at the top-level Markdown block: a single enormous table/list/code block still
has proportional rendering cost. Closing tabs releases their Markdown viewport;
reload clears prepared text and heights while preserving a bounded logical
position. Existing native HTML sandboxing and document lifecycle are unchanged.
