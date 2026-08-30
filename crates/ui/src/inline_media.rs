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
const MAX_MERMAID_SVG_BYTES: usize = 4 * 1024 * 1024;
/// Frame padding the diagram engine leaves around the drawing (its own default
/// is `40`, which is print-figure spacing: on an inline chat diagram it cost
/// 80px of width — enough to push an otherwise-fitting flowchart past the
/// transcript column — plus a matching band of dead vertical space above and
/// below. The block's own margin supplies the separation from the surrounding
/// text, so the SVG only needs enough to keep strokes off its own edge.
const MERMAID_FRAME_PADDING: u32 = 12;

pub struct LoadedInlineImage {
    pub relative_path: String,
    pub name: SharedString,
    pub image: Arc<Image>,
    pub bytes: u64,
}

pub struct RenderedMermaid {
    pub image: Arc<Image>,
    pub width: f32,
    pub height: f32,
    pub bytes: usize,
    pub svg: String,
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

/// Normaliza UM candidato ja delimitado (um valor de string do JSON da tool
/// call, por exemplo). `extract_image_paths` quebra em espaco e nao serve para
/// isso: o nome default de screenshot no macOS tem espacos.
pub fn image_path_candidate(raw: &str) -> Option<String> {
    normalize_candidate(raw)
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
    let raw = candidate.strip_prefix("file://").unwrap_or(candidate);
    let candidate = Path::new(raw);
    // Absoluto renderiza de onde estiver (`load_preview` o resolve sem raiz): o
    // screenshot que o agente acabou de tirar mora em /tmp ou ~/Desktop, nunca
    // no checkout, e o preview so mostra arquivo que a propria sessao ja abriu.
    // Relativo continua ancorado no checkout (inclusive symlink que escapa
    // dele) — sem raiz, "artifacts/x.png" nao quer dizer nada.
    if candidate.is_absolute() {
        let absolute = candidate
            .canonicalize()
            .map_err(|_| InlineImageError::OutsideCheckout)?;
        return Ok((root.to_path_buf(), absolute));
    }
    let root = root
        .canonicalize()
        .map_err(|_| InlineImageError::OutsideCheckout)?;
    let absolute = root
        .join(candidate)
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

pub fn strip_mermaid_frontmatter(code: &str) -> &str {
    let stripped_bom = code.strip_prefix('\u{FEFF}').unwrap_or(code);
    let leading_len = stripped_bom.len() - stripped_bom.trim_start().len();
    let candidate = &stripped_bom[leading_len..];

    let mut lines = candidate.split_inclusive('\n');
    let Some(first_line) = lines.next() else {
        return code;
    };
    if first_line.trim() != "---" {
        return code;
    }

    let mut consumed = first_line.len();
    let mut found_end = false;
    for line in lines {
        consumed += line.len();
        if line.trim() == "---" {
            found_end = true;
            break;
        }
    }

    if found_end {
        candidate[consumed..].trim_start()
    } else {
        code
    }
}

pub fn normalize_mermaid_source(code: &str) -> &str {
    let mut source = strip_mermaid_frontmatter(code);
    loop {
        let trimmed = source.trim_start();
        if trimmed.starts_with("%%") {
            if let Some(pos) = trimmed.find('\n') {
                source = &trimmed[pos + 1..];
                continue;
            } else {
                return "";
            }
        }
        return trimmed;
    }
}
use rquickjs::{Context, Runtime};
use std::cell::RefCell;

struct QuickJsMermaidEngine {
    _rt: Runtime,
    ctx: Context,
}

thread_local! {
    static ENGINE: RefCell<Option<QuickJsMermaidEngine>> = const { RefCell::new(None) };
}

static MERMAID_BUNDLE: &str = include_str!("../assets/mermaid_renderer.js");

fn with_mermaid_engine<R>(f: impl FnOnce(&Context) -> Result<R, String>) -> Result<R, String> {
    ENGINE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            let rt = Runtime::new().map_err(|e| format!("failed to initialize JS runtime: {e}"))?;
            rt.set_memory_limit(512 * 1024 * 1024);
            rt.set_max_stack_size(8 * 1024 * 1024);
            let ctx = Context::full(&rt).map_err(|e| format!("failed to create JS context: {e}"))?;
            ctx.with(|ctx| -> Result<(), String> {
                ctx.eval::<(), _>(
                    r#"
                    globalThis.global = globalThis;
                    globalThis.console = {
                        log: function(){},
                        warn: function(){},
                        error: function(){},
                        debug: function(){},
                        info: function(){}
                    };
                    globalThis.setTimeout = function(fn){ return 0; };
                    globalThis.clearTimeout = function(){};
                    globalThis.performance = { now: function(){ return Date.now(); } };
                    globalThis.atob = function(input) {
                        var chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=';
                        var str = String(input).replace(/=+$/, '');
                        var output = '';
                        if (str.length % 4 == 1) throw new Error('atob: invalid input');
                        for (var bc = 0, bs, buffer, idx = 0; (buffer = str.charAt(idx++)); ) {
                            buffer = chars.indexOf(buffer);
                            if (buffer === -1) continue;
                            bs = bc % 4 ? bs * 64 + buffer : buffer;
                            if (bc++ % 4) output += String.fromCharCode(255 & (bs >> ((-2 * bc) & 6)));
                        }
                        return output;
                    };
                    "#,
                )
                .map_err(|e| format!("failed to set up JS polyfills: {e}"))?;

                ctx.eval::<rquickjs::Value, _>(MERMAID_BUNDLE)
                    .map_err(|e| {
                        if let rquickjs::Error::Exception = e {
                            let ex = ctx.catch();
                            let msg = ex.as_exception().and_then(|x| x.message()).unwrap_or_default();
                            format!("failed to evaluate mermaid bundle: {msg}")
                        } else {
                            format!("failed to evaluate mermaid bundle: {e}")
                        }
                    })?;

                ctx.eval::<(), _>(
                    r#"
                    globalThis.__render = function(src, optsJson) {
                        return globalThis.__renderMermaidSVG(src, JSON.parse(optsJson));
                    };
                    "#,
                )
                .map_err(|e| format!("failed to bind render function: {e}"))?;

                Ok(())
            })?;
            *slot = Some(QuickJsMermaidEngine { _rt: rt, ctx });
        }

        let engine = slot.as_ref().unwrap();
        f(&engine.ctx)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MermaidColors {
    pub bg: String,
    pub fg: String,
    pub line: String,
    pub accent: String,
    pub muted: String,
    pub surface: String,
    pub border: String,
}

impl MermaidColors {
    pub fn dark() -> Self {
        Self {
            bg: "#0f1117".into(),
            fg: "#f4f4f5".into(),
            line: "#9ca3af".into(),
            accent: "#22c55e".into(),
            muted: "#a1a1aa".into(),
            surface: "#18181b".into(),
            border: "#3f3f46".into(),
        }
    }

    pub fn light() -> Self {
        Self {
            bg: "#ffffff".into(),
            fg: "#18181b".into(),
            line: "#71717a".into(),
            accent: "#16a34a".into(),
            muted: "#71717a".into(),
            surface: "#f4f4f5".into(),
            border: "#e4e4e7".into(),
        }
    }

    pub fn from_theme(theme: &crate::theme::Theme) -> Self {
        let hex = |c: gpui::Hsla| -> String {
            let rgba = gpui::Rgba::from(c);
            format!(
                "#{:02x}{:02x}{:02x}",
                (rgba.r * 255.0).round() as u8,
                (rgba.g * 255.0).round() as u8,
                (rgba.b * 255.0).round() as u8
            )
        };

        if theme.appearance.is_dark() {
            Self {
                bg: hex(theme.bg),
                fg: hex(theme.text),
                line: hex(theme.border_strong),
                accent: hex(theme.accent),
                muted: hex(theme.text_muted),
                surface: hex(theme.surface_raised),
                border: hex(theme.border),
            }
        } else {
            Self {
                bg: hex(theme.bg),
                fg: hex(theme.text),
                line: hex(theme.border_strong),
                accent: hex(theme.accent),
                muted: hex(theme.text_muted),
                surface: hex(theme.surface),
                border: hex(theme.border),
            }
        }
    }
}

fn hex_mix(fg: &str, pct: f32, bg: &str) -> String {
    fn parse(c: &str) -> (u8, u8, u8) {
        let c = c.trim_start_matches('#');
        if c.len() < 6 {
            return (0, 0, 0);
        }
        (
            u8::from_str_radix(&c[0..2], 16).unwrap_or(0),
            u8::from_str_radix(&c[2..4], 16).unwrap_or(0),
            u8::from_str_radix(&c[4..6], 16).unwrap_or(0),
        )
    }
    let (fr, fg_, fb) = parse(fg);
    let (br, bg_, bb) = parse(bg);
    let m = |a: u8, b: u8| ((a as f32 * pct + b as f32 * (1.0 - pct)).round()) as u8;
    format!("#{:02x}{:02x}{:02x}", m(fr, br), m(fg_, bg_), m(fb, bb))
}

fn resolve_svg_vars(svg: &str, colors: &MermaidColors) -> String {
    let pairs: [(&str, &str); 18] = [
        ("_text", &colors.fg),
        ("_text-sec", &colors.muted),
        ("_text-muted", &colors.muted),
        ("_text-faint", &hex_mix(&colors.fg, 0.25, &colors.bg)),
        ("_line", &colors.line),
        ("_arrow", &colors.accent),
        ("_node-fill", &colors.surface),
        ("_node-stroke", &colors.border),
        ("_group-fill", &colors.bg),
        ("_group-hdr", &hex_mix(&colors.fg, 0.05, &colors.bg)),
        ("_inner-stroke", &hex_mix(&colors.fg, 0.12, &colors.bg)),
        ("_key-badge", &hex_mix(&colors.fg, 0.10, &colors.bg)),
        ("bg", &colors.bg),
        ("fg", &colors.fg),
        ("line", &colors.line),
        ("accent", &colors.accent),
        ("muted", &colors.muted),
        ("surface", &colors.surface),
    ];
    let mut out = svg.to_string();
    for (name, val) in pairs {
        out = out.replace(&format!("var(--{name})"), val);
        loop {
            let pat = format!("var(--{name},");
            let Some(start) = out.find(&pat) else { break };
            let Some(end) = out[start..].find(')') else {
                break;
            };
            out.replace_range(start..start + end + 1, val);
        }
    }
    out = out.replace("var(--border)", &colors.border);
    while let Some(start) = out.find("<style>") {
        let Some(end) = out.find("</style>") else {
            break;
        };
        out.replace_range(start..end + "</style>".len(), "");
    }
    out
}

fn parse_svg_dimensions(svg: &str) -> (f32, f32) {
    let width = svg
        .find("width=\"")
        .and_then(|pos| {
            let start = pos + 7;
            let end = svg[start..].find('"')?;
            svg[start..start + end].parse::<f32>().ok()
        })
        .unwrap_or(400.0);
    let height = svg
        .find("height=\"")
        .and_then(|pos| {
            let start = pos + 8;
            let end = svg[start..].find('"')?;
            svg[start..start + end].parse::<f32>().ok()
        })
        .unwrap_or(200.0);
    (width.max(1.0), height.max(1.0))
}

pub fn render_mermaid_svg(source: &str, colors: &MermaidColors) -> Result<RenderedMermaid, String> {
    let source = normalize_mermaid_source(source.trim());
    if source.is_empty() {
        return Err("diagram source is empty".into());
    }
    if source.len() > MAX_MERMAID_BYTES {
        return Err("diagram source is too large".into());
    }
    if source.lines().count() > MAX_MERMAID_LINES {
        return Err("diagram has too many lines".into());
    }

    let opts_json = serde_json::json!({
        "bg": colors.bg,
        "fg": colors.fg,
        "line": colors.line,
        "accent": colors.accent,
        "muted": colors.muted,
        "surface": colors.surface,
        "border": colors.border,
        "padding": MERMAID_FRAME_PADDING,
        "transparent": true,
    })
    .to_string();

    let raw_svg = with_mermaid_engine(|ctx| {
        ctx.with(|ctx| {
            let f: rquickjs::Function = ctx
                .globals()
                .get("__render")
                .map_err(|e| format!("failed to find render function: {e}"))?;

            f.call::<_, String>((source, opts_json.as_str()))
                .map_err(|e| {
                    if let rquickjs::Error::Exception = e {
                        let ex = ctx.catch();
                        let msg = ex
                            .as_exception()
                            .and_then(|x| x.message())
                            .unwrap_or_default();
                        format!("mermaid render error: {msg}")
                    } else {
                        format!("mermaid render error: {e}")
                    }
                })
        })
    })?;

    let svg = resolve_svg_vars(&raw_svg, colors);
    if svg.len() > MAX_MERMAID_SVG_BYTES {
        return Err("rendered diagram is too large".into());
    }

    let (width, height) = parse_svg_dimensions(&svg);
    let bytes = svg.len();

    Ok(RenderedMermaid {
        image: Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            svg.clone().into_bytes(),
        )),
        width,
        height,
        bytes,
        svg,
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
    fn image_loader_takes_absolute_paths_and_confines_relative_ones() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("artifacts")).unwrap();
        fs::write(root.path().join("artifacts/plot.png"), b"png").unwrap();
        fs::write(outside.path().join("secret.png"), b"secret").unwrap();

        let loaded = load_checkout_image(root.path(), "artifacts/plot.png").unwrap();
        assert_eq!(loaded.relative_path, "artifacts/plot.png");
        assert_eq!(loaded.name.as_ref(), "plot.png");

        assert!(load_checkout_image(root.path(), "../secret.png").is_err());

        // Fora do checkout, mas absoluto: o agente leu, a UI mostra.
        fs::write(outside.path().join("Screen Shot.png"), b"png").unwrap();
        let shot = load_checkout_image(
            root.path(),
            outside.path().join("Screen Shot.png").to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(shot.name.as_ref(), "Screen Shot.png");

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
        let dark = super::MermaidColors::dark();
        let light = super::MermaidColors::light();
        assert!(render_mermaid_svg("flowchart LR\nA --> B", &dark).is_ok());
        assert!(render_mermaid_svg("flowchart LR\nA --> B", &light).is_ok());
        assert!(
            render_mermaid_svg(
                "%%{init: {'flowchart': {'curve': 'linear'}}}%%\nflowchart LR\nA --> B",
                &dark,
            )
            .is_ok()
        );
        assert!(render_mermaid_svg("%% generated diagram\nflowchart LR\nA --> B", &dark).is_ok());
        assert!(
            render_mermaid_svg("---\ntitle: Example\n---\nflowchart LR\nA --> B", &dark).is_ok()
        );
        assert!(render_mermaid_svg("notMermaid\nA --> B", &dark).is_err());
        assert!(render_mermaid_svg("   ", &dark).is_err());
        let oversized = format!("flowchart LR\n{}", "A --> B\n".repeat(10_000));
        assert!(render_mermaid_svg(&oversized, &dark).is_err());
    }
    #[test]
    fn mermaid_renderer_computes_diagram_dimensions() {
        let dark = super::MermaidColors::dark();
        let simple = render_mermaid_svg("flowchart LR\nA --> B", &dark).unwrap();
        assert!(simple.width > 0.0);
        assert!(simple.height > 0.0);
        assert!(
            !simple.svg.contains("var(--"),
            "all CSS vars must be resolved"
        );

        let wide = render_mermaid_svg(
            "flowchart LR\ncreate --> prepare --> attempt --> bind --> session --> submit --> verify --> close\nverify --> reject",
            &dark,
        )
        .unwrap();
        assert!(wide.width > simple.width);
        assert!(wide.height > 0.0);
    }
    #[test]
    fn frontmatter_and_comments_are_normalized() {
        let raw = "---\ntitle: Sample\n---\n%% comment\nflowchart LR\nA --> B";
        assert_eq!(
            super::strip_mermaid_frontmatter(raw),
            "%% comment\nflowchart LR\nA --> B"
        );
        assert_eq!(
            super::normalize_mermaid_source(raw),
            "flowchart LR\nA --> B"
        );
    }
    #[test]
    fn render_vision_flowchart() {
        let dark = super::MermaidColors::dark();
        let source = r#"flowchart TD
  User["Usuario abre o Vision"] --> Home["Home visual grade de icones honeycomb"]
  Close["Electron e fechado"] --> Disconnect["Agent bridge desmontado tarefa nao continua"]
  Home --> Worker["Worker"]
  Home --> Other["Outros icones Settings, Photos"]
  Disconnect --> Chat["Chat global decorativo"]
  Worker --> Card["WorkerCard"]
  Card --> Decision{"Worker ja existe?"}
  Card --> Controls["Controles tecnicos start, stop"]
  Decision -->|Nao| Create["Cria automaticamente Vision Worker"]
  Decision -->|Sim| Desktop["Desktop Linux local Docker + noVNC"]
  Create --> Desktop"#;
        let res = render_mermaid_svg(source, &dark).unwrap();
        std::fs::write("/tmp/vision.svg", res.svg.as_bytes()).unwrap();
        assert!(res.width > 0.0);
        assert!(res.height > 0.0);
    }
}
