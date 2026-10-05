// Exercise the real window appearance setters from the process main thread.
// This test stays harness-free because AppKit window creation and mutation are
// main-thread-only.
#[cfg(target_os = "macos")]
use cocoa::{
    appkit::{
        NSAppKitVersionNumber, NSAppKitVersionNumber12_0, NSApplication, NSView,
        NSViewHeightSizable, NSViewWidthSizable, NSVisualEffectBlendingMode,
        NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow,
    },
    base::{NO, id, nil},
    foundation::{NSDictionary, NSRect, NSSize, NSString, NSUInteger},
};
#[cfg(target_os = "macos")]
use gpui::{
    App, Application, Bounds, Context, Render, TitlebarOptions, Window, WindowBackgroundAppearance,
    WindowBounds, WindowOptions, div, prelude::*, px, size,
};
#[cfg(target_os = "macos")]
use gpui_macos::MacPlatform;
#[cfg(target_os = "macos")]
use objc::{class, msg_send, runtime::BOOL};
#[cfg(target_os = "macos")]
use std::{cell::Cell, ffi::CStr, rc::Rc};

#[cfg(target_os = "macos")]
struct EmptyView;

#[cfg(target_os = "macos")]
impl Render for EmptyView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

#[cfg(target_os = "macos")]
fn native_window_with_title(expected_title: &str) -> id {
    unsafe {
        let app = NSApplication::sharedApplication(nil);
        let windows: id = msg_send![app, windows];
        let count: NSUInteger = msg_send![windows, count];
        for index in 0..count {
            let window: id = msg_send![windows, objectAtIndex: index];
            let title: id = msg_send![window, title];
            if title == nil {
                continue;
            }
            let title_bytes = NSString::UTF8String(title);
            if !title_bytes.is_null()
                && CStr::from_ptr(title_bytes).to_string_lossy() == expected_title
            {
                return window;
            }
        }
        panic!("could not find the native test window named {expected_title:?}");
    }
}

#[cfg(target_os = "macos")]
unsafe fn vanilla_effect_views(window: id) -> Vec<id> {
    unsafe {
        let content_view = NSWindow::contentView(window);
        let subviews: id = msg_send![content_view, subviews];
        let count: NSUInteger = msg_send![subviews, count];
        (0..count)
            .filter_map(|index| {
                let view: id = msg_send![subviews, objectAtIndex: index];
                let is_vanilla: BOOL = msg_send![view, isMemberOfClass: class!(NSVisualEffectView)];
                (is_vanilla != NO).then_some(view)
            })
            .collect()
    }
}

#[cfg(target_os = "macos")]
fn only_backing_view(window: id) -> id {
    let views = unsafe { vanilla_effect_views(window) };
    assert_eq!(
        views.len(),
        1,
        "expected exactly one vanilla NSVisualEffectView backing for declared-radius glass"
    );
    views[0]
}

#[cfg(target_os = "macos")]
fn assert_no_backing_view(window: id) {
    assert!(
        unsafe { vanilla_effect_views(window) }.is_empty(),
        "vanilla NSVisualEffectView backing must be removed outside WindowServer blur mode"
    );
}

#[cfg(target_os = "macos")]
fn assert_frame_matches_bounds(view: id, content_view: id) {
    unsafe {
        let frame = NSView::frame(view);
        let bounds = NSView::bounds(content_view);
        let close = |actual: f64, expected: f64| (actual - expected).abs() < 0.5;
        assert!(close(frame.origin.x, bounds.origin.x));
        assert!(close(frame.origin.y, bounds.origin.y));
        assert!(close(frame.size.width, bounds.size.width));
        assert!(close(frame.size.height, bounds.size.height));
        let autoresizing_mask: NSUInteger = msg_send![view, autoresizingMask];
        assert_eq!(
            autoresizing_mask as u64,
            NSViewWidthSizable | NSViewHeightSizable,
            "backing view must autoresize with the window content view"
        );
    }
}

#[cfg(target_os = "macos")]
fn assert_backing_below_native_view(window: id, backing_view: id) {
    unsafe {
        let content_view = NSWindow::contentView(window);
        let subviews: id = msg_send![content_view, subviews];
        let count: NSUInteger = msg_send![subviews, count];
        let mut backing_index = None;
        let mut native_view_index = None;
        for index in 0..count {
            let view: id = msg_send![subviews, objectAtIndex: index];
            if view == backing_view {
                backing_index = Some(index);
                continue;
            }

            let is_native_view: BOOL = msg_send![view, isMemberOfClass: class!(GPUIView)];
            if is_native_view != NO {
                native_view_index = Some(index);
            }
        }

        let backing_index = backing_index.expect("backing view should be a content subview");
        let native_view_index = native_view_index.expect("content should contain the GPUIView");
        assert!(
            backing_index < native_view_index,
            "AppKit subviews are back-to-front; glass backing must precede GPUIView"
        );
    }
}

#[cfg(target_os = "macos")]
fn assert_backing_configuration(window: id, view: id) {
    unsafe {
        let alpha: f64 = msg_send![view, alphaValue];
        let material: u64 = msg_send![view, material];
        let blending_mode: u64 = msg_send![view, blendingMode];
        let state: u64 = msg_send![view, state];
        assert!((alpha - 0.01).abs() < 0.001, "backing alpha was {alpha}");
        assert_eq!(
            material,
            NSVisualEffectMaterial::UnderWindowBackground as u64
        );
        assert_eq!(
            blending_mode,
            NSVisualEffectBlendingMode::BehindWindow as u64
        );
        assert_eq!(state, NSVisualEffectState::Active as u64);
        let content_view = NSWindow::contentView(window);
        assert_backing_below_native_view(window, view);
        assert_frame_matches_bounds(view, content_view);
    }
}

#[cfg(target_os = "macos")]
fn run_regression() {
    let title = format!("zui-window-glass-backing-{}", std::process::id());
    let native_window = Rc::new(Cell::new(nil));
    let expected_backing = Rc::new(Cell::new(nil));
    register_temporary_autofill_default();
    Application::with_platform(Rc::new(MacPlatform::new(false))).run(move |cx: &mut App| {
        let window_title = title.clone();
        let native_window_for_build = native_window.clone();
        let expected_backing_for_build = expected_backing.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(400.0), px(280.0)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some(window_title.clone().into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                window_background: WindowBackgroundAppearance::Opaque,
                show: false,
                ..Default::default()
            },
            move |window, cx| {
                let native_window = native_window_with_title(&window_title);
                native_window_for_build.set(native_window);
                let content_view = unsafe { NSWindow::contentView(native_window) };
                assert_no_backing_view(native_window);

                window.set_background_blur_radius(Some(px(24.0)));
                window.set_background_appearance(WindowBackgroundAppearance::Blurred);
                let original_backing = only_backing_view(native_window);
                assert_backing_configuration(native_window, original_backing);

                // Reapplying the same surface and changing the authored radius
                // must retain one backing object.
                window.set_background_appearance(WindowBackgroundAppearance::Blurred);
                assert_eq!(only_backing_view(native_window), original_backing);
                window.set_background_blur_radius(Some(px(48.0)));
                assert_eq!(only_backing_view(native_window), original_backing);
                window.set_background_blur_radius(Some(px(24.0)));
                assert_eq!(only_backing_view(native_window), original_backing);

                // The ordinary AppKit material path is available on macOS 12+;
                // legacy systems keep their WindowServer default blur instead.
                window.set_background_blur_radius(None);
                let is_modern_appkit =
                    unsafe { NSAppKitVersionNumber >= NSAppKitVersionNumber12_0 };
                if is_modern_appkit {
                    assert_no_backing_view(native_window);
                } else {
                    assert_eq!(only_backing_view(native_window), original_backing);
                }

                window.set_background_blur_radius(Some(px(24.0)));
                let restored_backing = only_backing_view(native_window);
                assert_backing_configuration(native_window, restored_backing);
                window.set_background_appearance(WindowBackgroundAppearance::Opaque);
                assert_no_backing_view(native_window);

                window.set_background_appearance(WindowBackgroundAppearance::Blurred);
                let final_backing = only_backing_view(native_window);
                assert_backing_configuration(native_window, final_backing);
                window.set_background_appearance(WindowBackgroundAppearance::Transparent);
                assert_no_backing_view(native_window);
                window.set_background_appearance(WindowBackgroundAppearance::Blurred);
                let final_backing = only_backing_view(native_window);
                assert_backing_configuration(native_window, final_backing);

                // Resize the real native window and ensure its backing tracks
                // the content geometry through AppKit's autoresizing behavior.
                unsafe {
                    let frame = NSWindow::frame(native_window);
                    NSWindow::setFrame_display_(
                        native_window,
                        NSRect {
                            origin: frame.origin,
                            size: NSSize {
                                width: frame.size.width + 32.0,
                                height: frame.size.height + 24.0,
                            },
                        },
                        NO,
                    );
                }
                assert_frame_matches_bounds(final_backing, content_view);
                expected_backing_for_build.set(final_backing);
                cx.new(|_| EmptyView)
            },
        )
        .expect("native glass regression window should open");
        // App::open_window renders the first frame before returning. Confirm
        // that the backing still supports the rendered content.
        let native_window = native_window.get();
        let expected_backing = expected_backing.get();
        assert!(!native_window.is_null());
        assert!(!expected_backing.is_null());
        assert_eq!(only_backing_view(native_window), expected_backing);
        assert_backing_configuration(native_window, expected_backing);
        cx.quit();
    });
}

#[cfg(target_os = "macos")]
fn register_temporary_autofill_default() {
    unsafe {
        let user_defaults: id = msg_send![class!(NSUserDefaults), standardUserDefaults];
        let key = NSString::alloc(nil).init_str("NSAutoFillHeuristicControllerEnabled");
        let disabled: id = msg_send![class!(NSNumber), numberWithBool: NO];
        let registration = NSDictionary::dictionaryWithObject_forKey_(nil, disabled, key);
        let _: () = msg_send![user_defaults, registerDefaults: registration];
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    run_regression();
}
