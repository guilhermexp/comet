//! Linux capture of this process's OWN top-level window, never the desktop:
//! the X11 window whose `_NET_WM_PID` is this process (GPUI's X11 backend
//! sets it), the largest viewable one, written with ImageMagick's
//! `import -window <id>`. Any other display backend (Wayland, no display)
//! fails explicitly: unrelated desktop pixels are never fixture evidence.

use std::path::Path;

use x11rb::errors::ReplyError;
use x11rb::protocol::ErrorKind;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, MapState, Window};

/// Captures this process's own window into `path` and returns its X11 id.
pub fn capture_own_window(path: &Path) -> anyhow::Result<Window> {
    use x11rb::connection::Connection;

    anyhow::ensure!(
        std::env::var_os("DISPLAY").is_some(),
        "own-window capture needs an X11 display; this backend (Wayland or none) is unsupported"
    );
    let (conn, screen) = x11rb::connect(None)?;
    let root = conn.setup().roots[screen].root;
    // The root itself never vanishes: any error here is real.
    let children = conn.query_tree(root)?.reply()?.children;
    let pid = std::process::id();
    let window = find_own_window(&conn, children, pid)?
        .ok_or_else(|| anyhow::anyhow!("no viewable X11 window belongs to this process ({pid})"))?;
    let status = std::process::Command::new("import")
        .args(["-window", &format!("0x{window:x}")])
        .arg(path)
        .status()?;
    anyhow::ensure!(status.success(), "import of own window 0x{window:x} failed");
    Ok(window)
}

/// The largest viewable window declaring `_NET_WM_PID == pid` among
/// `candidates` and their descendants. A candidate that vanished while the
/// tree was walked (`BadWindow` for window requests, `BadDrawable` for
/// geometry, typically an unrelated window closing) is skipped; every other error, including connection errors,
/// fails the lookup.
pub fn find_own_window(
    conn: &impl ConnectionExt,
    candidates: Vec<Window>,
    pid: u32,
) -> anyhow::Result<Option<Window>> {
    let pid_atom = conn.intern_atom(true, b"_NET_WM_PID")?.reply()?.atom;
    anyhow::ensure!(
        pid_atom != x11rb::NONE,
        "no X11 window declares _NET_WM_PID"
    );
    let mut own: Option<(Window, u32)> = None;
    let mut pending = candidates;
    while let Some(window) = pending.pop() {
        let Some(tree) = unless_vanished(conn.query_tree(window)?.reply(), ErrorKind::Window)?
        else {
            continue;
        };
        pending.extend(tree.children);
        let Some(property) = unless_vanished(
            conn.get_property(false, window, pid_atom, AtomEnum::CARDINAL, 0, 1)?
                .reply(),
            ErrorKind::Window,
        )?
        else {
            continue;
        };
        let declared = property.value32().and_then(|mut values| values.next());
        if declared != Some(pid) {
            continue;
        }
        let Some(attributes) = unless_vanished(
            conn.get_window_attributes(window)?.reply(),
            ErrorKind::Window,
        )?
        else {
            continue;
        };
        if attributes.map_state != MapState::VIEWABLE {
            continue;
        }
        let Some(geometry) =
            unless_vanished(conn.get_geometry(window)?.reply(), ErrorKind::Drawable)?
        else {
            continue;
        };
        let area = u32::from(geometry.width) * u32::from(geometry.height);
        if own.is_none_or(|(_, best)| area > best) {
            own = Some((window, area));
        }
    }
    Ok(own.map(|(window, _)| window))
}

/// `None` only for this request's typed disappearance error.
fn unless_vanished<T>(
    reply: Result<T, ReplyError>,
    vanished: ErrorKind,
) -> Result<Option<T>, ReplyError> {
    match reply {
        Ok(value) => Ok(Some(value)),
        Err(ReplyError::X11Error(error)) if error.error_kind == vanished => Ok(None),
        Err(error) => Err(error),
    }
}
