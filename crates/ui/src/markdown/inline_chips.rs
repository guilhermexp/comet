//! Native inline boxes matching MonoCode's MarkdownCode geometry.
//! Source text stays intact; each chip participates in wrapping as a box.
//! MonoCode reference and MIT attribution: docs/monocode-file-chip-reference.md.

use std::ops::Range;

use gpui::{AnyElement, Context, Hsla, SharedString, Window, div, prelude::*, px};

use super::render::{FlatText, RenderOptions, flat_text_element};
use crate::theme::Theme;

pub(crate) fn file_hover_color(theme: &Theme) -> Hsla {
    gpui::rgb(if theme.appearance.is_dark() {
        0x7dd3fc
    } else {
        0x0369a1
    })
    .into()
}

pub(crate) struct FilePathTooltip(pub SharedString);

impl Render for FilePathTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx);
        div()
            .max_w(px(480.0))
            .px(px(9.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.surface_raised)
            .shadow_md()
            .text_size(px(12.0))
            .line_height(px(16.0))
            .text_color(theme.text)
            .child(self.0.clone())
    }
}

/// A bare web address written as code (`www.site.com.br`, `site.com/loja`).
///
/// The filename heuristic reads `.br` as an extension and resolves the host
/// against the project root, so clicking opened a missing local file. Only
/// unambiguous hosts qualify: a `www.` prefix, a generic label (`com`,
/// `net`, `org`, `gov`, `edu`) before a ccTLD (`x.com.br`), or a generic
/// TLD followed by a path (`site.com/loja`). `.md`, `.org` and `.com` are
/// file extensions too, so a bare `name.tld` never decides.
pub(super) fn web_host_url(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.contains(char::is_whitespace) || text.contains("://") {
        return None;
    }
    let (host, path) = text.split_once('/').unwrap_or((text, ""));
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2
        || labels.iter().any(|label| {
            label.is_empty() || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
    {
        return None;
    }
    let generic = |label: &&str| {
        ["com", "net", "org", "gov", "edu"]
            .iter()
            .any(|tld| label.eq_ignore_ascii_case(tld))
    };
    let (last, middle) = labels[1..].split_last()?;
    (labels[0].eq_ignore_ascii_case("www")
        || middle.iter().any(generic)
        || (generic(last) && !path.is_empty()))
    .then(|| format!("https://{text}"))
}

/// Follow the reference's filename heuristic without interpreting commands.
pub(super) fn file_target(text: &str) -> Option<&str> {
    let text = text.trim();
    if web_host_url(text).is_some() {
        return None;
    }
    if text.is_empty()
        || text.len() > 240
        || text.contains(char::is_whitespace)
        || text.contains([';', '`', '$', '=', '|', '?'])
        || text.contains("://")
    {
        return None;
    }
    let clean = crate::file_preview::model::strip_line_col(text);
    let name = clean.rsplit(['/', '\\']).find(|part| !part.is_empty())?;
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || "_%@+().-".contains(c))
    {
        return None;
    }
    if ["dockerfile", "makefile", "gemfile", "license"]
        .iter()
        .any(|known| name.eq_ignore_ascii_case(known))
    {
        return Some(text);
    }
    let (_, extension) = name.rsplit_once('.')?;
    let mut chars = extension.chars();
    (extension.len() <= 12
        && chars.next()?.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || "+-".contains(c)))
    .then_some(text)
}

/// Preserve run/link offsets while rendering a source fragment independently.
pub(super) fn fragment(flat: &FlatText, range: Range<usize>) -> FlatText {
    let mut offset = 0;
    let mut runs = Vec::new();
    for run in &flat.runs {
        let end = offset + run.len;
        let from = offset.max(range.start);
        let to = end.min(range.end);
        if from < to {
            let mut run = run.clone();
            run.len = to - from;
            run.background_color = None;
            runs.push(run);
        }
        offset = end;
    }
    let links = flat
        .links
        .iter()
        .filter_map(|(link, url)| {
            let start = link.start.max(range.start);
            let end = link.end.min(range.end);
            (start < end).then(|| ((start - range.start)..(end - range.start), url.clone()))
        })
        .collect();
    FlatText {
        original: None,
        text: flat.text[range].to_owned().into(),
        runs,
        links,
        chips: Vec::new(),
        hovered_chip: flat.hovered_chip.clone(),
    }
}

/// Prose wraps at word boundaries; code remains one inline box until it exceeds
/// the column. Offsets are source byte offsets, also stable during append.
/// A web link always starts a new segment so its favicon leads the link text.
pub(super) fn segments(flat: &FlatText) -> Vec<(Range<usize>, bool)> {
    let starts: Vec<usize> = web_links(flat).map(|(range, _)| range.start).collect();
    let mut out = Vec::new();
    let mut at = 0;
    for chip in &flat.chips {
        prose_segments(&flat.text, at..chip.start, &starts, &mut out);
        out.push((chip.clone(), true));
        at = chip.end;
    }
    prose_segments(&flat.text, at..flat.text.len(), &starts, &mut out);
    out
}

fn prose_segments(
    text: &str,
    range: Range<usize>,
    breaks: &[usize],
    out: &mut Vec<(Range<usize>, bool)>,
) {
    let mut at = range.start;
    for word in text[range].split_inclusive(char::is_whitespace) {
        let end = at + word.len();
        let start = at;
        for &split in breaks.iter().filter(|&&b| b > start && b < end) {
            out.push((at..split, false));
            at = split;
        }
        out.push((at..end, false));
        at = end;
    }
}

/// Link ranges whose destination is on the web (they get a favicon).
pub(super) fn web_links(flat: &FlatText) -> impl Iterator<Item = &(Range<usize>, String)> {
    flat.links
        .iter()
        .filter(|(_, url)| super::render::is_web_link(url))
}

/// Paragraphs with code chips or web links lay out as a wrapping flow of
/// inline boxes; everything else stays one shaped text element.
pub(super) fn needs_flow(flat: &FlatText) -> bool {
    !flat.chips.is_empty() || web_links(flat).next().is_some()
}

/// Width the favicon adds ahead of a link (icon + gap), for table measuring.
pub(super) const FAVICON_ADVANCE: f32 = 14.0 + 4.0;

/// The favicon box leading a web link: the site's icon once loaded, a globe
/// until then (or when the site has none).
fn favicon(url: &str, line_height: f32, theme: &Theme) -> AnyElement {
    let slot = div()
        .flex_none()
        .size(px(14.0))
        .mr(px(4.0))
        .flex()
        .items_center()
        .justify_center();
    let slot = match crate::link_favicons::favicon_for(url, theme.appearance.is_dark()) {
        Some(image) => slot.child(gpui::img(image).size(px(14.0)).rounded(px(3.0))),
        None => slot.child(
            crate::icons::icon(crate::icons::GLOBE)
                .size(px(13.0))
                .text_color(theme.text_muted),
        ),
    };
    div()
        .flex_none()
        .h(px(line_height))
        .flex()
        .items_center()
        .child(slot)
        .into_any_element()
}

pub(super) fn render(
    flat: &FlatText,
    ix: usize,
    text_size: f32,
    line_height: f32,
    opts: &RenderOptions,
    theme: &Theme,
) -> AnyElement {
    let group = format!("{}:{ix}", opts.row_key);
    let mut flow = div().min_w_0().flex().flex_wrap().items_center();
    for (range, is_chip) in segments(flat) {
        // RowVeil keys by element index, not row_key. Sharing index zero
        // rewrites the same fade baseline for every word on every frame.
        let part_ix = super::render::nested_ix(ix, usize::MAX, range.start);
        let part = fragment(flat, range.clone());
        let mut part_opts = opts.clone();
        part_opts.row_key = format!("{}-inline-{}", group, range.start).into();
        part_opts.selection_group = Some(group.clone());
        let lead = web_links(flat)
            .find(|(link, _)| link.start == range.start)
            .map(|(_, url)| favicon(url, line_height, theme));
        if !is_chip {
            let text = flat_text_element(&part, part_ix, &part_opts, theme);
            flow = flow.child(match lead {
                // Icon and first word share one box, so a wrap never strands
                // the favicon at the end of the previous line.
                Some(icon) => div()
                    .max_w_full()
                    .min_w_0()
                    .flex()
                    .items_start()
                    .child(icon)
                    .child(div().min_w_0().child(text)),
                None => div().max_w_full().min_w_0().child(text),
            });
            continue;
        }
        let web_host = part
            .links
            .is_empty()
            .then(|| web_host_url(&part.text))
            .flatten();
        match (lead, web_host.as_deref()) {
            (Some(icon), _) => flow = flow.child(icon),
            (None, Some(url)) => flow = flow.child(favicon(url, line_height, theme)),
            (None, None) => {}
        }
        let target = match part.links.first() {
            Some((_, url)) => super::render::is_previewable_file_link(url).then(|| url.clone()),
            None => file_target(&part.text).map(str::to_owned),
        };
        let tooltip = target.as_deref().map(|path| {
            let expanded = crate::file_preview::model::expand_tilde(path);
            match opts.file_root.as_deref() {
                Some(root) if !std::path::Path::new(expanded.as_ref()).is_absolute() => {
                    std::path::Path::new(root)
                        .join(expanded.as_ref())
                        .to_string_lossy()
                        .into_owned()
                }
                _ => expanded.into_owned(),
            }
        });
        let open = target
            .as_deref()
            .zip(opts.open_file.clone())
            .map(|(path, open)| (path.to_owned(), open));
        let hovered = file_hover_color(theme);
        let hover_state = flat.hovered_chip.clone();
        let chip_offset = range.start;
        let can_open = open.is_some();
        let mut chip = div()
            .id(SharedString::from(format!("{group}-chip-{}", range.start)))
            .min_w_0()
            .max_w_full()
            // Leave breathing room around the background while retaining
            // the full outer line box, including lines containing only chips.
            .my(px(2.0))
            .min_h(px(line_height - 4.0))
            .px(px(6.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded(px(6.0))
            .bg(theme.text.opacity(0.08))
            .text_size(px(text_size * 0.8))
            .line_height(px(line_height - 4.0))
            .font_family(theme.font_mono.clone())
            .text_color(theme.text)
            .when_some(tooltip, |chip, path| {
                chip.tooltip(move |_, cx| cx.new(|_| FilePathTooltip(path.clone().into())).into())
            })
            .when_some(open, |chip, (path, open)| {
                chip.cursor_pointer()
                    .on_hover(move |inside, window, _| {
                        let next = if *inside {
                            Some(chip_offset)
                        } else if hover_state.get() == Some(chip_offset) {
                            None
                        } else {
                            return;
                        };
                        if hover_state.replace(next) != next {
                            window.refresh();
                        }
                    })
                    .on_click(move |event, window, cx| {
                        if let gpui::ClickEvent::Mouse(event) = event
                            && (event.up.position - event.down.position).magnitude() > 2.0
                        {
                            return;
                        }
                        cx.stop_propagation();
                        open(&path, window, cx);
                    })
            });
        if let Some(path) = target.as_deref() {
            let descriptor =
                crate::tool_icons::tool_icon_descriptor(&zeron_proto::ToolCall::ReadFile {
                    path: crate::file_preview::model::strip_line_col(path).to_owned(),
                });
            if let Some(image) = descriptor.material_image() {
                chip = chip.child(gpui::img(image).size(px(14.0)).flex_none());
            }
        }
        // The text still uses the Markdown selection registry; no icon padding
        // enters its string. The box owns opening, so nested links cannot double-fire.
        let mut label = part;
        if target.is_some() {
            label.links.clear();
        }
        if let Some(url) = web_host {
            // Opens through the same link path as any prose web link.
            label.links = vec![(0..label.text.len(), url)];
            let blue = super::render::web_link_color(theme);
            for run in &mut label.runs {
                run.color = blue;
            }
        }
        for run in &mut label.runs {
            let active = can_open && flat.hovered_chip.get() == Some(range.start);
            if active {
                run.color = hovered;
            }
            // A code chip never carries a link underline of its own: either
            // the box owns opening (hover underline below) or it is inert.
            run.underline = None;
            if can_open {
                run.underline = active.then_some(gpui::UnderlineStyle {
                    color: Some(hovered),
                    thickness: px(1.0),
                    wavy: false,
                });
            }
        }
        let label = flat_text_element(&label, part_ix, &part_opts, theme);
        chip = chip.child(div().min_w_0().flex_shrink(1.0).child(label));
        flow = flow.child(chip);
    }
    flow.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::super::parser::{InlineRun, InlineStyle};
    use super::*;

    #[test]
    fn code_files_are_distinct_from_commands_and_symbols() {
        for path in [
            "knip.json",
            ".gitignore",
            "src/ação.tsx",
            "src/app/[...all]/route.ts",
            "Dockerfile",
            "src/main.rs:12:3",
        ] {
            assert_eq!(file_target(path), Some(path));
        }
        for code in [
            "npm run knip",
            "useState",
            "@types/papaparse",
            "foo()",
            "echo x;cat a.ts",
            "https://x.dev/a.ts",
            "www.jkdistribuicao.com.br",
            "example.com/loja",
        ] {
            assert_eq!(file_target(code), None);
        }
        for file in ["notes.org", "COMMAND.COM"] {
            assert_eq!(file_target(file), Some(file));
        }
    }

    #[test]
    fn bare_hosts_in_code_open_as_web_links() {
        assert_eq!(
            web_host_url("www.jkdistribuicao.com.br").as_deref(),
            Some("https://www.jkdistribuicao.com.br")
        );
        assert_eq!(
            web_host_url("jkdistribuicao.com.br/contato").as_deref(),
            Some("https://jkdistribuicao.com.br/contato")
        );
        for not_host in [
            "brain.md",
            "script.sh",
            "src/main.rs",
            "AGENTS.md",
            "README.org",
            "COMMAND.COM",
            "example.com",
            "www",
            "a..com",
        ] {
            assert_eq!(web_host_url(not_host), None, "{not_host}");
        }
    }

    #[test]
    fn inline_fragments_cover_source_without_invented_spacing() {
        let flat = super::super::render::flatten_runs(
            &[
                InlineRun {
                    text: "Veja o ".into(),
                    style: InlineStyle::default(),
                },
                InlineRun {
                    text: "src/ação.rs".into(),
                    style: InlineStyle {
                        code: true,
                        ..Default::default()
                    },
                },
                InlineRun {
                    text: " agora.".into(),
                    style: InlineStyle::default(),
                },
            ],
            &Theme::dark(),
            false,
        );
        let pieces = segments(&flat);
        let text = pieces
            .iter()
            .map(|(range, _)| fragment(&flat, range.clone()).text.to_string())
            .collect::<String>();
        assert_eq!(text, "Veja o src/ação.rs agora.");
        assert_eq!(pieces.iter().filter(|(_, chip)| *chip).count(), 1);
    }
}

#[cfg(test)]
mod streaming_regressions {
    use super::*;
    use crate::markdown::{parser::parse_full, render::flatten_runs, veil::RowVeil};
    use std::time::{Duration, Instant};

    #[test]
    fn inline_streaming_fragments_settle_before_the_turn_finishes() {
        let theme = Theme::dark();
        let veil = std::rc::Rc::new(std::cell::RefCell::new(RowVeil::default()));
        let now = Instant::now();
        let blocks = [
            "Antes `AGENTS.md` depois",
            "`TOOLS.md` passo 6",
            "receita `cargo run` inline",
        ];
        let flats: Vec<_> = blocks
            .iter()
            .map(|source| {
                let tree = parse_full(source);
                let crate::markdown::parser::Block::Paragraph { runs } = &tree.blocks[0].block
                else {
                    panic!("paragraph")
                };
                flatten_runs(runs, &theme, false)
            })
            .collect();
        let mut opts = RenderOptions::settled("streaming-test".into());
        opts.veil = Some(veil.clone());
        opts.now = now;
        for (ix, flat) in flats.iter().enumerate() {
            let _ = render(flat, ix, 14.0, 22.0, &opts, &theme);
        }
        assert!(veil.borrow().is_fading());
        // Invoke the real renderer repeatedly while the streaming veil remains
        // attached. The old index-zero implementation restarts on every call.
        for frame in 1..=3 {
            opts.now = now + Duration::from_millis(500 * frame);
            for (ix, flat) in flats.iter().enumerate() {
                let _ = render(flat, ix, 14.0, 22.0, &opts, &theme);
            }
            assert!(!veil.borrow().is_fading());
        }
    }
}
