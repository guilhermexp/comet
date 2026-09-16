use std::{
    fs,
    path::Path,
    sync::{Arc, LazyLock},
};

use super::model::{PreviewKind, classify_preview_kind};
use crate::markdown::parser::BlockTree;
use calamine::{Data, Range, Reader, open_workbook_auto};
use comet_syntax::HighlightedDocument;
use gpui::{
    Font, Image, ImageFormat, Pixels, SharedString, TextRun, TextSystem, WindowTextSystem, font, px,
};

const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_BINARY_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone)]
pub enum LoadedPreview {
    Markdown(Arc<BlockTree>),
    Code {
        lines: Arc<[SharedString]>,
        highlights: Option<Arc<HighlightedDocument>>,
        widest_line_ix: Option<usize>,
    },
    Html(Arc<str>),
    Image(Arc<Image>),
    Pdf,
    Video,
    Table(Arc<[Vec<SharedString>]>),
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewLoadError {
    OutsideCheckout,
    Missing,
    TooLarge,
    InvalidUtf8,
    Remote(String),
    Io(String),
}

pub fn isolated_html_document(source: &str) -> String {
    let constrained = format!(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; connect-src 'none'; img-src data: blob:; media-src data: blob:; style-src 'unsafe-inline'; font-src data:; object-src 'none'; base-uri 'none'; form-action 'none'\">{source}"
    );
    let escaped = constrained
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\0', "\u{fffd}");
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\"><style>html,body,iframe{{width:100%;height:100%;margin:0;border:0}}iframe{{display:block}}</style></head><body><iframe sandbox referrerpolicy=\"no-referrer\" srcdoc=\"{escaped}\"></iframe></body></html>"
    )
}

fn image_format(path: &Path) -> Option<ImageFormat> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => Some(ImageFormat::Png),
        Some("jpg" | "jpeg") => Some(ImageFormat::Jpeg),
        Some("gif") => Some(ImageFormat::Gif),
        Some("webp") => Some(ImageFormat::Webp),
        Some("svg") => Some(ImageFormat::Svg),
        Some("bmp") => Some(ImageFormat::Bmp),
        _ => None,
    }
}

pub fn detect_image_format(bytes: &[u8], path: &Path) -> Option<ImageFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(ImageFormat::Png);
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(ImageFormat::Jpeg);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(ImageFormat::Gif);
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some(ImageFormat::Webp);
    }
    if bytes.starts_with(b"BM") {
        return Some(ImageFormat::Bmp);
    }
    let trimmed = bytes
        .iter()
        .take(256)
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .collect::<Vec<u8>>();
    if trimmed.starts_with(b"<?xml") || trimmed.starts_with(b"<svg") {
        return Some(ImageFormat::Svg);
    }
    image_format(path)
}

fn shared_rows(rows: Vec<Vec<String>>) -> Arc<[Vec<SharedString>]> {
    rows.into_iter()
        .map(|row| row.into_iter().map(SharedString::from).collect())
        .collect::<Vec<_>>()
        .into()
}
/// Measures lines using real GPUI text shaping and returns the index of the widest line.
///
/// Uses GPUI's cross-platform text system with the configured typography so that font fallback,
/// ZWJ emoji sequences, tabs, combining marks, and accents reflect real pixel advances rather than
/// an arbitrary monospace cell grid or byte heuristic.
pub fn find_widest_line_index_with_system(
    lines: &[SharedString],
    text_system: &WindowTextSystem,
    font: &Font,
    font_size: Pixels,
) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }

    let shape_width = |line: &str| -> f32 {
        let clean = line.strip_suffix('\r').unwrap_or(line);
        let run = TextRun {
            len: clean.len(),
            font: font.clone(),
            color: gpui::Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        f32::from(
            text_system
                .shape_line(
                    SharedString::from(clean.to_string()),
                    font_size,
                    &[run],
                    None,
                )
                .width(),
        )
    };

    let mut candidate_indices: Vec<usize> = (0..lines.len()).collect();
    candidate_indices.sort_unstable_by(|&a, &b| lines[b].len().cmp(&lines[a].len()));

    let mut max_width = 0.0f32;
    let mut widest_ix = None;

    for &ix in &candidate_indices {
        let line = &lines[ix];
        let byte_len = line.len();
        if (byte_len as f32) * 40.0 < max_width {
            break;
        }
        let char_count = line.chars().count();
        if (char_count as f32) * 40.0 < max_width {
            continue;
        }
        let w = shape_width(line.as_ref());
        if w > max_width || widest_ix.is_none() {
            max_width = w;
            widest_ix = Some(ix);
        }
    }

    widest_ix
}

static PREVIEW_TEXT_SYSTEM: LazyLock<Arc<TextSystem>> = LazyLock::new(|| {
    let platform = gpui_platform::current_platform(true);
    let ts = Arc::new(TextSystem::new(platform.text_system()));
    let faces: Vec<std::borrow::Cow<'static, [u8]>> = crate::typography::GEIST_MONO
        .iter()
        .map(|bytes| std::borrow::Cow::Borrowed(*bytes))
        .collect();
    let _ = ts.add_fonts(faces);
    ts
});
/// Finds the index of the visually widest line in a code document using GPUI text shaping.
pub fn find_widest_line_index(lines: &[SharedString]) -> Option<usize> {
    let wts = WindowTextSystem::new(PREVIEW_TEXT_SYSTEM.clone());
    let font = font("Geist Mono");
    find_widest_line_index_with_system(lines, &wts, &font, px(12.5))
}

pub fn load_preview(root: &Path, relative_path: &Path) -> Result<LoadedPreview, PreviewLoadError> {
    load_preview_with_typography(root, relative_path, "Geist Mono".into(), px(12.5), None)
}

pub fn load_preview_with_typography(
    root: &Path,
    relative_path: &Path,
    font_family: SharedString,
    font_size: Pixels,
    text_system: Option<Arc<TextSystem>>,
) -> Result<LoadedPreview, PreviewLoadError> {
    if relative_path.as_os_str().is_empty() {
        return Err(PreviewLoadError::OutsideCheckout);
    }
    // Explicit read-only previews may cross projects, including symlinks.
    // Files mutation and remote workspace RPC keep their separate boundaries.
    let path = root
        .join(relative_path)
        .canonicalize()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => PreviewLoadError::Missing,
            _ => PreviewLoadError::Io(error.to_string()),
        })?;
    if path.is_dir() {
        let mut entries = fs::read_dir(&path)
            .map_err(|error| PreviewLoadError::Io(error.to_string()))?
            .take(2001)
            .map(|entry| {
                entry.map(|entry| {
                    let suffix = if entry.path().is_dir() { "/" } else { "" };
                    format!("{}{suffix}", entry.file_name().to_string_lossy())
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| PreviewLoadError::Io(error.to_string()))?;
        entries.sort();
        if entries.len() > 2000 {
            entries.truncate(2000);
            entries.push("[Directory listing limited to 2000 entries]".into());
        }
        return load_text_preview(
            Path::new("directory.txt"),
            format!("{}\n\n{}", path.display(), entries.join("\n")),
            font_family,
            font_size,
            text_system,
        );
    }
    if !path.is_file() {
        return Err(PreviewLoadError::Io(
            "This path is not a regular file or directory.".into(),
        ));
    }
    let kind = classify_preview_kind(path.to_string_lossy().as_ref());
    // A video is never read into memory: WebKit streams it off disk, so the
    // binary byte cap below does not apply. Capping it would reject the
    // ordinary case — a screen recording is routinely hundreds of megabytes.
    if kind == PreviewKind::Video {
        return Ok(LoadedPreview::Video);
    }
    let metadata = fs::metadata(&path).map_err(|error| PreviewLoadError::Io(error.to_string()))?;
    let binary = matches!(kind, PreviewKind::Image | PreviewKind::Pdf)
        || matches!(kind, PreviewKind::Data)
            && !matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("csv" | "tsv")
            );
    let limit = if binary {
        MAX_BINARY_BYTES
    } else {
        MAX_TEXT_BYTES
    };
    if metadata.len() > limit {
        return Err(PreviewLoadError::TooLarge);
    }
    if kind == PreviewKind::Data && binary {
        let mut workbook =
            open_workbook_auto(&path).map_err(|error| PreviewLoadError::Io(error.to_string()))?;
        let sheet = workbook
            .sheet_names()
            .first()
            .cloned()
            .ok_or_else(|| PreviewLoadError::Io("workbook has no worksheets".into()))?;
        let range = workbook
            .worksheet_range(&sheet)
            .map_err(|error| PreviewLoadError::Io(error.to_string()))?;
        return Ok(LoadedPreview::Table(shared_rows(workbook_rows(
            &range, 2_000, 100,
        ))));
    }
    if kind == PreviewKind::Pdf {
        return Ok(LoadedPreview::Pdf);
    }
    let bytes = fs::read(&path).map_err(|error| PreviewLoadError::Io(error.to_string()))?;
    if kind == PreviewKind::Image {
        let format = detect_image_format(&bytes, &path)
            .ok_or_else(|| PreviewLoadError::Io("the image format is not supported".to_owned()))?;
        return Ok(LoadedPreview::Image(Arc::new(Image::from_bytes(
            format, bytes,
        ))));
    }
    if kind == PreviewKind::Unsupported {
        if let Some(format) = detect_image_format(&bytes, &path) {
            return Ok(LoadedPreview::Image(Arc::new(Image::from_bytes(
                format, bytes,
            ))));
        }
        if bytes.starts_with(b"%PDF-") {
            return Ok(LoadedPreview::Pdf);
        }
        if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() {
            let mut source = format!(
                "Binary file · {} bytes\nHexadecimal preview (first 64 KiB)\n\n",
                bytes.len()
            );
            for (row, chunk) in bytes[..bytes.len().min(65536)].chunks(16).enumerate() {
                use std::fmt::Write;
                let _ = write!(source, "{:08x}  ", row * 16);
                for byte in chunk {
                    let _ = write!(source, "{byte:02x} ");
                }
                source.push('\n');
            }
            return load_text_preview(
                Path::new("binary.txt"),
                source,
                font_family,
                font_size,
                text_system,
            );
        }
    }
    let source = String::from_utf8(bytes).map_err(|_| PreviewLoadError::InvalidUtf8)?;
    load_text_preview(&path, source, font_family, font_size, text_system)
}

pub fn workbook_rows(range: &Range<Data>, max_rows: usize, max_columns: usize) -> Vec<Vec<String>> {
    range
        .rows()
        .take(max_rows)
        .map(|row| {
            row.iter()
                .take(max_columns)
                .map(ToString::to_string)
                .collect()
        })
        .collect()
}

/// Parses CSV or TSV tabular data following RFC 4180:
/// - Fields may be enclosed in double quotes.
/// - Double quotes inside quoted fields are escaped strictly with `""`.
/// - Backslashes are literal characters and never escape quotes.
/// - CRLF and multiline breaks inside quoted fields are preserved without silent normalization.
/// - For malformed input (e.g. unclosed quote at EOF), content is preserved as-is rather than failing.
pub fn parse_delimited_table(
    source: &str,
    separator: char,
    max_rows: usize,
    max_columns: usize,
) -> Vec<Vec<SharedString>> {
    let mut rows = Vec::new();
    let mut current_row: Vec<SharedString> = Vec::new();
    let mut current_field = String::new();
    let mut chars = source.chars().peekable();
    let mut in_quotes = false;
    let mut field_started = false;

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    current_field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                current_field.push(c);
            }
        } else {
            if c == '"' && !field_started {
                in_quotes = true;
                field_started = true;
            } else if c == separator {
                if current_row.len() < max_columns {
                    current_row.push(SharedString::from(std::mem::take(&mut current_field)));
                } else {
                    current_field.clear();
                }
                field_started = false;
            } else if c == '\n' || (c == '\r' && chars.peek() == Some(&'\n')) {
                if c == '\r' {
                    chars.next();
                }
                if field_started || !current_row.is_empty() {
                    if current_row.len() < max_columns {
                        current_row.push(SharedString::from(std::mem::take(&mut current_field)));
                    } else {
                        current_field.clear();
                    }
                    rows.push(std::mem::take(&mut current_row));
                }
                field_started = false;
                if rows.len() >= max_rows {
                    return rows;
                }
            } else {
                field_started = true;
                current_field.push(c);
            }
        }
    }

    if field_started || !current_row.is_empty() {
        if current_row.len() < max_columns {
            current_row.push(SharedString::from(current_field));
        }
        if !current_row.is_empty() {
            rows.push(current_row);
        }
    }

    rows
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use gpui::SharedString;

    use super::{
        LoadedPreview, PreviewLoadError, find_widest_line_index, isolated_html_document,
        load_preview,
    };
    use calamine::{Data, Range};

    #[test]
    fn prepares_markdown_inside_checkout() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("README.md"), "# Hello").unwrap();
        assert!(matches!(
            load_preview(temp.path(), Path::new("README.md")).unwrap(),
            LoadedPreview::Markdown(_)
        ));
    }

    #[test]
    fn loads_unknown_text_through_parent_paths_and_external_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("chat");
        fs::create_dir(&root).unwrap();
        fs::write(temp.path().join(".openspec-target"), "global").unwrap();
        for path in [
            temp.path().join(".openspec-target"),
            Path::new("../.openspec-target").to_owned(),
        ] {
            let LoadedPreview::Code { lines, .. } = load_preview(&root, &path).unwrap() else {
                panic!("unknown text must render");
            };
            assert_eq!(lines[0].as_ref(), "global");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(temp.path().join(".openspec-target"), root.join("linked"))
                .unwrap();
            assert!(matches!(
                load_preview(&root, Path::new("linked")),
                Ok(LoadedPreview::Code { .. })
            ));
        }
    }

    #[test]
    fn unknown_binary_has_a_hex_preview() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("payload.custom"), [0, 255, 42]).unwrap();
        let LoadedPreview::Code { lines, .. } =
            load_preview(root.path(), Path::new("payload.custom")).unwrap()
        else {
            panic!("binary preview");
        };
        assert!(lines.iter().any(|line| line.contains("00 ff 2a")));
    }

    #[test]
    fn loads_an_absolute_path_from_outside_the_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let report = outside.path().join("report.md");
        fs::write(&report, "# worker\n").unwrap();

        // A raiz e' de outro lugar: o caminho absoluto manda.
        assert!(matches!(
            load_preview(root.path(), &report),
            Ok(LoadedPreview::Markdown(_))
        ));
        assert!(matches!(
            load_preview(root.path(), &outside.path().join("missing.md")),
            Err(PreviewLoadError::Missing)
        ));
        assert!(matches!(
            load_preview(root.path(), outside.path()),
            Ok(LoadedPreview::Code { .. })
        ));
    }

    #[test]
    fn rejects_oversized_text_before_allocating_view_state() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("large.txt"),
            vec![b'x'; 4 * 1024 * 1024 + 1],
        )
        .unwrap();
        assert!(matches!(
            load_preview(temp.path(), Path::new("large.txt")),
            Err(PreviewLoadError::TooLarge)
        ));
    }

    /// A screen recording is routinely larger than the binary cap, and nothing
    /// reads it: the cap must not reach the video arm, or the ordinary case is
    /// the rejected one.
    #[test]
    fn a_video_past_the_binary_cap_is_still_previewable() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("recording.mov");
        let file = fs::File::create(&path).unwrap();
        file.set_len(64 * 1024 * 1024).unwrap();
        drop(file);
        assert!(matches!(
            load_preview(temp.path(), Path::new("recording.mov")).unwrap(),
            LoadedPreview::Video
        ));
    }

    #[test]
    fn workbook_rows_are_bounded_and_render_cell_display_values() {
        let mut range = Range::new((0, 0), (1, 1));
        range.set_value((0, 0), Data::String("Name".into()));
        range.set_value((0, 1), Data::String("Value".into()));
        range.set_value((1, 0), Data::String("Total".into()));
        range.set_value((1, 1), Data::Float(42.5));
        assert_eq!(
            super::workbook_rows(&range, 2, 2),
            vec![
                vec!["Name".to_string(), "Value".to_string()],
                vec!["Total".to_string(), "42.5".to_string()],
            ]
        );
    }

    #[test]
    fn prepares_code_lines_and_highlights_before_rendering() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("main.rs"), "fn main() {}\n").unwrap();
        let LoadedPreview::Code {
            lines,
            highlights,
            widest_line_ix,
        } = load_preview(temp.path(), Path::new("main.rs")).unwrap()
        else {
            panic!("expected prepared code preview");
        };
        assert_eq!(lines.as_ref(), ["fn main() {}", ""]);
        assert!(highlights.is_some());
        assert_eq!(widest_line_ix, Some(0));
    }

    #[test]
    fn widest_line_with_gpui_shaping_handles_zwj_ascii_tabs_and_emojis() {
        // Counterexample probe:
        // 100x 👨‍👩‍👧‍👦 (2500 bytes, 700 chars) = 1700px in GPUI Geist Mono.
        // 400x 'X' (400 bytes, 400 chars) = 3335px in GPUI Geist Mono.
        // Even though ZWJ 100 has more bytes and chars, ASCII 400 is visually wider:
        let zwj_100 = "👨‍👩‍👧‍👦".repeat(100);
        let ascii_400 = "X".repeat(400);

        let lines = vec![SharedString::from(zwj_100), SharedString::from(ascii_400)];
        assert_eq!(find_widest_line_index(&lines), Some(1));

        // Mixed lines with tabs, accents, and diverse emojis:
        let mixed = vec![
            SharedString::from("short line"),
            SharedString::from("\t\tindent"),
            SharedString::from("Silva, João ✅ ⚠️ 🏳️‍🌈"),
            SharedString::from("fn func() -> usize { 42 }"),
        ];
        assert_eq!(find_widest_line_index(&mixed), Some(3));

        // Empty slice and single line:
        assert_eq!(find_widest_line_index(&[]), None);
        assert_eq!(find_widest_line_index(&[SharedString::from("")]), Some(0));
    }
    #[test]
    fn isolated_html_is_serialized_into_a_sandboxed_document() {
        let document = isolated_html_document(
            "<script>window.top.location=\"https://example.com\"</script><h1>Preview</h1>",
        );
        assert!(document.contains("<iframe sandbox referrerpolicy=\"no-referrer\""));
        assert!(document.contains("default-src 'none'"));
        assert!(document.contains("window.top.location=&quot;https://example.com&quot;"));
        assert!(!document.contains("window.top.location=\"https://example.com\""));
    }

    #[test]
    fn parses_csv_respecting_quotes_commas_multiline_and_escapes() {
        let temp = tempfile::tempdir().unwrap();
        let csv_content = "name,age,notes\n\"Silva, João\",25,\"Line 1\nLine 2\"\n\"He said \"\"hello\"\"\",30,plain\n";
        fs::write(temp.path().join("test.csv"), csv_content).unwrap();
        let LoadedPreview::Table(rows) = load_preview(temp.path(), Path::new("test.csv")).unwrap()
        else {
            panic!("expected table preview");
        };
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows[0],
            vec![
                gpui::SharedString::from("name"),
                gpui::SharedString::from("age"),
                gpui::SharedString::from("notes"),
            ]
        );
        assert_eq!(
            rows[1],
            vec![
                gpui::SharedString::from("Silva, João"),
                gpui::SharedString::from("25"),
                gpui::SharedString::from("Line 1\nLine 2"),
            ]
        );
        assert_eq!(
            rows[2],
            vec![
                gpui::SharedString::from("He said \"hello\""),
                gpui::SharedString::from("30"),
                gpui::SharedString::from("plain"),
            ]
        );
    }

    #[test]
    fn parses_tsv_and_clamps_limits() {
        let tsv_content =
            "col1\tcol2\tcol3\n\"val\t1\"\t\"val\"\"2\"\tval3\nextra1\textra2\textra3\n";
        let rows = super::parse_delimited_table(tsv_content, '\t', 2, 2);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            vec![
                gpui::SharedString::from("col1"),
                gpui::SharedString::from("col2"),
            ]
        );
        assert_eq!(
            rows[1],
            vec![
                gpui::SharedString::from("val\t1"),
                gpui::SharedString::from("val\"2"),
            ]
        );
    }

    #[test]
    fn parses_empty_and_trailing_delimiter_fields() {
        let csv = "a,,c\n1,2,\n";
        let rows = super::parse_delimited_table(csv, ',', 10, 10);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            vec![
                gpui::SharedString::from("a"),
                gpui::SharedString::from(""),
                gpui::SharedString::from("c"),
            ]
        );
        assert_eq!(
            rows[1],
            vec![
                gpui::SharedString::from("1"),
                gpui::SharedString::from("2"),
                gpui::SharedString::from(""),
            ]
        );
    }
    #[test]
    fn parses_csv_with_trailing_backslash_in_quoted_field() {
        let temp = tempfile::tempdir().unwrap();
        let csv_content = "\"C:\\tmp\\\",42\n";
        fs::write(temp.path().join("path.csv"), csv_content).unwrap();
        let LoadedPreview::Table(rows) = load_preview(temp.path(), Path::new("path.csv")).unwrap()
        else {
            panic!("expected table preview");
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0],
            vec![
                gpui::SharedString::from("C:\\tmp\\"),
                gpui::SharedString::from("42"),
            ]
        );
    }
    #[test]
    fn parses_csv_preserving_crlf_inside_quotes() {
        let csv_content = "\"Line 1\r\nLine 2\",42\r\n";
        let rows = super::parse_delimited_table(csv_content, ',', 10, 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0],
            vec![
                gpui::SharedString::from("Line 1\r\nLine 2"),
                gpui::SharedString::from("42"),
            ]
        );
    }
}

// Shared by local files and text received through the owning device's RPC.
pub(crate) fn load_text_preview(
    path: &Path,
    source: String,
    font_family: SharedString,
    font_size: Pixels,
    text_system: Option<Arc<TextSystem>>,
) -> Result<LoadedPreview, PreviewLoadError> {
    if source.len() as u64 > MAX_TEXT_BYTES {
        return Err(PreviewLoadError::TooLarge);
    }
    let kind = classify_preview_kind(&path.to_string_lossy());
    let path = path.to_string_lossy();
    match kind {
        PreviewKind::Markdown => Ok(LoadedPreview::Markdown(Arc::new(
            crate::markdown::parse_full(&source),
        ))),
        PreviewKind::Code | PreviewKind::Unsupported => {
            let lines: Arc<[SharedString]> = source
                .split('\n')
                .map(SharedString::from)
                .collect::<Vec<_>>()
                .into();
            let widest_line_ix = {
                let ts = text_system.unwrap_or_else(|| PREVIEW_TEXT_SYSTEM.clone());
                let wts = WindowTextSystem::new(ts);
                let font = font(font_family);
                find_widest_line_index_with_system(&lines, &wts, &font, font_size)
            };
            Ok(LoadedPreview::Code {
                lines,
                highlights: comet_syntax::highlight(comet_syntax::HighlightRequest {
                    source: &source,
                    path: Some(path.as_ref()),
                    fence_tag: None,
                })
                .ok()
                .map(Arc::new),
                widest_line_ix,
            })
        }
        PreviewKind::Html => Ok(LoadedPreview::Html(isolated_html_document(&source).into())),
        PreviewKind::Data => {
            let separator = if path.to_ascii_lowercase().ends_with(".tsv") {
                '\t'
            } else {
                ','
            };
            Ok(LoadedPreview::Table(
                parse_delimited_table(&source, separator, 2_000, 100).into(),
            ))
        }
        PreviewKind::Image | PreviewKind::Pdf | PreviewKind::Video => {
            Ok(LoadedPreview::Unsupported)
        }
    }
}

#[cfg(test)]
mod remote_text_tests {
    use super::*;
    #[test]
    fn remote_documents_use_native_renderers_without_touching_a_local_path() {
        let path = Path::new("/remote-only/does-not-exist/report.md");
        assert!(
            matches!(load_text_preview(path, "# Relatório\n\nconteúdo".into(), "Geist Mono".into(), px(12.5), None).unwrap(), LoadedPreview::Markdown(tree) if tree.len() > 0)
        );
        let html = load_text_preview(
            Path::new("/remote-only/report.html"),
            "<h1>Remote</h1><script>fetch('/secret')</script>".into(),
            "Geist Mono".into(),
            px(12.5),
            None,
        )
        .unwrap();
        assert!(
            matches!(html, LoadedPreview::Html(source) if source.contains("iframe sandbox") && source.contains("connect-src 'none'"))
        );
    }
}
