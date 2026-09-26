//! Conservative cleanup for local branches whose worktree was just removed.
//!
//! A branch is deleted only when Git can prove that its tip is already present
//! in, or would add nothing to, the repository's known default branch. The
//! local branch update is compare-and-swap guarded by the OID observed before
//! worktree removal; every uncertain result preserves the branch.

use crate::git_command::{run_git, run_git_mutation_with_stdin};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BranchCleanupOutcome {
    Deleted,
    Retained(&'static str),
}

/// Remove `branch` only when its pre-removal tip is already integrated into
/// the known default branch. Callers capture `expected_oid` before removing
/// the clean worktree, then invoke this after Git has released the branch.
///
/// Errors and inconclusive Git results never delete the branch. The caller can
/// log an `Err` as a warning and continue treating worktree removal as
/// successful.
pub(crate) fn cleanup_integrated_branch(
    repository: &Path,
    branch: &str,
    expected_oid: &str,
) -> Result<BranchCleanupOutcome, String> {
    let repository = repository_root(repository)?;
    if branch.is_empty()
        || branch != branch.trim()
        || run_git(&repository, &["check-ref-format", "--branch", branch]).is_err()
    {
        return Err("Invalid local branch name; branch was preserved".into());
    }
    let branch_ref = format!("refs/heads/{branch}");
    let expected_oid = validate_commit_oid(&repository, expected_oid)?;
    if run_git(&repository, &["symbolic-ref", "--quiet", &branch_ref]).is_ok() {
        return Ok(BranchCleanupOutcome::Retained(
            "branch ref is symbolic and may point to another branch",
        ));
    }

    let Some(actual_oid) = local_branch_oid(&repository, &branch_ref)? else {
        return Ok(BranchCleanupOutcome::Retained("branch no longer exists"));
    };
    if actual_oid != expected_oid {
        return Ok(BranchCleanupOutcome::Retained(
            "branch moved after worktree removal began",
        ));
    }
    if is_protected_default_branch(&repository, &branch_ref)? {
        return Ok(BranchCleanupOutcome::Retained(
            "the branch is a default branch",
        ));
    }

    let Some((default_ref, default_oid)) = default_branch(&repository)? else {
        return Ok(BranchCleanupOutcome::Retained(
            "default branch could not be resolved",
        ));
    };
    if default_ref == branch_ref {
        return Ok(BranchCleanupOutcome::Retained(
            "the branch is the repository's default branch",
        ));
    }

    if !integrated(&repository, &default_ref, &default_oid, &expected_oid)? {
        return Ok(BranchCleanupOutcome::Retained(
            "integration into the default branch was not proven",
        ));
    }

    // Recheck immediately before the CAS delete. In particular, an agent may
    // have created another linked worktree for this branch while Git was
    // checking integration.
    let Some(current_oid) = local_branch_oid(&repository, &branch_ref)? else {
        return Ok(BranchCleanupOutcome::Retained("branch no longer exists"));
    };
    if current_oid != expected_oid {
        return Ok(BranchCleanupOutcome::Retained(
            "branch moved during cleanup",
        ));
    }
    if branch_is_checked_out(&repository, &branch_ref)? {
        return Ok(BranchCleanupOutcome::Retained(
            "another worktree still checks out the branch",
        ));
    }
    if default_branch(&repository)? != Some((default_ref.clone(), default_oid.clone())) {
        return Ok(BranchCleanupOutcome::Retained(
            "default branch moved during cleanup",
        ));
    }

    // One ref transaction checks the integration witness and deletes the
    // feature ref. A concurrent fetch/reset cannot invalidate the default
    // target in the gap between the last read and the branch deletion.
    match delete_if_refs_unchanged(
        &repository,
        &default_ref,
        &default_oid,
        &branch_ref,
        &expected_oid,
    ) {
        Ok(_) => Ok(BranchCleanupOutcome::Deleted),
        Err(error) => {
            // The CAS can lose a race after the last read. Distinguish that
            // expected case from a Git failure so the caller can log a useful
            // warning without suggesting that the branch disappeared.
            match local_branch_oid(&repository, &branch_ref) {
                Ok(Some(current)) if current != expected_oid => Ok(BranchCleanupOutcome::Retained(
                    "branch moved during cleanup",
                )),
                Ok(None) => Ok(BranchCleanupOutcome::Retained("branch no longer exists")),
                Ok(Some(_)) => Err(if error.trim().is_empty() {
                    "Git could not delete the integrated local branch; it was preserved".into()
                } else {
                    format!("Git could not delete the integrated local branch: {error}")
                }),
                Err(read_error) => Err(format!(
                    "Git could not delete the integrated local branch ({error}); branch state could not be rechecked ({read_error})"
                )),
            }
        }
    }
}

fn delete_if_refs_unchanged(
    repository: &Path,
    default_ref: &str,
    default_oid: &str,
    branch_ref: &str,
    expected_oid: &str,
) -> Result<String, String> {
    let transaction = format!(
        "start\noption no-deref\nverify {default_ref} {default_oid}\ndelete {branch_ref} {expected_oid}\nprepare\ncommit\n"
    );
    run_git_mutation_with_stdin(
        repository,
        &["update-ref", "--stdin"],
        transaction.as_bytes(),
    )
}

fn repository_root(repository: &Path) -> Result<PathBuf, String> {
    let canonical = std::fs::canonicalize(repository)
        .map_err(|error| format!("Cannot resolve repository path: {error}"))?;
    let root = run_git(&canonical, &["rev-parse", "--show-toplevel"])?;
    std::fs::canonicalize(root.trim())
        .map_err(|error| format!("Cannot resolve repository root: {error}"))
}

fn validate_commit_oid(repository: &Path, oid: &str) -> Result<String, String> {
    if oid != oid.trim() {
        return Err("Expected a full Git commit OID; branch was preserved".into());
    }
    if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Expected a full Git commit OID; branch was preserved".into());
    }
    let resolved = run_git(
        repository,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{oid}^{{commit}}"),
        ],
    )?;
    let resolved = resolved.trim().to_ascii_lowercase();
    if resolved != oid.to_ascii_lowercase() {
        return Err(
            "Expected OID does not identify the recorded commit; branch was preserved".into(),
        );
    }
    Ok(resolved)
}

fn local_branch_oid(repository: &Path, branch_ref: &str) -> Result<Option<String>, String> {
    match run_git(
        repository,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{branch_ref}^{{commit}}"),
        ],
    ) {
        Ok(oid) => Ok(Some(oid.trim().to_ascii_lowercase())),
        Err(error) if error.trim().is_empty() => Ok(None),
        Err(error) => Err(error),
    }
}

/// Resolve only an explicit `origin/HEAD` target, then the conventional local
/// `main` and `master` refs. The symbolic remote ref identifies the default
/// branch name; prefer its local branch, unless the upstream is strictly ahead
/// of that local branch. Never guess a feature branch or search arbitrary refs
/// for a likely integration target.
fn default_branch(repository: &Path) -> Result<Option<(String, String)>, String> {
    if let Some(name) = origin_default_branch_name(repository)? {
        let local_ref = format!("refs/heads/{name}");
        let upstream_ref = format!("refs/remotes/origin/{name}");
        let local_oid = commit_oid_at_ref(repository, &local_ref)?;
        let upstream_oid = commit_oid_at_ref(repository, &upstream_ref)?;
        match (local_oid, upstream_oid) {
            (Some(local_oid), Some(upstream_oid)) => {
                if upstream_oid != local_oid && is_ancestor(repository, &local_oid, &upstream_oid)?
                {
                    return Ok(Some((upstream_ref, upstream_oid)));
                }
                return Ok(Some((local_ref, local_oid)));
            }
            (Some(local_oid), None) => return Ok(Some((local_ref, local_oid))),
            (None, Some(upstream_oid)) => return Ok(Some((upstream_ref, upstream_oid))),
            (None, None) => {}
        }
    }

    for reference in ["refs/heads/main", "refs/heads/master"] {
        if let Some(oid) = commit_oid_at_ref(repository, &reference)? {
            return Ok(Some((reference.to_owned(), oid)));
        }
    }
    Ok(None)
}

fn origin_default_branch_name(repository: &Path) -> Result<Option<String>, String> {
    let symbolic = match run_git(
        repository,
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
    ) {
        Ok(symbolic) => symbolic,
        // No origin/HEAD is normal for local-only repositories.
        Err(error) if error.trim().is_empty() => return Ok(None),
        Err(error) => return Err(format!("Cannot resolve origin/HEAD: {error}")),
    };
    let Some(name) = symbolic
        .trim()
        .strip_prefix("refs/remotes/origin/")
        .filter(|name| !name.is_empty())
    else {
        return Ok(None);
    };
    if run_git(repository, &["check-ref-format", "--branch", name]).is_err() {
        return Ok(None);
    }
    Ok(Some(name.to_owned()))
}

fn is_protected_default_branch(repository: &Path, branch_ref: &str) -> Result<bool, String> {
    // These names are protected even when origin/HEAD is configured to a
    // differently named branch. They are the only safe fallback defaults in a
    // local-only repository.
    if matches!(branch_ref, "refs/heads/main" | "refs/heads/master") {
        return Ok(true);
    }
    Ok(origin_default_branch_name(repository)?
        .is_some_and(|name| branch_ref == format!("refs/heads/{name}")))
}

fn is_ancestor(repository: &Path, ancestor: &str, descendant: &str) -> Result<bool, String> {
    match run_git(repository, &["merge-base", ancestor, descendant]) {
        Ok(base) => Ok(base.trim() == ancestor),
        // Two valid but unrelated histories have no merge base. That does not
        // prove the upstream is ahead, so keep the local default selected.
        Err(error) if error.trim().is_empty() => Ok(false),
        Err(error) => Err(format!(
            "Cannot compare local and upstream default refs: {error}"
        )),
    }
}

fn commit_oid_at_ref(repository: &Path, reference: &str) -> Result<Option<String>, String> {
    match run_git(
        repository,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{reference}^{{commit}}"),
        ],
    ) {
        Ok(oid) => Ok(Some(oid.trim().to_ascii_lowercase())),
        Err(error) if error.trim().is_empty() => Ok(None),
        Err(error) => Err(error),
    }
}

fn integrated(
    repository: &Path,
    default_ref: &str,
    default_oid: &str,
    expected_oid: &str,
) -> Result<bool, String> {
    if default_oid == expected_oid {
        return Ok(true);
    }

    // A direct ancestry check is the strongest and cheapest proof for normal
    // merges and fast-forwards.
    match run_git(repository, &["merge-base", expected_oid, default_oid]) {
        Ok(base) if base.trim() == expected_oid => return Ok(true),
        Ok(_) => {}
        // Git exits without stderr when two valid commits have no common
        // ancestor; other failures carry diagnostics and stay fail-closed.
        Err(error) if error.trim().is_empty() => {}
        Err(error) => return Err(format!("Cannot compare branch ancestry: {error}")),
    }

    // An empty three-dot diff proves that the branch contributes no tree
    // changes since the merge base. `git diff --quiet` reports differences as
    // a nonzero exit with no stderr, which run_git represents as Err("").
    match run_git(
        repository,
        &[
            "diff",
            "--quiet",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=none",
            &format!("{default_ref}...{expected_oid}"),
        ],
    ) {
        Ok(_) => return Ok(true),
        Err(error) if error.trim().is_empty() => {}
        Err(error) => return Err(format!("Cannot compare branch trees: {error}")),
    }

    let default_tree = run_git(
        repository,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{default_oid}^{{tree}}"),
        ],
    )?;
    let branch_tree = run_git(
        repository,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{expected_oid}^{{tree}}"),
        ],
    )?;
    if default_tree.trim() == branch_tree.trim() {
        return Ok(true);
    }

    // This also recognizes a squash merge: the commits differ and the branch
    // tip's tree may differ because the default branch has unrelated newer
    // changes, but merging the branch into the default tree adds nothing.
    let merged = match run_git(
        repository,
        &["merge-tree", "--write-tree", default_oid, expected_oid],
    ) {
        Ok(merged) => merged,
        Err(error) if error.trim().is_empty() => return Ok(false),
        Err(error) => return Err(format!("Cannot prove branch integration: {error}")),
    };
    let merged_tree = merged.lines().next().unwrap_or_default().trim();
    Ok(merged_tree == default_tree.trim())
}

fn branch_is_checked_out(repository: &Path, branch_ref: &str) -> Result<bool, String> {
    let listing = run_git(repository, &["worktree", "list", "--porcelain"])?;
    Ok(listing
        .lines()
        .any(|line| line.strip_prefix("branch ") == Some(branch_ref)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    struct Repo(tempfile::TempDir);

    impl Repo {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("temporary repository");
            git(&dir.path(), &["init", "--quiet", "--initial-branch=main"]);
            git(&dir.path(), &["config", "user.name", "Comet Test"]);
            git(
                &dir.path(),
                &["config", "user.email", "comet-test@example.invalid"],
            );
            std::fs::write(dir.path().join("shared.txt"), "base\n").unwrap();
            git(&dir.path(), &["add", "shared.txt"]);
            git(&dir.path(), &["commit", "--quiet", "-m", "base"]);
            Self(dir)
        }

        fn path(&self) -> &Path {
            self.0.path()
        }

        fn branch_commit(&self, branch: &str) -> String {
            git(self.path(), &["rev-parse", &format!("refs/heads/{branch}")])
        }

        fn main_commit(&self) -> String {
            self.branch_commit("main")
        }

        fn commit_on(&self, branch: &str, file: &str, body: &str, message: &str) -> String {
            git(self.path(), &["checkout", "--quiet", branch]);
            std::fs::write(self.path().join(file), body).unwrap();
            git(self.path(), &["add", file]);
            git(self.path(), &["commit", "--quiet", "-m", message]);
            self.branch_commit(branch)
        }

        fn create_feature(&self) -> String {
            git(self.path(), &["checkout", "--quiet", "-b", "feature/test"]);
            self.commit_on(
                "feature/test",
                "shared.txt",
                "feature change\n",
                "feature change",
            )
        }
    }

    fn git(repository: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("git starts");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("git output is UTF-8")
            .trim()
            .to_owned()
    }

    fn cleanup(repo: &Repo, oid: &str) -> BranchCleanupOutcome {
        cleanup_integrated_branch(repo.path(), "feature/test", oid).unwrap()
    }

    fn branch_exists(repo: &Repo) -> bool {
        !git(
            repo.path(),
            &[
                "for-each-ref",
                "--format=%(refname)",
                "refs/heads/feature/test",
            ],
        )
        .is_empty()
    }

    #[test]
    fn deletes_a_branch_that_is_an_ancestor_of_main() {
        let repo = Repo::new();
        let feature_oid = repo.create_feature();
        git(repo.path(), &["checkout", "--quiet", "main"]);
        git(
            repo.path(),
            &["merge", "--quiet", "--ff-only", "feature/test"],
        );
        git(
            repo.path(),
            &[
                "commit",
                "--allow-empty",
                "--quiet",
                "-m",
                "later main commit",
            ],
        );

        assert_eq!(cleanup(&repo, &feature_oid), BranchCleanupOutcome::Deleted);
        assert!(!branch_exists(&repo));
    }

    #[test]
    fn prefers_local_main_when_origin_head_is_behind_it() {
        let repo = Repo::new();
        let origin_main_oid = repo.main_commit();
        let feature_oid = repo.create_feature();
        git(repo.path(), &["checkout", "--quiet", "main"]);
        git(
            repo.path(),
            &["merge", "--quiet", "--ff-only", "feature/test"],
        );
        git(
            repo.path(),
            &[
                "commit",
                "--allow-empty",
                "--quiet",
                "-m",
                "local main moved ahead",
            ],
        );
        git(
            repo.path(),
            &["update-ref", "refs/remotes/origin/main", &origin_main_oid],
        );
        git(
            repo.path(),
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );

        assert_eq!(cleanup(&repo, &feature_oid), BranchCleanupOutcome::Deleted);
        assert!(!branch_exists(&repo));
        assert_eq!(
            git(repo.path(), &["rev-parse", "refs/remotes/origin/main"]),
            origin_main_oid,
            "cleanup deletes only the local feature ref"
        );
    }

    #[test]
    fn uses_origin_default_when_it_is_strictly_ahead_of_local_main() {
        let repo = Repo::new();
        let feature_oid = repo.create_feature();
        git(
            repo.path(),
            &["update-ref", "refs/remotes/origin/main", &feature_oid],
        );
        git(
            repo.path(),
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
        git(repo.path(), &["checkout", "--quiet", "main"]);

        assert_eq!(cleanup(&repo, &feature_oid), BranchCleanupOutcome::Deleted);
        assert!(!branch_exists(&repo));
        assert_eq!(
            git(repo.path(), &["rev-parse", "refs/remotes/origin/main"]),
            feature_oid,
            "cleanup must never delete remote-tracking refs"
        );
    }

    #[test]
    fn never_deletes_local_default_branch_when_origin_head_is_ahead() {
        let repo = Repo::new();
        let local_main_oid = repo.main_commit();
        let upstream_oid = repo.create_feature();
        git(
            repo.path(),
            &["update-ref", "refs/remotes/origin/main", &upstream_oid],
        );
        git(
            repo.path(),
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );

        assert_eq!(
            cleanup_integrated_branch(repo.path(), "main", &local_main_oid).unwrap(),
            BranchCleanupOutcome::Retained("the branch is a default branch")
        );
        assert_eq!(repo.main_commit(), local_main_oid);
        assert_eq!(
            git(repo.path(), &["rev-parse", "refs/remotes/origin/main"]),
            upstream_oid
        );
    }

    #[test]
    fn retains_a_branch_with_unmerged_work() {
        let repo = Repo::new();
        let feature_oid = repo.create_feature();

        assert_eq!(
            cleanup(&repo, &feature_oid),
            BranchCleanupOutcome::Retained("integration into the default branch was not proven")
        );
        assert!(branch_exists(&repo));
    }

    #[test]
    fn deletes_a_squash_merged_branch_when_main_has_later_changes() {
        let repo = Repo::new();
        let feature_oid = repo.create_feature();
        git(repo.path(), &["checkout", "--quiet", "main"]);
        git(repo.path(), &["merge", "--squash", "feature/test"]);
        std::fs::write(repo.path().join("later.txt"), "main-only change\n").unwrap();
        git(repo.path(), &["add", "later.txt"]);
        git(
            repo.path(),
            &["commit", "--quiet", "-m", "squash feature plus main change"],
        );

        assert_ne!(
            git(repo.path(), &["rev-parse", "main^{tree}"]),
            git(repo.path(), &["rev-parse", "feature/test^{tree}"]),
            "the squash target has an unrelated extra file, so same-tree proof cannot decide this case"
        );
        assert_eq!(cleanup(&repo, &feature_oid), BranchCleanupOutcome::Deleted);
        assert!(!branch_exists(&repo));
    }

    #[test]
    fn retains_a_branch_whose_ref_moved_after_the_expected_oid_was_captured() {
        let repo = Repo::new();
        let expected_oid = repo.create_feature();
        let moved_oid = repo.commit_on(
            "feature/test",
            "shared.txt",
            "newer unmerged work\n",
            "newer feature commit",
        );

        assert_eq!(
            cleanup(&repo, &expected_oid),
            BranchCleanupOutcome::Retained("branch moved after worktree removal began")
        );
        assert_eq!(repo.branch_commit("feature/test"), moved_oid);
        assert!(branch_exists(&repo));
    }

    #[test]
    fn does_not_delete_a_branch_checked_out_in_another_worktree() {
        let repo = Repo::new();
        let feature_oid = repo.create_feature();
        git(repo.path(), &["checkout", "--quiet", "main"]);
        git(
            repo.path(),
            &["merge", "--quiet", "--ff-only", "feature/test"],
        );
        let other_worktree = repo.0.path().join("other-worktree");
        git(
            repo.path(),
            &[
                "worktree",
                "add",
                "--quiet",
                other_worktree.to_str().unwrap(),
                "feature/test",
            ],
        );

        assert_eq!(
            cleanup(&repo, &feature_oid),
            BranchCleanupOutcome::Retained("another worktree still checks out the branch")
        );
        assert!(branch_exists(&repo));
    }

    #[test]
    fn a_moving_default_ref_aborts_the_atomic_branch_delete() {
        let repo = Repo::new();
        let feature_oid = repo.create_feature();
        git(repo.path(), &["checkout", "--quiet", "main"]);
        git(
            repo.path(),
            &["merge", "--quiet", "--ff-only", "feature/test"],
        );
        let observed_main = repo.main_commit();
        git(
            repo.path(),
            &["commit", "--allow-empty", "--quiet", "-m", "main moved"],
        );

        assert!(
            delete_if_refs_unchanged(
                repo.path(),
                "refs/heads/main",
                &observed_main,
                "refs/heads/feature/test",
                &feature_oid,
            )
            .is_err()
        );
        assert!(branch_exists(&repo));
    }

    #[test]
    fn ignored_submodule_diffs_cannot_make_unmerged_gitlinks_look_integrated() {
        let repo = Repo::new();
        let base_oid = repo.main_commit();
        git(repo.path(), &["checkout", "--quiet", "-b", "feature/test"]);
        git(
            repo.path(),
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("160000,{base_oid},nested"),
            ],
        );
        git(repo.path(), &["commit", "--quiet", "-m", "add gitlink"]);
        let feature_oid = repo.branch_commit("feature/test");
        git(repo.path(), &["checkout", "--quiet", "main"]);
        git(repo.path(), &["config", "diff.ignoreSubmodules", "all"]);

        assert_eq!(
            cleanup(&repo, &feature_oid),
            BranchCleanupOutcome::Retained("integration into the default branch was not proven")
        );
        assert!(branch_exists(&repo));
    }

    #[test]
    fn a_symbolic_feature_ref_never_deletes_its_default_branch_target() {
        let repo = Repo::new();
        let main_oid = repo.main_commit();
        git(
            repo.path(),
            &["symbolic-ref", "refs/heads/feature/test", "refs/heads/main"],
        );

        assert_eq!(
            cleanup(&repo, &main_oid),
            BranchCleanupOutcome::Retained(
                "branch ref is symbolic and may point to another branch"
            )
        );
        assert_eq!(repo.main_commit(), main_oid);
        assert!(branch_exists(&repo));
    }
}
