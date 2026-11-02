//! W1: a Worker checkout lands where the device's saved worktree preference
//! says, the same root a Chat worktree uses, with `ZERON_WORKTREES_DIR`
//! still taking precedence. Everything runs inside temporary HOME, data dir
//! and Unpeel home, and Git runs with an empty global config, no system
//! config, no inherited overrides and none of Git's repository-selecting
//! variables; no real user directory, config or index is read or written.
//! Platform scope: the saved-root and environment-override stages run on
//! every platform; the default-root stage is Unix-only because only there
//! does the temporary HOME redirect the user's home directory.
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use zeron_workers_unpeel::{LocalWorkersClient, WorkersCreateWorktreeRequest};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard(Vec<(&'static str, Option<OsString>)>);

impl EnvGuard {
    fn apply(values: &[(&'static str, Option<PathBuf>)]) -> Self {
        let previous = values
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();
        // SAFETY: every environment mutation in this binary holds ENV_LOCK.
        unsafe {
            for (key, value) in values {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
        Self(previous)
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: the caller still holds ENV_LOCK.
        unsafe {
            for (key, previous) in self.0.drain(..) {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

fn git(repo: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .args(args)
            .current_dir(repo)
            .status()
            .unwrap()
            .success(),
        "git {args:?}"
    );
}

fn fixture_repo(repo: &Path) {
    fs::create_dir_all(repo).unwrap();
    fs::write(repo.join("README.md"), "fixture\n").unwrap();
    git(repo, &["init", "-b", "main"]);
    git(repo, &["config", "user.email", "workers@example.test"]);
    git(repo, &["config", "user.name", "Workers Tests"]);
    git(repo, &["add", "README.md"]);
    git(repo, &["commit", "-m", "fixture"]);
}

/// A deliberately hostile, synthetic "inherited" Git environment: global
/// config that signs every commit with a failing program and runs failing
/// hooks, a `GIT_CONFIG_COUNT` injection, and repository-selecting variables
/// pointing at sentinel files (`GIT_CONFIG`, `GIT_INDEX_FILE`) whose bytes
/// must never change. Every program leaves a marker. It all lives in the
/// test's temporary directory; no real user config or index is read.
fn hostile_inherited_git(temp: &Path) -> Vec<(&'static str, Option<PathBuf>)> {
    let dir = temp.join("hostile");
    let hooks = dir.join("hooks");
    fs::create_dir_all(&hooks).unwrap();
    let script = |path: &Path, marker: &str| {
        fs::write(
            path,
            format!(
                "#!/bin/sh\ntouch \"{}\"\nexit 1\n",
                dir.join(marker).display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    };
    script(&dir.join("gpg"), "signing-ran");
    for hook in ["pre-commit", "commit-msg", "post-checkout"] {
        script(&hooks.join(hook), "hook-ran");
    }
    let config = dir.join("gitconfig");
    fs::write(
        &config,
        format!(
            "[commit]\n\tgpgsign = true\n[gpg]\n\tprogram = {}\n[core]\n\thooksPath = {}\n",
            dir.join("gpg").display(),
            hooks.display()
        ),
    )
    .unwrap();
    fs::write(dir.join("sentinel-config"), SENTINEL_CONFIG).unwrap();
    fs::write(dir.join("sentinel-index"), SENTINEL_INDEX).unwrap();
    vec![
        ("GIT_CONFIG", Some(dir.join("sentinel-config"))),
        ("GIT_INDEX_FILE", Some(dir.join("sentinel-index"))),
        ("GIT_CONFIG_GLOBAL", Some(config)),
        ("GIT_CONFIG_COUNT", Some(PathBuf::from("1"))),
        ("GIT_CONFIG_KEY_0", Some(PathBuf::from("core.hooksPath"))),
        ("GIT_CONFIG_VALUE_0", Some(hooks)),
    ]
}

/// Git's repository-local variables (`git rev-parse --local-env-vars`), the
/// same list `src/git_command.rs` clears for product Git commands; mirrored
/// here so the test isolates without a new product API.
const REPOSITORY_ENV_VARS: [&str; 16] = [
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_INTERNAL_SUPER_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

const SENTINEL_CONFIG: &str = "# sentinel: an inherited GIT_CONFIG must stay untouched\n";
const SENTINEL_INDEX: &[u8] = b"sentinel: an inherited GIT_INDEX_FILE must stay untouched";

/// Markers of hostile programs that ran, and sentinel files that changed.
fn hostile_ran(temp: &Path) -> Vec<&'static str> {
    let dir = temp.join("hostile");
    let mut ran: Vec<&'static str> = ["signing-ran", "hook-ran"]
        .into_iter()
        .filter(|marker| dir.join(marker).exists())
        .collect();
    if fs::read(dir.join("sentinel-config")).ok().as_deref() != Some(SENTINEL_CONFIG.as_bytes()) {
        ran.push("sentinel-config-changed");
    }
    if fs::read(dir.join("sentinel-index")).ok().as_deref() != Some(SENTINEL_INDEX) {
        ran.push("sentinel-index-changed");
    }
    ran
}

fn save_preference(data_dir: &Path, enabled: bool, folder: &Path) {
    fs::create_dir_all(data_dir).unwrap();
    fs::write(
        data_dir.join("worktree-settings.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "useCustomDirectory": enabled,
            "customDirectory": folder,
        }))
        .unwrap(),
    )
    .unwrap();
}

fn create(branch: &str) -> PathBuf {
    let result = LocalWorkersClient::create_worktree_at(
        &unpeel_core::app_paths::app_state_path(),
        WorkersCreateWorktreeRequest {
            project_id: "root".into(),
            branch: branch.into(),
            name: None,
            base_ref: Some("main".into()),
        },
    )
    .expect("worktree created");
    fs::canonicalize(result.path).unwrap()
}

fn canonical(path: &Path) -> PathBuf {
    fs::create_dir_all(path).unwrap();
    fs::canonicalize(path).unwrap()
}

#[test]
fn worker_checkouts_follow_the_saved_root_with_environment_precedence() {
    let _lock = ENV_LOCK.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let data_dir = temp.path().join("data");
    let unpeel_home = temp.path().join("unpeel");
    let custom = temp.path().join("custom-worktrees");
    let forced = temp.path().join("forced-worktrees");
    let repo = temp.path().join("repo");
    for dir in [&home, &data_dir, &unpeel_home] {
        fs::create_dir_all(dir).unwrap();
    }
    // What the test process might inherit from a developer's machine.
    let _inherited = EnvGuard::apply(&hostile_inherited_git(temp.path()));
    // Isolation comes before any Git runs: temporary HOME and data dirs,
    // an empty global config, no system config, no inherited overrides and
    // no inherited repository selection.
    let git_global = temp.path().join("gitconfig");
    fs::write(&git_global, "").unwrap();
    let _repository = EnvGuard::apply(&REPOSITORY_ENV_VARS.map(|name| (name, None)));
    let _env = EnvGuard::apply(&[
        ("HOME", Some(home.clone())),
        ("ZERON_DATA_DIR", Some(data_dir.clone())),
        ("UNPEEL_HOME", Some(unpeel_home.clone())),
        (
            "ZERON_WORKTREE_OWNERSHIP_FILE",
            Some(temp.path().join("worktree-ownership.json")),
        ),
        ("ZERON_WORKTREES_DIR", None),
        ("XDG_CONFIG_HOME", Some(temp.path().join("xdg-config"))),
        ("GIT_CONFIG_GLOBAL", Some(git_global)),
        ("GIT_CONFIG_NOSYSTEM", Some(PathBuf::from("1"))),
    ]);
    fixture_repo(&repo);
    fs::write(
        unpeel_home.join("app-state.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "projects": [{
                "id": "root", "name": "Root", "path": repo,
                "workspace_id": "personal", "sort_order": 0
            }],
            "presets": [], "active_tabs": {}, "pinned_sessions": {}
        }))
        .unwrap(),
    )
    .unwrap();

    // Saved and enabled: the Worker checkout lands under the custom root.
    save_preference(&data_dir, true, &custom);
    let enabled = create("feature/custom-root");
    assert!(
        enabled.starts_with(canonical(&custom)),
        "{} is not under the saved root",
        enabled.display()
    );

    // The environment override still wins over the saved preference.
    let _forced = EnvGuard::apply(&[("ZERON_WORKTREES_DIR", Some(forced.clone()))]);
    let overridden = create("feature/forced-root");
    assert!(overridden.starts_with(canonical(&forced)));
    drop(_forced);

    // An explicit disable keeps the folder remembered but uses the default.
    // Unix only: the default root comes from the user's home directory, and
    // only on Unix does the temporary HOME above redirect it. On Windows the
    // home is the profile folder (not HOME), so this stage would touch the
    // real profile; the saved-root and override stages above, and the pure
    // Windows resolver tests, cover what can be isolated there.
    #[cfg(unix)]
    {
        save_preference(&data_dir, false, &custom);
        let disabled = create("feature/default-root");
        assert!(disabled.starts_with(canonical(&home.join(".zeron").join("worktrees"))));
    }

    // Existing checkouts are never moved by a later preference change.
    assert!(enabled.join("README.md").exists());
    assert!(overridden.join("README.md").exists());
    // No inherited signing program or hook ran, in the fixture or the product.
    assert_eq!(hostile_ran(temp.path()), Vec::<&str>::new());
}
