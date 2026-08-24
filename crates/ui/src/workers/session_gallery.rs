use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use chrono::Local;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    Area,
    Window,
    FullScreen,
}

impl CaptureMode {
    pub const ALL: [Self; 3] = [Self::Area, Self::Window, Self::FullScreen];

    pub fn label(self) -> &'static str {
        match self {
            Self::Area => "Capture area",
            Self::Window => "Capture window",
            Self::FullScreen => "Capture full screen",
        }
    }

    #[cfg(target_os = "macos")]
    fn fallback_svg(self) -> &'static [u8] {
        match self {
            Self::Area => br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect x="2" y="3" width="14" height="12" rx="1.5" fill="none" stroke="#000" stroke-width="1.5" stroke-dasharray="2 2"/></svg>"##,
            Self::Window => br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect x="2" y="3" width="14" height="12" rx="2" fill="none" stroke="#000" stroke-width="1.5"/><path d="M2.5 6h13" fill="none" stroke="#000" stroke-width="1.5"/></svg>"##,
            Self::FullScreen => br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18"><rect x="2" y="3" width="14" height="10" rx="1.5" fill="none" stroke="#000" stroke-width="1.5"/><path d="M6 16h6M9 13v3" fill="none" stroke="#000" stroke-width="1.5" stroke-linecap="round"/></svg>"##,
        }
    }

    pub fn arguments(self, destination: &Path) -> Vec<String> {
        let destination = destination.to_string_lossy().into_owned();
        match self {
            Self::Area => vec!["-i".into(), destination],
            Self::Window => vec!["-i".into(), "-W".into(), "-o".into(), destination],
            Self::FullScreen => vec![destination],
        }
    }
}

/// What `capture_screenshot` reports when the user dismisses macOS's own
/// capture UI. Dismissing is not a failure, so callers stay silent.
pub const CAPTURE_CANCELLED: &str = "Screenshot capture was cancelled";

/// The whole capture gesture: the native mode menu, then the screenshot on a
/// background thread. `Ok(None)` means the user cancelled - the menu or the
/// capture itself. Both the orchestrator titlebar and the Workers gallery
/// drive this; the gesture used to be written once per caller, cancel-string
/// comparison included.
pub async fn pick_and_capture(
    directory: PathBuf,
    executor: gpui::BackgroundExecutor,
) -> Result<Option<PathBuf>, String> {
    #[cfg(target_os = "macos")]
    {
        let Ok(Some(mode)) = native::show_async().await else {
            return Ok(None);
        };
        match executor
            .spawn(async move { capture_screenshot(&directory, mode) })
            .await
        {
            Ok(path) => Ok(Some(path)),
            Err(error) if error == CAPTURE_CANCELLED => Ok(None),
            Err(error) => Err(error),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (directory, executor);
        Ok(None)
    }
}

pub fn capture_screenshot(directory: &Path, mode: CaptureMode) -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    if !screen_recording_access() {
        let _ = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn();
        return Err("Screen Recording permission is required to capture screenshots".into());
    }

    std::fs::create_dir_all(directory)
        .map_err(|error| format!("failed to create screenshot directory: {error}"))?;
    let destination = directory.join(format!(
        "screenshot-{}.png",
        Local::now().format("%Y-%m-%d-%H%M%S")
    ));

    if mode == CaptureMode::FullScreen {
        thread::sleep(Duration::from_millis(400));
    }

    let status = Command::new("/usr/sbin/screencapture")
        .args(mode.arguments(&destination))
        .status()
        .map_err(|error| format!("failed to launch screencapture: {error}"))?;
    if !status.success() {
        return Err(if status.code() == Some(1) {
            CAPTURE_CANCELLED.into()
        } else {
            format!("screencapture exited with status {status}")
        });
    }
    if !destination.is_file() {
        return Err("Screenshot capture produced no file".into());
    }
    Ok(destination)
}

#[cfg(target_os = "macos")]
fn screen_recording_access() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    unsafe { CGPreflightScreenCaptureAccess() || CGRequestScreenCaptureAccess() }
}

pub fn shell_quote_path(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
pub mod native {
    use std::ffi::{CString, c_void};
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicI64, Ordering};

    use futures::channel::oneshot;
    use objc::declare::ClassDecl;
    use objc::runtime::{Object, Sel};
    use objc::{class, msg_send, sel, sel_impl};

    use super::CaptureMode;

    static SELECTED_TAG: AtomicI64 = AtomicI64::new(-1);
    type CGFloat = f64;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSPoint {
        x: CGFloat,
        y: CGFloat,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSSize {
        width: CGFloat,
        height: CGFloat,
    }

    struct MenuContext {
        menu: *mut Object,
        event: *mut Object,
        view: *mut Object,
        sender: oneshot::Sender<Option<CaptureMode>>,
    }

    unsafe extern "C" {
        #[link_name = "_dispatch_main_q"]
        static DISPATCH_MAIN_QUEUE: c_void;
        fn dispatch_async_f(
            queue: *mut c_void,
            context: *mut c_void,
            work: extern "C" fn(*mut c_void),
        );
    }

    extern "C" fn perform_action(_this: &Object, _cmd: Sel, sender: *mut Object) {
        let tag: i64 = unsafe { msg_send![sender, tag] };
        SELECTED_TAG.store(tag, Ordering::SeqCst);
    }

    extern "C" fn pop_up_menu(context: *mut c_void) {
        let context = unsafe { Box::from_raw(context.cast::<MenuContext>()) };
        unsafe {
            let window_location: NSPoint = msg_send![context.event, locationInWindow];
            let view_location: NSPoint = msg_send![context.view,
                convertPoint: window_location
                fromView: std::ptr::null_mut::<Object>()
            ];
            let menu_size: NSSize = msg_send![context.menu, size];
            let origin = NSPoint {
                x: view_location.x + 10.0 - menu_size.width,
                y: view_location.y - 4.0,
            };
            let _: bool = msg_send![context.menu,
                popUpMenuPositioningItem: std::ptr::null_mut::<Object>()
                atLocation: origin
                inView: context.view
            ];
            let _: () = msg_send![context.menu, release];
            let _: () = msg_send![context.event, release];
            let _: () = msg_send![context.view, release];
        }
        let selection = match SELECTED_TAG.swap(-1, Ordering::SeqCst) {
            0 => Some(CaptureMode::Area),
            1 => Some(CaptureMode::Window),
            2 => Some(CaptureMode::FullScreen),
            _ => None,
        };
        let _ = context.sender.send(selection);
    }

    fn action_target() -> *mut Object {
        static TARGET: OnceLock<usize> = OnceLock::new();
        *TARGET.get_or_init(|| unsafe {
            let class = if let Some(mut declaration) =
                ClassDecl::new("ZeronWorkersScreenshotMenuTarget", class!(NSObject))
            {
                declaration.add_method(
                    sel!(performAction:),
                    perform_action as extern "C" fn(&Object, Sel, *mut Object),
                );
                declaration.register()
            } else {
                objc::runtime::Class::get("ZeronWorkersScreenshotMenuTarget")
                    .expect("registered workers screenshot menu target")
            };
            let target: *mut Object = msg_send![class, new];
            target as usize
        }) as *mut Object
    }

    fn ns_string(value: &str) -> *mut Object {
        let value = CString::new(value).expect("menu labels cannot contain null bytes");
        unsafe { msg_send![class!(NSString), stringWithUTF8String: value.as_ptr()] }
    }

    unsafe fn add_action(menu: *mut Object, mode: CaptureMode, tag: i64) {
        let item: *mut Object = msg_send![class!(NSMenuItem), alloc];
        let item: *mut Object = msg_send![item,
            initWithTitle: ns_string(mode.label())
            action: sel!(performAction:)
            keyEquivalent: ns_string("")
        ];
        let _: () = msg_send![item, setTarget: action_target()];
        let _: () = msg_send![item, setTag: tag];
        let _: () = msg_send![item, setEnabled: true];
        let svg = mode.fallback_svg();
        let data: *mut Object = msg_send![class!(NSData),
            dataWithBytes: svg.as_ptr()
            length: svg.len()
        ];
        let fallback_image: *mut Object = msg_send![class!(NSImage), alloc];
        let fallback_image: *mut Object = msg_send![fallback_image, initWithData: data];
        let image = fallback_image;
        if !image.is_null() {
            let _: () = msg_send![image, setSize: NSSize { width: 14.0, height: 14.0 }];
            let _: () = msg_send![image, setTemplate: true];
            let _: () = msg_send![item, setImage: image];
        }
        if !fallback_image.is_null() {
            let _: () = msg_send![fallback_image, release];
        }
        let _: () = msg_send![menu, addItem: item];
        let _: () = msg_send![item, release];
    }

    pub fn show_async() -> oneshot::Receiver<Option<CaptureMode>> {
        let (sender, receiver) = oneshot::channel();
        unsafe {
            SELECTED_TAG.store(-1, Ordering::SeqCst);
            let menu: *mut Object = msg_send![class!(NSMenu), alloc];
            let menu: *mut Object = msg_send![menu, initWithTitle: ns_string("")];
            let _: () = msg_send![menu, setAutoenablesItems: false];
            for (tag, mode) in CaptureMode::ALL.into_iter().enumerate() {
                add_action(menu, mode, tag as i64);
            }

            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let event: *mut Object = msg_send![app, currentEvent];
            let event: *mut Object = msg_send![event, retain];
            let window: *mut Object = msg_send![event, window];
            let view: *mut Object = msg_send![window, contentView];
            let view: *mut Object = msg_send![view, retain];
            let context = Box::new(MenuContext {
                menu,
                event,
                view,
                sender,
            });
            dispatch_async_f(
                std::ptr::addr_of!(DISPATCH_MAIN_QUEUE).cast_mut(),
                Box::into_raw(context).cast(),
                pop_up_menu,
            );
        }
        receiver
    }
}

#[cfg(test)]
mod tests {
    use super::{CaptureMode, shell_quote_path};
    use std::path::Path;

    #[test]
    fn capture_modes_match_unpeel_screencapture_arguments() {
        let destination = Path::new("/tmp/session screenshot.png");

        assert_eq!(
            CaptureMode::Area.arguments(destination),
            vec!["-i", "/tmp/session screenshot.png"]
        );
        assert_eq!(
            CaptureMode::Window.arguments(destination),
            vec!["-i", "-W", "-o", "/tmp/session screenshot.png"]
        );
        assert_eq!(
            CaptureMode::FullScreen.arguments(destination),
            vec!["/tmp/session screenshot.png"]
        );
    }

    #[test]
    fn shell_quote_path_preserves_spaces_and_single_quotes() {
        assert_eq!(
            shell_quote_path(Path::new("/tmp/Guilherme's screenshot.png")),
            "'/tmp/Guilherme'\\''s screenshot.png'"
        );
    }
}
