//! Runs production native detach against a real GPUI/AppKit window on the main thread.
//! Opt in with ZERON_NATIVE_PREVIEW_FOCUS_TEST=1; no engine or user data is opened.
#[cfg(target_os = "macos")]
#[allow(dead_code, unused_imports)]
mod native {
    #[link(name = "WebKit", kind = "framework")]
    unsafe extern "C" {}
    include!("../src/file_preview/native_document.rs");

    pub unsafe fn check(window: *mut Object) {
        unsafe {
            assert!(!window.is_null(), "native GPUI window must exist");
            let content: *mut Object = msg_send![window, contentView];
            let gpui: *mut Object = msg_send![window, firstResponder];
            assert_ne!(
                content, gpui,
                "fixture must distinguish container from keyboard view"
            );
            let is_gpui: BOOL = msg_send![gpui, isKindOfClass: Class::get("GPUIView").unwrap()];
            assert_eq!(is_gpui, YES);
            for descendant in [false, true] {
                let mut preview = NativeDocumentView::open_html("<p>Focus regression</p>").unwrap();
                let _: () = msg_send![content, addSubview: preview.view];
                let child: *mut Object = msg_send![class!(NSTextView), new];
                let target = if descendant {
                    let _: () = msg_send![preview.view, addSubview: child];
                    child
                } else {
                    preview.view
                };
                let accepted: BOOL = msg_send![window, makeFirstResponder: target];
                assert_eq!(accepted, YES, "preview fixture must own keyboard");
                preview.hide();
                let first: *mut Object = msg_send![window, firstResponder];
                let _: () = msg_send![child, release];
                assert_eq!(
                    first, gpui,
                    "detaching preview must restore GPUIView, not contentView"
                );
                preview.hide();
                let first: *mut Object = msg_send![window, firstResponder];
                assert_eq!(first, gpui, "repeated hide must preserve GPUI focus");
            }
            let preview = NativeDocumentView::open_html("<p>Drop</p>").unwrap();
            let _: () = msg_send![content, addSubview: preview.view];
            let accepted: BOOL = msg_send![window, makeFirstResponder: preview.view];
            assert_eq!(accepted, YES);
            drop(preview);
            let first: *mut Object = msg_send![window, firstResponder];
            assert_eq!(first, gpui, "Drop must restore GPUI focus");

            let mut preview = NativeDocumentView::open_html("<p>Unfocused</p>").unwrap();
            let _: () = msg_send![content, addSubview: preview.view];
            let other: *mut Object = msg_send![class!(NSTextView), new];
            let _: () = msg_send![content, addSubview: other];
            let accepted: BOOL = msg_send![window, makeFirstResponder: other];
            assert_eq!(accepted, YES);
            preview.hide();
            let first: *mut Object = msg_send![window, firstResponder];
            assert_eq!(first, other, "unrelated responder must keep keyboard");
            let _: BOOL = msg_send![window, makeFirstResponder: gpui];
            let _: () = msg_send![other, removeFromSuperview];
            let _: () = msg_send![other, release];
        }
    }
}

#[cfg(target_os = "macos")]
fn main() {
    use gpui::{AppContext, Context, IntoElement, Render, Window, WindowOptions, div};
    use objc::{class, msg_send, runtime::Object, sel, sel_impl};
    if std::env::var("ZERON_NATIVE_PREVIEW_FOCUS_TEST").as_deref() != Ok("1") {
        println!(
            "SKIPPED native AppKit test: set ZERON_NATIVE_PREVIEW_FOCUS_TEST=1 on macOS desktop"
        );
        return;
    }
    struct Empty;
    impl Render for Empty {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }
    gpui_platform::application().run(move |cx| {
        cx.open_window(WindowOptions::default(), |_, cx| cx.new(|_| Empty)).unwrap();
        cx.defer(move |_cx| {
            let outcome = std::panic::catch_unwind(|| unsafe {
                let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
                let windows: *mut Object = msg_send![app, windows];
                let window: *mut Object = msg_send![windows, firstObject];
                native::check(window);
            });
            if outcome.is_ok() {
                println!("PASS: native preview hide, descendant, repeated hide, Drop, unrelated responder");
            }
            std::process::exit(if outcome.is_ok() { 0 } else { 1 });
        });
    });
    panic!("native preview test exited without running assertions");
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("SKIPPED: native preview focus requires macOS");
}
