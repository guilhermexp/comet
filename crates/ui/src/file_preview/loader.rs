use std::{
    fs,
    path::{Component, Path},
    sync::Arc,
};

use calamine::{Data, Range, Reader, open_workbook_auto};
use comet_syntax::HighlightedDocument;
use gpui::{Image, ImageFormat, SharedString};

use super::model::{PreviewKind, classify_preview_kind};
use crate::markdown::parser::BlockTree;

const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_BINARY_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone)]
pub enum LoadedPreview {
    Markdown(Arc<BlockTree>),
    Code {
        lines: Arc<[SharedString]>,
        highlights: Option<Arc<HighlightedDocument>>,
    },
    Html(Arc<str>),
    Image(Arc<Image>),
    Pdf,
    Table(Arc<[Vec<SharedString>]>),
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewLoadError {
    OutsideCheckout,
    Missing,
    TooLarge,
    InvalidUtf8,
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

fn shared_rows(rows: Vec<Vec<String>>) -> Arc<[Vec<SharedString>]> {
    rows.into_iter()
        .map(|row| row.into_iter().map(SharedString::from).collect())
        .collect::<Vec<_>>()
        .into()
}

pub fn load_preview(root: &Path, relative_path: &Path) -> Result<LoadedPreview, PreviewLoadError> {
    if relative_path.as_os_str().is_empty()
        || relative_path.is_absolute()
        || !relative_path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(PreviewLoadError::OutsideCheckout);
    }
    let root = root.canonicalize().map_err(|_| PreviewLoadError::Missing)?;
    let path = root
        .join(relative_path)
        .canonicalize()
        .map_err(|_| PreviewLoadError::Missing)?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(PreviewLoadError::OutsideCheckout);
    }
    let kind = classify_preview_kind(path.to_string_lossy().as_ref());
    if kind == PreviewKind::Unsupported {
        return Ok(LoadedPreview::Unsupported);
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
        let format = image_format(&path)
            .ok_or_else(|| PreviewLoadError::Io("the image format is not supported".to_owned()))?;
        return Ok(LoadedPreview::Image(Arc::new(Image::from_bytes(
            format, bytes,
        ))));
    }
    let source = String::from_utf8(bytes).map_err(|_| PreviewLoadError::InvalidUtf8)?;
    let path = path.to_string_lossy();
    match kind {
        PreviewKind::Markdown => Ok(LoadedPreview::Markdown(Arc::new(
            crate::markdown::parse_full(&source),
        ))),
        PreviewKind::Code => Ok(LoadedPreview::Code {
            lines: source
                .split('\n')
                .map(SharedString::from)
                .collect::<Vec<_>>()
                .into(),
            highlights: comet_syntax::highlight(comet_syntax::HighlightRequest {
                source: &source,
                path: Some(path.as_ref()),
                fence_tag: None,
            })
            .ok()
            .map(Arc::new),
        }),
        PreviewKind::Html => Ok(LoadedPreview::Html(isolated_html_document(&source).into())),
        PreviewKind::Data => {
            let separator = if path.to_ascii_lowercase().ends_with(".tsv") {
                '\t'
            } else {
                ','
            };
            Ok(LoadedPreview::Table(
                source
                    .lines()
                    .map(|line| line.split(separator).map(SharedString::from).collect())
                    .collect::<Vec<_>>()
                    .into(),
            ))
        }
        PreviewKind::Image | PreviewKind::Pdf | PreviewKind::Unsupported => {
            Ok(LoadedPreview::Unsupported)
        }
    }
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

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::{LoadedPreview, PreviewLoadError, isolated_html_document, load_preview};
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
    fn rejects_parent_traversal_and_external_symlink() {
        let temp = tempfile::tempdir().unwrap();
        assert!(matches!(
            load_preview(temp.path(), Path::new("../secret")),
            Err(PreviewLoadError::OutsideCheckout)
        ));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/hosts", temp.path().join("hosts")).unwrap();
            assert!(matches!(
                load_preview(temp.path(), Path::new("hosts")),
                Err(PreviewLoadError::OutsideCheckout)
            ));
        }
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
        let LoadedPreview::Code { lines, highlights } =
            load_preview(temp.path(), Path::new("main.rs")).unwrap()
        else {
            panic!("expected prepared code preview");
        };
        assert_eq!(lines.as_ref(), ["fn main() {}", ""]);
        assert!(highlights.is_some());
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
}
