//! Details → Servers: TCP listeners running inside the chat's folder or any
//! Project worked, polled from the chat's host device while Details shows
//! them. Hovering a row reveals Stop (`StopDevServer`, re-validated on the host).
use super::*;
use zeron_proto::DevServer;

/// Poll cadence: a listener scan is one `ps` + two `lsof` on the host.
const DEV_SERVER_POLL: std::time::Duration = std::time::Duration::from_secs(5);
const DEV_SERVER_ROW_HEIGHT: f32 = 28.0;
/// Rows shown before the list scrolls inside the card.
const DEV_SERVER_VISIBLE_ROWS: f32 = 3.5;

pub(super) type DevServerKey = (Option<String>, Vec<String>);

impl DetailsSidebar {
    /// Keep one poll loop for `(target device, roots)`; an empty root list
    /// stops it and clears the rows.
    pub(super) fn ensure_dev_server_watch(
        &mut self,
        target_device: Option<String>,
        mut roots: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        roots.retain(|root| !root.trim().is_empty());
        roots.sort();
        roots.dedup();
        if roots.is_empty() {
            self.dev_server_watch = None;
            self.dev_server_key = None;
            self.dev_servers.clear();
            return;
        }
        let key = (target_device, roots);
        if self.dev_server_key.as_ref() == Some(&key) {
            return;
        }
        self.dev_server_watch = None;
        self.dev_servers.clear();
        self.dev_server_key = None;
        let Some(engine) = self.app_state.read(cx).engine().cloned() else {
            return;
        };
        self.dev_server_key = Some(key.clone());
        self.dev_server_watch = Some(cx.spawn(async move |this, cx| {
            loop {
                let mut params = serde_json::json!({ "cwds": key.1 });
                if let Some(device) = key.0.as_ref() {
                    params["targetDeviceId"] = device.clone().into();
                }
                let servers = engine
                    .client()
                    .call(methods::LIST_DEV_SERVERS, params)
                    .await
                    .ok()
                    .and_then(|value| serde_json::from_value::<Vec<DevServer>>(value).ok());
                let keep_going = this
                    .update(cx, |this, cx| {
                        if this.dev_server_key.as_ref() != Some(&key) {
                            return false;
                        }
                        // A failed scan (device unreachable, old engine) keeps
                        // the last rows instead of flashing them away.
                        if let Some(servers) = servers
                            && this.dev_servers != servers
                        {
                            this.dev_servers = servers;
                            this.dev_server_stopping
                                .retain(|pid| this.dev_servers.iter().any(|s| s.pid == *pid));
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !keep_going {
                    return;
                }
                cx.background_executor().timer(DEV_SERVER_POLL).await;
            }
        }));
    }

    fn stop_dev_server(&mut self, server: DevServer, cx: &mut Context<Self>) {
        let Some(engine) = self.app_state.read(cx).engine().cloned() else {
            return;
        };
        let device = self.dev_server_key.as_ref().and_then(|key| key.0.clone());
        self.dev_server_stopping.insert(server.pid);
        self.dev_server_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let mut params = serde_json::json!({
                "pid": server.pid,
                "startedAt": server.started_at,
                "root": server.root,
            });
            if let Some(device) = device {
                params["targetDeviceId"] = device.into();
            }
            let result = engine.client().call(methods::STOP_DEV_SERVER, params).await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(_) => {
                        // Drop the row now; the next poll confirms.
                        this.dev_servers.retain(|s| s.pid != server.pid);
                    }
                    Err(error) => {
                        tracing::warn!(%error, pid = server.pid, "stop dev server failed");
                        this.dev_server_error = Some(format!(":{} — {error}", server.port).into());
                    }
                }
                this.dev_server_stopping.remove(&server.pid);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn render_dev_servers(
        &mut self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        // Only what this chat's agent started: a server whose port or dev
        // command appears in one of the chat's own commands.
        let visible: Vec<DevServer> = {
            let transcript = &self.app_state.read(cx).transcript;
            let commands = chat_commands(transcript);
            self.dev_servers
                .iter()
                .filter(|server| started_by_chat(server, &commands))
                .cloned()
                .collect()
        };
        if visible.is_empty() && self.dev_server_error.is_none() {
            return None;
        }
        let header = div()
            .h(px(DEV_SERVER_ROW_HEIGHT))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .child(
                icons::icon(icons::GLOBE)
                    .size(px(14.0))
                    .text_color(theme.text_muted),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(px(12.0))
                    .text_color(theme.text_muted)
                    .child("Servers"),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_muted)
                    .child(visible.len().to_string()),
            );
        let rows = visible
            .into_iter()
            .enumerate()
            .map(|(index, server)| {
                let group = SharedString::from(format!("dev-server-{index}"));
                let stopping = self.dev_server_stopping.contains(&server.pid);
                let url = format!("http://127.0.0.1:{}", server.port);
                let port_label = format!(":{}", server.port);
                let project = std::path::Path::new(&server.root)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                // The dev command reads better than the listener (`next dev`, not
                // `next-server (v16.3.8)`).
                let label = server
                    .signatures
                    .last()
                    .cloned()
                    .unwrap_or_else(|| server_label(&server.command));
                let stop = div()
                    .id(("dev-server-stop", index))
                    .debug_selector(move || format!("dev-server-stop-{index}"))
                    .flex_none()
                    .size(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .invisible()
                    .group_hover(group.clone(), |s| s.visible())
                    .hover(|s| s.bg(crate::theme::ink(0.08)))
                    .child(
                        icons::icon(icons::STOP)
                            .size(px(12.0))
                            .text_color(theme.danger),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.stop_dev_server(server.clone(), cx);
                    }));
                div()
                    .id(("dev-server", index))
                    .debug_selector(move || format!("dev-server-row-{index}"))
                    .group(group)
                    .h(px(DEV_SERVER_ROW_HEIGHT))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|style| style.bg(crate::theme::ink(0.045)))
                    .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url)))
                    .child(
                        div()
                            .flex_none()
                            .size(px(7.0))
                            .rounded_full()
                            .bg(if stopping {
                                theme.text_muted
                            } else {
                                theme.success
                            }),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(12.0))
                            .text_color(theme.text)
                            .child(port_label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(format!("{label} · {project}")),
                    )
                    .child(stop)
            })
            .collect::<Vec<_>>();
        let mut section = div()
            .id("dev-servers-section")
            .mt(px(4.0))
            .pt(px(4.0))
            .border_t_1()
            .border_color(theme.border.opacity(0.50))
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("dev-servers-body")
                    .max_h(px(DEV_SERVER_ROW_HEIGHT * DEV_SERVER_VISIBLE_ROWS))
                    .overflow_y_scroll()
                    .children(rows),
            );
        if let Some(error) = self.dev_server_error.clone() {
            section = section.child(
                div()
                    .px(px(10.0))
                    .py(px(4.0))
                    .text_size(px(11.0))
                    .text_color(theme.danger_muted)
                    .child(error),
            );
        }
        Some(div().child(section))
    }
}

/// Every shell command this chat's agent ran.
fn chat_commands(transcript: &[zeron_doc::SessionMessageEntry]) -> Vec<String> {
    transcript
        .iter()
        .filter(|entry| entry.role == zeron_doc::MessageRole::Assistant)
        .flat_map(|entry| entry.parts.iter())
        .filter_map(|part| match part {
            zeron_doc::MessagePart::Tool {
                call: zeron_proto::ToolCall::Exec { command },
                ..
            } => Some(command.split_whitespace().collect::<Vec<_>>().join(" ")),
            _ => None,
        })
        .collect()
}

/// The chat started `server` when one of its commands names the port
/// (`--port 3210`, `localhost:3210`) or runs one of the server's dev
/// commands (`npm run dev`, `next dev`) as whole words.
pub(super) fn started_by_chat(server: &DevServer, commands: &[String]) -> bool {
    let port = server.port.to_string();
    commands.iter().any(|command| {
        contains_word(command, &port, |c| c.is_ascii_digit())
            || server.signatures.iter().any(|signature| {
                contains_word(command, signature, |c| {
                    c.is_alphanumeric() || c == '-' || c == '_'
                })
            })
    })
}

/// `needle` occurs in `haystack` not glued to a `joins` character on either side.
fn contains_word(haystack: &str, needle: &str, joins: impl Fn(char) -> bool) -> bool {
    if needle.is_empty() {
        return false;
    }
    haystack.match_indices(needle).any(|(at, _)| {
        let before = haystack[..at].chars().next_back();
        let after = haystack[at + needle.len()..].chars().next();
        !before.is_some_and(&joins) && !after.is_some_and(&joins)
    })
}

/// The program a listener runs, without paths: `next-server (v16.3.8)`,
/// `node …/.bin/next dev` → `next dev`, `python3 -m http.server` as is.
pub(super) fn server_label(command: &str) -> String {
    let mut words = command.split_whitespace().peekable();
    let Some(first) = words.next() else {
        return String::new();
    };
    let base = |word: &str| {
        std::path::Path::new(word)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(word)
            .to_owned()
    };
    let mut label = base(first);
    // `node /…/node_modules/.bin/next dev` reads better as `next dev`.
    if matches!(label.as_str(), "node" | "bun" | "deno")
        && let Some(script) = words.peek()
        && script.contains('/')
    {
        label = base(script);
        words.next();
    }
    for word in words.take(2) {
        if word.starts_with('-') {
            break;
        }
        label.push(' ');
        label.push_str(&base(word));
    }
    label
}

#[cfg(test)]
mod tests {
    use super::{server_label, started_by_chat};
    use zeron_proto::DevServer;

    fn server(port: u16, signatures: &[&str]) -> DevServer {
        DevServer {
            pid: 1,
            started_at: 1,
            port,
            cwd: "/p/bops".into(),
            root: "/p/bops".into(),
            command: "next-server".into(),
            signatures: signatures.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn only_servers_named_by_the_chats_commands_count() {
        let commands = vec![
            "cd /p/bops && npx next dev --port 3210 -H 127.0.0.1".to_owned(),
            "curl http://127.0.0.1:3210/api/tailnet".to_owned(),
        ];
        assert!(started_by_chat(&server(3210, &["next dev"]), &commands));
        // Same dev command, other port: the chat ran `next dev`.
        assert!(started_by_chat(&server(4000, &["next dev"]), &commands));
        // Neither the port nor the command: started elsewhere.
        assert!(!started_by_chat(&server(5173, &["vite"]), &commands));
        // A port inside a longer number is not that port.
        assert!(!started_by_chat(&server(321, &["vite"]), &commands));
        let installs = vec!["npm i vite-plugin-x".to_owned()];
        assert!(!started_by_chat(&server(5173, &["vite"]), &installs));
    }

    #[test]
    fn labels_drop_paths_and_flags() {
        assert_eq!(
            server_label("node /p/bops/node_modules/.bin/next dev --port 3210"),
            "next dev"
        );
        assert_eq!(
            server_label("next-server (v16.3.8)"),
            "next-server (v16.3.8)"
        );
        assert_eq!(server_label("/usr/bin/python3 -m http.server"), "python3");
        assert_eq!(server_label(""), "");
    }
}
