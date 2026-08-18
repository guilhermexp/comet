use zeron_workers_unpeel::WorkersPreset;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkersNewSessionMenuItem {
    Terminal,
    Preset(String),
    WorktreeTerminal,
    WorktreePreset(String),
    ManagePresets,
}

pub fn new_session_menu_items(
    presets: &[WorkersPreset],
    allow_worktree: bool,
) -> Vec<WorkersNewSessionMenuItem> {
    let enabled = presets
        .iter()
        .filter(|preset| preset.enabled)
        .collect::<Vec<_>>();
    let mut items = Vec::with_capacity(enabled.len() * 2 + 3);
    items.push(WorkersNewSessionMenuItem::Terminal);
    items.extend(
        enabled
            .iter()
            .map(|preset| WorkersNewSessionMenuItem::Preset(preset.id.clone())),
    );
    if allow_worktree {
        items.push(WorkersNewSessionMenuItem::WorktreeTerminal);
        items.extend(
            enabled
                .iter()
                .map(|preset| WorkersNewSessionMenuItem::WorktreePreset(preset.id.clone())),
        );
    }
    items.push(WorkersNewSessionMenuItem::ManagePresets);
    items
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
    use zeron_workers_unpeel::WorkersPreset;

    use crate::workers::presentation::runtime_icon_path;

    use super::WorkersNewSessionMenuItem as Item;

    static SELECTED_TAG: AtomicI64 = AtomicI64::new(-1);

    #[repr(C)]
    struct NSSize {
        width: f64,
        height: f64,
    }

    struct MenuContext {
        menu: *mut Object,
        event: *mut Object,
        view: *mut Object,
        selections: Vec<Item>,
        sender: oneshot::Sender<Option<Item>>,
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
            let _: () = msg_send![class!(NSMenu),
                popUpContextMenu: context.menu
                withEvent: context.event
                forView: context.view
            ];
            let _: () = msg_send![context.menu, release];
            let _: () = msg_send![context.event, release];
            let _: () = msg_send![context.view, release];
        }
        let tag = SELECTED_TAG.swap(-1, Ordering::SeqCst);
        let selection = usize::try_from(tag)
            .ok()
            .and_then(|index| context.selections.get(index).cloned());
        let _ = context.sender.send(selection);
    }

    fn action_target() -> *mut Object {
        static TARGET: OnceLock<usize> = OnceLock::new();
        *TARGET.get_or_init(|| unsafe {
            let class = if let Some(mut declaration) =
                ClassDecl::new("ZeronWorkersNewSessionMenuTarget", class!(NSObject))
            {
                declaration.add_method(
                    sel!(performAction:),
                    perform_action as extern "C" fn(&Object, Sel, *mut Object),
                );
                declaration.register()
            } else {
                objc::runtime::Class::get("ZeronWorkersNewSessionMenuTarget")
                    .expect("registered workers new-session menu target")
            };
            let target: *mut Object = msg_send![class, new];
            target as usize
        }) as *mut Object
    }

    fn ns_string(value: &str) -> *mut Object {
        let value = CString::new(value).expect("menu labels cannot contain null bytes");
        unsafe { msg_send![class!(NSString), stringWithUTF8String: value.as_ptr()] }
    }

    unsafe fn new_menu(title: &str) -> *mut Object {
        let menu: *mut Object = msg_send![class!(NSMenu), alloc];
        let menu: *mut Object = msg_send![menu, initWithTitle: ns_string(title)];
        let _: () = msg_send![menu, setAutoenablesItems: false];
        menu
    }

    unsafe fn add_separator(menu: *mut Object) {
        let item: *mut Object = msg_send![class!(NSMenuItem), separatorItem];
        let _: () = msg_send![menu, addItem: item];
    }

    unsafe fn add_action(
        menu: *mut Object,
        title: &str,
        icon_path: Option<&str>,
        selection: Item,
        selections: &mut Vec<Item>,
    ) {
        let tag = selections.len() as i64;
        selections.push(selection);
        let item: *mut Object = msg_send![class!(NSMenuItem), alloc];
        let item: *mut Object = msg_send![item,
            initWithTitle: ns_string(title)
            action: sel!(performAction:)
            keyEquivalent: ns_string("")
        ];
        let _: () = msg_send![item, setTarget: action_target()];
        let _: () = msg_send![item, setTag: tag];
        let _: () = msg_send![item, setEnabled: true];
        if let Some(icon_path) = icon_path {
            unsafe { set_item_icon(item, icon_path) };
        }
        let _: () = msg_send![menu, addItem: item];
        let _: () = msg_send![item, release];
    }

    unsafe fn set_item_icon(item: *mut Object, path: &str) {
        let Some(bytes) = icon_bytes(path) else {
            return;
        };
        let data: *mut Object = msg_send![class!(NSData),
            dataWithBytes: bytes.as_ptr()
            length: bytes.len()
        ];
        let image: *mut Object = msg_send![class!(NSImage), alloc];
        let image: *mut Object = msg_send![image, initWithData: data];
        if image.is_null() {
            return;
        }
        let _: () = msg_send![image, setSize: NSSize { width: 16.0, height: 16.0 }];
        let _: () = msg_send![image, setTemplate: true];
        let _: () = msg_send![item, setImage: image];
        let _: () = msg_send![image, release];
    }

    fn icon_bytes(path: &str) -> Option<&'static [u8]> {
        match path {
            "terminal" => Some(include_bytes!("../../assets/icons/terminal.svg")),
            "workers/chrome-branch" => Some(include_bytes!(
                "../../assets/icons/workers/chrome-branch.svg"
            )),
            "workers/chrome-settings" => Some(include_bytes!(
                "../../assets/icons/workers/chrome-settings.svg"
            )),
            "workers/amp" => Some(include_bytes!("../../assets/icons/workers/amp.svg")),
            "workers/claude" => Some(include_bytes!("../../assets/icons/workers/claude.svg")),
            "workers/cline" => Some(include_bytes!("../../assets/icons/workers/cline.svg")),
            "workers/codex" => Some(include_bytes!("../../assets/icons/workers/codex.svg")),
            "workers/cursor-agent" => Some(include_bytes!(
                "../../assets/icons/workers/cursor-agent.svg"
            )),
            "workers/gemini" => Some(include_bytes!("../../assets/icons/workers/gemini.svg")),
            "workers/generic-agent" => Some(include_bytes!(
                "../../assets/icons/workers/generic-agent.svg"
            )),
            "workers/grok" => Some(include_bytes!("../../assets/icons/workers/grok.svg")),
            "workers/kimi" => Some(include_bytes!("../../assets/icons/workers/kimi.svg")),
            "workers/kiro" => Some(include_bytes!("../../assets/icons/workers/kiro.svg")),
            "workers/muse-code" => Some(include_bytes!("../../assets/icons/workers/muse-code.svg")),
            "workers/opencode" => Some(include_bytes!("../../assets/icons/workers/opencode.svg")),
            "workers/pi" => Some(include_bytes!("../../assets/icons/workers/pi.svg")),
            _ => None,
        }
    }

    unsafe fn add_submenu(
        menu: *mut Object,
        title: &str,
        icon_path: Option<&str>,
        submenu: *mut Object,
    ) {
        let item: *mut Object = msg_send![class!(NSMenuItem), alloc];
        let item: *mut Object = msg_send![item,
            initWithTitle: ns_string(title)
            action: sel!(performAction:)
            keyEquivalent: ns_string("")
        ];
        if let Some(icon_path) = icon_path {
            unsafe { set_item_icon(item, icon_path) };
        }
        let _: () = msg_send![item, setSubmenu: submenu];
        let _: () = msg_send![menu, addItem: item];
        let _: () = msg_send![item, release];
        let _: () = msg_send![submenu, release];
    }

    pub fn show_async(
        presets: &[WorkersPreset],
        allow_worktree: bool,
    ) -> oneshot::Receiver<Option<Item>> {
        let (sender, receiver) = oneshot::channel();
        unsafe {
            SELECTED_TAG.store(-1, Ordering::SeqCst);
            let menu = new_menu("");
            let enabled = presets
                .iter()
                .filter(|preset| preset.enabled)
                .collect::<Vec<_>>();
            let mut selections = Vec::new();

            add_action(
                menu,
                "Terminal",
                Some(crate::icons::TERMINAL),
                Item::Terminal,
                &mut selections,
            );
            if !enabled.is_empty() {
                add_separator(menu);
                for preset in &enabled {
                    add_action(
                        menu,
                        &preset.label,
                        Some(runtime_icon_path(
                            preset.cli_id.as_deref(),
                            Some(preset.command.as_str()),
                        )),
                        Item::Preset(preset.id.clone()),
                        &mut selections,
                    );
                }
            }
            if allow_worktree {
                add_separator(menu);
                let submenu = new_menu("In a new worktree");
                add_action(
                    submenu,
                    "Terminal",
                    Some(crate::icons::TERMINAL),
                    Item::WorktreeTerminal,
                    &mut selections,
                );
                if !enabled.is_empty() {
                    add_separator(submenu);
                    for preset in &enabled {
                        add_action(
                            submenu,
                            &preset.label,
                            Some(runtime_icon_path(
                                preset.cli_id.as_deref(),
                                Some(preset.command.as_str()),
                            )),
                            Item::WorktreePreset(preset.id.clone()),
                            &mut selections,
                        );
                    }
                }
                add_submenu(
                    menu,
                    "In a new worktree",
                    Some(crate::icons::WORKER_BRANCH),
                    submenu,
                );
            }
            add_separator(menu);
            add_action(
                menu,
                "Manage presets…",
                Some(crate::icons::WORKER_SETTINGS),
                Item::ManagePresets,
                &mut selections,
            );

            let application: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let event: *mut Object = msg_send![application, currentEvent];
            let window: *mut Object = msg_send![event, window];
            let view: *mut Object = msg_send![window, contentView];
            if !event.is_null() && !view.is_null() {
                let event: *mut Object = msg_send![event, retain];
                let view: *mut Object = msg_send![view, retain];
                let context = Box::new(MenuContext {
                    menu,
                    event,
                    view,
                    selections,
                    sender,
                });
                dispatch_async_f(
                    std::ptr::addr_of!(DISPATCH_MAIN_QUEUE).cast_mut(),
                    Box::into_raw(context).cast::<c_void>(),
                    pop_up_menu,
                );
            } else {
                let _: () = msg_send![menu, release];
                let _ = sender.send(None);
            }
        }
        receiver
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkersNewSessionMenuItem as Item, new_session_menu_items};
    use zeron_workers_unpeel::WorkersPreset;

    fn preset(id: &str, enabled: bool) -> WorkersPreset {
        WorkersPreset {
            id: id.into(),
            label: id.into(),
            command: id.into(),
            cli_id: None,
            enabled,
            quick_launch: false,
            is_default: false,
            tint_color_hex: None,
        }
    }

    #[test]
    fn project_plus_menu_matches_unpeel_order_and_filters_disabled_presets() {
        let presets = vec![
            preset("claude", true),
            preset("gemini", false),
            preset("pi", true),
        ];
        assert_eq!(
            new_session_menu_items(&presets, true),
            vec![
                Item::Terminal,
                Item::Preset("claude".into()),
                Item::Preset("pi".into()),
                Item::WorktreeTerminal,
                Item::WorktreePreset("claude".into()),
                Item::WorktreePreset("pi".into()),
                Item::ManagePresets,
            ]
        );
    }

    #[test]
    fn worktree_project_omits_nested_worktree_submenu() {
        assert_eq!(
            new_session_menu_items(&[preset("claude", true)], false),
            vec![
                Item::Terminal,
                Item::Preset("claude".into()),
                Item::ManagePresets,
            ]
        );
    }
}
