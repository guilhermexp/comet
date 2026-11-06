//! Isolated Accounts render against the real macOS AccessKit adapter.
//! Run with `cargo run -p zeron-ui --features appshots-fixture --example accounts-a11y-fixture -- <full|embedded> <isolated-settings-dir>`.
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("accounts-a11y-fixture requires macOS");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() {
    use gpui::{AppContext, Bounds, WindowBounds, WindowOptions, point, px, size};
    use objc::{class, msg_send, runtime::Object, sel, sel_impl};
    use zeron_ui::{icons, settings, settings::accounts, theme::Theme};

    let mut args = std::env::args().skip(1);
    let mode = args.next().expect("full or embedded");
    assert!(mode == "full" || mode == "embedded");
    let embedded = mode == "embedded";
    let settings_dir = std::path::PathBuf::from(args.next().expect("isolated settings directory"));
    std::fs::create_dir_all(&settings_dir).unwrap();
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx| {
            gpui_base::init(cx);
            settings::init(Default::default(), &settings_dir, cx);
            cx.set_global(Theme::default());
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            point(px(-10000.0), px(-10000.0)),
                            size(px(1000.0), px(1400.0)),
                        ))),
                        focus: false,
                        ..Default::default()
                    },
                    |_, cx| cx.new(|cx| accounts::accessibility_fixture(embedded, cx)),
                )
                .unwrap();
            cx.defer(move |cx| {
                // AccessKit subclasses the content view. Asking its native children
                // activates the real accessibility adapter before the next frame.
                unsafe {
                    let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
                    let windows: *mut Object = msg_send![app, windows];
                    let native: *mut Object = msg_send![windows, firstObject];
                    assert!(!native.is_null());
                    let content: *mut Object = msg_send![native, contentView];
                    let _: *mut Object = msg_send![content, accessibilityChildren];
                }
                let tree = cx
                    .update_window(window.into(), |_, window, cx| {
                        window.draw(cx).clear();
                        window
                            .debug_a11y_tree_json()
                            .expect("accessibility adapter must be active")
                    })
                    .unwrap();
                let tree: serde_json::Value = serde_json::from_str(&tree).unwrap();
                let nodes = tree["nodes"].as_object().unwrap();
                let expected = if embedded {
                    ["openai", "copilot"]
                } else {
                    ["claude", "codex"]
                };
                for id in expected {
                    for label in [
                        format!("Actions for {id}@example.invalid"),
                        format!("Show {id}@example.invalid in the Usage widget"),
                    ] {
                        assert!(
                            nodes
                                .values()
                                .any(|node| node["aria"]["label"].as_str() == Some(&label)),
                            "missing {label}: {tree}"
                        );
                    }
                }
                println!(
                    "PASS: {mode} exposes both account controls in the native accessibility tree"
                );
                cx.quit();
            });
        });
}
