#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceOpenTarget {
    VisualStudioCode,
    Cursor,
    Zed,
    IntelliJIdea,
    WebStorm,
    Xcode,
    GitHubDesktop,
    Fork,
    Tower,
    SourceTree,
    GitKraken,
    SublimeMerge,
    Finder,
    Terminal,
    ITerm2,
    Ghostty,
    Warp,
    WezTerm,
    Kitty,
    Alacritty,
    Tabby,
    Hyper,
    Rio,
    Wave,
}

impl WorkspaceOpenTarget {
    pub const GROUPS: [&'static [Self]; 4] = [
        &[
            Self::VisualStudioCode,
            Self::Cursor,
            Self::Zed,
            Self::IntelliJIdea,
            Self::WebStorm,
            Self::Xcode,
        ],
        &[
            Self::GitHubDesktop,
            Self::Fork,
            Self::Tower,
            Self::SourceTree,
            Self::GitKraken,
            Self::SublimeMerge,
        ],
        &[Self::Finder],
        &[
            Self::Terminal,
            Self::ITerm2,
            Self::Ghostty,
            Self::Warp,
            Self::WezTerm,
            Self::Kitty,
            Self::Alacritty,
            Self::Tabby,
            Self::Hyper,
            Self::Rio,
            Self::Wave,
        ],
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::VisualStudioCode => "VS Code",
            Self::Cursor => "Cursor",
            Self::Zed => "Zed",
            Self::IntelliJIdea => "IntelliJ",
            Self::WebStorm => "WebStorm",
            Self::Xcode => "Xcode",
            Self::GitHubDesktop => "GitHub Desktop",
            Self::Fork => "Fork",
            Self::Tower => "Tower",
            Self::SourceTree => "Sourcetree",
            Self::GitKraken => "GitKraken",
            Self::SublimeMerge => "Sublime Merge",
            Self::Finder => "Finder",
            Self::Terminal => "Terminal",
            Self::ITerm2 => "iTerm2",
            Self::Ghostty => "Ghostty",
            Self::Warp => "Warp",
            Self::WezTerm => "WezTerm",
            Self::Kitty => "kitty",
            Self::Alacritty => "Alacritty",
            Self::Tabby => "Tabby",
            Self::Hyper => "Hyper",
            Self::Rio => "Rio",
            Self::Wave => "Wave",
        }
    }

    pub fn bundle_ids(self) -> &'static [&'static str] {
        match self {
            Self::VisualStudioCode => &["com.microsoft.VSCode"],
            Self::Cursor => &["com.todesktop.230313mzl4w4u92"],
            Self::Zed => &[
                "dev.zed.Zed",
                "dev.zed.Zed-Preview",
                "dev.zed.Zed-Nightly",
                "dev.zed.Zed-Dev",
            ],
            Self::IntelliJIdea => &["com.jetbrains.intellij", "com.jetbrains.intellij.ce"],
            Self::WebStorm => &["com.jetbrains.WebStorm"],
            Self::Xcode => &["com.apple.dt.Xcode"],
            Self::GitHubDesktop => &["com.github.GitHubClient"],
            Self::Fork => &["com.DanPristupov.Fork"],
            Self::Tower => &["com.fournova.Tower3", "com.fournova.Tower"],
            Self::SourceTree => &["com.torusknot.SourceTreeNotMAS"],
            Self::GitKraken => &["com.axosoft.gitkraken"],
            Self::SublimeMerge => &["com.sublimemerge"],
            Self::Finder => &["com.apple.finder"],
            Self::Terminal => &["com.apple.Terminal"],
            Self::ITerm2 => &["com.googlecode.iterm2"],
            Self::Ghostty => &["com.mitchellh.ghostty"],
            Self::Warp => &["dev.warp.Warp-Stable", "dev.warp.Warp"],
            Self::WezTerm => &["com.github.wez.wezterm"],
            Self::Kitty => &["net.kovidgoyal.kitty"],
            Self::Alacritty => &["org.alacritty"],
            Self::Tabby => &["org.tabby"],
            Self::Hyper => &["co.zeit.hyper"],
            Self::Rio => &["com.raphaelamorim.rio"],
            Self::Wave => &["dev.commandline.waveterm"],
        }
    }

    pub fn app_names(self) -> &'static [&'static str] {
        match self {
            Self::VisualStudioCode => &["Visual Studio Code"],
            Self::Cursor => &["Cursor"],
            Self::Zed => &["Zed", "Zed Preview", "Zed Nightly"],
            Self::IntelliJIdea => &["IntelliJ IDEA", "IntelliJ IDEA CE"],
            Self::WebStorm => &["WebStorm"],
            Self::Xcode => &["Xcode"],
            Self::GitHubDesktop => &["GitHub Desktop"],
            Self::Fork => &["Fork"],
            Self::Tower => &["Tower"],
            Self::SourceTree => &["Sourcetree", "SourceTree"],
            Self::GitKraken => &["GitKraken"],
            Self::SublimeMerge => &["Sublime Merge"],
            Self::Finder => &["Finder"],
            Self::Terminal => &["Terminal"],
            Self::ITerm2 => &["iTerm", "iTerm2"],
            Self::Ghostty => &["Ghostty"],
            Self::Warp => &["Warp"],
            Self::WezTerm => &["WezTerm"],
            Self::Kitty => &["kitty"],
            Self::Alacritty => &["Alacritty"],
            Self::Tabby => &["Tabby"],
            Self::Hyper => &["Hyper"],
            Self::Rio => &["Rio", "rio"],
            Self::Wave => &["Wave Terminal", "Wave"],
        }
    }

    pub fn is_available(self) -> bool {
        // Finder and Terminal are built into macOS. Keep both visible even
        // before Launch Services has indexed their system application paths.
        if matches!(self, Self::Finder | Self::Terminal) {
            return true;
        }
        self.candidate_paths().into_iter().any(|path| path.exists())
    }

    fn candidate_paths(self) -> Vec<std::path::PathBuf> {
        let user_applications = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .map(|home| home.join("Applications"));
        let mut paths = self
            .app_names()
            .iter()
            .flat_map(|name| {
                [
                    Some(std::path::PathBuf::from("/Applications")),
                    user_applications.clone(),
                    Some(std::path::PathBuf::from("/System/Applications")),
                    Some(std::path::PathBuf::from("/System/Applications/Utilities")),
                    Some(std::path::PathBuf::from("/System/Library/CoreServices")),
                ]
                .into_iter()
                .flatten()
                .map(move |root| root.join(format!("{name}.app")))
            })
            .collect::<Vec<_>>();
        match self {
            Self::Finder => paths.push("/System/Library/CoreServices/Finder.app".into()),
            Self::Terminal => paths.push("/System/Applications/Utilities/Terminal.app".into()),
            _ => {}
        }
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::WorkspaceOpenTarget as Target;

    #[test]
    fn target_groups_match_unpeels_menu_order() {
        assert_eq!(Target::GROUPS.len(), 4);
        assert_eq!(Target::GROUPS[0].first(), Some(&Target::VisualStudioCode));
        assert_eq!(Target::GROUPS[1].first(), Some(&Target::GitHubDesktop));
        assert_eq!(Target::GROUPS[2], &[Target::Finder]);
        assert_eq!(Target::GROUPS[3].first(), Some(&Target::Terminal));
        assert_eq!(Target::GROUPS[3].last(), Some(&Target::Wave));
    }

    #[test]
    fn target_metadata_matches_unpeels_aliases() {
        assert_eq!(Target::VisualStudioCode.title(), "VS Code");
        assert!(Target::Zed.bundle_ids().contains(&"dev.zed.Zed-Preview"));
        assert!(
            Target::IntelliJIdea
                .app_names()
                .contains(&"IntelliJ IDEA CE")
        );
        assert_eq!(Target::Rio.bundle_ids(), &["com.raphaelamorim.rio"]);
        assert_eq!(Target::Wave.bundle_ids(), &["dev.commandline.waveterm"]);
    }

    #[test]
    fn terminal_candidates_include_macos_utilities() {
        assert!(Target::Terminal.candidate_paths().iter().any(
            |path| path == std::path::Path::new("/System/Applications/Utilities/Terminal.app")
        ));
        assert!(Target::Terminal.is_available());
    }
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

    use super::WorkspaceOpenTarget as Target;

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
        selections: Vec<Target>,
        sender: oneshot::Sender<Option<Target>>,
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

            // Match Unpeel's anchored popup instead of NSMenu's context-menu
            // positioning. Context menus opened inside the app-owned titlebar
            // can be constrained to the titlebar strip, which silently clips
            // the final groups. The click is in the split button's trailing
            // segment, about ten points before its right edge.
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
        let selection = usize::try_from(SELECTED_TAG.swap(-1, Ordering::SeqCst))
            .ok()
            .and_then(|index| context.selections.get(index).copied());
        let _ = context.sender.send(selection);
    }

    fn action_target() -> *mut Object {
        static TARGET: OnceLock<usize> = OnceLock::new();
        *TARGET.get_or_init(|| unsafe {
            let class = if let Some(mut declaration) =
                ClassDecl::new("ZeronWorkersWorkspaceOpenMenuTarget", class!(NSObject))
            {
                declaration.add_method(
                    sel!(performAction:),
                    perform_action as extern "C" fn(&Object, Sel, *mut Object),
                );
                declaration.register()
            } else {
                objc::runtime::Class::get("ZeronWorkersWorkspaceOpenMenuTarget")
                    .expect("registered workers workspace-open menu target")
            };
            let target: *mut Object = msg_send![class, new];
            target as usize
        }) as *mut Object
    }

    fn ns_string(value: &str) -> *mut Object {
        let value = CString::new(value).expect("menu labels cannot contain null bytes");
        unsafe { msg_send![class!(NSString), stringWithUTF8String: value.as_ptr()] }
    }

    unsafe fn add_separator(menu: *mut Object) {
        let item: *mut Object = msg_send![class!(NSMenuItem), separatorItem];
        let _: () = msg_send![menu, addItem: item];
    }

    unsafe fn add_action(menu: *mut Object, target: Target, selections: &mut Vec<Target>) {
        let tag = selections.len() as i64;
        selections.push(target);
        let item: *mut Object = msg_send![class!(NSMenuItem), alloc];
        let item: *mut Object = msg_send![item,
            initWithTitle: ns_string(target.title())
            action: sel!(performAction:)
            keyEquivalent: ns_string("")
        ];
        let _: () = msg_send![item, setTarget: action_target()];
        let _: () = msg_send![item, setTag: tag];
        let _: () = msg_send![item, setEnabled: true];
        let _: () = msg_send![menu, addItem: item];
        let _: () = msg_send![item, release];
    }

    pub fn show_async() -> oneshot::Receiver<Option<Target>> {
        let (sender, receiver) = oneshot::channel();
        unsafe {
            SELECTED_TAG.store(-1, Ordering::SeqCst);
            let menu: *mut Object = msg_send![class!(NSMenu), alloc];
            let menu: *mut Object = msg_send![menu, initWithTitle: ns_string("")];
            let _: () = msg_send![menu, setAutoenablesItems: false];
            let mut selections = Vec::new();
            let mut has_previous_group = false;
            for group in Target::GROUPS {
                let available = group
                    .iter()
                    .copied()
                    .filter(|target| target.is_available())
                    .collect::<Vec<_>>();
                if available.is_empty() {
                    continue;
                }
                if has_previous_group {
                    add_separator(menu);
                }
                for target in available {
                    add_action(menu, target, &mut selections);
                }
                has_previous_group = true;
            }

            let event: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let event: *mut Object = msg_send![event, currentEvent];
            let event: *mut Object = msg_send![event, retain];
            let window: *mut Object = msg_send![event, window];
            let view: *mut Object = msg_send![window, contentView];
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
                Box::into_raw(context).cast(),
                pop_up_menu,
            );
        }
        receiver
    }
}
