//! Parser and template renderer for the project hook subset in `.config/wt.toml`.
//!
//! Callers choose the checkout explicitly: creation passes its source checkout,
//! while removal passes the checkout being removed. This module prepares every
//! command in one hook before returning anything an executor could run.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(unix))]
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const CONFIG_RELATIVE_PATH: &str = ".config/wt.toml";
const STDERR_TAIL_BYTES: usize = 64 * 1024;
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(25);
const POST_START_STOP_TIMEOUT: Duration = Duration::from_secs(2);

const SUPPORTED_VARIABLES: &[&str] = &[
    "branch",
    "worktree_path",
    "worktree_name",
    "repo",
    "repo_path",
    "primary_worktree_path",
    "commit",
    "short_commit",
    "base",
    "default_branch",
    "hook_type",
    "cwd",
];

/// Ref names reach hooks from agent input. Quoting protects only the first
/// shell level, so a hook that re-evaluates them (`sh -c`, `tmux`, `eval`)
/// must never see shell syntax in these values.
const REF_NAME_VARIABLES: &[&str] = &["branch", "base", "default_branch"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum HookKind {
    PreStart,
    PostStart,
    PreRemove,
    PostRemove,
}

impl HookKind {
    const ALL: [Self; 4] = [
        Self::PreStart,
        Self::PostStart,
        Self::PreRemove,
        Self::PostRemove,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PreStart => "pre-start",
            Self::PostStart => "post-start",
            Self::PreRemove => "pre-remove",
            Self::PostRemove => "post-remove",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HookError {
    message: String,
}

impl HookError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for HookError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for HookError {}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct WorktreeHooks {
    hooks: BTreeMap<HookKind, HookPlan>,
}

impl WorktreeHooks {
    /// Read the project config relative to the caller-selected checkout root.
    /// A missing file means there are no project hooks; malformed or unreadable
    /// files fail closed before a caller starts executing a hook.
    pub(crate) fn read_from(checkout_root: &Path) -> Result<Self, HookError> {
        let path = checkout_root.join(CONFIG_RELATIVE_PATH);
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(HookError::new(format!(
                    "cannot read {}: {error}",
                    path.display()
                )));
            }
        };
        Self::parse(&source)
            .map_err(|error| HookError::new(format!("invalid {}: {error}", path.display())))
    }

    /// Parse only the four lifecycle hooks used by Comet. Other valid fields in
    /// the Worktrunk config are deliberately ignored.
    pub(crate) fn parse(source: &str) -> Result<Self, HookError> {
        let document = source
            .parse::<toml_edit::DocumentMut>()
            .map_err(|error| HookError::new(format!("invalid TOML: {error}")))?;
        let mut hooks = BTreeMap::new();

        for kind in HookKind::ALL {
            let Some(item) = document.get(kind.as_str()) else {
                continue;
            };
            if let Some(plan) = parse_hook_item(kind, item)? {
                hooks.insert(kind, plan);
            }
        }

        Ok(Self { hooks })
    }

    pub(crate) fn hook(&self, kind: HookKind) -> Option<&HookPlan> {
        self.hooks.get(&kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HookPlan {
    kind: HookKind,
    /// Each outer element is a sequential pipeline stage. Commands within a
    /// stage are concurrent and retain their names for approval/UI reporting.
    stages: Vec<Vec<HookCommand>>,
}

impl HookPlan {
    pub(crate) fn stages(&self) -> &[Vec<HookCommand>] {
        &self.stages
    }

    /// Render all commands as one transaction. The caller receives no
    /// executable command unless every command in every stage validates.
    pub(crate) fn render(&self, context: &HookRenderContext) -> Result<RenderedHook, HookError> {
        let mut stages = Vec::with_capacity(self.stages.len());
        for stage in &self.stages {
            let mut rendered_stage = Vec::with_capacity(stage.len());
            for command in stage {
                rendered_stage.push(command.render(self.kind, context)?);
            }
            stages.push(rendered_stage);
        }
        Ok(RenderedHook {
            kind: self.kind,
            stages,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HookCommand {
    /// `None` for the string form; otherwise the command key from a table.
    pub(crate) name: Option<String>,
    /// Original unexpanded command, also used later for approval identity.
    pub(crate) source: String,
}

impl HookCommand {
    fn render(
        &self,
        kind: HookKind,
        context: &HookRenderContext,
    ) -> Result<RenderedHookCommand, HookError> {
        let parts = parse_template(&self.source).map_err(|problem| {
            template_error(kind, self.name.as_deref(), &problem.token, &problem.message)
        })?;
        let mut command = String::new();
        for part in parts {
            match part {
                TemplatePart::Literal(text) => command.push_str(&text),
                TemplatePart::Variable {
                    name,
                    sanitize,
                    shell_context,
                } => {
                    let Some(value) = context.values.get(&name) else {
                        return Err(template_error(
                            kind,
                            self.name.as_deref(),
                            &name,
                            "variable has no value for this hook",
                        ));
                    };
                    if REF_NAME_VARIABLES.contains(&name.as_str()) && !is_plain_ref_name(value) {
                        return Err(template_error(
                            kind,
                            self.name.as_deref(),
                            &name,
                            "value may contain only letters, digits and . _ / @ + -",
                        ));
                    }
                    let value = if sanitize {
                        sanitize_value(value)
                    } else {
                        value.clone()
                    };
                    command.push_str(&escape_template_value(&value, shell_context));
                }
            }
        }
        Ok(RenderedHookCommand {
            name: self.name.clone(),
            command,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HookRenderContext {
    /// Values for the supported template variables. Names outside the explicit
    /// allowlist are never evaluated, even if a caller puts one here.
    pub(crate) values: BTreeMap<String, String>,
}

impl HookRenderContext {
    pub(crate) fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.values.insert(name.into(), value.into());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedHook {
    kind: HookKind,
    /// The executor runs stages in order and commands within a stage together.
    stages: Vec<Vec<RenderedHookCommand>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedHookCommand {
    name: Option<String>,
    command: String,
}

fn parse_hook_item(kind: HookKind, item: &toml_edit::Item) -> Result<Option<HookPlan>, HookError> {
    let stages = if let Some(command) = item.as_str() {
        if command.trim().is_empty() {
            Vec::new()
        } else {
            vec![vec![HookCommand {
                name: None,
                source: command.to_owned(),
            }]]
        }
    } else if let Some(table) = item.as_table() {
        let commands =
            parse_named_commands(kind, table.iter().map(|(name, item)| (name, item.as_str())))?;
        vec![commands]
    } else if let Some(table) = item.as_inline_table() {
        let commands = parse_named_commands(
            kind,
            table
                .iter()
                .map(|(name, value)| (name, Some(value.as_str()).flatten())),
        )?;
        vec![commands]
    } else if let Some(pipeline) = item.as_array_of_tables() {
        let mut stages = Vec::with_capacity(pipeline.len());
        for table in pipeline.iter() {
            stages.push(parse_named_commands(
                kind,
                table.iter().map(|(name, item)| (name, item.as_str())),
            )?);
        }
        stages
    } else {
        return Err(HookError::new(format!(
            "{} must be a string, a command table, or an array of command tables",
            kind.as_str()
        )));
    };

    let stages = stages
        .into_iter()
        .filter(|stage| !stage.is_empty())
        .collect::<Vec<_>>();
    if stages.is_empty() {
        Ok(None)
    } else {
        Ok(Some(HookPlan { kind, stages }))
    }
}

fn parse_named_commands<'a>(
    kind: HookKind,
    entries: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
) -> Result<Vec<HookCommand>, HookError> {
    let mut commands = Vec::new();
    for (name, value) in entries {
        let Some(value) = value else {
            return Err(HookError::new(format!(
                "{} command `{name}` must be a string",
                kind.as_str()
            )));
        };
        if !value.trim().is_empty() {
            commands.push(HookCommand {
                name: Some(name.to_owned()),
                source: value.to_owned(),
            });
        }
    }
    Ok(commands)
}

fn template_error(
    kind: HookKind,
    command_name: Option<&str>,
    token: &str,
    detail: &str,
) -> HookError {
    let command = command_name
        .map(|name| format!(" command `{name}`"))
        .unwrap_or_default();
    HookError::new(format!(
        "{}{} template token `{token}`: {detail}",
        kind.as_str(),
        command
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TemplateProblem {
    token: String,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TemplatePart {
    Literal(String),
    Variable {
        name: String,
        sanitize: bool,
        shell_context: ShellContext,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellContext {
    Unquoted,
    SingleQuoted,
    DoubleQuoted,
}

fn parse_template(source: &str) -> Result<Vec<TemplatePart>, TemplateProblem> {
    let mut parts = Vec::new();
    let mut cursor = 0;
    let mut literal_start = 0;
    let mut shell_context = ShellContext::Unquoted;
    let mut escaped = false;
    let mut word_start = true;
    let mut in_comment = false;
    let mut unsupported_ansi_quote = false;
    while cursor < source.len() {
        let remaining = &source[cursor..];
        if in_comment {
            let character = remaining.chars().next().expect("cursor is before end");
            if character == '\n' {
                // `sh -c` ends a shell comment at the physical newline,
                // including when the comment text ends in a backslash.
                in_comment = false;
                shell_context = ShellContext::Unquoted;
                escaped = false;
                word_start = true;
            }
            cursor += character.len_utf8();
            continue;
        }
        if remaining.starts_with("{{") {
            if literal_start < cursor {
                parts.push(TemplatePart::Literal(
                    source[literal_start..cursor].to_owned(),
                ));
            }
            let expression_start = cursor + 2;
            let Some(relative_end) = source[expression_start..].find("}}") else {
                return Err(TemplateProblem {
                    token: "{{".into(),
                    message: "unterminated variable expression".into(),
                });
            };
            let expression_end = expression_start + relative_end;
            let expression = source[expression_start..expression_end].trim();
            let variable = parse_expression(expression)?;
            if source.as_bytes().windows(2).any(|pair| pair == b"\\\n") {
                return Err(TemplateProblem {
                    token: variable.name,
                    message:
                        "templates in commands with backslash-newline continuation are unsupported"
                            .into(),
                });
            }
            if escaped {
                return Err(TemplateProblem {
                    token: variable.name,
                    message: "interpolation after a shell escape is unsupported".into(),
                });
            }
            if source[..cursor].ends_with('$') {
                return Err(TemplateProblem {
                    token: variable.name,
                    message: "interpolation immediately after a literal '$' is unsupported".into(),
                });
            }
            // The small shell-context tracker below handles ordinary quoted
            // words. Here-doc bodies and nested shell expansions have distinct
            // expansion rules, so never substitute a shell-escaped word there.
            // This deliberately rejects later templates too if one of these
            // constructs already occurred in the command; it is safer than
            // misclassifying a nested shell context as an ordinary word.
            let prefix = &source[..cursor];
            if unsupported_ansi_quote {
                return Err(TemplateProblem {
                    token: variable.name,
                    message:
                        "interpolation after ANSI-C or locale-specific shell quoting is unsupported"
                            .into(),
                });
            }
            if prefix.contains("<<")
                || prefix.contains("$(")
                || prefix.contains("$[")
                || prefix.contains("<(")
                || prefix.contains(">(")
                || prefix.contains("((")
                || prefix.contains('`')
                || prefix.contains("${")
            {
                return Err(TemplateProblem {
                    token: variable.name,
                    message: "interpolation in or after a heredoc or nested shell expansion is unsupported".into(),
                });
            }
            parts.push(TemplatePart::Variable {
                name: variable.name,
                sanitize: variable.sanitize,
                shell_context,
            });
            word_start = false;
            cursor = expression_end + 2;
            literal_start = cursor;
        } else if remaining.starts_with("{%") {
            let end = source[cursor + 2..]
                .find("%}")
                .map(|offset| cursor + 2 + offset + 2)
                .unwrap_or(source.len());
            let token = source[cursor..end].trim().to_owned();
            return Err(TemplateProblem {
                token,
                message: "control blocks and conditionals are unsupported".into(),
            });
        } else if remaining.starts_with("{#") {
            let end = source[cursor + 2..]
                .find("#}")
                .map(|offset| cursor + 2 + offset + 2)
                .unwrap_or(source.len());
            return Err(TemplateProblem {
                token: source[cursor..end].trim().to_owned(),
                message: "template comments are unsupported".into(),
            });
        } else if remaining.starts_with("}}") || remaining.starts_with("%}") {
            let token = remaining.chars().take(2).collect::<String>();
            return Err(TemplateProblem {
                token,
                message: "unmatched template delimiter".into(),
            });
        } else {
            let character = remaining.chars().next().expect("cursor is before end");
            if escaped {
                escaped = false;
                if character != '\n' {
                    word_start = false;
                }
            } else {
                match shell_context {
                    ShellContext::Unquoted => match character {
                        '\\' => {
                            escaped = true;
                        }
                        '\'' => {
                            shell_context = ShellContext::SingleQuoted;
                            word_start = false;
                        }
                        '$' if remaining.starts_with("$'") || remaining.starts_with("$\"") => {
                            unsupported_ansi_quote = true;
                            word_start = false;
                        }
                        '"' => {
                            shell_context = ShellContext::DoubleQuoted;
                            word_start = false;
                        }
                        '#' if word_start => in_comment = true,
                        '\n' | ' ' | '\t' => word_start = true,
                        '|' | '&' | ';' | '(' | ')' | '<' | '>' => word_start = true,
                        _ => word_start = false,
                    },
                    ShellContext::SingleQuoted if character == '\'' => {
                        shell_context = ShellContext::Unquoted;
                    }
                    ShellContext::DoubleQuoted if character == '"' => {
                        shell_context = ShellContext::Unquoted;
                    }
                    ShellContext::DoubleQuoted if character == '\\' => escaped = true,
                    _ => {}
                }
            }
            cursor += character.len_utf8();
        }
    }
    if literal_start < source.len() {
        parts.push(TemplatePart::Literal(source[literal_start..].to_owned()));
    }
    Ok(parts)
}

struct ParsedExpression {
    name: String,
    sanitize: bool,
}

fn parse_expression(expression: &str) -> Result<ParsedExpression, TemplateProblem> {
    let mut fields = expression.split('|').map(str::trim);
    let name = fields.next().unwrap_or_default();
    if !is_identifier(name) {
        return Err(TemplateProblem {
            token: if name.is_empty() {
                expression.to_owned()
            } else {
                name.to_owned()
            },
            message: "only a single named variable is supported".into(),
        });
    }
    if !SUPPORTED_VARIABLES.contains(&name) {
        return Err(TemplateProblem {
            token: name.to_owned(),
            message: "unsupported template variable".into(),
        });
    }

    let mut sanitize = false;
    for filter in fields {
        if filter == "sanitize" && !sanitize {
            sanitize = true;
        } else {
            return Err(TemplateProblem {
                token: filter.to_owned(),
                message: "unsupported template filter".into(),
            });
        }
    }

    Ok(ParsedExpression {
        name: name.to_owned(),
        sanitize,
    })
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn is_plain_ref_name(value: &str) -> bool {
    value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '/' | '@' | '+' | '-')
    })
}

fn sanitize_value(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '/' | '\\' => '-',
            other => other,
        })
        .collect()
}

/// Quote a template value as one POSIX shell word. Worktrunk templates are
/// shell commands; quoting the interpolation keeps data from becoming syntax.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn escape_template_value(value: &str, context: ShellContext) -> String {
    match context {
        ShellContext::Unquoted => shell_quote(value),
        // Close and reopen the shell's single-quoted string around an embedded
        // quote. Shell expansion stays disabled throughout these segments.
        ShellContext::SingleQuoted => value.replace('\'', "'\\''"),
        // In double quotes, POSIX shells still evaluate parameter, command and
        // arithmetic substitutions. Escape all syntax-bearing characters.
        ShellContext::DoubleQuoted => {
            let mut escaped = String::with_capacity(value.len());
            for character in value.chars() {
                if matches!(character, '\\' | '"' | '$' | '`') {
                    escaped.push('\\');
                }
                escaped.push(character);
            }
            escaped
        }
    }
}

#[cfg(not(unix))]
type StopRequest = SyncSender<()>;

#[derive(Debug)]
struct TrackedPostStart {
    id: u64,
    #[cfg(not(unix))]
    stop: Sender<StopRequest>,
}

#[derive(Debug, Clone)]
struct PostStartControl {
    manifest_path: std::path::PathBuf,
    fifo_path: std::path::PathBuf,
    ready_path: std::path::PathBuf,
    token: String,
    process_group: i32,
}

impl PostStartControl {
    #[cfg(unix)]
    fn create(directory: &Path) -> Result<Self, HookError> {
        fs::create_dir_all(directory).map_err(|error| {
            HookError::new(format!(
                "cannot create post-start control directory: {error}"
            ))
        })?;
        let token = uuid::Uuid::new_v4().simple().to_string();
        let manifest_path = directory.join("post-start.control");
        let fifo_path = directory.join(format!("post-start-{token}.fifo"));
        let ready_path = directory.join(format!("post-start-{token}.ready"));
        create_private_fifo(&fifo_path).map_err(|error| {
            HookError::new(format!("cannot create post-start stop channel: {error}"))
        })?;

        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut opened = options.open(&manifest_path);
        if opened
            .as_ref()
            .is_err_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists)
        {
            if let Ok(Some(existing)) = Self::for_directory(directory) {
                if existing.is_orphaned().unwrap_or(false) {
                    existing.cleanup();
                    opened = options.open(&manifest_path);
                }
            }
        }
        let mut manifest = match opened {
            Ok(manifest) => manifest,
            Err(error) => {
                let _ = fs::remove_file(&fifo_path);
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    return Err(HookError::new(
                        "post-start is already tracked for this checkout",
                    ));
                }
                return Err(HookError::new(format!(
                    "cannot register post-start stop channel: {error}"
                )));
            }
        };
        if let Err(error) = writeln!(manifest, "{token}\n0").and_then(|()| manifest.sync_all()) {
            let _ = fs::remove_file(&manifest_path);
            let _ = fs::remove_file(&fifo_path);
            return Err(HookError::new(format!(
                "cannot persist post-start stop channel: {error}"
            )));
        }

        Ok(Self {
            manifest_path,
            fifo_path,
            ready_path,
            token,
            process_group: 0,
        })
    }

    fn for_checkout(checkout: &Path) -> Result<Option<Self>, HookError> {
        let name = checkout.file_name().unwrap_or_else(|| checkout.as_os_str());
        Self::for_directory(
            &checkout
                .parent()
                .unwrap_or(checkout)
                .join(".logs")
                .join(name),
        )
    }

    fn for_directory(directory: &Path) -> Result<Option<Self>, HookError> {
        let manifest_path = directory.join("post-start.control");
        let Some(contents) = read_private_control_contents(&manifest_path)? else {
            return Ok(None);
        };
        let mut lines = contents.lines();
        let token = lines.next().unwrap_or_default().to_owned();
        let parsed = uuid::Uuid::parse_str(&token).map_err(|_| {
            HookError::new("invalid post-start control token; refusing to stop any process")
        })?;
        let canonical_token = parsed.simple().to_string();
        if token != canonical_token {
            return Err(HookError::new(
                "invalid post-start control token; refusing to stop any process",
            ));
        }
        let process_group = lines
            .next()
            .and_then(|line| line.parse::<i32>().ok())
            .filter(|process_group| *process_group >= 0)
            .ok_or_else(|| {
                HookError::new("invalid post-start process identity; refusing to stop any process")
            })?;
        if lines.next().is_some() {
            return Err(HookError::new(
                "invalid post-start control file; refusing to stop any process",
            ));
        }
        Ok(Some(Self {
            manifest_path,
            fifo_path: directory.join(format!("post-start-{token}.fifo")),
            ready_path: directory.join(format!("post-start-{token}.ready")),
            token,
            process_group,
        }))
    }

    #[cfg(unix)]
    fn set_process_group(&mut self, process_group: i32) -> Result<(), HookError> {
        let before = fs::symlink_metadata(&self.manifest_path).map_err(|error| {
            HookError::new(format!("cannot inspect post-start control file: {error}"))
        })?;
        validate_control_metadata(&self.manifest_path, &before, false)?;
        let mut options = OpenOptions::new();
        options.write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let mut file = options.open(&self.manifest_path).map_err(|error| {
            HookError::new(format!("cannot update post-start control file: {error}"))
        })?;
        let opened = file.metadata().map_err(|error| {
            HookError::new(format!("cannot inspect post-start control file: {error}"))
        })?;
        validate_control_metadata(&self.manifest_path, &opened, false)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            if before.dev() != opened.dev() || before.ino() != opened.ino() {
                return Err(HookError::new(
                    "post-start control file changed while being updated; refusing to register process",
                ));
            }
        }
        writeln!(file, "{}\n{process_group}", self.token).map_err(|error| {
            HookError::new(format!(
                "cannot persist post-start process identity: {error}"
            ))
        })?;
        file.sync_all().map_err(|error| {
            HookError::new(format!(
                "cannot persist post-start process identity: {error}"
            ))
        })?;
        self.process_group = process_group;
        Ok(())
    }

    fn cleanup(&self) {
        let expected = format!("{}\n{}", self.token, self.process_group);
        let matches = read_private_control_contents(&self.manifest_path)
            .ok()
            .flatten()
            .is_some_and(|contents| contents.trim_end_matches('\n') == expected);
        if !matches {
            return;
        }
        remove_owned_control_file(&self.ready_path, false);
        remove_owned_control_file(&self.fifo_path, true);
        // Keep the manifest until its unique FIFO and ready marker have been
        // removed. A concurrent spawner sees the existing manifest and fails
        // closed until cleanup is complete.
        remove_owned_control_file(&self.manifest_path, false);
    }

    #[cfg(unix)]
    fn request_stop(&self) -> std::io::Result<()> {
        let mut writer = open_verified_fifo_writer(&self.fifo_path)?;
        writeln!(writer, "{}", self.token)
    }

    #[cfg(unix)]
    fn is_orphaned(&self) -> Result<bool, HookError> {
        let listening = self.watcher_is_open().map_err(|error| {
            HookError::new(format!(
                "could not inspect post-start stop channel: {error}"
            ))
        })?;
        Ok(!listening && self.process_group_is_gone())
    }

    /// Group `0` records a spawn whose group id was never persisted; only its
    /// stop watcher can prove it is running.
    #[cfg(unix)]
    fn process_group_is_gone(&self) -> bool {
        self.process_group <= 0 || !process_group_exists(self.process_group)
    }

    #[cfg(unix)]
    fn watcher_is_open(&self) -> std::io::Result<bool> {
        match open_verified_fifo_writer(&self.fifo_path) {
            Ok(writer) => {
                drop(writer);
                Ok(true)
            }
            Err(error) if matches!(error.raw_os_error(), Some(libc::ENXIO | libc::ENOENT)) => {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }
}

fn read_private_control_contents(path: &Path) -> Result<Option<String>, HookError> {
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(HookError::new(format!(
                "cannot inspect post-start control file: {error}"
            )));
        }
    };
    validate_control_metadata(path, &before, false)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = options
        .open(path)
        .map_err(|error| HookError::new(format!("cannot read post-start control file: {error}")))?;
    let opened = file.metadata().map_err(|error| {
        HookError::new(format!("cannot inspect post-start control file: {error}"))
    })?;
    validate_control_metadata(path, &opened, false)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if before.dev() != opened.dev() || before.ino() != opened.ino() {
            return Err(HookError::new(
                "post-start control file changed while being read; refusing to stop any process",
            ));
        }
    }
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|error| HookError::new(format!("cannot read post-start control file: {error}")))?;
    if contents.is_empty() || contents.contains('\r') {
        return Err(HookError::new(
            "invalid post-start control file; refusing to stop any process",
        ));
    }
    Ok(Some(contents))
}

fn validate_control_metadata(
    path: &Path,
    metadata: &fs::Metadata,
    fifo: bool,
) -> Result<(), HookError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _};
        let correct_type = if fifo {
            metadata.file_type().is_fifo()
        } else {
            metadata.file_type().is_file()
        };
        if !correct_type {
            return Err(HookError::new(format!(
                "unexpected post-start control file type at {}; refusing to stop any process",
                path.display()
            )));
        }
        // The control endpoint is a same-user capability. Reject links, files
        // owned by another user, and group/world-readable records or FIFOs.
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(HookError::new(format!(
                "post-start control endpoint is not private at {}; refusing to stop any process",
                path.display()
            )));
        }
    }
    #[cfg(not(unix))]
    {
        let correct_type = if fifo {
            false
        } else {
            metadata.file_type().is_file()
        };
        if !correct_type {
            return Err(HookError::new(format!(
                "unexpected post-start control file type at {}; refusing to stop any process",
                path.display()
            )));
        }
    }
    Ok(())
}

fn remove_owned_control_file(path: &Path, fifo: bool) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if validate_control_metadata(path, &metadata, fifo).is_ok() {
        let _ = fs::remove_file(path);
    }
}

#[cfg(unix)]
fn create_private_fifo(path: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::PermissionsExt as _;

    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains NUL"))?;
    // SAFETY: `c_path` is a valid NUL-terminated path and mkfifo only creates it.
    if unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(unix)]
fn open_verified_fifo_writer(path: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};

    let before = fs::symlink_metadata(path)?;
    validate_control_metadata(path, &before, true)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::PermissionDenied))?;
    let mut options = OpenOptions::new();
    options
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let file = options.open(path)?;
    let opened = file.metadata()?;
    validate_control_metadata(path, &opened, true)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::PermissionDenied))?;
    if before.dev() != opened.dev() || before.ino() != opened.ino() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "post-start stop channel changed while opening",
        ));
    }
    Ok(file)
}

#[cfg(unix)]
fn wait_for_post_start_ready(
    control: &PostStartControl,
    child: &mut Child,
) -> Result<(), HookError> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match fs::symlink_metadata(&control.ready_path) {
            Ok(metadata) => {
                validate_control_metadata(&control.ready_path, &metadata, false)?;
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(HookError::new(format!(
                    "cannot inspect post-start readiness: {error}"
                )));
            }
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| HookError::new(format!("cannot wait for post-start hook: {error}")))?
        {
            return Err(HookError::new(format!(
                "post-start supervisor exited before its stop channel was ready: {status}"
            )));
        }
        if Instant::now() >= deadline {
            return Err(HookError::new(
                "post-start stop channel did not become ready within 2 seconds",
            ));
        }
        thread::sleep(PROCESS_POLL_INTERVAL);
    }
}

#[cfg(unix)]
fn post_start_script(hook: &RenderedHook, control: &PostStartControl) -> String {
    let fifo = shell_quote(&control.fifo_path.to_string_lossy());
    let ready = shell_quote(&control.ready_path.to_string_lossy());
    let token = shell_quote(&control.token);
    format!(
        "umask 077\n(\n  trap '' TERM\n  while IFS= read -r __comet_stop_token; do\n    if [ \"$__comet_stop_token\" = {token} ]; then\n      kill -TERM -$$ 2>/dev/null\n      sleep 0.15\n      kill -KILL -$$ 2>/dev/null\n      exit 0\n    fi\n  done\n) < {fifo} &\n__comet_stop_watcher=$!\nexec 3> {fifo}\n: > {ready}\nprintf '%s\\n' ready >&3\n{commands}\n__comet_post_status=$?\nexec 3>&-\nwait \"$__comet_stop_watcher\" 2>/dev/null\nexit \"$__comet_post_status\"\n",
        commands = rendered_script(hook),
    )
}

fn tracked_post_starts() -> &'static Mutex<BTreeMap<std::path::PathBuf, TrackedPostStart>> {
    static TRACKED: OnceLock<Mutex<BTreeMap<std::path::PathBuf, TrackedPostStart>>> =
        OnceLock::new();
    TRACKED.get_or_init(|| Mutex::new(BTreeMap::new()))
}

static NEXT_POST_START_ID: AtomicU64 = AtomicU64::new(1);

/// Run a rendered blocking hook. Rendering is deliberately a separate step:
/// callers must prepare the complete hook before invoking this executor.
pub(crate) fn run_pre_hook(
    hook: &RenderedHook,
    cwd: &Path,
    timeout: Duration,
) -> Result<(), HookError> {
    if !matches!(hook.kind, HookKind::PreStart | HookKind::PreRemove) {
        return Err(HookError::new(format!(
            "{} is not a blocking hook",
            hook.kind.as_str()
        )));
    }
    run_blocking_script(hook, cwd, timeout)
}

/// Spawn a rendered post hook and return once the detached process is started.
/// `checkout_path` identifies the worktree whose per-checkout log is supplied;
/// for `post-remove`, `cwd` should be the primary checkout while the template
/// context still describes the removed checkout.
pub(crate) fn spawn_post_hook(
    hook: RenderedHook,
    cwd: &Path,
    checkout_path: &Path,
    log_path: &Path,
) -> Result<(), HookError> {
    if !matches!(hook.kind, HookKind::PostStart | HookKind::PostRemove) {
        return Err(HookError::new(format!(
            "{} is not a background hook",
            hook.kind.as_str()
        )));
    }

    let tracked_path = if hook.kind == HookKind::PostStart {
        Some(fs::canonicalize(checkout_path).map_err(|error| {
            HookError::new(format!(
                "cannot identify post-start checkout {}: {error}",
                checkout_path.display()
            ))
        })?)
    } else {
        None
    };

    if let Some(parent) = log_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            HookError::new(format!("cannot create hook log directory: {error}"))
        })?;
    }
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|error| HookError::new(format!("cannot open hook log: {error}")))?;
    let stderr = log
        .try_clone()
        .map_err(|error| HookError::new(format!("cannot open hook log: {error}")))?;

    let mut registry = if tracked_path.is_some() {
        Some(
            tracked_post_starts()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
    } else {
        None
    };
    if let (Some(path), Some(registry)) = (tracked_path.as_ref(), registry.as_ref())
        && registry.contains_key(path)
    {
        return Err(HookError::new(format!(
            "post-start is already tracked for {}",
            path.display()
        )));
    }

    let mut post_start_control = None;
    #[cfg(unix)]
    if hook.kind == HookKind::PostStart {
        let control =
            PostStartControl::create(log_path.parent().unwrap_or_else(|| Path::new(".")))?;
        post_start_control = Some(control);
    }

    let mut command = Command::new("sh");
    let script = {
        #[cfg(unix)]
        {
            if let Some(control) = &post_start_control {
                post_start_script(&hook, control)
            } else {
                rendered_script(&hook)
            }
        }
        #[cfg(not(unix))]
        {
            rendered_script(&hook)
        }
    };
    command
        .arg("-c")
        .arg(script)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            if let Some(control) = &post_start_control {
                control.cleanup();
            }
            return Err(HookError::new(format!(
                "cannot start {} hook: {error}",
                hook.kind.as_str()
            )));
        }
    };
    let child_id = child.id();

    #[cfg(unix)]
    if let Some(control) = &mut post_start_control {
        let process_group = match i32::try_from(child_id) {
            Ok(process_group) if process_group > 0 => process_group,
            _ => {
                terminate_child_tree(&mut child);
                control.cleanup();
                return Err(HookError::new(
                    "post-start process identity is out of range; process was stopped",
                ));
            }
        };
        if let Err(error) = control.set_process_group(process_group) {
            terminate_child_tree(&mut child);
            control.cleanup();
            return Err(error);
        }
    }

    if let Some(path) = tracked_path {
        let id = NEXT_POST_START_ID.fetch_add(1, Ordering::Relaxed);
        #[cfg(not(unix))]
        let (stop_tx, stop_rx) = mpsc::channel();
        registry
            .as_mut()
            .expect("post-start holds its registry lock")
            .insert(
                path.clone(),
                TrackedPostStart {
                    id,
                    #[cfg(not(unix))]
                    stop: stop_tx,
                },
            );
        drop(registry);
        if let Some(control) = &post_start_control
            && let Err(error) = wait_for_post_start_ready(control, &mut child)
        {
            terminate_child_tree(&mut child);
            control.cleanup();
            tracked_post_starts()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .retain(|_, tracked| tracked.id != id);
            return Err(error);
        }
        thread::Builder::new()
            .name("comet-post-start-monitor".into())
            .spawn({
                let monitor_control = post_start_control.clone();
                move || {
                    monitor_post_start(
                        child,
                        path,
                        id,
                        #[cfg(not(unix))]
                        stop_rx,
                        monitor_control,
                    )
                }
            })
            .map_err(|error| {
                terminate_remaining_process_group(child_id);
                if let Some(control) = &post_start_control {
                    control.cleanup();
                }
                let mut registry = tracked_post_starts()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                registry.retain(|_, tracked| tracked.id != id);
                HookError::new(format!("cannot monitor post-start process: {error}"))
            })?;
    } else {
        thread::Builder::new()
            .name("comet-post-remove-reaper".into())
            .spawn(move || {
                let mut child = child;
                let _ = child.wait();
            })
            .map_err(|error| {
                terminate_remaining_process_group(child_id);
                HookError::new(format!("cannot monitor post-remove process: {error}"))
            })?;
    }

    Ok(())
}

/// Stop only a `post-start` process group previously created by
/// [`spawn_post_hook`]. Other processes in or outside the checkout are not
/// inspected or signaled.
#[cfg(unix)]
pub(crate) fn stop_post_start(checkout_path: &Path) -> Result<bool, HookError> {
    let key = fs::canonicalize(checkout_path).map_err(|error| {
        HookError::new(format!(
            "cannot identify post-start checkout {}: {error}",
            checkout_path.display()
        ))
    })?;
    let Some(control) = PostStartControl::for_checkout(&key)? else {
        if tracked_post_starts()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains_key(&key)
        {
            return Err(HookError::new(
                "post-start is active but its private stop channel is missing; refusing removal",
            ));
        }
        return Ok(false);
    };
    if control.is_orphaned()? {
        control.cleanup();
        return Ok(false);
    }

    match control.request_stop() {
        Ok(()) => {}
        Err(error) if error.raw_os_error() == Some(libc::ENXIO) => {
            return Err(HookError::new(
                "post-start manifest exists but its stop watcher is not listening; refusing checkout removal",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(HookError::new(
                "post-start manifest exists but its stop channel is missing; refusing checkout removal",
            ));
        }
        Err(error) => {
            return Err(HookError::new(format!(
                "could not request post-start shutdown: {error}"
            )));
        }
    }

    let deadline = Instant::now() + POST_START_STOP_TIMEOUT;
    loop {
        match control.watcher_is_open() {
            Ok(true) => {}
            Ok(false) => {
                if control.process_group_is_gone() {
                    // The in-group watcher sends TERM then KILL before its FIFO
                    // reader closes. No external PID or process group is signaled.
                    control.cleanup();
                    return Ok(true);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if control.process_group_is_gone() {
                    control.cleanup();
                    return Ok(true);
                }
            }
            Err(error) => {
                return Err(HookError::new(format!(
                    "could not verify post-start shutdown: {error}"
                )));
            }
        }
        if Instant::now() >= deadline {
            return Err(HookError::new(
                "post-start stop channel did not close; refusing checkout removal",
            ));
        }
        thread::sleep(PROCESS_POLL_INTERVAL);
    }
}

#[cfg(not(unix))]
pub(crate) fn stop_post_start(checkout_path: &Path) -> Result<bool, HookError> {
    let key = fs::canonicalize(checkout_path).map_err(|error| {
        HookError::new(format!(
            "cannot identify post-start checkout {}: {error}",
            checkout_path.display()
        ))
    })?;
    let tracked = tracked_post_starts()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&key);
    let Some(tracked) = tracked else {
        return Ok(false);
    };

    let (acknowledge, stopped) = mpsc::sync_channel(0);
    if tracked.stop.send(acknowledge).is_err() {
        return Ok(true);
    }
    stopped
        .recv_timeout(POST_START_STOP_TIMEOUT)
        .map_err(|error| HookError::new(format!("could not stop post-start process: {error}")))?;
    Ok(true)
}

fn rendered_script(hook: &RenderedHook) -> String {
    let mut script = String::new();
    for (stage_index, stage) in hook.stages.iter().enumerate() {
        for (command_index, command) in stage.iter().enumerate() {
            if let Some(name) = command.name.as_deref() {
                script.push_str("printf '%s\\n' ");
                script.push_str(&shell_quote(&format!(
                    "{} command {name}",
                    hook.kind.as_str()
                )));
                script.push_str("\n");
            }
            script.push_str("sh -c ");
            script.push_str(&shell_quote(&command.command));
            script.push_str(" &\n__comet_pid_");
            script.push_str(&stage_index.to_string());
            script.push('_');
            script.push_str(&command_index.to_string());
            script.push_str("=$!\n");
        }
        script.push_str("__comet_stage_status=0\n");
        for (command_index, command) in stage.iter().enumerate() {
            script.push_str("if wait \"$__comet_pid_");
            script.push_str(&stage_index.to_string());
            script.push('_');
            script.push_str(&command_index.to_string());
            script.push_str("\"; then :; else __comet_stage_status=$?; printf '%s\\n' ");
            let name = command
                .name
                .as_deref()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{} command {}", hook.kind.as_str(), command_index + 1));
            script.push_str(&shell_quote(&format!("{name} failed")));
            script.push_str(" >&2; fi\n");
        }
        script.push_str("[ \"$__comet_stage_status\" -eq 0 ] || exit \"$__comet_stage_status\"\n");
    }
    if script.is_empty() {
        script.push_str(":\n");
    }
    script
}

fn run_blocking_script(
    hook: &RenderedHook,
    cwd: &Path,
    timeout: Duration,
) -> Result<(), HookError> {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(rendered_script(hook))
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|error| {
        HookError::new(format!("cannot start {} hook: {error}", hook.kind.as_str()))
    })?;
    let group_leader = child.id();
    let stderr_reader = child.stderr.take().map(|mut stderr| {
        let (sender, receiver) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let mut tail = Vec::new();
            let mut chunk = [0_u8; 8 * 1024];
            loop {
                let read = match stderr.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => read,
                };
                let overflow = tail
                    .len()
                    .saturating_add(read)
                    .saturating_sub(STDERR_TAIL_BYTES);
                if overflow > 0 {
                    tail.drain(..overflow);
                }
                tail.extend_from_slice(&chunk[..read]);
            }
            let _ = sender.send(String::from_utf8_lossy(&tail).trim().to_owned());
        });
        receiver
    });

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                terminate_remaining_process_group(group_leader);
                let stderr = join_stderr(stderr_reader);
                if status.success() {
                    return Ok(());
                }
                let detail = if stderr.is_empty() {
                    format!("exit {status}")
                } else {
                    stderr
                };
                return Err(HookError::new(format!(
                    "{} hook failed: {detail}",
                    hook.kind.as_str()
                )));
            }
            Ok(None) if Instant::now() >= deadline => {
                terminate_child_tree(&mut child);
                let _ = join_stderr(stderr_reader);
                return Err(HookError::new(format!(
                    "{} hook timed out after {:?}",
                    hook.kind.as_str(),
                    timeout
                )));
            }
            Ok(None) => thread::sleep(PROCESS_POLL_INTERVAL),
            Err(error) => {
                terminate_child_tree(&mut child);
                let _ = join_stderr(stderr_reader);
                return Err(HookError::new(format!(
                    "could not wait for {} hook: {error}",
                    hook.kind.as_str()
                )));
            }
        }
    }
}

fn join_stderr(reader: Option<std::sync::mpsc::Receiver<String>>) -> String {
    reader
        .and_then(|reader| reader.recv_timeout(std::time::Duration::from_secs(1)).ok())
        .unwrap_or_default()
}

fn monitor_post_start(
    mut child: Child,
    checkout_path: std::path::PathBuf,
    id: u64,
    #[cfg(not(unix))] stop: Receiver<StopRequest>,
    control: Option<PostStartControl>,
) {
    let process_group = child.id() as i32;
    let mut leader_exited = false;
    loop {
        if !leader_exited {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => leader_exited = true,
                Ok(None) => {}
            }
        }
        #[cfg(unix)]
        let group_is_alive = process_group_exists(process_group);
        #[cfg(not(unix))]
        let group_is_alive = !leader_exited;
        if leader_exited && !group_is_alive {
            break;
        }

        #[cfg(unix)]
        thread::sleep(PROCESS_POLL_INTERVAL);
        #[cfg(not(unix))]
        match stop.recv_timeout(PROCESS_POLL_INTERVAL) {
            Ok(acknowledge) => {
                terminate_managed_process_group(&mut child, process_group);
                let _ = acknowledge.send(());
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let mut registry = tracked_post_starts()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if registry
        .get(&checkout_path)
        .is_some_and(|tracked| tracked.id == id)
    {
        registry.remove(&checkout_path);
    }
    drop(registry);
    if let Some(control) = control {
        control.cleanup();
    }
}

#[cfg(unix)]
fn process_group_exists(process_group: i32) -> bool {
    // SAFETY: signal 0 probes the process group without delivering a signal.
    let result = unsafe { libc::kill(-process_group, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn process_group_exists(_process_group: i32) -> bool {
    false
}

#[cfg(unix)]
fn terminate_remaining_process_group(group_leader: u32) {
    let group = -(group_leader as i32);
    // SAFETY: commands are started in a new process group with their pid as id.
    let sent = unsafe { libc::kill(group, libc::SIGTERM) } == 0;
    if sent {
        thread::sleep(Duration::from_millis(50));
        // SAFETY: same process group created above; SIGKILL is the timeout fallback.
        unsafe {
            libc::kill(group, libc::SIGKILL);
        }
    }
}

#[cfg(not(unix))]
fn terminate_remaining_process_group(_group_leader: u32) {}

#[cfg(unix)]
fn terminate_child_tree(child: &mut Child) {
    let group = -(child.id() as i32);
    // SAFETY: process_group(0) created a dedicated process group for this hook.
    unsafe {
        libc::kill(group, libc::SIGTERM);
    }
    let grace = Instant::now() + Duration::from_millis(150);
    while Instant::now() < grace {
        if child.try_wait().ok().flatten().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    // SAFETY: terminate descendants still in the dedicated hook process group.
    unsafe {
        libc::kill(group, libc::SIGKILL);
    }
    let _ = child.wait();
}

#[cfg(not(unix))]
fn terminate_child_tree(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(unix))]
fn terminate_managed_process_group(child: &mut Child, _process_group: i32) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    const TEST_TIMEOUT: Duration = Duration::from_secs(2);

    fn render_context(branch: &str) -> HookRenderContext {
        let mut context = HookRenderContext::default();
        context.insert("branch", branch);
        context.insert("worktree_path", "/tmp/worktree with spaces");
        context.insert("repo", "comet");
        context
    }

    fn hook(source: &str, kind: HookKind) -> HookPlan {
        WorktreeHooks::parse(source)
            .unwrap()
            .hook(kind)
            .unwrap()
            .clone()
    }

    #[test]
    fn parses_string_table_pipeline_and_ignores_other_worktrunk_fields() {
        let hooks = WorktreeHooks::parse(
            r#"
worktree-path = "../{{ repo }}.{{ branch | sanitize }}"
pre-start = "npm install {{ branch }}"
post-remove = "echo removed {{ worktree_path }}"

[post-start]
server = "npm run dev"
watch = "npm run watch"

[[pre-remove]]
cache = "rm -rf .cache"

[[pre-remove]]
generated = "rm -rf generated"
"#,
        )
        .unwrap();

        let pre_start = hooks.hook(HookKind::PreStart).unwrap();
        assert_eq!(pre_start.stages().len(), 1);
        assert_eq!(pre_start.stages()[0][0].name, None);

        let post_start = hooks.hook(HookKind::PostStart).unwrap();
        assert_eq!(post_start.stages().len(), 1);
        assert_eq!(
            post_start.stages()[0]
                .iter()
                .map(|command| command.name.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["server", "watch"]
        );

        let pre_remove = hooks.hook(HookKind::PreRemove).unwrap();
        assert_eq!(pre_remove.stages().len(), 2);
        assert_eq!(pre_remove.stages()[0][0].name.as_deref(), Some("cache"));
        assert_eq!(pre_remove.stages()[1][0].name.as_deref(), Some("generated"));
        assert_eq!(hooks.hook(HookKind::PostRemove).unwrap().stages().len(), 1);
    }

    #[test]
    fn reads_hooks_from_the_explicit_source_or_target_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        fs::create_dir_all(source.join(".config")).unwrap();
        fs::create_dir_all(target.join(".config")).unwrap();
        fs::write(
            source.join(CONFIG_RELATIVE_PATH),
            "pre-start = \"echo source\"\n",
        )
        .unwrap();
        fs::write(
            target.join(CONFIG_RELATIVE_PATH),
            "pre-remove = \"echo target\"\n",
        )
        .unwrap();

        let source_hooks = WorktreeHooks::read_from(&source).unwrap();
        let target_hooks = WorktreeHooks::read_from(&target).unwrap();

        assert_eq!(
            source_hooks.hook(HookKind::PreStart).unwrap().stages()[0][0].source,
            "echo source"
        );
        assert!(source_hooks.hook(HookKind::PreRemove).is_none());
        assert_eq!(
            target_hooks.hook(HookKind::PreRemove).unwrap().stages()[0][0].source,
            "echo target"
        );
        assert!(target_hooks.hook(HookKind::PreStart).is_none());
    }

    fn path_context(worktree_path: &str) -> HookRenderContext {
        let mut context = render_context("feature/a");
        context.insert("worktree_path", worktree_path);
        context
    }

    #[test]
    fn renders_path_with_spaces_and_sanitize_as_shell_words() {
        let plan = hook(
            "pre-start = \"printf '%s|%s' {{ worktree_path }} {{ worktree_path | sanitize }}\"",
            HookKind::PreStart,
        );
        let rendered = plan.render(&path_context("feature/it's ready")).unwrap();
        assert_eq!(
            rendered.stages[0][0].command,
            r"printf '%s|%s' 'feature/it'\''s ready' 'feature-it'\''s ready'"
        );
    }

    #[test]
    fn ref_name_values_with_shell_syntax_fail_render_for_nested_shells() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("must-not-exist");
        let payload = format!("x;$(touch${{IFS}}{})", marker.display());
        let plan = hook(
            "pre-start = \"sh -c 'echo {{ branch }} {{ base }}'\"",
            HookKind::PreStart,
        );
        for name in ["branch", "base"] {
            let mut context = render_context("feature/a");
            context.insert("base", "origin/main");
            context.insert(name, payload.clone());
            let error = plan.render(&context).unwrap_err();
            assert!(error.to_string().contains(name), "{error}");
        }

        let mut context = render_context("feature/a.b_c@d+e-1");
        context.insert("base", "origin/main");
        let rendered = plan.render(&context).unwrap();
        run_pre_hook(&rendered, temp.path(), TEST_TIMEOUT).unwrap();
        assert!(!marker.exists());
    }

    #[test]
    fn double_quoted_interpolation_does_not_run_command_substitution() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("must-not-exist");
        let output = temp.path().join("output.txt");
        let value = format!("$(touch {})", marker.display());
        let source = format!(
            "pre-start = {:?}\n",
            format!(
                "printf '%s' \"{{{{ worktree_path }}}}\" > {}",
                shell_quote(output.to_str().unwrap())
            )
        );
        let rendered = hook(&source, HookKind::PreStart)
            .render(&path_context(&value))
            .unwrap();
        run_pre_hook(&rendered, temp.path(), TEST_TIMEOUT).unwrap();

        assert!(
            !marker.exists(),
            "template data must not become shell syntax"
        );
        assert_eq!(fs::read_to_string(output).unwrap(), value);
    }

    #[test]
    fn shell_comment_apostrophe_does_not_change_later_template_context() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output.txt");
        let value = "$(printf INJECTED)";
        let command = format!(
            "# user's note\nprintf '%s\\n' {{{{ worktree_path }}}} > {}",
            shell_quote(output.to_str().unwrap())
        );
        let source = format!("pre-start = {command:?}\n");
        let rendered = hook(&source, HookKind::PreStart)
            .render(&path_context(value))
            .unwrap();

        run_pre_hook(&rendered, temp.path(), TEST_TIMEOUT).unwrap();

        let output = fs::read_to_string(output).unwrap();
        assert_eq!(output, "$(printf INJECTED)\n");
        assert_ne!(output, "INJECTED\n");
    }

    #[test]
    fn ansi_c_quote_before_template_rejects_the_whole_pipeline() {
        let temp = tempfile::tempdir().unwrap();
        let earlier_stage_marker = temp.path().join("earlier-stage-must-not-run");
        let output = temp.path().join("output.txt");
        let source = format!(
            "[[pre-start]]\nfirst = {:?}\n[[pre-start]]\nsecond = {:?}\n",
            format!(
                "touch {}",
                shell_quote(earlier_stage_marker.to_str().unwrap())
            ),
            format!(
                "printf '%s\\n' $'it\\'s' {{{{ branch }}}} > {}",
                shell_quote(output.to_str().unwrap())
            )
        );
        let rendered =
            hook(&source, HookKind::PreStart).render(&render_context("$(printf INJECTED)"));
        if let Ok(rendered) = &rendered {
            // If a future parser accepts this unsupported context, exercise it
            // as a real shell command so the regression catches execution.
            let _ = run_pre_hook(rendered, temp.path(), TEST_TIMEOUT);
        }
        assert!(
            !fs::read_to_string(output)
                .unwrap_or_default()
                .contains("INJECTED"),
            "the template payload must not execute inside unsupported shell quoting"
        );
        assert!(
            !earlier_stage_marker.exists(),
            "render validates the whole pipeline before any stage can execute"
        );
        let error = rendered.expect_err("unsupported quoting must fail closed");
        assert!(error.to_string().contains("ANSI-C or locale-specific"));
    }

    #[test]
    fn backslash_newline_before_ansi_quote_rejects_the_whole_pipeline() {
        let temp = tempfile::tempdir().unwrap();
        let earlier_stage_marker = temp.path().join("earlier-stage-must-not-run");
        let output = temp.path().join("output.txt");
        let command = format!(
            "printf '%s\\n' $\\\n'it\\'s' {{{{ branch }}}} > {}",
            shell_quote(output.to_str().unwrap())
        );
        let source = format!(
            "[[pre-start]]\nfirst = {:?}\n[[pre-start]]\nsecond = {:?}\n",
            format!(
                "touch {}",
                shell_quote(earlier_stage_marker.to_str().unwrap())
            ),
            command
        );
        let rendered =
            hook(&source, HookKind::PreStart).render(&render_context("$(printf INJECTED)"));
        if let Ok(rendered) = &rendered {
            // Keep this executable: the shell removes the continuation before
            // interpreting `$'...'`, so accepting the plan can expose data.
            let _ = run_pre_hook(rendered, temp.path(), TEST_TIMEOUT);
        }

        assert!(
            !fs::read_to_string(output)
                .unwrap_or_default()
                .contains("INJECTED"),
            "the interpolated branch must not execute after a line continuation"
        );
        assert!(
            !earlier_stage_marker.exists(),
            "the whole pipeline must validate before any stage runs"
        );
        let error = rendered.expect_err("templates in line-continued commands are unsupported");
        assert!(error.to_string().contains("backslash-newline"));
    }

    #[test]
    fn arithmetic_dollar_bracket_before_template_rejects_the_whole_pipeline() {
        let temp = tempfile::tempdir().unwrap();
        let earlier_stage_marker = temp.path().join("earlier-stage-must-not-run");
        let output = temp.path().join("output.txt");
        let command = format!(
            "printf '%s\\n' $[{{{{ branch }}}}] > {}",
            shell_quote(output.to_str().unwrap())
        );
        let source = format!(
            "[[pre-start]]\nfirst = {:?}\n[[pre-start]]\nsecond = {:?}\n",
            format!(
                "touch {}",
                shell_quote(earlier_stage_marker.to_str().unwrap())
            ),
            command
        );
        let rendered =
            hook(&source, HookKind::PreStart).render(&render_context("$(printf INJECTED)"));
        let mut execution_error = None;
        if let Ok(rendered) = &rendered {
            // `$[...]` is arithmetic expansion in the target shell. Command
            // substitutions inside it run even though interpolation is quoted
            // as an ordinary shell word.
            execution_error = run_pre_hook(rendered, temp.path(), TEST_TIMEOUT)
                .err()
                .map(|error| error.to_string());
        }

        assert!(
            !execution_error
                .as_deref()
                .is_some_and(|error| error.contains("INJECTED")),
            "the shell error must not contain output from an executed command substitution"
        );
        assert!(
            !fs::read_to_string(output)
                .unwrap_or_default()
                .contains("INJECTED"),
            "branch data must not become command substitution inside arithmetic expansion"
        );
        assert!(
            !earlier_stage_marker.exists(),
            "the whole pipeline must validate before any stage runs"
        );
        let error = rendered.expect_err("arithmetic expansion before a template is unsupported");
        assert!(error.to_string().contains("nested shell expansion"));
    }

    #[test]
    fn literal_dollar_before_unquoted_template_rejects_before_execution() {
        let temp = tempfile::tempdir().unwrap();
        let earlier_stage_marker = temp.path().join("earlier-stage-must-not-run");
        let output = temp.path().join("output.txt");
        let source = format!(
            "[[pre-start]]\nfirst = {:?}\n[[pre-start]]\nsecond = {:?}\n",
            format!(
                "touch {}",
                shell_quote(earlier_stage_marker.to_str().unwrap())
            ),
            format!(
                "printf '%s\\n' ${{{{ worktree_path }}}} > {}",
                shell_quote(output.to_str().unwrap())
            )
        );
        let mut context = render_context("branch");
        let path_value = r"\';printf INJECTED >&2;false;#";
        context.insert("worktree_path", path_value);
        let rendered = hook(&source, HookKind::PreStart).render(&context);
        let mut execution_error = None;
        if let Ok(rendered) = &rendered {
            execution_error = run_pre_hook(rendered, temp.path(), TEST_TIMEOUT)
                .err()
                .map(|error| error.to_string());
        }

        assert!(
            !execution_error
                .as_deref()
                .is_some_and(|error| error.contains("INJECTED")),
            "the error stream must not contain output from an executed injected command: {execution_error:?}"
        );
        assert!(
            !output.exists(),
            "render rejection must keep the unquoted hook command from running"
        );
        assert!(!earlier_stage_marker.exists());
        let error = rendered.expect_err("literal-dollar interpolation must fail closed");
        assert!(error.to_string().contains("literal '$'"));
    }

    #[test]
    fn literal_dollar_before_double_quoted_template_rejects_before_execution() {
        let temp = tempfile::tempdir().unwrap();
        let earlier_stage_marker = temp.path().join("earlier-stage-must-not-run");
        let output = temp.path().join("output.txt");
        let source = format!(
            "[[pre-start]]\nfirst = {:?}\n[[pre-start]]\nsecond = {:?}\n",
            format!(
                "touch {}",
                shell_quote(earlier_stage_marker.to_str().unwrap())
            ),
            format!(
                "printf '%s\\n' \"${{{{ worktree_path }}}}\" > {}",
                shell_quote(output.to_str().unwrap())
            )
        );
        let mut context = render_context("branch");
        context.insert("worktree_path", "(printf INJECTED)");
        let rendered = hook(&source, HookKind::PreStart).render(&context);
        if let Ok(rendered) = &rendered {
            let _ = run_pre_hook(rendered, temp.path(), TEST_TIMEOUT);
        }

        assert!(
            !fs::read_to_string(output)
                .unwrap_or_default()
                .contains("INJECTED"),
            "the value must not complete command substitution after a literal dollar"
        );
        assert!(!earlier_stage_marker.exists());
        let error = rendered.expect_err("literal-dollar interpolation must fail closed");
        assert!(error.to_string().contains("literal '$'"));
    }

    #[test]
    fn quoted_newline_keeps_template_inside_double_quotes() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output.txt");
        let value = "$(printf INJECTED)";
        let command = format!(
            "printf '%s' \"prefix\n{{{{ worktree_path }}}}\" > {}",
            shell_quote(output.to_str().unwrap())
        );
        let source = format!("pre-start = {command:?}\n");
        let rendered = hook(&source, HookKind::PreStart)
            .render(&path_context(value))
            .unwrap();

        run_pre_hook(&rendered, temp.path(), TEST_TIMEOUT).unwrap();

        assert_eq!(
            fs::read_to_string(output).unwrap(),
            "prefix\n$(printf INJECTED)"
        );
    }

    #[test]
    fn rejects_hash_port_and_vars_as_named_unsupported_tokens() {
        for (expression, token) in [
            ("{{ branch | hash_port }}", "hash_port"),
            ("{{ vars.port }}", "vars.port"),
        ] {
            let config = format!("pre-start = \"echo {expression}\"\n");
            let error = hook(&config, HookKind::PreStart)
                .render(&render_context("feature/a b"))
                .unwrap_err();
            assert!(error.to_string().contains(token), "{error}");
            assert!(error.to_string().contains("pre-start"), "{error}");
        }
    }

    #[test]
    fn rejects_conditionals_and_malformed_delimiters() {
        for command in [
            "echo {% if branch %}yes{% endif %}",
            "echo {{ branch",
            "echo branch }}",
        ] {
            let config = format!("pre-start = {command:?}\n");
            let error = hook(&config, HookKind::PreStart)
                .render(&render_context("main"))
                .unwrap_err();
            assert!(error.to_string().contains("pre-start"), "{error}");
        }
    }

    #[test]
    fn pipeline_template_validation_precedes_every_command_execution() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("first-stage-ran");
        let marker_word = shell_quote(marker.to_str().unwrap());
        let config = format!(
            "[[pre-start]]\nfirst = \"touch {marker_word}\"\n\
             [[pre-start]]\nsecond = \"echo {{{{ branch | hash_port }}}}\"\n"
        );
        let plan = hook(&config, HookKind::PreStart);
        let prepared = plan.render(&render_context("feature/a"));

        // The executor can only receive a fully rendered hook. A validation
        // failure therefore prevents even the valid first pipeline stage.
        if let Ok(prepared) = prepared {
            for command in prepared.stages.into_iter().flatten() {
                let status = Command::new("sh")
                    .arg("-c")
                    .arg(command.command)
                    .status()
                    .unwrap();
                assert!(status.success());
            }
        }

        assert!(!marker.exists());
    }

    #[test]
    fn heredoc_and_nested_expansion_templates_are_rejected_before_any_stage_runs() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("first-stage-ran");
        let first = format!("touch {}", shell_quote(marker.to_str().unwrap()));
        let heredoc = format!("cat <<EOF\n{{{{ branch }}}}\nEOF");
        let config = format!(
            "[[pre-start]]\nfirst = {:?}\n[[pre-start]]\nsecond = {:?}\n",
            first, heredoc
        );
        let plan = hook(&config, HookKind::PreStart);
        let error = plan
            .render(&render_context("$(touch injected)"))
            .unwrap_err();
        assert!(error.to_string().contains("branch"), "{error}");
        assert!(error.to_string().contains("heredoc"), "{error}");
        assert!(
            !marker.exists(),
            "no pipeline command ran after validation failed"
        );

        for command in [
            "echo \"$(printf '%s' {{ branch }})\"",
            "printf '%s' $[{{ branch }}]",
            "echo <(printf '%s' {{ branch }})",
            "echo >(cat {{ branch }})",
            "(( {{ branch }} ))",
            "echo `printf '%s' {{ branch }}`",
            "echo \"${branch:-{{ branch }}}\"",
        ] {
            let source = format!("pre-start = {:?}\n", command);
            let error = hook(&source, HookKind::PreStart)
                .render(&render_context("$(touch injected)"))
                .unwrap_err();
            assert!(error.to_string().contains("branch"), "{error}");
            assert!(error.to_string().contains("unsupported"), "{error}");
        }
    }

    #[test]
    fn allows_only_the_declared_variables_and_sanitize_filter() {
        let mut context = HookRenderContext::default();
        context.insert("worktree_path", "/tmp/a b");
        context.insert("cwd", "/tmp/cwd");
        let plan = hook(
            "pre-remove = \"cd {{ cwd }} && printf {{ worktree_path | sanitize }}\"",
            HookKind::PreRemove,
        );
        assert_eq!(
            plan.render(&context).unwrap().stages[0][0].command,
            "cd '/tmp/cwd' && printf '-tmp-a b'"
        );
    }

    #[test]
    fn blocking_hook_returns_failure_and_names_the_failed_command() {
        let temp = tempfile::tempdir().unwrap();
        let plan = hook(
            "[pre-start]\ninstall = \"printf blocked >&2; exit 17\"\n",
            HookKind::PreStart,
        );
        let rendered = plan.render(&render_context("feature/a")).unwrap();
        let error = run_pre_hook(&rendered, temp.path(), TEST_TIMEOUT).unwrap_err();

        assert!(error.to_string().contains("pre-start"), "{error}");
        assert!(error.to_string().contains("install failed"), "{error}");
        assert!(error.to_string().contains("blocked"), "{error}");
    }

    #[test]
    fn blocking_hook_timeout_terminates_the_process_group() {
        let temp = tempfile::tempdir().unwrap();
        let plan = hook("pre-remove = \"sleep 30\"\n", HookKind::PreRemove);
        let rendered = plan.render(&render_context("feature/a")).unwrap();
        let started = Instant::now();
        let error = run_pre_hook(&rendered, temp.path(), Duration::from_millis(60)).unwrap_err();

        assert!(error.to_string().contains("timed out"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn post_start_runs_in_background_logs_output_and_can_be_stopped() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().join("worktree");
        fs::create_dir_all(&checkout).unwrap();
        let log_path = temp.path().join(".logs/worktree/post-start.log");
        let plan = hook(
            "post-start = \"pwd; echo ready; sleep 30\"\n",
            HookKind::PostStart,
        );
        let rendered = plan.render(&render_context("feature/a")).unwrap();

        spawn_post_hook(rendered, &checkout, &checkout, &log_path).unwrap();
        let deadline = Instant::now() + TEST_TIMEOUT;
        let output = loop {
            if let Ok(output) = fs::read_to_string(&log_path)
                && output.contains("ready")
            {
                break output;
            }
            assert!(
                Instant::now() < deadline,
                "post-start did not write its log"
            );
            thread::sleep(PROCESS_POLL_INTERVAL);
        };

        assert!(output.contains(checkout.to_str().unwrap()), "{output}");
        assert!(stop_post_start(&checkout).unwrap());
        let deadline = Instant::now() + TEST_TIMEOUT;
        loop {
            match stop_post_start(&checkout) {
                Ok(false) => break,
                Ok(true) => panic!("a stopped post-start was reported active again"),
                Err(error)
                    if Instant::now() < deadline
                        && (error.to_string().contains("not listening")
                            || error
                                .to_string()
                                .contains("private stop channel is missing")) =>
                {
                    // Shutdown completion is cross-process; the first caller
                    // may observe the FIFO closed just before the local reaper
                    // removes its in-memory marker.
                    thread::sleep(PROCESS_POLL_INTERVAL);
                }
                Err(error) => panic!("second stop failed: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "post-start reaper did not finish"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn post_start_can_be_stopped_from_a_separate_comet_process() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().join("worktree");
        fs::create_dir_all(&checkout).unwrap();
        let child_ready = temp.path().join("child-ready");
        let child_exit = temp.path().join("child-exit");
        let test_exe = std::env::current_exe().unwrap();
        let mut child = Command::new(test_exe)
            .arg("--exact")
            .arg("worktrunk_hooks::tests::post_start_control_child_process_helper")
            .arg("--nocapture")
            .env("COMET_TEST_POST_START_CHECKOUT", &checkout)
            .env("COMET_TEST_POST_START_READY", &child_ready)
            .env("COMET_TEST_POST_START_EXIT", &child_exit)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while !child_ready.exists() && Instant::now() < deadline {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("helper Comet process exited before readiness: {status}");
            }
            thread::sleep(PROCESS_POLL_INTERVAL);
        }
        assert!(child_ready.exists(), "helper Comet process did not start");

        let stopped = stop_post_start(&checkout).unwrap();
        assert!(stopped, "the other process's post-start should be stopped");
        fs::write(&child_exit, "exit").unwrap();
        let status = child.wait().unwrap();
        assert!(status.success(), "helper Comet process failed: {status}");
    }

    #[cfg(unix)]
    #[test]
    fn orphaned_post_start_manifest_is_cleared_without_signaling_a_process() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().join("worktree");
        fs::create_dir_all(&checkout).unwrap();
        let log_dir = temp.path().join(".logs/worktree");
        let mut control = PostStartControl::create(&log_dir).unwrap();
        // A positive, nonexistent process-group identifier lets the persisted
        // record parse, while no watcher has ever opened this FIFO.
        control.set_process_group(1_900_000_000).unwrap();

        assert!(!stop_post_start(&checkout).unwrap());
        assert!(!control.manifest_path.exists());
        assert!(!control.fifo_path.exists());
        assert!(checkout.exists());
    }

    #[cfg(unix)]
    #[test]
    fn orphaned_post_start_manifest_does_not_block_a_new_post_start() {
        let temp = tempfile::tempdir().unwrap();
        let log_dir = temp.path().join(".logs/worktree");
        let mut orphan = PostStartControl::create(&log_dir).unwrap();
        orphan.set_process_group(1_900_000_000).unwrap();

        let replacement = PostStartControl::create(&log_dir).unwrap();

        assert_ne!(replacement.token, orphan.token);
        assert!(!orphan.fifo_path.exists());
        replacement.cleanup();
    }

    #[cfg(unix)]
    #[test]
    fn unfinished_post_start_registration_does_not_block_removal_or_restart() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().join("worktree");
        fs::create_dir_all(&checkout).unwrap();
        let log_dir = temp.path().join(".logs/worktree");
        let unfinished = PostStartControl::create(&log_dir).unwrap();

        assert!(!stop_post_start(&checkout).unwrap());
        assert!(!unfinished.manifest_path.exists());

        let _second_unfinished = PostStartControl::create(&log_dir).unwrap();
        let replacement = PostStartControl::create(&log_dir).unwrap();
        replacement.cleanup();
    }

    #[cfg(unix)]
    #[test]
    fn post_start_control_child_process_helper() {
        let (Ok(checkout), Ok(ready), Ok(exit)) = (
            std::env::var("COMET_TEST_POST_START_CHECKOUT"),
            std::env::var("COMET_TEST_POST_START_READY"),
            std::env::var("COMET_TEST_POST_START_EXIT"),
        ) else {
            return;
        };
        let checkout = Path::new(&checkout);
        let plan = hook("post-start = \"sleep 30\"\n", HookKind::PostStart);
        spawn_post_hook(
            plan.render(&render_context("feature/processes")).unwrap(),
            checkout,
            checkout,
            &checkout
                .parent()
                .unwrap()
                .join(".logs/worktree/post-start.log"),
        )
        .unwrap();
        fs::write(&ready, "ready").unwrap();
        let exit = Path::new(&exit);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !exit.exists() && Instant::now() < deadline {
            thread::sleep(PROCESS_POLL_INTERVAL);
        }
        assert!(exit.exists(), "parent did not release helper process");
    }

    #[test]
    fn post_remove_uses_the_supplied_primary_cwd_after_target_is_gone() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("primary");
        let removed = temp.path().join("removed-worktree");
        let log_path = temp.path().join("logs/removed/post-remove.log");
        fs::create_dir_all(&primary).unwrap();
        let mut context = render_context("feature/gone");
        context.insert("worktree_path", removed.to_string_lossy());
        let plan = hook("post-remove = \"pwd\"\n", HookKind::PostRemove);
        let rendered = plan.render(&context).unwrap();

        spawn_post_hook(rendered, &primary, &removed, &log_path).unwrap();
        let deadline = Instant::now() + TEST_TIMEOUT;
        let output = loop {
            if let Ok(output) = fs::read_to_string(&log_path)
                && output.contains(primary.to_str().unwrap())
            {
                break output;
            }
            assert!(
                Instant::now() < deadline,
                "post-remove did not write its log"
            );
            thread::sleep(PROCESS_POLL_INTERVAL);
        };
        assert!(output.contains(primary.to_str().unwrap()), "{output}");
        assert!(!removed.exists());
    }

    #[test]
    fn stopping_post_start_does_not_signal_other_processes() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().join("worktree");
        fs::create_dir_all(&checkout).unwrap();
        let log_path = temp.path().join(".logs/worktree/post-start.log");
        let plan = hook("post-start = \"sleep 30\"\n", HookKind::PostStart);
        let rendered = plan.render(&render_context("feature/a")).unwrap();
        let mut unrelated = Command::new("sh")
            .arg("-c")
            .arg("sleep 30")
            .spawn()
            .unwrap();

        spawn_post_hook(rendered, &checkout, &checkout, &log_path).unwrap();
        assert!(stop_post_start(&checkout).unwrap());
        assert!(unrelated.try_wait().unwrap().is_none());
        unrelated.kill().unwrap();
        let _ = unrelated.wait();
    }
}
