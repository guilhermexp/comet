use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui::{Image, ImageFormat, SharedString};

use crate::file_preview::loader::{LoadedPreview, PreviewLoadError, load_preview};
use crate::markdown::parser::Block;

const MAX_INLINE_IMAGES: usize = 6;
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"];
const MAX_MERMAID_BYTES: usize = 64 * 1024;
const MAX_MERMAID_LINES: usize = 2_000;
const MAX_MERMAID_STRUCTURE: usize = 4_000;
const MAX_MERMAID_SVG_BYTES: usize = 4 * 1024 * 1024;

pub struct LoadedInlineImage {
    pub relative_path: String,
    pub name: SharedString,
    pub image: Arc<Image>,
    pub bytes: u64,
}

pub struct RenderedMermaid {
    pub image: Arc<Image>,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineImageError {
    OutsideCheckout,
    Unsupported,
    Load(PreviewLoadError),
}

fn normalize_candidate(raw: &str) -> Option<String> {
    if raw.trim_start().starts_with("![") {
        return None;
    }
    let trimmed = raw
        .trim()
        .trim_matches(|ch: char| matches!(ch, '"' | '\'' | '`' | '<' | '>'))
        .trim_end_matches(|ch: char| matches!(ch, ')' | ']' | ',' | '.' | ';' | ':' | '!' | '?'));
    let path = trimmed.strip_prefix("file://").unwrap_or(trimmed);
    if path.is_empty()
        || path.starts_with("http://")
        || path.starts_with("https://")
        || path.contains('\0')
    {
        return None;
    }
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())?
        .to_ascii_lowercase();
    IMAGE_EXTENSIONS
        .contains(&extension.as_str())
        .then(|| path.to_owned())
}

fn push_candidate(raw: &str, seen: &mut HashSet<String>, paths: &mut Vec<String>) {
    if paths.len() >= MAX_INLINE_IMAGES {
        return;
    }
    if let Some(path) = normalize_candidate(raw)
        && seen.insert(path.clone())
    {
        paths.push(path);
    }
}

pub fn extract_image_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();

    let mut rest = text;
    while let Some(image_start) = rest.find("![") {
        let after_start = &rest[image_start + 2..];
        let Some(label_end) = after_start.find("](") else {
            break;
        };
        let destination = &after_start[label_end + 2..];
        let Some(destination_end) = destination.find(')') else {
            break;
        };
        push_candidate(&destination[..destination_end], &mut seen, &mut paths);
        rest = &destination[destination_end + 1..];
    }

    for token in text.split_whitespace() {
        push_candidate(token, &mut seen, &mut paths);
        if paths.len() == MAX_INLINE_IMAGES {
            break;
        }
    }
    paths
}

fn canonical_candidate(
    root: &Path,
    candidate: &str,
) -> Result<(PathBuf, PathBuf), InlineImageError> {
    let root = root
        .canonicalize()
        .map_err(|_| InlineImageError::OutsideCheckout)?;
    let raw = candidate.strip_prefix("file://").unwrap_or(candidate);
    let candidate = Path::new(raw);
    let absolute = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let absolute = absolute
        .canonicalize()
        .map_err(|_| InlineImageError::OutsideCheckout)?;
    let relative = absolute
        .strip_prefix(&root)
        .map_err(|_| InlineImageError::OutsideCheckout)?
        .to_path_buf();
    Ok((root, relative))
}

pub fn load_checkout_image(
    root: &Path,
    candidate: &str,
) -> Result<LoadedInlineImage, InlineImageError> {
    let (root, relative) = canonical_candidate(root, candidate)?;
    let bytes = std::fs::metadata(root.join(&relative))
        .map_err(|error| InlineImageError::Load(PreviewLoadError::Io(error.to_string())))?
        .len();
    let relative_path = relative.to_string_lossy().into_owned();
    let name = relative
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(&relative_path)
        .to_owned();
    match load_preview(&root, &relative).map_err(InlineImageError::Load)? {
        LoadedPreview::Image(image) => Ok(LoadedInlineImage {
            relative_path,
            name: name.into(),
            image,
            bytes,
        }),
        _ => Err(InlineImageError::Unsupported),
    }
}

pub fn mermaid_source(block: &Block) -> Option<&str> {
    let Block::CodeBlock {
        language: Some(language),
        code,
    } = block
    else {
        return None;
    };
    matches!(
        language.trim().to_ascii_lowercase().as_str(),
        "mermaid" | "mmd"
    )
    .then_some(code.as_str())
}

pub fn render_mermaid_svg(source: &str, dark: bool) -> Result<RenderedMermaid, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("diagram source is empty".into());
    }
    if source.len() > MAX_MERMAID_BYTES {
        return Err("diagram source is too large".into());
    }
    if source.lines().count() > MAX_MERMAID_LINES {
        return Err("diagram has too many lines".into());
    }
    let structure = source.lines().count()
        + source.matches("-->").count()
        + source.matches("->>").count()
        + source.matches("---").count()
        + source.matches(';').count();
    if structure > MAX_MERMAID_STRUCTURE {
        return Err("diagram is too complex".into());
    }
    let options = mermaid_rs_renderer::RenderOptions {
        theme: if dark {
            mermaid_rs_renderer::Theme::dark()
        } else {
            mermaid_rs_renderer::Theme::mermaid_default()
        },
        ..Default::default()
    };
    let svg = mermaid_rs_renderer::render_with_options(source, options)
        .map_err(|error| error.to_string())?;
    if svg.len() > MAX_MERMAID_SVG_BYTES {
        return Err("rendered diagram is too large".into());
    }
    let bytes = svg.len();
    Ok(RenderedMermaid {
        image: Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())),
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{extract_image_paths, load_checkout_image, mermaid_source, render_mermaid_svg};
    use crate::markdown::parser::Block;

    #[test]
    fn extracts_markdown_absolute_and_relative_images_without_duplicates() {
        let text = concat!(
            "Preview ![chart](assets/chart.png), then /tmp/render.webp). ",
            "The same assets/chart.png and file:///tmp/final.jpg are repeated."
        );

        assert_eq!(
            extract_image_paths(text),
            vec!["assets/chart.png", "/tmp/render.webp", "/tmp/final.jpg"]
        );
    }

    #[test]
    fn image_extraction_rejects_false_positives_and_caps_the_gallery() {
        let text = concat!(
            "notes.txt archive.png.zip https://example.com/remote.png ",
            "a.png b.jpg c.jpeg d.gif e.webp f.bmp g.svg h.png"
        );

        assert_eq!(
            extract_image_paths(text),
            vec!["a.png", "b.jpg", "c.jpeg", "d.gif", "e.webp", "f.bmp"]
        );
    }

    #[test]
    fn checkout_image_loader_confines_candidates_to_the_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("artifacts")).unwrap();
        fs::write(root.path().join("artifacts/plot.png"), b"png").unwrap();
        fs::write(outside.path().join("secret.png"), b"secret").unwrap();

        let loaded = load_checkout_image(root.path(), "artifacts/plot.png").unwrap();
        assert_eq!(loaded.relative_path, "artifacts/plot.png");
        assert_eq!(loaded.name.as_ref(), "plot.png");

        assert!(load_checkout_image(root.path(), "../secret.png").is_err());
        assert!(
            load_checkout_image(
                root.path(),
                outside.path().join("secret.png").to_str().unwrap()
            )
            .is_err()
        );

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                outside.path().join("secret.png"),
                root.path().join("artifacts/escape.png"),
            )
            .unwrap();
            assert!(load_checkout_image(root.path(), "artifacts/escape.png").is_err());
        }
    }

    #[test]
    fn mermaid_fences_route_without_promoting_ordinary_code() {
        let mermaid = Block::CodeBlock {
            language: Some("mermaid".into()),
            code: "flowchart LR\nA --> B".into(),
        };
        let alias = Block::CodeBlock {
            language: Some("MMD".into()),
            code: "sequenceDiagram\nA->>B: hi".into(),
        };
        let rust = Block::CodeBlock {
            language: Some("rust".into()),
            code: "fn main() {}".into(),
        };

        assert_eq!(mermaid_source(&mermaid), Some("flowchart LR\nA --> B"));
        assert_eq!(mermaid_source(&alias), Some("sequenceDiagram\nA->>B: hi"));
        assert_eq!(mermaid_source(&rust), None);
    }

    #[test]
    fn mermaid_renderer_returns_svg_images_and_rejects_invalid_source() {
        assert!(render_mermaid_svg("flowchart LR\nA --> B", true).is_ok());
        assert!(render_mermaid_svg("flowchart LR\nA --> B", false).is_ok());
        assert!(
            render_mermaid_svg(
                "%%{init: {'flowchart': {'curve': 'linear'}}}%%\nflowchart LR\nA --> B",
                true,
            )
            .is_ok()
        );
        assert!(render_mermaid_svg("%% generated diagram\nflowchart LR\nA --> B", true).is_ok());
        assert!(
            render_mermaid_svg("---\ntitle: Example\n---\nflowchart LR\nA --> B", true).is_ok()
        );
        assert!(render_mermaid_svg("notMermaid\nA --> B", true).is_err());
        assert!(render_mermaid_svg("   ", true).is_err());
        let oversized = format!("flowchart LR\n{}", "A --> B\n".repeat(10_000));
        assert!(render_mermaid_svg(&oversized, true).is_err());
    }
}
