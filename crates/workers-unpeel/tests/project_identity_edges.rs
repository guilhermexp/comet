use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use zeron_workers_unpeel::project_identity::{
    self, CheckoutAvailability, CheckoutKind, CheckoutOwnership, IdentityRegistry,
};

fn git_at(path: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git -C {} {args:?} failed: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

fn git_raw(args: &[&str]) -> Result<(), Box<dyn Error>> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

fn init_repo(path: &Path, branch: &str, contents: &str) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(path)?;
    git_at(path, &["init", "-q", "-b", branch])?;
    git_at(path, &["config", "user.email", "workers@example.test"])?;
    git_at(path, &["config", "user.name", "Workers Tests"])?;
    fs::write(path.join("README.md"), contents)?;
    git_at(path, &["add", "README.md"])?;
    git_at(path, &["commit", "-qm", "fixture"])?;
    Ok(())
}

fn clone_repo(remote: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    let remote = remote.to_str().ok_or("remote path is not UTF-8")?;
    let destination = destination
        .to_str()
        .ok_or("clone destination is not UTF-8")?;
    git_raw(&["clone", "-q", remote, destination])
}

fn project(id: &str, path: &Path) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": id,
        "path": path.to_string_lossy(),
        "workspace_id": "identity-edge-tests"
    })
}

fn state_for(projects: &[(&str, &Path)]) -> serde_json::Value {
    serde_json::json!({
        "projects": projects
            .iter()
            .map(|(id, path)| project(id, path))
            .collect::<Vec<_>>()
    })
}

fn observation(id: &str, path: &Path) -> project_identity::CheckoutObservation {
    let mut observation = project_identity::probe_checkout(path);
    observation.project_id = Some(id.to_owned());
    observation.ownership = Some(CheckoutOwnership::External);
    observation
}

fn registry(state: &serde_json::Value) -> Result<IdentityRegistry, Box<dyn Error>> {
    Ok(serde_json::from_value(
        state[project_identity::IDENTITY_KEY].clone(),
    )?)
}

fn apply(
    state: &mut serde_json::Value,
    observations: &[project_identity::CheckoutObservation],
) -> Result<(), Box<dyn Error>> {
    let plan = project_identity::plan_reconciliation(state, observations);
    project_identity::apply_reconciliation(state, &plan)?;
    Ok(())
}

#[test]
fn distinct_clones_with_the_same_remote_remain_separate_repositories() -> Result<(), Box<dyn Error>>
{
    let home = TempDir::new()?;
    let seed = home.path().join("seed");
    let remote = home.path().join("origin.git");
    let clone_a = home.path().join("clone-a");
    let clone_b = home.path().join("clone-b");

    init_repo(&seed, "main", "seed\n")?;
    git_raw(&[
        "init",
        "--bare",
        "-q",
        remote.to_str().ok_or("remote path is not UTF-8")?,
    ])?;
    git_at(
        &seed,
        &[
            "remote",
            "add",
            "origin",
            remote.to_str().ok_or("remote path is not UTF-8")?,
        ],
    )?;
    git_at(&seed, &["push", "-q", "-u", "origin", "main"])?;
    clone_repo(&remote, &clone_a)?;
    clone_repo(&remote, &clone_b)?;

    let mut state = state_for(&[("clone-a", &clone_a), ("clone-b", &clone_b)]);
    let observations = [
        observation("clone-a", &clone_a),
        observation("clone-b", &clone_b),
    ];
    apply(&mut state, &observations)?;
    let registry = registry(&state)?;

    assert_eq!(registry.repositories.len(), 2);
    let first = registry.checkout("clone-a").expect("first checkout");
    let second = registry.checkout("clone-b").expect("second checkout");
    assert_ne!(first.repository_id, second.repository_id);
    assert_ne!(first.observed_common_dir, second.observed_common_dir);
    assert_eq!(first.remotes, second.remotes);
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlink_alias_of_one_repository_uses_one_repository_identity() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let repository = home.path().join("repository");
    let alias = home.path().join("repository-alias");
    init_repo(&repository, "main", "repository\n")?;
    std::os::unix::fs::symlink(&repository, &alias)?;

    let mut state = state_for(&[("real", &repository), ("alias", &alias)]);
    let observations = [
        observation("real", &repository),
        observation("alias", &alias),
    ];
    apply(&mut state, &observations)?;
    let registry = registry(&state)?;

    let real = registry.checkout("real").expect("real checkout");
    let alias = registry.checkout("alias").expect("alias checkout");
    assert_eq!(real.repository_id, alias.repository_id);
    assert_eq!(real.canonical_path, alias.canonical_path);
    assert_eq!(real.observed_common_dir, alias.observed_common_dir);
    assert_eq!(registry.repositories.len(), 1);
    Ok(())
}

#[test]
fn detached_checkout_persists_oid_without_live_branch_context() -> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let repository = home.path().join("repository");
    init_repo(&repository, "feature/pr-42", "repository\n")?;
    let mut state = state_for(&[("detached", &repository)]);

    apply(&mut state, &[observation("detached", &repository)])?;
    git_at(&repository, &["checkout", "-q", "--detach", "HEAD"])?;
    apply(&mut state, &[observation("detached", &repository)])?;

    let registry = registry(&state)?;
    let checkout = registry.checkout("detached").expect("detached checkout");
    assert_eq!(checkout.kind, CheckoutKind::Primary);
    assert_eq!(checkout.availability, CheckoutAvailability::Available);
    assert_eq!(checkout.branch, None);
    assert!(checkout.detached);
    assert!(checkout.detached_oid.is_some());
    assert_eq!(checkout.last_known_branch.as_deref(), Some("feature/pr-42"));
    Ok(())
}

#[test]
fn reusing_a_project_path_for_another_repository_keeps_old_association_as_conflict()
-> Result<(), Box<dyn Error>> {
    let home = TempDir::new()?;
    let repository_path = home.path().join("reused-path");
    let old_remote = home.path().join("old-origin.git");
    let new_remote = home.path().join("new-origin.git");
    let old_seed = home.path().join("old-seed");
    let new_seed = home.path().join("new-seed");

    init_repo(&old_seed, "main", "old\n")?;
    init_repo(&new_seed, "main", "new\n")?;
    git_raw(&[
        "init",
        "--bare",
        "-q",
        old_remote.to_str().ok_or("old remote path is not UTF-8")?,
    ])?;
    git_raw(&[
        "init",
        "--bare",
        "-q",
        new_remote.to_str().ok_or("new remote path is not UTF-8")?,
    ])?;
    git_at(
        &old_seed,
        &[
            "remote",
            "add",
            "origin",
            old_remote.to_str().ok_or("old remote path is not UTF-8")?,
        ],
    )?;
    git_at(&old_seed, &["push", "-q", "-u", "origin", "main"])?;
    git_at(
        &new_seed,
        &[
            "remote",
            "add",
            "origin",
            new_remote.to_str().ok_or("new remote path is not UTF-8")?,
        ],
    )?;
    git_at(&new_seed, &["push", "-q", "-u", "origin", "main"])?;

    fs::rename(&old_seed, &repository_path)?;
    let mut state = state_for(&[("reused", &repository_path)]);
    apply(&mut state, &[observation("reused", &repository_path)])?;
    let old_repository_id = registry(&state)?
        .checkout("reused")
        .and_then(|checkout| checkout.repository_id.clone())
        .expect("initial repository association");

    fs::remove_dir_all(&repository_path)?;
    fs::rename(&new_seed, &repository_path)?;
    apply(&mut state, &[observation("reused", &repository_path)])?;

    let registry = registry(&state)?;
    let checkout = registry.checkout("reused").expect("reused checkout");
    assert_eq!(
        checkout.repository_id.as_deref(),
        Some(old_repository_id.as_str())
    );
    assert!(
        checkout.conflict.is_some(),
        "replacing a registered path must not silently inherit its old repository identity"
    );
    Ok(())
}
