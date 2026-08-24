use std::{
    ffi::{CStr, CString, c_void},
    path::Path,
    sync::OnceLock,
};

use block2::Block;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{BOOL, Class, NO, Object, Sel, YES},
    sel, sel_impl,
};

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
    navigation_delegate: *mut Object,
}

fn navigation_policy(
    scheme: Option<&str>,
    allow_file: bool,
    file_navigation_used: bool,
) -> (isize, bool) {
    match scheme {
        Some("about") => (1, file_navigation_used),
        Some("file") if allow_file && !file_navigation_used => (1, true),
        _ => (0, file_navigation_used),
    }
}

extern "C" fn decide_navigation(
    this: &mut Object,
    _selector: Sel,
    _view: *mut Object,
    action: *mut Object,
    decision_handler: *mut c_void,
) {
    unsafe {
        let request: *mut Object = msg_send![action, request];
        let url: *mut Object = msg_send![request, URL];
        let scheme: *mut Object = msg_send![url, scheme];
        let scheme = if scheme.is_null() {
            None
        } else {
            let value: *const std::ffi::c_char = msg_send![scheme, UTF8String];
            (!value.is_null()).then(|| CStr::from_ptr(value).to_string_lossy())
        };
        let allow_file = *this.get_ivar::<BOOL>("_allowFile") == YES;
        let file_navigation_used = *this.get_ivar::<BOOL>("_fileNavigationUsed") == YES;
        let (policy, file_navigation_used) =
            navigation_policy(scheme.as_deref(), allow_file, file_navigation_used);
        this.set_ivar(
            "_fileNavigationUsed",
            if file_navigation_used { YES } else { NO },
        );
        let handler = &*(decision_handler as *const Block<dyn Fn(isize)>);
        handler.call((policy,));
    }
}

fn navigation_delegate() -> *mut Object {
    static CLASS: OnceLock<usize> = OnceLock::new();
    let class = *CLASS.get_or_init(|| unsafe {
        let class = if let Some(mut declaration) =
            ClassDecl::new("ZeronFilePreviewNavigationDelegate", class!(NSObject))
        {
            declaration.add_ivar::<BOOL>("_allowFile");
            declaration.add_ivar::<BOOL>("_fileNavigationUsed");
            declaration.add_method(
                sel!(webView:decidePolicyForNavigationAction:decisionHandler:),
                decide_navigation
                    as extern "C" fn(&mut Object, Sel, *mut Object, *mut Object, *mut c_void),
            );
            declaration.register()
        } else {
            Class::get("ZeronFilePreviewNavigationDelegate")
                .expect("registered file preview navigation delegate")
        };
        class as *const Class as usize
    }) as *const Class;
    unsafe {
        let delegate: *mut Object = msg_send![class, new];
        (&mut *delegate).set_ivar("_allowFile", NO);
        (&mut *delegate).set_ivar("_fileNavigationUsed", NO);
        delegate
    }
}

unsafe fn isolated_view(allow_file: bool) -> Option<NativeDocumentView> {
    let configuration: *mut Object = msg_send![class!(WKWebViewConfiguration), new];
    let data_store: *mut Object = msg_send![class!(WKWebsiteDataStore), nonPersistentDataStore];
    let _: () = msg_send![configuration, setWebsiteDataStore: data_store];
    let preferences: *mut Object = msg_send![configuration, preferences];
    let _: () = msg_send![preferences, setJavaScriptEnabled: NO];
    let page_preferences: *mut Object = msg_send![configuration, defaultWebpagePreferences];
    if !page_preferences.is_null() {
        let _: () = msg_send![page_preferences, setAllowsContentJavaScript: NO];
    }
    let view: *mut Object = msg_send![class!(WKWebView), alloc];
    let zero = NSRect {
        origin: NSPoint { x: 0.0, y: 0.0 },
        size: NSSize {
            width: 1.0,
            height: 1.0,
        },
    };
    let view: *mut Object = msg_send![view, initWithFrame: zero configuration: configuration];
    let _: () = msg_send![configuration, release];
    if view.is_null() {
        return None;
    }
    let navigation_delegate = navigation_delegate();
    unsafe {
        (&mut *navigation_delegate).set_ivar("_allowFile", if allow_file { YES } else { NO });
    }
    let _: () = msg_send![view, setNavigationDelegate: navigation_delegate];
    Some(NativeDocumentView {
        view,
        navigation_delegate,
    })
}

impl NativeDocumentView {
    pub fn open_html(document: &str) -> Option<Self> {
        unsafe {
            let view = isolated_view(false)?;
            let document = ns_string(document);
            let base_url: *mut Object = std::ptr::null_mut();
            let _: *mut Object = msg_send![view.view, loadHTMLString: document baseURL: base_url];
            Some(view)
        }
    }

    pub fn open_pdf(path: &Path) -> Option<Self> {
        unsafe {
            let view = isolated_view(true)?;
            let path_string = ns_string(path.to_string_lossy().as_ref());
            let url: *mut Object = msg_send![class!(NSURL), fileURLWithPath: path_string];
            let _: *mut Object =
                msg_send![view.view, loadFileURL: url allowingReadAccessToURL: url];
            Some(view)
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
                let nil: *mut Object = std::ptr::null_mut();
                let _: () = msg_send![self.view, setNavigationDelegate: nil];
                let _: () = msg_send![self.view, removeFromSuperview];
                let _: () = msg_send![self.view, release];
                self.view = std::ptr::null_mut();
            }
            if !self.navigation_delegate.is_null() {
                let _: () = msg_send![self.navigation_delegate, release];
                self.navigation_delegate = std::ptr::null_mut();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{appkit_frame, navigation_policy};

    #[test]
    fn converts_gpui_top_left_bounds_to_appkit_bottom_left_frame() {
        assert_eq!(
            appkit_frame(20.0, 100.0, 640.0, 480.0, 900.0),
            (20.0, 320.0, 640.0, 480.0)
        );
    }

    #[test]
    fn native_preview_allows_only_its_initial_navigation() {
        assert_eq!(navigation_policy(Some("about"), false, false), (1, false));
        assert_eq!(navigation_policy(Some("https"), false, false), (0, false));
        assert_eq!(navigation_policy(Some("file"), true, false), (1, true));
        assert_eq!(navigation_policy(Some("file"), true, true), (0, true));
    }
}
