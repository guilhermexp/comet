use zeron_workers_unpeel::{WorkersProject, WorkersSession};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkersSessionMenuItem {
    Rename,
    Pin,
    Unpin,
    MoveTo,
    ClearAttention,
    ResumeAgent,
    Resume,
    Fork,
    AppendSystemContext,
    NotifyWhenDone,
    CopyTranscript,
    CopySessionId,
    CopyTranscript20,
    CopyTranscript50,
    CopyTranscriptAll,
    StopAndArchive,
    Archive,
    Restore,
    RestoreAndResume,
    Remove,
}

pub fn session_menu_items(
    session: &WorkersSession,
    move_targets: &[WorkersProject],
) -> Vec<WorkersSessionMenuItem> {
    let live = session.is_live();
    let starting = session.activity == "starting" || session.runtime_launch_pending;
    let mut items = vec![
        WorkersSessionMenuItem::Rename,
        if session.pinned {
            WorkersSessionMenuItem::Unpin
        } else {
            WorkersSessionMenuItem::Pin
        },
    ];

    if move_targets
        .iter()
        .any(|project| project.id != session.project_id)
    {
        items.push(WorkersSessionMenuItem::MoveTo);
    }
    if session.activity == "blocked" {
        items.push(WorkersSessionMenuItem::ClearAttention);
    }
    if !starting && !session.archived {
        if live && session.capabilities.resume_agent {
            items.push(WorkersSessionMenuItem::ResumeAgent);
        } else if !live && session.capabilities.restart {
            items.push(WorkersSessionMenuItem::Resume);
        }
        if session.capabilities.fork {
            items.push(WorkersSessionMenuItem::Fork);
        }
        if session.capabilities.append_system_context {
            items.push(WorkersSessionMenuItem::AppendSystemContext);
        }
    }
    if session.capabilities.notify_when_done {
        items.push(WorkersSessionMenuItem::NotifyWhenDone);
    }

    items.extend([
        WorkersSessionMenuItem::CopyTranscript,
        WorkersSessionMenuItem::CopySessionId,
    ]);

    if session.archived {
        items.push(if session.capabilities.restart {
            WorkersSessionMenuItem::RestoreAndResume
        } else {
            WorkersSessionMenuItem::Restore
        });
    } else if session.capabilities.archive {
        items.push(if live {
            WorkersSessionMenuItem::StopAndArchive
        } else {
            WorkersSessionMenuItem::Archive
        });
    }
    items.push(WorkersSessionMenuItem::Remove);
    items
}

#[cfg(test)]
mod tests {
    use super::{WorkersSessionMenuItem as Item, session_menu_items};
    use zeron_workers_unpeel::{WorkersProject, WorkersSession, WorkersSessionCapabilities};

    fn project(id: &str) -> WorkersProject {
        WorkersProject {
            id: id.into(),
            name: id.into(),
            path: format!("/tmp/{id}"),
            folder_id: None,
            parent_project_id: None,
            is_group: false,
            worktree_branch: None,
            git_branch: None,
            archived_session_count: 0,
            folder_color_id: None,
            session_sort: Default::default(),
        }
    }

    fn session() -> WorkersSession {
        WorkersSession {
            id: "session-1".into(),
            project_id: "project-1".into(),
            title: "Worker".into(),
            command: "codex".into(),
            state: "running".into(),
            activity: "working".into(),
            unread: false,
            pinned: false,
            archived: false,
            provider_id: Some("com.openai.codex".into()),
            active_runtime_id: Some("com.openai.codex".into()),
            runtime_launch_pending: false,
            runtime_generation: 1,
            notify_when_done: false,
            terminal_background_hex: None,
            worktree_branch: None,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 2,
            idle_since_unix_ms: None,
            idle_confirmed_by_hook: false,
            resumable_conversation: false,
            hibernation_activity_token: None,
            total_tokens: None,
            model_usage: Vec::new(),
            capabilities: WorkersSessionCapabilities {
                restart: true,
                resume_agent: true,
                fork: true,
                archive: true,
                append_system_context: true,
                notify_when_done: true,
            },
        }
    }

    #[test]
    fn live_worker_menu_matches_unpeel_capability_order() {
        let items = session_menu_items(&session(), &[project("project-1"), project("project-2")]);
        assert_eq!(
            items,
            vec![
                Item::Rename,
                Item::Pin,
                Item::MoveTo,
                Item::ResumeAgent,
                Item::Fork,
                Item::AppendSystemContext,
                Item::NotifyWhenDone,
                Item::CopyTranscript,
                Item::CopySessionId,
                Item::StopAndArchive,
                Item::Remove,
            ]
        );
    }

    #[test]
    fn archived_worker_has_one_restore_verb_and_no_standalone_resume() {
        let mut archived = session();
        archived.state = "exited".into();
        archived.activity = "idle".into();
        archived.archived = true;
        assert_eq!(
            session_menu_items(&archived, &[]),
            vec![
                Item::Rename,
                Item::Pin,
                Item::NotifyWhenDone,
                Item::CopyTranscript,
                Item::CopySessionId,
                Item::RestoreAndResume,
                Item::Remove,
            ]
        );
    }
}

#[cfg(target_os = "macos")]
pub mod native {
    use std::ffi::CString;
    use std::ffi::c_void;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicI64, Ordering};

    use futures::channel::oneshot;
    use objc::declare::ClassDecl;
    use objc::runtime::{Object, Sel};
    use objc::{class, msg_send, sel, sel_impl};

    use super::WorkersSessionMenuItem as Item;
    use zeron_workers_unpeel::{WorkersProject, WorkersSession};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Selection {
        Item(Item),
        MoveTo(String),
    }

    static SELECTED_TAG: AtomicI64 = AtomicI64::new(-1);

    struct MenuContext {
        menu: *mut Object,
        event: *mut Object,
        view: *mut Object,
        selections: Vec<Selection>,
        sender: oneshot::Sender<Option<Selection>>,
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
                ClassDecl::new("ZeronWorkersSessionMenuTarget", class!(NSObject))
            {
                declaration.add_method(
                    sel!(performAction:),
                    perform_action as extern "C" fn(&Object, Sel, *mut Object),
                );
                declaration.register()
            } else {
                objc::runtime::Class::get("ZeronWorkersSessionMenuTarget")
                    .expect("registered workers session menu target")
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
        selection: Selection,
        checked: bool,
        selections: &mut Vec<Selection>,
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
        if checked {
            let _: () = msg_send![item, setState: 1_i64];
        }
        let _: () = msg_send![menu, addItem: item];
        let _: () = msg_send![item, release];
    }

    unsafe fn add_submenu(menu: *mut Object, title: &str, submenu: *mut Object) {
        let item: *mut Object = msg_send![class!(NSMenuItem), alloc];
        let item: *mut Object = msg_send![item,
            initWithTitle: ns_string(title)
            action: sel!(performAction:)
            keyEquivalent: ns_string("")
        ];
        let _: () = msg_send![item, setSubmenu: submenu];
        let _: () = msg_send![menu, addItem: item];
        let _: () = msg_send![item, release];
        let _: () = msg_send![submenu, release];
    }

    fn label(item: Item) -> &'static str {
        match item {
            Item::Rename => "Rename",
            Item::Pin => "Pin in project",
            Item::Unpin => "Unpin from project",
            Item::MoveTo => "Move to",
            Item::ClearAttention => "Clear attention",
            Item::ResumeAgent => "Resume Agent",
            Item::Resume => "Resume",
            Item::Fork => "Fork",
            Item::AppendSystemContext => "Append system context…",
            Item::NotifyWhenDone => "Notify when done",
            Item::CopyTranscript => "Copy transcript",
            Item::CopySessionId => "Copy session ID",
            Item::CopyTranscript20 => "Last 20 entries",
            Item::CopyTranscript50 => "Last 50 entries",
            Item::CopyTranscriptAll => "Whole conversation",
            Item::StopAndArchive => "Stop and archive",
            Item::Archive => "Archive",
            Item::Restore => "Restore from archive",
            Item::RestoreAndResume => "Restore & Resume",
            Item::Remove => "Remove session",
        }
    }

    pub fn show_async(
        session: &WorkersSession,
        move_targets: &[WorkersProject],
        items: &[Item],
    ) -> oneshot::Receiver<Option<Selection>> {
        let (sender, receiver) = oneshot::channel();
        unsafe {
            SELECTED_TAG.store(-1, Ordering::SeqCst);
            let menu = new_menu("");
            let mut selections = Vec::new();
            let mut final_separator_added = false;

            for item in items.iter().copied() {
                if item == Item::CopyTranscript {
                    add_separator(menu);
                }
                if matches!(
                    item,
                    Item::StopAndArchive
                        | Item::Archive
                        | Item::Restore
                        | Item::RestoreAndResume
                        | Item::Remove
                ) && !final_separator_added
                {
                    add_separator(menu);
                    final_separator_added = true;
                }
                match item {
                    Item::MoveTo => {
                        let submenu = new_menu("Move to");
                        for project in move_targets {
                            add_action(
                                submenu,
                                &project.name,
                                Selection::MoveTo(project.id.clone()),
                                false,
                                &mut selections,
                            );
                        }
                        add_submenu(menu, label(item), submenu);
                    }
                    Item::CopyTranscript => {
                        let submenu = new_menu("Copy transcript");
                        for child in [
                            Item::CopyTranscript20,
                            Item::CopyTranscript50,
                            Item::CopyTranscriptAll,
                        ] {
                            add_action(
                                submenu,
                                label(child),
                                Selection::Item(child),
                                false,
                                &mut selections,
                            );
                        }
                        add_submenu(menu, label(item), submenu);
                    }
                    Item::NotifyWhenDone => add_action(
                        menu,
                        label(item),
                        Selection::Item(item),
                        session.notify_when_done,
                        &mut selections,
                    ),
                    Item::CopyTranscript20 | Item::CopyTranscript50 | Item::CopyTranscriptAll => {}
                    _ => {
                        let title = if item == Item::Remove && !session.is_live() {
                            "Remove from list"
                        } else {
                            label(item)
                        };
                        add_action(menu, title, Selection::Item(item), false, &mut selections)
                    }
                }
            }

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
