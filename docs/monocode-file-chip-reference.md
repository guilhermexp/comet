# MonoCode file chip reference

Source: https://github.com/hardbeat920/monocode/tree/e7d5623480b217fa443f57572f3a6719cc139a88 (inspected 2026-09-13).

- `src/surfaces/AgentMarkdown.tsx::MarkdownCode`: real inline-flex code box, content at 8% for the fill, 6px horizontal padding, 6px radius, minimum 24px height, 0.8em mono text, 4px icon gap and 14px Material icon. File chips hover sky-300 with underline. Plain Markdown links remain links.
- `src/surfaces/AgentTranscript.tsx::ToolCallSummary`: action in sans 14px at 50% content; target mono 13px at 70%; chip fill 6%, hover 10%, 4px padding and radius, 4px icon gap, 16px Material icon. The target retains its displayed path. The file button stops propagation to tool disclosure.
- `src/chrome/FileTypeIcon.tsx`: Material Icon Theme mapping by filename and compound extension; generic file fallback.
- `src/lib/paths.ts`: file navigation resolves against cwd; absolute paths retain their identity. Comet reuses its existing device-scoped OpenFile routing.

Comet adapts these presentation rules to native GPUI in `markdown/inline_chips.rs` and the transcript ReadFile header. It keeps its own existing typography for surrounding prose, event grouping, preview loader and selection registry. Inline boxes use source fragments; copied text contains no icon placeholders or synthetic padding.

## Attribution

Presentation and filename recognition adapted from MonoCode. Its license follows:

MIT License

Copyright (c) 2026 Nick

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
