//! Site favicons ahead of web links in rendered Markdown (the Codex transcript
//! look). Rendering asks [`favicon_for`] synchronously; a miss queues one
//! background fetch per host and the icon appears on the next frame after it
//! lands (`refresh_windows`). Icons come straight from the site — never from a
//! third-party favicon service, which would learn every domain in the chat.
//!
//! Resolution order per host: `/favicon.ico`, then the page's
//! `<link rel="icon">`, then the parent domain once. Results (including
//! misses) persist under `~/.zeron/cache/favicons`, so relaunches cost no
//! network and a dead host is not retried on every frame.

use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

use futures::StreamExt as _;
use gpui::{App, Image, ImageFormat};

const MAX_BYTES: usize = 512 * 1024;
const MISS_TTL: Duration = Duration::from_secs(3 * 24 * 60 * 60);
const ICON_PX: u32 = 32;

#[derive(Clone)]
enum Entry {
    Pending,
    Ready(Icon),
    Missing,
}

/// A loaded favicon plus, for a monochrome mark, its inverted twin: a black
/// GitHub mark vanishes on the dark transcript (and a white one on light),
/// so the appearance it would disappear on draws the flipped copy instead.
#[derive(Clone)]
struct Icon {
    image: Arc<Image>,
    flipped: Option<Arc<Image>>,
    /// `true`: the mark is dark (flip on dark themes); `false`: light.
    flip_on_dark: bool,
}

/// Rasterized, classified icon ready for the UI thread.
struct Prepared {
    png: Vec<u8>,
    flipped: Option<(Vec<u8>, bool)>,
}

thread_local! {
    static CACHE: RefCell<HashMap<String, Entry>> = RefCell::default();
    static REQUESTS: RefCell<Option<futures::channel::mpsc::UnboundedSender<String>>> =
        const { RefCell::new(None) };
}

/// Start the fetch loop. Without it (tests, fixtures) links render with the
/// fallback globe and nothing touches the network.
pub fn init(cx: &mut App) {
    let (tx, mut rx) = futures::channel::mpsc::unbounded::<String>();
    REQUESTS.with(|requests| *requests.borrow_mut() = Some(tx));
    cx.spawn(async move |cx| {
        while let Some(host) = rx.next().await {
            let download = cx.update(|cx| {
                let host = host.clone();
                gpui_tokio::Tokio::spawn(cx, async move {
                    let (format, bytes) = resolve(&host).await?;
                    prepare(format, &bytes)
                })
            });
            cx.spawn(async move |cx| {
                let icon = download.await.ok().flatten();
                cx.update(|cx| {
                    let entry = match icon {
                        Some(prepared) => {
                            let png = |bytes| Arc::new(Image::from_bytes(ImageFormat::Png, bytes));
                            let (flipped, flip_on_dark) = match prepared.flipped {
                                Some((bytes, dark)) => (Some(png(bytes)), dark),
                                None => (None, false),
                            };
                            Entry::Ready(Icon {
                                image: png(prepared.png),
                                flipped,
                                flip_on_dark,
                            })
                        }
                        None => Entry::Missing,
                    };
                    CACHE.with(|cache| cache.borrow_mut().insert(host, entry));
                    cx.refresh_windows();
                });
            })
            .detach();
        }
    })
    .detach();
}

/// The favicon for `url`'s site, if already loaded, in the variant that stays
/// visible on the current appearance. A first miss schedules the fetch;
/// `None` then means "draw the fallback".
pub fn favicon_for(url: &str, dark: bool) -> Option<Arc<Image>> {
    let host = fetchable_host(url)?;
    let hit = CACHE.with(|cache| cache.borrow().get(&host).cloned());
    match hit {
        Some(Entry::Ready(icon)) => {
            return Some(match icon.flipped {
                Some(flipped) if icon.flip_on_dark == dark => flipped,
                _ => icon.image,
            });
        }
        Some(Entry::Pending | Entry::Missing) => return None,
        None => {}
    }
    let queued = REQUESTS.with(|requests| {
        requests
            .borrow()
            .as_ref()
            .is_some_and(|tx| tx.unbounded_send(host.clone()).is_ok())
    });
    if queued {
        CACHE.with(|cache| cache.borrow_mut().insert(host, Entry::Pending));
    }
    None
}

/// Public web hosts only: loopback, private-network, `.local` and bare-IP
/// destinations are never contacted just because an agent mentioned them.
fn fetchable_host(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = match parsed.host()? {
        url::Host::Domain(domain) => domain.trim_end_matches('.').to_ascii_lowercase(),
        url::Host::Ipv4(_) | url::Host::Ipv6(_) => return None,
    };
    if !host.contains('.')
        || host == "localhost"
        || [".localhost", ".local", ".internal", ".lan", ".home.arpa"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
    {
        return None;
    }
    Some(host)
}

fn cache_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".zeron/cache/favicons"))
}

fn cache_file(host: &str, extension: &str) -> Option<PathBuf> {
    let safe: String = host
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        .collect();
    Some(cache_dir()?.join(format!("{safe}.{extension}")))
}

fn read_cached(host: &str) -> Option<Option<(ImageFormat, Vec<u8>)>> {
    for (extension, format) in [("png", ImageFormat::Png), ("svg", ImageFormat::Svg)] {
        if let Some(bytes) = cache_file(host, extension).and_then(|p| std::fs::read(p).ok()) {
            return Some(Some((format, bytes)));
        }
    }
    let miss = cache_file(host, "none")?;
    let age = std::fs::metadata(&miss)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|at| SystemTime::now().duration_since(at).ok())?;
    (age < MISS_TTL).then_some(None)
}

fn write_cached(host: &str, icon: Option<&(ImageFormat, Vec<u8>)>) {
    let Some(dir) = cache_dir() else { return };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let (extension, bytes): (&str, &[u8]) = match icon {
        Some((ImageFormat::Svg, bytes)) => ("svg", bytes),
        Some((_, bytes)) => ("png", bytes),
        None => ("none", b""),
    };
    if let Some(path) = cache_file(host, extension) {
        let _ = std::fs::write(path, bytes);
    }
}

async fn resolve(host: &str) -> Option<(ImageFormat, Vec<u8>)> {
    if let Some(cached) = read_cached(host) {
        return cached;
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::limited(3))
        .user_agent("Mozilla/5.0 (Macintosh) zeron-favicon")
        .build()
        .ok()?;
    let mut icon = fetch_site(&client, host).await;
    if icon.is_none()
        && let Some(parent) = parent_domain(host)
    {
        icon = fetch_site(&client, &parent).await;
    }
    write_cached(host, icon.as_ref());
    icon
}

/// `docs.example.com` → `example.com`; a two-label host has no parent.
fn parent_domain(host: &str) -> Option<String> {
    let (_, rest) = host.split_once('.')?;
    rest.contains('.').then(|| rest.to_owned())
}

async fn fetch_site(client: &reqwest::Client, host: &str) -> Option<(ImageFormat, Vec<u8>)> {
    let root = format!("https://{host}/");
    if let Some(bytes) = fetch(client, &format!("{root}favicon.ico")).await
        && let Some(icon) = decode(&bytes)
    {
        return Some(icon);
    }
    let html = fetch(client, &root).await?;
    let html = String::from_utf8_lossy(&html);
    let base = url::Url::parse(&root).ok()?;
    for href in icon_links(&html) {
        let Ok(target) = base.join(&href) else {
            continue;
        };
        if !matches!(target.scheme(), "http" | "https") {
            continue;
        }
        if let Some(bytes) = fetch(client, target.as_str()).await
            && let Some(icon) = decode(&bytes)
        {
            return Some(icon);
        }
    }
    None
}

async fn fetch(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let mut response = client.get(url).send().await.ok()?.error_for_status().ok()?;
    if response
        .content_length()
        .is_some_and(|n| n as usize > MAX_BYTES)
    {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > MAX_BYTES {
            // HTML heads fit well inside the cap; keep what we have for the
            // `<link>` scan, but an icon this large is not an icon.
            break;
        }
        bytes.extend_from_slice(&chunk);
    }
    Some(bytes)
}

/// Raster icons are normalized to a 32px PNG; SVG passes through for gpui.
fn decode(bytes: &[u8]) -> Option<(ImageFormat, Vec<u8>)> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]);
    let head = head.trim_start();
    if head.starts_with("<svg") || (head.starts_with("<?xml") && head.contains("<svg")) {
        return Some((ImageFormat::Svg, bytes.to_vec()));
    }
    if head.starts_with('<') {
        // An HTML error page served with 200.
        return None;
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(1024);
    limits.max_alloc = Some(8 * 1024 * 1024);
    reader.limits(limits);
    let icon = reader.decode().ok()?.thumbnail(ICON_PX, ICON_PX);
    let mut png = std::io::Cursor::new(Vec::new());
    icon.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some((ImageFormat::Png, png.into_inner()))
}

/// Rasterize (SVG) or decode to a 32px RGBA icon, then classify it: a mark
/// whose visible pixels are all near-black or near-white and colourless gets
/// an inverted twin for the appearance it would disappear on.
fn prepare(format: ImageFormat, bytes: &[u8]) -> Option<Prepared> {
    let rgba = match format {
        ImageFormat::Svg => rasterize_svg(bytes)?,
        _ => image::load_from_memory(bytes)
            .ok()?
            .thumbnail(ICON_PX, ICON_PX)
            .to_rgba8(),
    };
    let flipped = monochrome_tone(&rgba).map(|dark| {
        let mut inverted = rgba.clone();
        for pixel in inverted.pixels_mut() {
            let [r, g, b, a] = pixel.0;
            pixel.0 = [255 - r, 255 - g, 255 - b, a];
        }
        (encode_png(&inverted), dark)
    });
    let flipped = match flipped {
        Some((Some(bytes), dark)) => Some((bytes, dark)),
        _ => None,
    };
    Some(Prepared {
        png: encode_png(&rgba)?,
        flipped,
    })
}

fn rasterize_svg(bytes: &[u8]) -> Option<image::RgbaImage> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = ICON_PX as f32 / size.width().max(size.height()).max(1.0);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(ICON_PX, ICON_PX)?;
    let dx = (ICON_PX as f32 - size.width() * scale) / 2.0;
    let dy = (ICON_PX as f32 - size.height() * scale) / 2.0;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, dx, dy),
        &mut pixmap.as_mut(),
    );
    let mut rgba = Vec::with_capacity((ICON_PX * ICON_PX * 4) as usize);
    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        rgba.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
    }
    image::RgbaImage::from_raw(ICON_PX, ICON_PX, rgba)
}

fn encode_png(rgba: &image::RgbaImage) -> Option<Vec<u8>> {
    let mut png = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(png.into_inner())
}

/// `Some(true)` for a dark monochrome mark, `Some(false)` for a light one,
/// `None` for anything with colour, mid-tones, or an opaque backdrop (an
/// icon that paints its own tile is visible on every theme).
fn monochrome_tone(rgba: &image::RgbaImage) -> Option<bool> {
    let (mut visible, mut opaque_px, mut dark, mut light) = (0u32, 0u32, 0u32, 0u32);
    for pixel in rgba.pixels() {
        let [r, g, b, a] = pixel.0;
        if a < 48 {
            continue;
        }
        visible += 1;
        if a > 240 {
            opaque_px += 1;
        }
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        if max - min > 40 {
            return None;
        }
        let luma = (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000;
        if luma < 90 {
            dark += 1;
        } else if luma > 200 {
            light += 1;
        }
    }
    let total = rgba.width() * rgba.height();
    // Nothing to see, or the icon fills its square (its own background).
    if visible == 0 || opaque_px * 10 > total * 9 {
        return None;
    }
    if dark * 10 >= visible * 9 {
        Some(true)
    } else if light * 10 >= visible * 9 {
        Some(false)
    } else {
        None
    }
}

/// `href`s of `<link rel="…icon…">` tags, `icon` before `apple-touch-icon`.
fn icon_links(html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut found: Vec<(u8, String)> = Vec::new();
    let mut at = 0;
    while let Some(rel) = lower[at..].find("<link") {
        let start = at + rel;
        let Some(len) = lower[start..].find('>') else {
            break;
        };
        let tag = &html[start..start + len];
        let tag_lower = &lower[start..start + len];
        at = start + len;
        let Some(rel) = attribute(tag, tag_lower, "rel") else {
            continue;
        };
        let rel = rel.to_ascii_lowercase();
        if !rel.split_whitespace().any(|word| word.contains("icon")) {
            continue;
        }
        if let Some(href) = attribute(tag, tag_lower, "href") {
            let rank = if rel.contains("apple") { 1 } else { 0 };
            found.push((rank, href.replace("&amp;", "&")));
        }
    }
    found.sort_by_key(|(rank, _)| *rank);
    found.into_iter().map(|(_, href)| href).collect()
}

fn attribute(tag: &str, tag_lower: &str, name: &str) -> Option<String> {
    let mut from = 0;
    while let Some(ix) = tag_lower[from..].find(name) {
        let at = from + ix;
        from = at + name.len();
        let boundary = tag_lower[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace);
        let rest = tag_lower[from..].trim_start();
        if !boundary || !rest.starts_with('=') {
            continue;
        }
        let value_at = tag.len() - rest.len() + 1;
        let value = tag[value_at..].trim_start();
        return Some(match value.chars().next()? {
            quote @ ('"' | '\'') => value[1..].split(quote).next()?.to_owned(),
            _ => value
                .split(char::is_whitespace)
                .next()?
                .trim_end_matches('/')
                .to_owned(),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_public_web_hosts_are_fetched() {
        assert_eq!(
            fetchable_host("https://Learn.Omacom.io/2/the-omarchy-manual").as_deref(),
            Some("learn.omacom.io")
        );
        for url in [
            "http://localhost:3000/x",
            "http://127.0.0.1/x",
            "http://[::1]/x",
            "http://router.local/",
            "http://intranet/",
            "file:///etc/hosts",
            "mailto:a@b.co",
            "zeron:pending-link",
        ] {
            assert_eq!(fetchable_host(url), None, "{url}");
        }
    }

    #[test]
    fn icon_links_prefer_plain_icons_and_resolve_quotes() {
        let html = r#"<head>
            <link rel="apple-touch-icon" href="/apple.png">
            <link href='/static/fav.svg?v=2&amp;x=1' rel='icon' type="image/svg+xml">
            <link rel=stylesheet href=/a.css>
            <LINK REL="shortcut icon" HREF=/favicon-32.png>
        </head>"#;
        assert_eq!(
            icon_links(html),
            vec![
                "/static/fav.svg?v=2&x=1".to_owned(),
                "/favicon-32.png".to_owned(),
                "/apple.png".to_owned(),
            ]
        );
    }

    #[test]
    fn parent_domain_stops_at_the_registrable_pair() {
        assert_eq!(
            parent_domain("docs.example.com").as_deref(),
            Some("example.com")
        );
        assert_eq!(parent_domain("example.com"), None);
    }

    #[test]
    fn monochrome_marks_get_a_flipped_twin_for_the_theme_they_vanish_on() {
        let mark = |fill: [u8; 4]| {
            let mut img = image::RgbaImage::new(ICON_PX, ICON_PX);
            for (x, y, pixel) in img.enumerate_pixels_mut() {
                if (8..24).contains(&x) && (8..24).contains(&y) {
                    pixel.0 = fill;
                }
            }
            img
        };
        assert_eq!(monochrome_tone(&mark([24, 23, 23, 255])), Some(true));
        assert_eq!(monochrome_tone(&mark([250, 250, 250, 255])), Some(false));
        assert_eq!(monochrome_tone(&mark([30, 200, 90, 255])), None);
        // A full opaque tile carries its own background: leave it alone.
        let tile = image::RgbaImage::from_pixel(ICON_PX, ICON_PX, image::Rgba([0, 0, 0, 255]));
        assert_eq!(monochrome_tone(&tile), None);

        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><circle cx="8" cy="8" r="6"/></svg>"#;
        let prepared = prepare(ImageFormat::Svg, svg).expect("svg rasterizes");
        assert!(matches!(prepared.flipped, Some((_, true))));
    }

    #[test]
    fn html_error_pages_are_not_icons() {
        assert!(decode(b"<!doctype html><html></html>").is_none());
        assert!(matches!(
            decode(br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#),
            Some((ImageFormat::Svg, _))
        ));
    }
}
