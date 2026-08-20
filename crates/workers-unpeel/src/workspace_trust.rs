use crate::{WorkersError, WorkersLaunchRequest, WorkersPreset, WorkersProject};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use toml_edit::{DocumentMut, value};

fn trust_write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceTrustOverrides {
    pub claude_config_dir: Option<PathBuf>,
    pub codex_home: Option<PathBuf>,
}

impl WorkspaceTrustOverrides {
    fn from_environment() -> Self {
        Self {
            claude_config_dir: environment_path("CLAUDE_CONFIG_DIR"),
            codex_home: environment_path("CODEX_HOME"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceTrustLocations {
    home: PathBuf,
    pub claude: PathBuf,
    pub codex: PathBuf,
}

impl WorkspaceTrustLocations {
    pub fn resolve(home: &Path, overrides: WorkspaceTrustOverrides) -> Self {
        let claude = overrides
            .claude_config_dir
            .unwrap_or_else(|| home.to_path_buf())
            .join(".claude.json");
        let codex = overrides
            .codex_home
            .unwrap_or_else(|| home.join(".codex"))
            .join("config.toml");
        Self {
            home: home.to_path_buf(),
            claude,
            codex,
        }
    }

    fn from_environment(home: &Path) -> Self {
        Self::resolve(home, WorkspaceTrustOverrides::from_environment())
    }

    fn defaults(home: &Path) -> Self {
        Self::resolve(home, WorkspaceTrustOverrides::default())
    }

    #[doc(hidden)]
    pub fn for_command(&self, command: &str) -> Self {
        let effects = command_environment_effects(command);
        let mut locations = if effects.clear_environment {
            Self::defaults(&self.home)
        } else {
            self.clone()
        };
        if let Some(effect) = effects.claude_config_dir {
            locations.claude = effect
                .map(|path| path.join(".claude.json"))
                .unwrap_or_else(|| self.home.join(".claude.json"));
        }
        if let Some(effect) = effects.codex_home {
            locations.codex = effect
                .map(|path| path.join("config.toml"))
                .unwrap_or_else(|| self.home.join(".codex").join("config.toml"));
        }
        locations
    }
}

#[derive(Debug, Default)]
struct CommandEnvironmentEffects {
    clear_environment: bool,
    claude_config_dir: Option<Option<PathBuf>>,
    codex_home: Option<Option<PathBuf>>,
}

fn command_environment_effects(command: &str) -> CommandEnvironmentEffects {
    let mut effects = CommandEnvironmentEffects::default();
    let Ok(tokens) = shell_words::split(command) else {
        return effects;
    };
    let mut parts = tokens.into_iter().peekable();
    let mut env_wrapper = false;
    while let Some(part) = parts.next() {
        match part.as_str() {
            "env" => {
                env_wrapper = true;
                continue;
            }
            "command" | "exec" => continue,
            "-i" | "--ignore-environment" if env_wrapper => {
                effects.clear_environment = true;
                continue;
            }
            "-u" if env_wrapper => {
                if let Some(key) = parts.next() {
                    apply_environment_effect(&mut effects, &key, None);
                }
                continue;
            }
            _ if env_wrapper && part.starts_with("--unset=") => {
                apply_environment_effect(&mut effects, part.trim_start_matches("--unset="), None);
                continue;
            }
            _ if env_wrapper && part.starts_with('-') => continue,
            _ => {}
        }
        if let Some((key, value)) = part.split_once('=')
            && shell_assignment(&part)
        {
            apply_environment_effect(&mut effects, key, Some(PathBuf::from(value)));
            continue;
        }
        break;
    }
    effects
}

fn apply_environment_effect(
    effects: &mut CommandEnvironmentEffects,
    key: &str,
    value: Option<PathBuf>,
) {
    match key {
        "CLAUDE_CONFIG_DIR" => effects.claude_config_dir = Some(value),
        "CODEX_HOME" => effects.codex_home = Some(value),
        _ => {}
    }
}

fn environment_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub fn prepare_workspace_trust(command: &str, workspace: &Path) -> Result<(), WorkersError> {
    let home = dirs::home_dir()
        .ok_or_else(|| WorkersError::State("Could not resolve the home directory".into()))?;
    let locations = WorkspaceTrustLocations::from_environment(&home);
    prepare_workspace_trust_at(command, workspace, &locations)
}

pub fn prepare_launch_workspace_trust(
    launch: &WorkersLaunchRequest,
    projects: &[WorkersProject],
    presets: &[WorkersPreset],
) -> Result<WorkersLaunchRequest, WorkersError> {
    let home = dirs::home_dir()
        .ok_or_else(|| WorkersError::State("Could not resolve the home directory".into()))?;
    let locations = WorkspaceTrustLocations::from_environment(&home);
    prepare_launch_workspace_trust_at(launch, projects, presets, &locations)
}

#[doc(hidden)]
pub fn prepare_launch_workspace_trust_in_home(
    launch: &WorkersLaunchRequest,
    projects: &[WorkersProject],
    presets: &[WorkersPreset],
    home: &Path,
) -> Result<WorkersLaunchRequest, WorkersError> {
    let locations = WorkspaceTrustLocations::defaults(home);
    prepare_launch_workspace_trust_at(launch, projects, presets, &locations)
}

fn prepare_launch_workspace_trust_at(
    launch: &WorkersLaunchRequest,
    projects: &[WorkersProject],
    presets: &[WorkersPreset],
    locations: &WorkspaceTrustLocations,
) -> Result<WorkersLaunchRequest, WorkersError> {
    let Some(project) = projects
        .iter()
        .find(|project| project.id == launch.project_id)
    else {
        return Ok(launch.clone());
    };
    let workspace = launch
        .worktree_path
        .as_deref()
        .unwrap_or(project.path.as_str());
    let command = match (&launch.command, &launch.preset_id) {
        (Some(command), _) if !command.trim().is_empty() => command.as_str(),
        (_, Some(preset_id)) => {
            let Some(preset) = presets.iter().find(|preset| preset.id == *preset_id) else {
                return Ok(launch.clone());
            };
            preset.command.as_str()
        }
        _ => return Ok(launch.clone()),
    };
    prepare_workspace_trust_at(command, Path::new(workspace), locations)?;
    let trusted_command = prepare_native_session_trust_command(command);
    if trusted_command == command {
        return Ok(launch.clone());
    }
    let mut prepared = launch.clone();
    prepared.preset_id = None;
    prepared.command = Some(trusted_command);
    Ok(prepared)
}

#[doc(hidden)]
pub fn prepare_workspace_trust_in_home(
    command: &str,
    workspace: &Path,
    home: &Path,
) -> Result<(), WorkersError> {
    let locations = WorkspaceTrustLocations::defaults(home);
    prepare_workspace_trust_at(command, workspace, &locations)
}

fn prepare_workspace_trust_at(
    command: &str,
    workspace: &Path,
    locations: &WorkspaceTrustLocations,
) -> Result<(), WorkersError> {
    let provider = provider_for_command(command);
    let Some(provider) = provider else {
        return Ok(());
    };
    let locations = locations.for_command(command);
    let workspace = workspace
        .canonicalize()
        .map_err(|error| WorkersError::InvalidProject {
            path: workspace.display().to_string(),
            message: error.to_string(),
        })?;
    let _guard = trust_write_lock()
        .lock()
        .map_err(|_| WorkersError::State("Workspace trust lock was poisoned".into()))?;

    let config_path = match provider {
        Provider::Claude => &locations.claude,
        Provider::Codex => &locations.codex,
    };
    let _provider_lock = ProviderFileLock::acquire(config_path)?;

    match provider {
        Provider::Claude => trust_claude(&locations.claude, &workspace)?,
        Provider::Codex => trust_codex(&locations.codex, &workspace)?,
    }
    Ok(())
}

struct ProviderFileLock {
    path: PathBuf,
    token: String,
}

impl ProviderFileLock {
    fn acquire(config_path: &Path) -> Result<Self, WorkersError> {
        let parent = config_path.parent().ok_or_else(|| {
            WorkersError::State(format!("{} has no parent directory", config_path.display()))
        })?;
        fs::create_dir_all(parent).map_err(|error| io_error("create", parent, error))?;
        let lock_path = PathBuf::from(format!("{}.lock", config_path.display()));
        for attempt in 0..20 {
            match fs::create_dir(&lock_path) {
                Ok(()) => {
                    let token = uuid::Uuid::new_v4().to_string();
                    let owner = format!("{}\n{token}\n", std::process::id());
                    if let Err(error) = fs::write(lock_path.join("comet-owner"), owner) {
                        let _ = fs::remove_file(lock_path.join("comet-owner"));
                        let _ = fs::remove_dir(&lock_path);
                        return Err(io_error("record lock owner for", config_path, error));
                    }
                    return Ok(Self {
                        path: lock_path,
                        token,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if stale_lock_can_be_removed(&lock_path)? {
                        continue;
                    }
                    if attempt == 19 {
                        return Err(WorkersError::State(format!(
                            "Workspace trust file {} is locked by another process",
                            config_path.display()
                        )));
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                Err(error) => return Err(io_error("lock", config_path, error)),
            }
        }
        unreachable!("lock attempts either return or fail")
    }
}

fn stale_lock_can_be_removed(lock_path: &Path) -> Result<bool, WorkersError> {
    const STALE_AFTER: Duration = Duration::from_secs(30);
    let metadata = match fs::metadata(lock_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(io_error("inspect lock for", lock_path, error)),
    };
    let modified = metadata
        .modified()
        .map_err(|error| io_error("read lock timestamp for", lock_path, error))?;
    if modified.elapsed().unwrap_or_default() <= STALE_AFTER {
        return Ok(false);
    }
    let owner_path = lock_path.join("comet-owner");
    if let Ok(owner) = fs::read_to_string(&owner_path) {
        let mut lines = owner.lines();
        if let Some(pid) = lines.next().and_then(|value| value.parse::<u32>().ok())
            && process_is_alive(pid)
        {
            return Ok(false);
        }
    }
    let unchanged = fs::metadata(lock_path)
        .and_then(|current| current.modified())
        .is_ok_and(|current| current == modified);
    if !unchanged {
        return Ok(false);
    }
    let removal = if metadata.is_dir() {
        let _ = fs::remove_file(&owner_path);
        fs::remove_dir(lock_path)
    } else {
        fs::remove_file(lock_path)
    };
    match removal {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(io_error("remove stale lock for", lock_path, error)),
    }
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    // SAFETY: signal 0 performs existence/permission checking without sending a signal.
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn process_is_alive(_pid: u32) -> bool {
    false
}

impl Drop for ProviderFileLock {
    fn drop(&mut self) {
        let owner_path = self.path.join("comet-owner");
        let owns_lock = fs::read_to_string(&owner_path)
            .ok()
            .and_then(|owner| owner.lines().nth(1).map(str::to_owned))
            .is_some_and(|token| token == self.token);
        if owns_lock {
            let _ = fs::remove_file(owner_path);
            let _ = fs::remove_dir(&self.path);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Provider {
    Claude,
    Codex,
}

fn provider_for_command(command: &str) -> Option<Provider> {
    match command_executable(command)?.as_str() {
        "claude" | "claude-code" => Some(Provider::Claude),
        "codex" => Some(Provider::Codex),
        _ => None,
    }
}

fn command_executable(command: &str) -> Option<String> {
    let mut parts = shell_words::split(command).ok()?.into_iter().peekable();
    let mut env_wrapper = false;
    let executable = loop {
        let part = parts.next()?;
        if matches!(part.as_str(), "env" | "command" | "exec") {
            env_wrapper = part == "env";
            continue;
        }
        if shell_assignment(&part) {
            continue;
        }
        if env_wrapper && part == "-u" {
            parts.next()?;
            continue;
        }
        if env_wrapper && part.starts_with('-') {
            continue;
        }
        break part;
    };
    let executable = Path::new(&executable)
        .file_name()
        .and_then(|value| value.to_str())?;
    Some(executable.to_ascii_lowercase())
}

fn prepare_native_session_trust_command(command: &str) -> String {
    let trimmed = command.trim();
    let tokens = shell_words::split(trimmed).unwrap_or_default();
    match command_executable(trimmed).as_deref() {
        Some("gemini")
            if !tokens
                .iter()
                .any(|part| part == "GEMINI_CLI_TRUST_WORKSPACE=true") =>
        {
            format!("GEMINI_CLI_TRUST_WORKSPACE=true {trimmed}")
        }
        Some("pi")
            if !tokens
                .iter()
                .any(|part| matches!(part.as_str(), "--approve" | "-a")) =>
        {
            format!("{trimmed} --approve")
        }
        _ => trimmed.to_owned(),
    }
}

fn shell_assignment(value: &str) -> bool {
    let Some((name, _)) = value.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name.chars().enumerate().all(|(index, character)| {
            character == '_'
                || character.is_ascii_alphanumeric() && (index > 0 || !character.is_ascii_digit())
        })
}

fn trust_claude(path: &Path, workspace: &Path) -> Result<(), WorkersError> {
    let mut root = read_json_object(&path)?;
    let projects = object_entry(&mut root, "projects")?;
    let key = workspace.to_string_lossy().into_owned();
    let project = object_entry(projects, &key)?;
    project.insert("hasTrustDialogAccepted".into(), Value::Bool(true));
    project.insert("hasCompletedProjectOnboarding".into(), Value::Bool(true));
    write_json_atomic(&path, &Value::Object(root))
}

fn trust_codex(path: &Path, workspace: &Path) -> Result<(), WorkersError> {
    let raw = read_optional_string(path)?;
    let mut document = if raw.trim().is_empty() {
        DocumentMut::new()
    } else {
        raw.parse::<DocumentMut>().map_err(|error| {
            WorkersError::State(format!(
                "Failed to parse workspace trust file {}: {error}",
                path.display()
            ))
        })?
    };
    let workspace = workspace.to_string_lossy().into_owned();
    document["projects"][&workspace]["trust_level"] = value("trusted");
    write_atomic(path, document.to_string().as_bytes())
}

fn read_optional_string(path: &Path) -> Result<String, WorkersError> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(raw),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(io_error("read", path, error)),
    }
}

fn read_json_object(path: &Path) -> Result<Map<String, Value>, WorkersError> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
            .map_err(WorkersError::InvalidResponse)?
            .as_object()
            .cloned()
            .ok_or_else(|| {
                WorkersError::State(format!("{} must contain a JSON object", path.display()))
            }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(error) => Err(io_error("read", path, error)),
    }
}

fn object_entry<'a>(
    object: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Map<String, Value>, WorkersError> {
    let value = object
        .entry(key.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    value.as_object_mut().ok_or_else(|| {
        WorkersError::State(format!(
            "Workspace trust field '{key}' must be a JSON object"
        ))
    })
}

fn write_json_atomic(path: &Path, value: &Value) -> Result<(), WorkersError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(WorkersError::InvalidResponse)?;
    bytes.push(b'\n');
    write_atomic(path, &bytes)
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), WorkersError> {
    let resolved_path = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => path
            .canonicalize()
            .map_err(|error| io_error("resolve symlink for", path, error))?,
        Ok(_) => path.to_path_buf(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => path.to_path_buf(),
        Err(error) => return Err(io_error("inspect", path, error)),
    };
    let path = resolved_path.as_path();
    let parent = path.parent().ok_or_else(|| {
        WorkersError::State(format!("{} has no parent directory", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|error| io_error("create", parent, error))?;
    let temp_path = temporary_path(path);
    fs::write(&temp_path, contents).map_err(|error| io_error("write", &temp_path, error))?;
    if let Ok(metadata) = fs::metadata(path) {
        fs::set_permissions(&temp_path, metadata.permissions())
            .map_err(|error| io_error("set permissions on", &temp_path, error))?;
    }
    fs::rename(&temp_path, path).map_err(|error| io_error("replace", path, error))
}

fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("workspace-trust");
    path.with_file_name(format!(".{name}.comet-{}.tmp", uuid::Uuid::new_v4()))
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> WorkersError {
    WorkersError::State(format!(
        "Failed to {action} workspace trust file {}: {error}",
        path.display()
    ))
}
