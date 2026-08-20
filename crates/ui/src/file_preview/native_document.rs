use std::{ffi::CString, path::Path};

use objc::{class, msg_send, runtime::Object, sel, sel_impl};

#[repr(C)]
#[derive(Clone, Copy)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NSSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}

pub fn appkit_frame(
    x: f64,
    top: f64,
    width: f64,
    height: f64,
    content_height: f64,
) -> (f64, f64, f64, f64) {
    (x, content_height - top - height, width, height)
}

unsafe fn ns_string(value: &str) -> *mut Object {
    let value = CString::new(value).expect("path contains no NUL");
    msg_send![class!(NSString), stringWithUTF8String: value.as_ptr()]
}

pub struct NativeDocumentView {
    view: *mut Object,
}

impl NativeDocumentView {
    pub fn open(path: &Path, read_root: &Path) -> Option<Self> {
        unsafe {
            let configuration: *mut Object = msg_send![class!(WKWebViewConfiguration), new];
            let view: *mut Object = msg_send![class!(WKWebView), alloc];
            let zero = NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: 1.0,
                    height: 1.0,
                },
            };
            let view: *mut Object =
                msg_send![view, initWithFrame: zero configuration: configuration];
            let _: () = msg_send![configuration, release];
            if view.is_null() {
                return None;
            }
            let path_string = ns_string(path.to_string_lossy().as_ref());
            let root_string = ns_string(read_root.to_string_lossy().as_ref());
            let url: *mut Object = msg_send![class!(NSURL), fileURLWithPath: path_string];
            let root_url: *mut Object = msg_send![class!(NSURL), fileURLWithPath: root_string];
            let _: *mut Object =
                msg_send![view, loadFileURL: url allowingReadAccessToURL: root_url];
            Some(Self { view })
        }
    }

    pub fn attach_and_layout(
        &mut self,
        x: f64,
        top: f64,
        width: f64,
        height: f64,
        content_height: f64,
    ) {
        unsafe {
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let window: *mut Object = msg_send![app, keyWindow];
            if window.is_null() || self.view.is_null() {
                return;
            }
            let content: *mut Object = msg_send![window, contentView];
            let superview: *mut Object = msg_send![self.view, superview];
            if superview.is_null() {
                let _: () = msg_send![content, addSubview: self.view];
            }
            let (x, y, width, height) = appkit_frame(x, top, width, height, content_height);
            let frame = NSRect {
                origin: NSPoint { x, y },
                size: NSSize { width, height },
            };
            let _: () = msg_send![self.view, setFrame: frame];
        }
    }

    pub fn hide(&mut self) {
        unsafe {
            if !self.view.is_null() {
                let _: () = msg_send![self.view, removeFromSuperview];
            }
        }
    }
}

impl Drop for NativeDocumentView {
    fn drop(&mut self) {
        unsafe {
            if !self.view.is_null() {
                let _: () = msg_send![self.view, removeFromSuperview];
                let _: () = msg_send![self.view, release];
                self.view = std::ptr::null_mut();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::appkit_frame;

    #[test]
    fn converts_gpui_top_left_bounds_to_appkit_bottom_left_frame() {
        assert_eq!(
            appkit_frame(20.0, 100.0, 640.0, 480.0, 900.0),
            (20.0, 320.0, 640.0, 480.0)
        );
    }
}
