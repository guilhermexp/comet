use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use gpui::{Context, Entity, Global, Subscription, Task};

use super::activity_menu::project_activity_menu;
use super::model::{WorkersModel, WorkersSessionTarget};

pub const ALL_RECENT_TAG: i64 = 10_000;
const STATUS_ITEM_TAG: i64 = -1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuBarIntent {
    SelectSession(WorkersSessionTarget),
    ShowAllRecent,
    TogglePopover,
}

#[derive(Debug, Clone, Default)]
pub struct MenuBarBindings {
    session_targets: Vec<WorkersSessionTarget>,
}

impl MenuBarBindings {
    pub fn new(targets: impl IntoIterator<Item = WorkersSessionTarget>) -> Self {
        Self {
            session_targets: targets.into_iter().collect(),
        }
    }

    pub fn intent_for_tag(&self, tag: i64) -> Option<MenuBarIntent> {
        if tag == ALL_RECENT_TAG {
            return Some(MenuBarIntent::ShowAllRecent);
        }
        if tag == STATUS_ITEM_TAG {
            return Some(MenuBarIntent::TogglePopover);
        }
        usize::try_from(tag)
            .ok()
            .and_then(|index| self.session_targets.get(index))
            .cloned()
            .map(MenuBarIntent::SelectSession)
    }
}

pub struct WorkersMenuBarController {
    model: Entity<WorkersModel>,
    native: platform::NativeMenuBar,
    receiver: Receiver<MenuBarIntent>,
    _model_observation: Subscription,
    _intent_task: Task<()>,
    _spinner_task: Task<()>,
}

impl WorkersMenuBarController {
    pub fn new(model: Entity<WorkersModel>, cx: &mut Context<Self>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let menu = model
            .read(cx)
            .snapshot
            .as_ref()
            .map(project_activity_menu)
            .unwrap_or_default();
        let native = platform::NativeMenuBar::new(sender, &menu);
        let model_observation = cx.observe(&model, |this, model, cx| {
            let menu = model
                .read(cx)
                .snapshot
                .as_ref()
                .map(project_activity_menu)
                .unwrap_or_default();
            this.native.update(&menu);
        });
        let intent_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        while let Ok(intent) = this.receiver.try_recv() {
                            match intent {
                                MenuBarIntent::SelectSession(target) => {
                                    this.native.close();
                                    this.model.update(cx, |model, cx| {
                                        model.request_session_reveal(target, cx)
                                    });
                                    cx.activate(true);
                                }
                                MenuBarIntent::ShowAllRecent => {
                                    this.native.close();
                                    this.model
                                        .update(cx, |model, cx| model.request_recent_reveal(cx));
                                    cx.activate(true);
                                }
                                MenuBarIntent::TogglePopover => this.native.toggle(),
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let spinner_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                if this
                    .update(cx, |this, _cx| this.native.tick_spinner())
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            model,
            native,
            receiver,
            _model_observation: model_observation,
            _intent_task: intent_task,
            _spinner_task: spinner_task,
        }
    }
}

pub struct WorkersMenuBarGlobal {
    pub controller: Entity<WorkersMenuBarController>,
}

impl Global for WorkersMenuBarGlobal {}

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::CString;
    use std::sync::mpsc::Sender;
    use std::sync::{Mutex, OnceLock};

    use objc::declare::ClassDecl;
    use objc::runtime::{Object, Sel};
    use objc::{class, msg_send, sel, sel_impl};

    use super::super::activity_menu::{
        CONTENT_WIDTH, FOOTER_HEIGHT, OUTER_PADDING, ROW_HEIGHT, WorkersActivityMenu,
        WorkersActivityRow, WorkersActivityRowKind, WorkersMenuBarMode, menu_popover_size,
    };
    use super::{ALL_RECENT_TAG, MenuBarBindings, MenuBarIntent, STATUS_ITEM_TAG};
    use crate::workers::model::WorkersSessionTarget;
    use crate::workers::presentation::SPINNER_FRAMES;

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

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSRect {
        origin: NSPoint,
        size: NSSize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSRange {
        location: usize,
        length: usize,
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
        NSRect {
            origin: NSPoint { x, y },
            size: NSSize { width, height },
        }
    }

    struct Bridge {
        sender: Sender<MenuBarIntent>,
        bindings: MenuBarBindings,
    }

    static BRIDGE: OnceLock<Mutex<Option<Bridge>>> = OnceLock::new();

    extern "C" fn perform_action(_this: &Object, _cmd: Sel, sender: *mut Object) {
        let tag: i64 = unsafe { msg_send![sender, tag] };
        let Some(bridge) = BRIDGE
            .get()
            .and_then(|bridge| bridge.lock().ok())
            .and_then(|bridge| {
                bridge.as_ref().and_then(|bridge| {
                    bridge
                        .bindings
                        .intent_for_tag(tag)
                        .map(|intent| (bridge.sender.clone(), intent))
                })
            })
        else {
            return;
        };
        let _ = bridge.0.send(bridge.1);
    }

    fn action_target() -> *mut Object {
        static TARGET: OnceLock<usize> = OnceLock::new();
        *TARGET.get_or_init(|| unsafe {
            let class = if let Some(mut declaration) =
                ClassDecl::new("ZeronWorkersMenuBarTarget", class!(NSObject))
            {
                declaration.add_method(
                    sel!(performAction:),
                    perform_action as extern "C" fn(&Object, Sel, *mut Object),
                );
                declaration.register()
            } else {
                objc::runtime::Class::get("ZeronWorkersMenuBarTarget")
                    .expect("registered menu bar target")
            };
            let target: *mut Object = msg_send![class, new];
            target as usize
        }) as *mut Object
    }

    pub struct NativeMenuBar {
        status_bar: *mut Object,
        status_item: *mut Object,
        popover: *mut Object,
        target: *mut Object,
        mode: WorkersMenuBarMode,
        frame: usize,
        spinner_labels: Vec<*mut Object>,
    }

    impl NativeMenuBar {
        pub fn new(sender: Sender<MenuBarIntent>, menu: &WorkersActivityMenu) -> Self {
            unsafe {
                let target = action_target();
                let status_bar: *mut Object = msg_send![class!(NSStatusBar), systemStatusBar];
                let status_item: *mut Object =
                    msg_send![status_bar, statusItemWithLength: -1.0_f64];
                let _: *mut Object = msg_send![status_item, retain];
                let button: *mut Object = msg_send![status_item, button];
                let _: () = msg_send![button, setTarget: target];
                let _: () = msg_send![button, setAction: sel!(performAction:)];
                let _: () = msg_send![button, setTag: STATUS_ITEM_TAG];
                let _: () = msg_send![button, setToolTip: ns_string("Zeron Workers sessions")];
                let font: *mut Object =
                    msg_send![class!(NSFont), monospacedSystemFontOfSize: 15.0_f64 weight: 0.4_f64];
                let _: () = msg_send![button, setFont: font];

                let popover: *mut Object = msg_send![class!(NSPopover), new];
                let _: () = msg_send![popover, setBehavior: 1_i64];
                let _: () = msg_send![popover, setAnimates: true];

                BRIDGE.get_or_init(|| Mutex::new(None));
                *BRIDGE.get().unwrap().lock().unwrap() = Some(Bridge {
                    sender,
                    bindings: MenuBarBindings::default(),
                });
                let mut native = Self {
                    status_bar,
                    status_item,
                    popover,
                    target,
                    mode: WorkersMenuBarMode::Idle,
                    frame: 0,
                    spinner_labels: Vec::new(),
                };
                native.update(menu);
                native
            }
        }

        pub fn update(&mut self, menu: &WorkersActivityMenu) {
            self.mode = menu.mode;
            self.frame = 0;
            let targets = menu
                .blockers
                .iter()
                .chain(menu.jobs.iter())
                .chain(menu.finished.iter())
                .map(|row| WorkersSessionTarget::new(&row.project_id, &row.session_id));
            if let Some(mut guard) = BRIDGE.get().and_then(|bridge| bridge.lock().ok()) {
                if let Some(bridge) = guard.as_mut() {
                    bridge.bindings = MenuBarBindings::new(targets);
                }
            }
            unsafe {
                self.refresh_status_button();
                let (width, height) = menu_popover_size(menu);
                let _: () = msg_send![self.popover, setContentSize: NSSize { width, height }];
                let controller: *mut Object = msg_send![class!(NSViewController), new];
                let (content, spinner_labels) = build_content(menu, self.target, height);
                self.spinner_labels = spinner_labels;
                let _: () = msg_send![controller, setView: content];
                let _: () = msg_send![self.popover, setContentViewController: controller];
                let _: () = msg_send![controller, release];
            }
        }

        pub fn toggle(&mut self) {
            unsafe {
                let shown: bool = msg_send![self.popover, isShown];
                if shown {
                    self.close();
                    return;
                }
                let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
                let _: () = msg_send![app, activateIgnoringOtherApps: true];
                let button: *mut Object = msg_send![self.status_item, button];
                let bounds: NSRect = msg_send![button, bounds];
                let _: () = msg_send![self.popover, showRelativeToRect: bounds ofView: button preferredEdge: 1_u64];
            }
        }

        pub fn close(&mut self) {
            unsafe {
                let _: () = msg_send![self.popover, performClose: std::ptr::null_mut::<Object>()];
            }
        }

        pub fn tick_spinner(&mut self) {
            if !matches!(self.mode, WorkersMenuBarMode::Working { .. }) {
                return;
            }
            self.frame = (self.frame + 1) % SPINNER_FRAMES.len();
            unsafe {
                self.refresh_status_button();
                for label in &self.spinner_labels {
                    let _: () =
                        msg_send![*label, setStringValue: ns_string(SPINNER_FRAMES[self.frame])];
                }
            };
        }

        unsafe fn refresh_status_button(&self) {
            let button: *mut Object = unsafe { msg_send![self.status_item, button] };
            let working = matches!(self.mode, WorkersMenuBarMode::Working { .. });
            let image = if working {
                std::ptr::null_mut()
            } else {
                status_mark()
            };
            let _: () = unsafe { msg_send![button, setImage: image] };
            let _: () = unsafe {
                msg_send![button, setImagePosition: if working { 0_u64 } else if matches!(self.mode, WorkersMenuBarMode::Idle) { 1_u64 } else { 2_u64 }]
            };
            match self.mode {
                WorkersMenuBarMode::Working { blocked: true } => unsafe {
                    set_colored_title(button, SPINNER_FRAMES[self.frame], 0xF59E0B)
                },
                WorkersMenuBarMode::Working { blocked: false } => {
                    let _: () = unsafe {
                        msg_send![button, setTitle: ns_string(SPINNER_FRAMES[self.frame])]
                    };
                }
                WorkersMenuBarMode::Blocked => unsafe {
                    set_colored_title(button, "●", 0xF59E0B)
                },
                WorkersMenuBarMode::Unread => unsafe { set_colored_title(button, "●", 0x60A5FA) },
                WorkersMenuBarMode::Idle => {
                    let _: () = unsafe { msg_send![button, setTitle: ns_string("")] };
                }
            }
        }
    }

    impl Drop for NativeMenuBar {
        fn drop(&mut self) {
            unsafe {
                self.close();
                let _: () = msg_send![self.status_bar, removeStatusItem: self.status_item];
                let _: () = msg_send![self.status_item, release];
                let _: () = msg_send![self.popover, release];
            }
            if let Some(bridge) = BRIDGE.get() {
                *bridge.lock().unwrap() = None;
            }
        }
    }

    unsafe fn build_content(
        menu: &WorkersActivityMenu,
        target: *mut Object,
        height: f64,
    ) -> (*mut Object, Vec<*mut Object>) {
        let view: *mut Object = unsafe { msg_send![class!(NSView), alloc] };
        let view: *mut Object =
            unsafe { msg_send![view, initWithFrame: rect(0.0, 0.0, 332.0, height)] };
        let mut y = height - OUTER_PADDING;
        let mut tag = 0_i64;
        let mut spinner_labels = Vec::new();
        if menu.is_empty() {
            y -= 34.0;
            unsafe {
                add_label(
                    view,
                    "No active sessions",
                    rect(12.0, y + 8.0, 296.0, 18.0),
                    13.0,
                )
            };
        } else {
            for (section_index, rows) in [&menu.blockers, &menu.jobs, &menu.finished]
                .into_iter()
                .filter(|rows| !rows.is_empty())
                .enumerate()
            {
                if section_index > 0 {
                    y -= 9.0;
                }
                for row in rows {
                    y -= ROW_HEIGHT;
                    if let Some(label) = unsafe { add_row(view, row, target, tag, y) } {
                        spinner_labels.push(label);
                    }
                    tag += 1;
                }
            }
        }
        let footer: *mut Object = unsafe {
            msg_send![class!(NSButton), buttonWithTitle: ns_string("All recent  ›") target: target action: sel!(performAction:)]
        };
        let _: () = unsafe { msg_send![footer, setTag: ALL_RECENT_TAG] };
        let _: () = unsafe { msg_send![footer, setBordered: false] };
        let _: () =
            unsafe { msg_send![footer, setFrame: rect(6.0, 0.0, CONTENT_WIDTH, FOOTER_HEIGHT)] };
        let _: () = unsafe { msg_send![view, addSubview: footer] };
        (view, spinner_labels)
    }

    unsafe fn add_row(
        view: *mut Object,
        row: &WorkersActivityRow,
        target: *mut Object,
        tag: i64,
        y: f64,
    ) -> Option<*mut Object> {
        let leading = match row.kind {
            WorkersActivityRowKind::Working => SPINNER_FRAMES[0],
            WorkersActivityRowKind::Blocked => "●",
            WorkersActivityRowKind::Unread => "●",
        };
        let leading_label =
            unsafe { add_label(view, leading, rect(12.0, y + 13.0, 18.0, 18.0), 13.0) };
        match row.kind {
            WorkersActivityRowKind::Blocked => unsafe {
                let _: () = msg_send![leading_label, setTextColor: ns_color(0xF59E0B)];
            },
            WorkersActivityRowKind::Unread => unsafe {
                let _: () = msg_send![leading_label, setTextColor: ns_color(0x60A5FA)];
            },
            WorkersActivityRowKind::Working => {}
        }
        unsafe { add_label(view, &row.title, rect(38.0, y + 20.0, 202.0, 18.0), 13.0) };
        unsafe { add_label(view, &row.project, rect(38.0, y + 4.0, 202.0, 15.0), 11.0) };
        unsafe { add_label(view, row.status, rect(240.0, y + 5.0, 62.0, 15.0), 11.0) };
        if let Some(bytes) = provider_icon_bytes(row.runtime_icon) {
            let data: *mut Object = unsafe {
                msg_send![class!(NSData), dataWithBytes: bytes.as_ptr() length: bytes.len()]
            };
            let image: *mut Object = unsafe { msg_send![class!(NSImage), alloc] };
            let image: *mut Object = unsafe { msg_send![image, initWithData: data] };
            if !image.is_null() {
                let image_view: *mut Object = unsafe { msg_send![class!(NSImageView), alloc] };
                let image_view: *mut Object = unsafe {
                    msg_send![image_view, initWithFrame: rect(296.0, y + 13.0, 16.0, 16.0)]
                };
                let _: () = unsafe { msg_send![image_view, setImage: image] };
                let _: () = unsafe { msg_send![view, addSubview: image_view] };
                let _: () = unsafe { msg_send![image_view, release] };
                let _: () = unsafe { msg_send![image, release] };
            }
        }
        let button: *mut Object = unsafe {
            msg_send![class!(NSButton), buttonWithTitle: ns_string("") target: target action: sel!(performAction:)]
        };
        let _: () = unsafe { msg_send![button, setTag: tag] };
        let _: () = unsafe { msg_send![button, setBordered: false] };
        let _: () = unsafe { msg_send![button, setFrame: rect(6.0, y, CONTENT_WIDTH, ROW_HEIGHT)] };
        let _: () = unsafe { msg_send![view, addSubview: button] };
        matches!(row.kind, WorkersActivityRowKind::Working).then_some(leading_label)
    }

    unsafe fn add_label(view: *mut Object, text: &str, frame: NSRect, size: f64) -> *mut Object {
        let label: *mut Object =
            unsafe { msg_send![class!(NSTextField), labelWithString: ns_string(text)] };
        let font: *mut Object = unsafe { msg_send![class!(NSFont), systemFontOfSize: size] };
        let _: () = unsafe { msg_send![label, setFont: font] };
        let _: () = unsafe { msg_send![label, setLineBreakMode: 4_u64] };
        let _: () = unsafe { msg_send![label, setFrame: frame] };
        let _: () = unsafe { msg_send![view, addSubview: label] };
        label
    }

    fn status_mark() -> *mut Object {
        static MARK: OnceLock<usize> = OnceLock::new();
        *MARK.get_or_init(|| unsafe {
            let bytes = include_bytes!("../../assets/icons/zeron-logo.svg");
            let data: *mut Object =
                msg_send![class!(NSData), dataWithBytes: bytes.as_ptr() length: bytes.len()];
            let image: *mut Object = msg_send![class!(NSImage), alloc];
            let image: *mut Object = msg_send![image, initWithData: data];
            let _: () = msg_send![image, setSize: NSSize { width: 16.0, height: 16.0 }];
            let _: () = msg_send![image, setTemplate: true];
            image as usize
        }) as *mut Object
    }

    fn ns_string(value: &str) -> *mut Object {
        let value = CString::new(value.replace('\0', "")).expect("valid native string");
        unsafe { msg_send![class!(NSString), stringWithUTF8String: value.as_ptr()] }
    }

    fn ns_color(hex: u32) -> *mut Object {
        let red = f64::from((hex >> 16) & 0xFF) / 255.0;
        let green = f64::from((hex >> 8) & 0xFF) / 255.0;
        let blue = f64::from(hex & 0xFF) / 255.0;
        unsafe {
            msg_send![class!(NSColor), colorWithSRGBRed: red green: green blue: blue alpha: 1.0_f64]
        }
    }

    unsafe fn set_colored_title(button: *mut Object, title: &str, hex: u32) {
        let attributed: *mut Object =
            unsafe { msg_send![class!(NSMutableAttributedString), alloc] };
        let attributed: *mut Object =
            unsafe { msg_send![attributed, initWithString: ns_string(title)] };
        let range = NSRange {
            location: 0,
            length: title.encode_utf16().count(),
        };
        let _: () = unsafe {
            msg_send![attributed, addAttribute: ns_string("NSColor") value: ns_color(hex) range: range]
        };
        let font: *mut Object = unsafe {
            msg_send![class!(NSFont), monospacedSystemFontOfSize: 15.0_f64 weight: 0.4_f64]
        };
        let _: () = unsafe {
            msg_send![attributed, addAttribute: ns_string("NSFont") value: font range: range]
        };
        let _: () = unsafe { msg_send![button, setAttributedTitle: attributed] };
        let _: () = unsafe { msg_send![attributed, release] };
    }

    fn provider_icon_bytes(path: &str) -> Option<&'static [u8]> {
        match path {
            "workers/amp" => Some(include_bytes!("../../assets/icons/workers/amp.svg")),
            "workers/claude" => Some(include_bytes!("../../assets/icons/workers/claude.svg")),
            "workers/cline" => Some(include_bytes!("../../assets/icons/workers/cline.svg")),
            "workers/codex" => Some(include_bytes!("../../assets/icons/workers/codex.svg")),
            "workers/cursor-agent" => Some(include_bytes!(
                "../../assets/icons/workers/cursor-agent.svg"
            )),
            "workers/gemini" => Some(include_bytes!("../../assets/icons/workers/gemini.svg")),
            "workers/grok" => Some(include_bytes!("../../assets/icons/workers/grok.svg")),
            "workers/kimi" => Some(include_bytes!("../../assets/icons/workers/kimi.svg")),
            "workers/kiro" => Some(include_bytes!("../../assets/icons/workers/kiro.svg")),
            "workers/muse-code" => Some(include_bytes!("../../assets/icons/workers/muse-code.svg")),
            "workers/opencode" => Some(include_bytes!("../../assets/icons/workers/opencode.svg")),
            "workers/pi" => Some(include_bytes!("../../assets/icons/workers/pi.svg")),
            "workers/generic-agent" => Some(include_bytes!(
                "../../assets/icons/workers/generic-agent.svg"
            )),
            _ => None,
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use std::sync::mpsc::Sender;

    use super::{MenuBarIntent, WorkersActivityMenu};

    pub struct NativeMenuBar;

    impl NativeMenuBar {
        pub fn new(_sender: Sender<MenuBarIntent>, _menu: &WorkersActivityMenu) -> Self {
            Self
        }
        pub fn update(&mut self, _menu: &WorkersActivityMenu) {}
        pub fn toggle(&mut self) {}
        pub fn close(&mut self) {}
        pub fn tick_spinner(&mut self) {}
    }
}

#[cfg(test)]
mod tests {
    use super::{ALL_RECENT_TAG, MenuBarBindings, MenuBarIntent};
    use crate::workers::model::WorkersSessionTarget;

    #[test]
    fn native_tags_resolve_to_the_exact_session_intent() {
        let bindings = MenuBarBindings::new([
            WorkersSessionTarget::new("project-a", "same-title-a"),
            WorkersSessionTarget::new("project-b", "same-title-b"),
        ]);
        assert_eq!(
            bindings.intent_for_tag(1),
            Some(MenuBarIntent::SelectSession(WorkersSessionTarget::new(
                "project-b",
                "same-title-b",
            )))
        );
        assert_eq!(bindings.intent_for_tag(99), None);
    }

    #[test]
    fn all_recent_has_a_reserved_non_session_tag() {
        let bindings = MenuBarBindings::new([WorkersSessionTarget::new("project-a", "session-a")]);
        assert_eq!(
            bindings.intent_for_tag(ALL_RECENT_TAG),
            Some(MenuBarIntent::ShowAllRecent)
        );
    }
}
