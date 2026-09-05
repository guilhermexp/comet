//! Worker CLI maintenance: version detection, registry querying, advisory assembly,
//! and safe update execution.
//!
//! Replicates the architecture from Orchestrator.dev adapted to Comet's native Rust runtime.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const LATEST_VERSION_CACHE_TTL: Duration = Duration::from_secs(60 * 60);
const LATEST_VERSION_TIMEOUT: Duration = Duration::from_secs(4);
const VERSION_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const UPDATE_COMMAND_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const UPDATE_OUTPUT_MAX_BYTES: usize = 10 * 1024;

const UPDATE_SUCCEEDED_MESSAGE: &str = "CLI updated successfully.";
const UPDATE_UNCHANGED_MESSAGE: &str =
    "Update command completed, but Comet still detects an outdated version.";
const UPDATE_NO_COMMAND_MESSAGE: &str = "No one-click update is available for this install source.";

// ---------------------------------------------------------------------------
// Semver Helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSemver {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Vec<String>,
}

fn normalize_semver_str(version: &str) -> String {
    let trimmed = version.trim().trim_start_matches('v');
    let mut parts = trimmed.splitn(2, '-');
    let main = parts.next().unwrap_or("");
    let prerelease = parts.next();

    let mut segments: Vec<&str> = main
        .split('.')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if segments.len() == 2 {
        segments.push("0");
    }

    if let Some(pre) = prerelease {
        format!("{}-{}", segments.join("."), pre)
    } else {
        segments.join(".")
    }
}

pub fn parse_semver(version: &str) -> Option<ParsedSemver> {
    let normalized = normalize_semver_str(version);
    let mut parts = normalized.splitn(2, '-');
    let main = parts.next().unwrap_or("");
    let prerelease_part = parts.next();

    let segments: Vec<&str> = main.split('.').collect();
    if segments.len() != 3 {
        return None;
    }

    let major = segments[0].parse::<u64>().ok()?;
    let minor = segments[1].parse::<u64>().ok()?;
    let patch = segments[2].parse::<u64>().ok()?;

    let prerelease = prerelease_part
        .map(|p| {
            p.split('.')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    Some(ParsedSemver {
        major,
        minor,
        patch,
        prerelease,
    })
}

fn compare_prerelease_ident(left: &str, right: &str) -> std::cmp::Ordering {
    let left_num = left.parse::<u64>().ok();
    let right_num = right.parse::<u64>().ok();

    match (left_num, right_num) {
        (Some(l), Some(r)) => l.cmp(&r),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => left.cmp(right),
    }
}

pub fn compare_semver(a: &str, b: &str) -> std::cmp::Ordering {
    let (parsed_a, parsed_b) = match (parse_semver(a), parse_semver(b)) {
        (Some(pa), Some(pb)) => (pa, pb),
        _ => return a.cmp(b),
    };

    if parsed_a.major != parsed_b.major {
        return parsed_a.major.cmp(&parsed_b.major);
    }
    if parsed_a.minor != parsed_b.minor {
        return parsed_a.minor.cmp(&parsed_b.minor);
    }
    if parsed_a.patch != parsed_b.patch {
        return parsed_a.patch.cmp(&parsed_b.patch);
    }

    // A version without prerelease outranks one with a prerelease
    match (
        parsed_a.prerelease.is_empty(),
        parsed_b.prerelease.is_empty(),
    ) {
        (true, true) => return std::cmp::Ordering::Equal,
        (true, false) => return std::cmp::Ordering::Greater,
        (false, true) => return std::cmp::Ordering::Less,
        (false, false) => {}
    }

    let max_len = parsed_a.prerelease.len().max(parsed_b.prerelease.len());
    for i in 0..max_len {
        let left = parsed_a.prerelease.get(i);
        let right = parsed_b.prerelease.get(i);
        match (left, right) {
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(l), Some(r)) => {
                let order = compare_prerelease_ident(l, r);
                if order != std::cmp::Ordering::Equal {
                    return order;
                }
            }
            (None, None) => break,
        }
    }

    std::cmp::Ordering::Equal
}

/// Extract semver from arbitrary `--version` output.
pub fn parse_generic_cli_version(output: &str) -> Option<String> {
    for token in output.split_whitespace() {
        let clean = token.trim().trim_start_matches('v');
        if let Some(parsed) = parse_semver(clean) {
            return if parsed.prerelease.is_empty() {
                Some(format!(
                    "{}.{}.{}",
                    parsed.major, parsed.minor, parsed.patch
                ))
            } else {
                Some(format!(
                    "{}.{}.{}-{}",
                    parsed.major,
                    parsed.minor,
                    parsed.patch,
                    parsed.prerelease.join(".")
                ))
            };
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Maintenance Registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomebrewKind {
    Cask,
    Formula,
}

#[derive(Debug, Clone)]
pub struct HomebrewSource {
    pub name: &'static str,
    pub kind: HomebrewKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeUpdateStrategy {
    Always,
    MatchingPath,
}

#[derive(Debug, Clone)]
pub struct NativeUpdateDefinition {
    pub executable: &'static str,
    pub args: &'static [&'static str],
    pub strategy: NativeUpdateStrategy,
}

#[derive(Debug, Clone)]
pub struct RuntimeMaintenanceDefinition {
    pub cli_id: &'static str,
    pub binary_name: &'static str,
    pub npm_package_name: &'static str,
    pub homebrew: Option<HomebrewSource>,
    pub native_update: Option<NativeUpdateDefinition>,
    pub probe_args: &'static [&'static str],
}

pub static MAINTAINED_RUNTIMES: &[RuntimeMaintenanceDefinition] = &[
    RuntimeMaintenanceDefinition {
        cli_id: "pi",
        binary_name: "pi",
        npm_package_name: "@earendil-works/pi-coding-agent",
        homebrew: None,
        native_update: Some(NativeUpdateDefinition {
            executable: "pi",
            args: &["update"],
            strategy: NativeUpdateStrategy::Always,
        }),
        probe_args: &["--version"],
    },
    RuntimeMaintenanceDefinition {
        cli_id: "omp",
        binary_name: "omp",
        npm_package_name: "@oh-my-pi/pi-coding-agent",
        homebrew: None,
        native_update: Some(NativeUpdateDefinition {
            executable: "omp",
            args: &["update"],
            strategy: NativeUpdateStrategy::Always,
        }),
        probe_args: &["--no-extensions", "--version"],
    },
    RuntimeMaintenanceDefinition {
        cli_id: "claude-code",
        binary_name: "claude",
        npm_package_name: "@anthropic-ai/claude-code",
        homebrew: Some(HomebrewSource {
            name: "claude-code",
            kind: HomebrewKind::Cask,
        }),
        native_update: Some(NativeUpdateDefinition {
            executable: "claude",
            args: &["update"],
            strategy: NativeUpdateStrategy::MatchingPath,
        }),
        probe_args: &["--version"],
    },
    RuntimeMaintenanceDefinition {
        cli_id: "codex",
        binary_name: "codex",
        npm_package_name: "@openai/codex",
        homebrew: Some(HomebrewSource {
            name: "codex",
            kind: HomebrewKind::Cask,
        }),
        native_update: None,
        probe_args: &["--version"],
    },
    RuntimeMaintenanceDefinition {
        cli_id: "opencode",
        binary_name: "opencode",
        npm_package_name: "opencode-ai",
        homebrew: Some(HomebrewSource {
            name: "anomalyco/tap/opencode",
            kind: HomebrewKind::Formula,
        }),
        native_update: Some(NativeUpdateDefinition {
            executable: "opencode",
            args: &["upgrade"],
            strategy: NativeUpdateStrategy::Always,
        }),
        probe_args: &["--version"],
    },
    RuntimeMaintenanceDefinition {
        cli_id: "agy",
        binary_name: "agy",
        npm_package_name: "agy",
        homebrew: None,
        native_update: Some(NativeUpdateDefinition {
            executable: "agy",
            args: &["update"],
            strategy: NativeUpdateStrategy::Always,
        }),
        probe_args: &["--version"],
    },
];

pub fn get_maintenance_definition(cli_id: &str) -> Option<&'static RuntimeMaintenanceDefinition> {
    MAINTAINED_RUNTIMES
        .iter()
        .find(|def| def.cli_id == cli_id || def.binary_name == cli_id)
}

// ---------------------------------------------------------------------------
// Install Source Classification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInstallSource {
    Npm,
    Bun,
    Pnpm,
    Homebrew,
    Native,
    Unknown,
}

impl RuntimeInstallSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Bun => "bun",
            Self::Pnpm => "pnpm",
            Self::Homebrew => "homebrew",
            Self::Native => "native",
            Self::Unknown => "unknown",
        }
    }
}

pub fn detect_install_source(
    binary_path: &Path,
    _cli_id: Option<&str>,
) -> (RuntimeInstallSource, Option<PathBuf>) {
    let path_str = binary_path.to_string_lossy().to_ascii_lowercase();

    // Native installer check (e.g. Anthropic ~/.local/bin/claude)
    if let Some(home) = dirs::home_dir() {
        let local_bin = home.join(".local").join("bin");
        if binary_path.starts_with(&local_bin) {
            return (RuntimeInstallSource::Native, None);
        }
    }

    // Resolve canonical realpath to follow symlinks
    let canonical = std::fs::canonicalize(binary_path).ok();
    let real_str = canonical
        .as_ref()
        .map(|p| p.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|| path_str.clone());

    if real_str.contains("/.bun/bin/") {
        return (RuntimeInstallSource::Bun, None);
    }

    if real_str.contains("/.local/share/pnpm/")
        || real_str.contains("/library/pnpm/")
        || real_str.contains("/pnpm/global/")
    {
        return (RuntimeInstallSource::Pnpm, None);
    }

    if real_str.contains("/caskroom/") || real_str.contains("/cellar/") {
        return (RuntimeInstallSource::Homebrew, None);
    }

    if real_str.contains("/node_modules/.bin/")
        || real_str.contains("/lib/node_modules/")
        || real_str.contains("/npm/node_modules/")
    {
        let prefix = derive_npm_prefix(canonical.as_deref().unwrap_or(binary_path));
        return (RuntimeInstallSource::Npm, prefix);
    }

    (RuntimeInstallSource::Unknown, None)
}

fn derive_npm_prefix(resolved_path: &Path) -> Option<PathBuf> {
    let path_str = resolved_path.to_string_lossy();
    let marker = "/lib/node_modules/";
    if let Some(idx) = path_str.find(marker) {
        return Some(PathBuf::from(&path_str[..idx]));
    }
    if let Some(parent) = resolved_path.parent() {
        if parent.file_name().and_then(|n| n.to_str()) == Some("bin") {
            if let Some(prefix) = parent.parent() {
                return Some(prefix.to_path_buf());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Binary Resolution & Local Probe
// ---------------------------------------------------------------------------

pub fn resolve_binary(binary_name: &str) -> Option<PathBuf> {
    if binary_name.contains('/') || binary_name.contains('\\') {
        let path = PathBuf::from(binary_name);
        return if path.is_file() { Some(path) } else { None };
    }

    let search_dirs = unpeel_core::setup::search_dirs();
    for dir in search_dirs {
        let candidate = dir.join(binary_name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{binary_name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
            let cmd = dir.join(format!("{binary_name}.cmd"));
            if cmd.is_file() {
                return Some(cmd);
            }
        }
    }
    None
}

pub struct DetectedLocalCli {
    pub installed: bool,
    pub version: Option<String>,
    pub binary_path: Option<PathBuf>,
    pub install_source: RuntimeInstallSource,
    pub npm_prefix: Option<PathBuf>,
}

pub async fn probe_installed_cli(def: &RuntimeMaintenanceDefinition) -> DetectedLocalCli {
    let Some(binary_path) = resolve_binary(def.binary_name) else {
        return DetectedLocalCli {
            installed: false,
            version: None,
            binary_path: None,
            install_source: RuntimeInstallSource::Unknown,
            npm_prefix: None,
        };
    };

    let (install_source, npm_prefix) = detect_install_source(&binary_path, Some(def.cli_id));

    // Run <binary> <probe_args> with timeout
    let mut cmd = tokio::process::Command::new(&binary_path);
    cmd.args(def.probe_args);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let output = match tokio::time::timeout(VERSION_COMMAND_TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) => {
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            parse_generic_cli_version(&combined)
        }
        _ => None,
    };

    DetectedLocalCli {
        installed: output.is_some(),
        version: output,
        binary_path: Some(binary_path),
        install_source,
        npm_prefix,
    }
}

// ---------------------------------------------------------------------------
// Latest Version Remote Queries with Cache
// ---------------------------------------------------------------------------

struct CacheEntry {
    version: Option<String>,
    expires_at: Instant,
}

static LATEST_VERSION_CACHE: LazyLock<RwLock<HashMap<String, CacheEntry>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

async fn fetch_npm_latest(pkg: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(LATEST_VERSION_TIMEOUT)
        .build()
        .ok()?;

    let url = format!("https://registry.npmjs.org/{pkg}/latest");
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    #[derive(Deserialize)]
    struct NpmLatest {
        version: Option<String>,
    }

    let payload: NpmLatest = resp.json().await.ok()?;
    payload.version.filter(|v| !v.trim().is_empty())
}

async fn fetch_homebrew_latest(source: &HomebrewSource) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(LATEST_VERSION_TIMEOUT)
        .build()
        .ok()?;

    let kind_str = match source.kind {
        HomebrewKind::Cask => "cask",
        HomebrewKind::Formula => "formula",
    };

    let url = format!(
        "https://formulae.brew.sh/api/{kind_str}/{}.json",
        source.name
    );
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    #[derive(Deserialize)]
    struct BrewInfo {
        version: Option<String>,
        versions: Option<BrewVersions>,
    }

    #[derive(Deserialize)]
    struct BrewVersions {
        stable: Option<String>,
    }

    let payload: BrewInfo = resp.json().await.ok()?;
    match source.kind {
        HomebrewKind::Cask => payload.version,
        HomebrewKind::Formula => payload.versions.and_then(|v| v.stable),
    }
    .filter(|v| !v.trim().is_empty())
}

pub async fn query_latest_version(
    def: &RuntimeMaintenanceDefinition,
    install_source: RuntimeInstallSource,
) -> Option<String> {
    let cache_key = if install_source == RuntimeInstallSource::Homebrew && def.homebrew.is_some() {
        format!("brew:{}", def.homebrew.as_ref().unwrap().name)
    } else {
        format!("npm:{}", def.npm_package_name)
    };

    {
        let cache = LATEST_VERSION_CACHE.read().ok()?;
        if let Some(entry) = cache.get(&cache_key) {
            if entry.expires_at > Instant::now() {
                return entry.version.clone();
            }
        }
    }

    let version = if install_source == RuntimeInstallSource::Homebrew {
        if let Some(brew) = &def.homebrew {
            fetch_homebrew_latest(brew).await
        } else {
            fetch_npm_latest(def.npm_package_name).await
        }
    } else {
        fetch_npm_latest(def.npm_package_name).await
    };

    if let Ok(mut cache) = LATEST_VERSION_CACHE.write() {
        cache.insert(
            cache_key,
            CacheEntry {
                version: version.clone(),
                expires_at: Instant::now() + LATEST_VERSION_CACHE_TTL,
            },
        );
    }

    version
}

// ---------------------------------------------------------------------------
// Advisory Assembly
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeUpdateStatus {
    Unknown,
    Current,
    BehindLatest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeVersionAdvisory {
    pub cli_id: String,
    pub binary_name: String,
    pub status: RuntimeUpdateStatus,
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    pub install_source: RuntimeInstallSource,
    pub update_command: Option<String>,
    pub can_update: bool,
}

pub struct UpdateInvocation {
    pub executable: String,
    pub args: Vec<String>,
}

impl UpdateInvocation {
    pub fn to_command_string(&self) -> String {
        format!("{} {}", self.executable, self.args.join(" "))
    }
}

pub fn resolve_update_invocation(
    def: &RuntimeMaintenanceDefinition,
    install_source: RuntimeInstallSource,
    npm_prefix: Option<&Path>,
) -> Option<UpdateInvocation> {
    if let Some(native) = &def.native_update {
        if native.strategy == NativeUpdateStrategy::Always
            || (native.strategy == NativeUpdateStrategy::MatchingPath
                && install_source == RuntimeInstallSource::Native)
        {
            return Some(UpdateInvocation {
                executable: native.executable.to_owned(),
                args: native.args.iter().map(|s| (*s).to_owned()).collect(),
            });
        }
    }

    match install_source {
        RuntimeInstallSource::Native => None,
        RuntimeInstallSource::Npm => {
            let mut args = vec!["install".to_owned(), "-g".to_owned()];
            if let Some(prefix) = npm_prefix {
                args.push("--prefix".to_owned());
                args.push(prefix.to_string_lossy().to_string());
            }
            args.push(format!("{}@latest", def.npm_package_name));
            Some(UpdateInvocation {
                executable: "npm".to_owned(),
                args,
            })
        }
        RuntimeInstallSource::Bun => Some(UpdateInvocation {
            executable: "bun".to_owned(),
            args: vec![
                "i".to_owned(),
                "-g".to_owned(),
                format!("{}@latest", def.npm_package_name),
            ],
        }),
        RuntimeInstallSource::Pnpm => Some(UpdateInvocation {
            executable: "pnpm".to_owned(),
            args: vec![
                "add".to_owned(),
                "-g".to_owned(),
                format!("{}@latest", def.npm_package_name),
            ],
        }),
        RuntimeInstallSource::Homebrew => def.homebrew.as_ref().map(|brew| {
            let mut args = vec!["upgrade".to_owned()];
            if brew.kind == HomebrewKind::Cask {
                args.push("--cask".to_owned());
            }
            args.push(brew.name.to_owned());
            UpdateInvocation {
                executable: "brew".to_owned(),
                args,
            }
        }),
        RuntimeInstallSource::Unknown => None,
    }
}

pub fn build_advisory(
    def: &RuntimeMaintenanceDefinition,
    current_version: Option<String>,
    latest_version: Option<String>,
    install_source: RuntimeInstallSource,
    npm_prefix: Option<&Path>,
) -> RuntimeVersionAdvisory {
    let invocation = resolve_update_invocation(def, install_source, npm_prefix);
    let (update_command, can_update) = if let Some(inv) = invocation {
        (Some(inv.to_command_string()), true)
    } else {
        (
            Some(format!("npm install -g {}@latest", def.npm_package_name)),
            false,
        )
    };

    let (status, cmd) = match (&current_version, &latest_version) {
        (Some(curr), Some(lat)) => {
            if compare_semver(curr, lat) == std::cmp::Ordering::Less {
                (RuntimeUpdateStatus::BehindLatest, update_command)
            } else {
                (RuntimeUpdateStatus::Current, None)
            }
        }
        _ => (RuntimeUpdateStatus::Unknown, None),
    };

    RuntimeVersionAdvisory {
        cli_id: def.cli_id.to_owned(),
        binary_name: def.binary_name.to_owned(),
        status,
        current_version,
        latest_version,
        install_source,
        update_command: cmd,
        can_update: status == RuntimeUpdateStatus::BehindLatest && can_update,
    }
}

pub async fn get_all_advisories() -> Vec<RuntimeVersionAdvisory> {
    let mut advisories = Vec::with_capacity(MAINTAINED_RUNTIMES.len());
    for def in MAINTAINED_RUNTIMES {
        let local = probe_installed_cli(def).await;
        let latest = if local.installed {
            query_latest_version(def, local.install_source).await
        } else {
            None
        };

        advisories.push(build_advisory(
            def,
            local.version,
            latest,
            local.install_source,
            local.npm_prefix.as_deref(),
        ));
    }
    advisories
}

// ---------------------------------------------------------------------------
// Update Execution
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateOutcomeStatus {
    Succeeded,
    Unchanged,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeUpdateResult {
    pub cli_id: String,
    pub status: UpdateOutcomeStatus,
    pub message: String,
    pub advisory: Option<RuntimeVersionAdvisory>,
    pub manual_command: Option<String>,
    pub stderr: Option<String>,
}

static UPDATES_IN_FLIGHT: LazyLock<tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Notify>>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(HashMap::new()));

pub async fn run_runtime_update(cli_id: &str) -> RuntimeUpdateResult {
    let Some(def) = get_maintenance_definition(cli_id) else {
        return RuntimeUpdateResult {
            cli_id: cli_id.to_owned(),
            status: UpdateOutcomeStatus::Failed,
            message: "Runtime is not managed for updates.".to_owned(),
            advisory: None,
            manual_command: None,
            stderr: None,
        };
    };

    // Serialize in-flight updates per cli_id
    let notify = {
        let mut in_flight = UPDATES_IN_FLIGHT.lock().await;
        if let Some(existing) = in_flight.get(cli_id) {
            let notifier = existing.clone();
            drop(in_flight);
            notifier.notified().await;
            // Return fresh advisory after prior completed
            let local = probe_installed_cli(def).await;
            let latest = query_latest_version(def, local.install_source).await;
            let adv = build_advisory(
                def,
                local.version,
                latest,
                local.install_source,
                local.npm_prefix.as_deref(),
            );
            return RuntimeUpdateResult {
                cli_id: cli_id.to_owned(),
                status: if adv.status == RuntimeUpdateStatus::Current {
                    UpdateOutcomeStatus::Succeeded
                } else {
                    UpdateOutcomeStatus::Unchanged
                },
                message: if adv.status == RuntimeUpdateStatus::Current {
                    UPDATE_SUCCEEDED_MESSAGE.to_owned()
                } else {
                    UPDATE_UNCHANGED_MESSAGE.to_owned()
                },
                advisory: Some(adv),
                manual_command: None,
                stderr: None,
            };
        }
        let notify = Arc::new(tokio::sync::Notify::new());
        in_flight.insert(cli_id.to_owned(), notify.clone());
        notify
    };

    let result = run_runtime_update_inner(def).await;

    {
        let mut in_flight = UPDATES_IN_FLIGHT.lock().await;
        in_flight.remove(cli_id);
        notify.notify_waiters();
    }

    result
}

async fn run_runtime_update_inner(def: &RuntimeMaintenanceDefinition) -> RuntimeUpdateResult {
    let before = probe_installed_cli(def).await;
    let invocation =
        resolve_update_invocation(def, before.install_source, before.npm_prefix.as_deref());

    let manual_command = format!("npm install -g {}@latest", def.npm_package_name);

    let Some(inv) = invocation else {
        let latest = query_latest_version(def, before.install_source).await;
        let advisory = build_advisory(
            def,
            before.version,
            latest,
            before.install_source,
            before.npm_prefix.as_deref(),
        );
        return RuntimeUpdateResult {
            cli_id: def.cli_id.to_owned(),
            status: UpdateOutcomeStatus::Unchanged,
            message: UPDATE_NO_COMMAND_MESSAGE.to_owned(),
            advisory: Some(advisory),
            manual_command: Some(manual_command),
            stderr: None,
        };
    };

    let mut cmd = tokio::process::Command::new(&inv.executable);
    cmd.args(&inv.args);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let output = match tokio::time::timeout(UPDATE_COMMAND_TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(err)) => {
            return RuntimeUpdateResult {
                cli_id: def.cli_id.to_owned(),
                status: UpdateOutcomeStatus::Failed,
                message: format!("Failed to spawn update command: {err}"),
                advisory: None,
                manual_command: Some(manual_command),
                stderr: None,
            };
        }
        Err(_) => {
            return RuntimeUpdateResult {
                cli_id: def.cli_id.to_owned(),
                status: UpdateOutcomeStatus::Failed,
                message: "Update command timed out after 5 minutes.".to_owned(),
                advisory: None,
                manual_command: Some(manual_command),
                stderr: None,
            };
        }
    };

    if !output.status.success() {
        let stderr_raw = String::from_utf8_lossy(&output.stderr);
        let truncated = if stderr_raw.len() > UPDATE_OUTPUT_MAX_BYTES {
            format!("{}\n…(truncated)", &stderr_raw[..UPDATE_OUTPUT_MAX_BYTES])
        } else {
            stderr_raw.to_string()
        };
        return RuntimeUpdateResult {
            cli_id: def.cli_id.to_owned(),
            status: UpdateOutcomeStatus::Failed,
            message: format!(
                "Update command exited with status {:?}",
                output.status.code()
            ),
            advisory: None,
            manual_command: Some(manual_command),
            stderr: Some(truncated),
        };
    }

    // Re-probe after update
    let after = probe_installed_cli(def).await;
    let latest = query_latest_version(def, after.install_source).await;
    let advisory = build_advisory(
        def,
        after.version,
        latest,
        after.install_source,
        after.npm_prefix.as_deref(),
    );

    if advisory.status == RuntimeUpdateStatus::BehindLatest {
        RuntimeUpdateResult {
            cli_id: def.cli_id.to_owned(),
            status: UpdateOutcomeStatus::Unchanged,
            message: UPDATE_UNCHANGED_MESSAGE.to_owned(),
            advisory: Some(advisory),
            manual_command: Some(manual_command),
            stderr: None,
        }
    } else {
        RuntimeUpdateResult {
            cli_id: def.cli_id.to_owned(),
            status: UpdateOutcomeStatus::Succeeded,
            message: UPDATE_SUCCEEDED_MESSAGE.to_owned(),
            advisory: Some(advisory),
            manual_command: None,
            stderr: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_parsing_and_normalization() {
        let v1 = parse_semver("0.85.0").unwrap();
        assert_eq!(v1.major, 0);
        assert_eq!(v1.minor, 85);
        assert_eq!(v1.patch, 0);
        assert!(v1.prerelease.is_empty());

        let v2 = parse_semver("v1.2.3-alpha.1").unwrap();
        assert_eq!(v2.major, 1);
        assert_eq!(v2.minor, 2);
        assert_eq!(v2.patch, 3);
        assert_eq!(v2.prerelease, vec!["alpha", "1"]);

        let v_short = parse_semver("1.4").unwrap();
        assert_eq!(v_short.major, 1);
        assert_eq!(v_short.minor, 4);
        assert_eq!(v_short.patch, 0);
    }

    #[test]
    fn semver_comparison() {
        assert_eq!(compare_semver("0.85.0", "0.85.1"), std::cmp::Ordering::Less);
        assert_eq!(compare_semver("1.0.0", "1.0.0"), std::cmp::Ordering::Equal);
        assert_eq!(
            compare_semver("2.1.0", "2.0.99"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_semver("1.0.0-alpha", "1.0.0"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_semver("1.0.0-beta.2", "1.0.0-beta.10"),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn generic_version_extraction() {
        assert_eq!(
            parse_generic_cli_version("pi 0.85.0").as_deref(),
            Some("0.85.0")
        );
        assert_eq!(
            parse_generic_cli_version("claude-code v2.1.261 (Darwin)").as_deref(),
            Some("2.1.261")
        );
        assert_eq!(
            parse_generic_cli_version("opencode 1.18.29").as_deref(),
            Some("1.18.29")
        );
    }

    #[test]
    fn advisory_status_generation() {
        let def = get_maintenance_definition("pi").unwrap();
        let adv_behind = build_advisory(
            def,
            Some("0.85.0".into()),
            Some("0.85.1".into()),
            RuntimeInstallSource::Native,
            None,
        );
        assert_eq!(adv_behind.status, RuntimeUpdateStatus::BehindLatest);
        assert!(adv_behind.can_update);
        assert_eq!(adv_behind.update_command.as_deref(), Some("pi update"));

        let adv_current = build_advisory(
            def,
            Some("0.85.1".into()),
            Some("0.85.1".into()),
            RuntimeInstallSource::Native,
            None,
        );
        assert_eq!(adv_current.status, RuntimeUpdateStatus::Current);
        assert!(!adv_current.can_update);
    }
}
