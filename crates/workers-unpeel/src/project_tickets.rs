//! The harness work tickets that target a project — the Tickets tab of
//! Settings → Projects and the `tickets` section of the Workers controller's
//! `list_projects` read them here, with the same parser and matching. They
//! still live in the Orchestrator workspace
//! (`<workspace>/brain-source/projects/<name>/tickets/WT-*.md`, written only by
//! `work-ticket.sh`); this module reads them and never writes.
//! [`read_tickets`] is the cheap read (frontmatter only); [`load_tickets`]
//! also links OpenSpec changes, which runs git and reads every OpenSpec tree,
//! so only the Settings page pays for it, off the render path.

use std::path::{Path, PathBuf};

use crate::project_activity::ProjectEntry;

/// One harness ticket, from its restricted YAML frontmatter (scalars and
/// lists of strings) plus the markdown body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub id: String,
    pub title: String,
    /// `open`, `closed-out`, `accepted` or `rejected`.
    pub status: String,
    /// `created` as written by the harness (`YYYY-MM-DDTHH:MM:SS`, local).
    pub created: String,
    /// The checkout the ticket works in.
    pub cwd: String,
    pub workers: Vec<String>,
    pub proven: Vec<String>,
    pub not_proven: Vec<String>,
    /// `claim => evidence`: a not-proven claim settled later.
    pub resolved: Vec<String>,
    /// `axis:tool=evidence` for the three proof axes (code, review, reality).
    pub axes: Vec<String>,
    pub gate: String,
    /// `baseline..head` of the recorded delivery, from `record`.
    pub record_range: Option<(String, String)>,
    pub next: Option<String>,
    /// `brain-source/projects/<name>`: the harness's own project name.
    pub brain_project: String,
    pub path: PathBuf,
    pub body: String,
    /// OpenSpec changes tied to the ticket by real evidence; filled by
    /// [`load_tickets`], empty when nothing links them.
    pub specs: Vec<SpecLink>,
}

impl Ticket {
    /// A not-proven claim the harness later settled (`resolved` keeps
    /// `claim => evidence`; the claim stays in `not_proven`).
    pub fn claim_resolved(&self, claim: &str) -> bool {
        self.resolved
            .iter()
            .any(|entry| entry.split(" => ").next() == Some(claim))
    }

    /// `created` is the harness's local wall-clock time.
    pub fn created_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        chrono::NaiveDateTime::parse_from_str(&self.created, "%Y-%m-%dT%H:%M:%S")
            .ok()?
            .and_local_timezone(chrono::Local)
            .single()
            .map(|at| at.with_timezone(&chrono::Utc))
    }
}

/// Where a ticket ↔ change link comes from. The harness keeps no explicit
/// link, so every one is evidence found on disk, named in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpecSource {
    /// The ticket's delivered commits (`record` range) touched the change.
    Commits,
    /// The Worker brief lists the change in `refs:`.
    Brief,
    /// The ticket itself names the change's path.
    Ticket,
    /// A file of the change cites the ticket id.
    Citation,
    /// The ticket id's slug is the change's name.
    Name,
}

impl SpecSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Commits => "ticket commits",
            Self::Brief => "worker brief",
            Self::Ticket => "ticket text",
            Self::Citation => "cites the ticket",
            Self::Name => "same name",
        }
    }
}

/// One OpenSpec change tied to a ticket, read from the repo it lives in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecLink {
    /// Change directory name without the archive date prefix.
    pub change: String,
    pub archived: bool,
    pub path: PathBuf,
    /// First paragraph of the proposal's `## Why`.
    pub why: Option<String>,
    pub tasks_done: usize,
    pub tasks_total: usize,
    /// `specs/<capability>/` directories of the change.
    pub capabilities: Vec<String>,
    pub sources: Vec<SpecSource>,
}

/// `$ORCH_WORKSPACE`, else `~/orchestrator` — the same workspace
/// `work_ticket_lib.py` resolves.
pub fn orchestrator_workspace() -> Option<PathBuf> {
    std::env::var_os("ORCH_WORKSPACE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join("orchestrator")))
}

/// Every readable ticket of the workspace, with its spec links resolved.
/// An unreadable ticket or a repo git cannot read is skipped; only a
/// workspace whose ticket root cannot be read is an error.
pub fn load_tickets(workspace: &Path) -> Result<Vec<Ticket>, String> {
    let mut tickets = read_tickets(workspace)?;
    let briefs = workspace.join(".tmp").join("briefs");
    let mut index = RepoIndex::default();
    for ticket in &mut tickets {
        ticket.specs = index.link_specs(ticket, &briefs);
    }
    Ok(tickets)
}

/// Every readable ticket of the workspace, frontmatter and body only (no
/// spec links: no git, no OpenSpec reads). The error names the ticket root
/// that could not be read.
pub fn read_tickets(workspace: &Path) -> Result<Vec<Ticket>, String> {
    let root = workspace.join("brain-source").join("projects");
    let projects = std::fs::read_dir(&root)
        .map_err(|error| format!("Orchestrator tickets at {}: {error}", root.display()))?;
    let mut tickets = Vec::new();
    for project in projects.flatten() {
        let brain_project = project.file_name().to_string_lossy().into_owned();
        let Ok(files) = std::fs::read_dir(project.path().join("tickets")) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            tickets.extend(parse_ticket(&text, &brain_project, path));
        }
    }
    Ok(tickets)
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    let quoted = value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')));
    if quoted {
        value[1..value.len() - 1].replace("\\\"", "\"")
    } else {
        value.to_owned()
    }
}

/// The frontmatter the harness writes: `key: scalar` or `key:` followed by
/// `  - "item"` lines. Anything else is ignored.
fn parse_ticket(text: &str, brain_project: &str, path: PathBuf) -> Option<Ticket> {
    let rest = text.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    let (frontmatter, body) = (&rest[..end], &rest[end + "\n---\n".len()..]);
    let mut scalars: Vec<(String, String)> = Vec::new();
    let mut lists: Vec<(String, Vec<String>)> = Vec::new();
    let mut current_list: Option<usize> = None;
    for line in frontmatter.lines() {
        if let Some(item) = line.trim_start().strip_prefix("- ")
            && line.starts_with(' ')
        {
            if let Some(index) = current_list {
                lists[index].1.push(unquote(item));
            }
            continue;
        }
        current_list = None;
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_owned();
        if value.trim().is_empty() {
            lists.push((key.clone(), Vec::new()));
            current_list = Some(lists.len() - 1);
            scalars.push((key, String::new()));
        } else {
            scalars.push((key, unquote(value)));
        }
    }
    let scalar = |key: &str| {
        scalars
            .iter()
            .find(|(known, _)| known == key)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let list = |key: &str| {
        lists
            .iter()
            .find(|(known, _)| known == key)
            .map(|(_, items)| items.clone())
            .unwrap_or_default()
    };
    let id = scalar("id");
    if id.is_empty() {
        return None;
    }
    let next = scalar("next");
    // record = "<by>|<baseline>..<head>|<fingerprint>|…"
    let record = scalar("record");
    let record_range = record
        .split('|')
        .nth(1)
        .and_then(|range| range.split_once(".."))
        .filter(|(base, head)| !base.is_empty() && !head.is_empty())
        .map(|(base, head)| (base.to_owned(), head.to_owned()));
    Some(Ticket {
        title: Some(scalar("title"))
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| id.clone()),
        id,
        status: scalar("status"),
        created: scalar("created"),
        cwd: scalar("cwd"),
        workers: list("workers"),
        proven: list("proven"),
        not_proven: list("not_proven"),
        resolved: list("resolved"),
        axes: list("axes"),
        gate: scalar("gate"),
        record_range,
        next: (!next.is_empty()).then_some(next),
        brain_project: brain_project.to_owned(),
        path,
        body: body.trim().to_owned(),
        specs: Vec::new(),
    })
}

// ── Spec links ──────────────────────────────────────────────────────────────

/// `openspec/changes/<name>` or `openspec/changes/archive/<date>-<name>` in
/// any text: the change names it points at, archive date stripped.
fn change_names_in(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for (at, _) in text.match_indices("openspec/changes/") {
        let rest = &text[at + "openspec/changes/".len()..];
        let rest = rest.strip_prefix("archive/").unwrap_or(rest);
        let name: String = rest
            .chars()
            .take_while(|character| {
                character.is_ascii_alphanumeric() || *character == '-' || *character == '_'
            })
            .collect();
        let name = strip_archive_date(&name).to_owned();
        if !name.is_empty() && name != "archive" && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// `2026-09-30-worker-wait-and-notice` → `worker-wait-and-notice`.
fn strip_archive_date(name: &str) -> &str {
    let bytes = name.as_bytes();
    let dated = bytes.len() > 11
        && bytes[..10].iter().enumerate().all(|(ix, byte)| {
            if ix == 4 || ix == 7 {
                *byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
        && bytes[10] == b'-';
    if dated { &name[11..] } else { name }
}

/// `WT-20260929-memoria-wiki-retomada-1` → `memoria-wiki`.
fn ticket_slug(id: &str) -> &str {
    let slug = id
        .strip_prefix("WT-")
        .and_then(|rest| {
            rest.get(9..)
                .filter(|_| rest.as_bytes().get(8) == Some(&b'-'))
        })
        .unwrap_or(id);
    match slug.rsplit_once("-retomada-") {
        Some((base, suffix)) if suffix.chars().all(|c| c.is_ascii_digit()) => base,
        _ => slug,
    }
}

fn git(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The main checkout of `cwd`'s repository (a worktree's shared root).
fn repo_root(cwd: &Path) -> PathBuf {
    git(
        cwd,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .map(|dir| PathBuf::from(dir.trim()))
    .and_then(|dir| dir.parent().map(Path::to_path_buf))
    .unwrap_or_else(|| cwd.to_path_buf())
}

fn read_change(name: &str, path: PathBuf, archived: bool, sources: Vec<SpecSource>) -> SpecLink {
    let tasks = std::fs::read_to_string(path.join("tasks.md")).unwrap_or_default();
    let tasks_done = tasks
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            line.starts_with("- [x]") || line.starts_with("- [X]")
        })
        .count();
    let tasks_total = tasks_done
        + tasks
            .lines()
            .filter(|line| line.trim_start().starts_with("- [ ]"))
            .count();
    let proposal = std::fs::read_to_string(path.join("proposal.md")).unwrap_or_default();
    let why = proposal
        .split("\n## ")
        .find(|section| section.starts_with("Why"))
        .map(|section| {
            section
                .lines()
                .skip(1)
                .skip_while(|line| line.trim().is_empty())
                .take_while(|line| !line.trim().is_empty())
                .map(str::trim)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|why| !why.is_empty());
    let mut capabilities: Vec<String> = std::fs::read_dir(path.join("specs"))
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    capabilities.sort();
    SpecLink {
        change: name.to_owned(),
        archived,
        path,
        why,
        tasks_done,
        tasks_total,
        capabilities,
        sources,
    }
}

/// Every markdown file of one change, concatenated: what a citation of a
/// ticket id is searched in.
fn change_text(dir: &Path, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            change_text(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            out.push_str(&text);
            out.push('\n');
        }
    }
}

struct ChangeDoc {
    name: String,
    path: PathBuf,
    archived: bool,
    text: String,
}

/// Per-load cache: each checkout's repo root and each OpenSpec tree is read
/// once, however many tickets point at it.
#[derive(Default)]
struct RepoIndex {
    roots: std::collections::HashMap<PathBuf, PathBuf>,
    changes: std::collections::HashMap<PathBuf, Vec<ChangeDoc>>,
}

impl RepoIndex {
    fn root(&mut self, cwd: &Path) -> PathBuf {
        self.roots
            .entry(cwd.to_path_buf())
            .or_insert_with(|| repo_root(cwd))
            .clone()
    }

    fn changes(&mut self, base: &Path) -> &[ChangeDoc] {
        self.changes.entry(base.to_path_buf()).or_insert_with(|| {
            let changes = base.join("openspec").join("changes");
            let mut dirs: Vec<(PathBuf, bool)> = std::fs::read_dir(&changes)
                .map(|entries| {
                    entries
                        .flatten()
                        .map(|entry| entry.path())
                        .filter(|path| {
                            path.is_dir() && path.file_name().is_some_and(|name| name != "archive")
                        })
                        .map(|path| (path, false))
                        .collect()
                })
                .unwrap_or_default();
            dirs.extend(
                std::fs::read_dir(changes.join("archive"))
                    .map(|entries| {
                        entries
                            .flatten()
                            .map(|entry| entry.path())
                            .filter(|path| path.is_dir())
                            .map(|path| (path, true))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            );
            dirs.into_iter()
                .filter_map(|(path, archived)| {
                    let name = strip_archive_date(path.file_name()?.to_str()?).to_owned();
                    let mut text = String::new();
                    change_text(&path, &mut text);
                    Some(ChangeDoc {
                        name,
                        path,
                        archived,
                        text,
                    })
                })
                .collect()
        })
    }

    /// Every change of the ticket's repo tied to it by evidence on disk: its
    /// delivered commits, its Worker brief, its own text, a change citing its
    /// id, or a change named after its slug. Nothing is guessed beyond that.
    fn link_specs(&mut self, ticket: &Ticket, briefs: &Path) -> Vec<SpecLink> {
        let cwd = Path::new(&ticket.cwd);
        if ticket.cwd.is_empty() || !cwd.is_dir() {
            return Vec::new();
        }
        let root = self.root(cwd);
        let mut found: Vec<(String, SpecSource)> = Vec::new();
        if let Some((base, head)) = &ticket.record_range
            && base != head
            && let Some(files) = git(cwd, &["diff", "--name-only", base, head, "--", "openspec"])
        {
            found.extend(
                change_names_in(&files)
                    .into_iter()
                    .map(|name| (name, SpecSource::Commits)),
            );
        }
        if let Ok(brief) = std::fs::read_to_string(briefs.join(format!("{}.md", ticket.id))) {
            let refs = brief
                .lines()
                .filter(|line| line.starts_with("refs:"))
                .collect::<Vec<_>>()
                .join("\n");
            found.extend(
                change_names_in(&refs)
                    .into_iter()
                    .map(|name| (name, SpecSource::Brief)),
            );
        }
        let own_text = std::fs::read_to_string(&ticket.path).unwrap_or_default();
        found.extend(
            change_names_in(&own_text)
                .into_iter()
                .map(|name| (name, SpecSource::Ticket)),
        );
        found.push((ticket_slug(&ticket.id).to_owned(), SpecSource::Name));
        for base in [cwd, root.as_path()] {
            for doc in self.changes(base) {
                if doc.text.contains(&ticket.id) {
                    found.push((doc.name.clone(), SpecSource::Citation));
                }
            }
        }
        let mut links: Vec<SpecLink> = Vec::new();
        for (name, source) in found {
            if let Some(link) = links.iter_mut().find(|link| link.change == name) {
                if !link.sources.contains(&source) {
                    link.sources.push(source);
                }
                continue;
            }
            let located = [cwd, root.as_path()].into_iter().find_map(|base| {
                self.changes(base)
                    .iter()
                    .find(|doc| doc.name == name)
                    .map(|doc| (doc.path.clone(), doc.archived))
            });
            if let Some((path, archived)) = located {
                links.push(read_change(&name, path, archived, vec![source]));
            }
        }
        for link in &mut links {
            link.sources.sort();
        }
        links.sort_by(|left, right| {
            left.sources
                .cmp(&right.sources)
                .then_with(|| left.change.cmp(&right.change))
        });
        links
    }
}

fn within(path: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    !root.is_empty() && (path == root || path.starts_with(&format!("{root}/")))
}

/// `JK Distribuição` and the harness folder `jk-distribuicao` name the same
/// project: lowercase, common accents folded, separators as `-`.
fn project_key(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .chars()
        .map(|character| match character {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            ' ' | '_' => '-',
            other => other,
        })
        .collect()
}

/// Whether the ticket ran in the project: its `cwd` is the project folder or
/// one of its checkouts (principal or worktree), or lies inside one.
fn ran_in(entry: &ProjectEntry, ticket: &Ticket) -> bool {
    within(&ticket.cwd, &entry.space.path)
        || entry
            .group
            .checkouts
            .iter()
            .any(|checkout| within(&ticket.cwd, &checkout.path))
}

/// The project's tickets, newest first: those whose `cwd` is one of its
/// checkouts (principal or worktree). A ticket whose `cwd` is in none of the
/// registered `projects` falls back to its harness folder: it belongs to the
/// project of the same name. A ticket filed in project A's folder but run in
/// project B belongs only to B.
pub fn tickets_for_project<'a>(
    entry: &ProjectEntry,
    projects: &[ProjectEntry],
    tickets: &'a [Ticket],
) -> Vec<&'a Ticket> {
    let name = project_key(&entry.space.name);
    let mut rows: Vec<&Ticket> = tickets
        .iter()
        .filter(|ticket| {
            ran_in(entry, ticket)
                || (project_key(&ticket.brain_project) == name
                    && !projects.iter().any(|project| ran_in(project, ticket)))
        })
        .collect();
    rows.sort_by(|left, right| {
        right
            .created
            .cmp(&left.created)
            .then_with(|| left.id.cmp(&right.id))
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICKET: &str = r#"---
id: WT-20260929-memoria-wiki
title: Memoria wiki nucleo e backend
status: closed-out
created: "2026-09-29T22:42:54"
cwd: /p/denchclaw-crm
gate: "test -s x && grep -q \"^## A\" x"
workers:
  - "dench-memoria"
  - "dench-memoria-r2"
proven:
  - "gate verde"
not_proven:
  - "boot real não rodado"
next:
---

# Memoria wiki nucleo e backend

## Gate
"#;

    #[test]
    fn frontmatter_scalars_lists_and_body_are_read() {
        let ticket = parse_ticket(TICKET, "denchclaw-crm", PathBuf::from("/t.md")).unwrap();
        assert_eq!(ticket.id, "WT-20260929-memoria-wiki");
        assert_eq!(ticket.title, "Memoria wiki nucleo e backend");
        assert_eq!(ticket.status, "closed-out");
        assert_eq!(ticket.created, "2026-09-29T22:42:54");
        assert_eq!(ticket.cwd, "/p/denchclaw-crm");
        assert_eq!(ticket.workers, vec!["dench-memoria", "dench-memoria-r2"]);
        assert_eq!(ticket.proven, vec!["gate verde"]);
        assert_eq!(ticket.not_proven, vec!["boot real não rodado"]);
        assert_eq!(ticket.next, None);
        assert!(ticket.body.starts_with("# Memoria wiki nucleo e backend"));
    }

    #[test]
    fn a_file_without_frontmatter_or_id_is_not_a_ticket() {
        assert!(parse_ticket("# notes\n", "x", PathBuf::from("/a.md")).is_none());
        assert!(parse_ticket("---\ntitle: x\n---\nbody\n", "x", PathBuf::from("/b.md")).is_none());
    }

    #[test]
    fn tickets_load_from_every_harness_project_and_skip_other_files() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path().join("brain-source").join("projects");
        let tickets = root.join("denchclaw-crm").join("tickets");
        std::fs::create_dir_all(&tickets).unwrap();
        std::fs::write(tickets.join("WT-1.md"), TICKET).unwrap();
        std::fs::write(tickets.join("notes.txt"), TICKET).unwrap();
        std::fs::write(root.join("denchclaw-crm").join("brain.md"), "# brain").unwrap();
        let loaded = load_tickets(workspace.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].brain_project, "denchclaw-crm");
        let missing = load_tickets(&workspace.path().join("missing")).unwrap_err();
        assert!(missing.contains("brain-source/projects"), "{missing}");
    }

    #[test]
    fn record_range_and_proof_fields_are_read() {
        let text = TICKET.replace(
            "next:\n",
            "next:\nresolved:\n  - \"boot => /r.md\"\naxes:\n  - \"code:gate=/g.json\"\nrecord: \"v|aaa..bbb|fp|x\"\n",
        );
        let ticket = parse_ticket(&text, "p", PathBuf::from("/t.md")).unwrap();
        assert_eq!(ticket.record_range, Some(("aaa".into(), "bbb".into())));
        assert_eq!(ticket.resolved, vec!["boot => /r.md"]);
        assert_eq!(ticket.axes, vec!["code:gate=/g.json"]);
        assert!(ticket.gate.starts_with("test -s x"));
    }

    #[test]
    fn resolved_claims_and_creation_time_are_read() {
        let mut ticket = parse_ticket(TICKET, "p", PathBuf::from("/t.md")).unwrap();
        ticket.resolved = vec!["boot real não rodado => /r.md".into()];
        assert!(ticket.claim_resolved("boot real não rodado"));
        assert!(!ticket.claim_resolved("boot"));
        assert!(ticket.created_at().is_some());
        ticket.created = "ontem".into();
        assert!(ticket.created_at().is_none());
    }

    #[test]
    fn change_paths_are_found_in_any_text_with_archive_dates_stripped() {
        let text = "refs: /r/openspec/changes/memoria-wiki/proposal.md, \
                    /r/openspec/changes/archive/2026-09-30-worker-wait/tasks.md \
                    openspec/changes/memoria-wiki/design.md";
        assert_eq!(change_names_in(text), vec!["memoria-wiki", "worker-wait"]);
        assert_eq!(strip_archive_date("2026-09-30-x"), "x");
        assert_eq!(strip_archive_date("plain-name"), "plain-name");
    }

    #[test]
    fn ticket_slugs_drop_the_date_and_retry_suffix() {
        assert_eq!(ticket_slug("WT-20260929-memoria-wiki"), "memoria-wiki");
        assert_eq!(
            ticket_slug("WT-20260929-memoria-wiki-retomada-1"),
            "memoria-wiki"
        );
        assert_eq!(ticket_slug("WT-20260913-001"), "001");
    }

    #[test]
    fn specs_link_by_brief_citation_and_name_and_read_their_progress() {
        let repo = tempfile::tempdir().unwrap();
        let changes = repo.path().join("openspec").join("changes");
        let active = changes.join("memoria-wiki");
        std::fs::create_dir_all(active.join("specs").join("memoria")).unwrap();
        std::fs::write(
            active.join("proposal.md"),
            "# Change\n\n## Why\n\nO time perde contexto\nentre sessões.\n\n## What Changes\n",
        )
        .unwrap();
        std::fs::write(
            active.join("tasks.md"),
            "- [x] 1.1 a\n- [ ] 1.2 b\n- [x] 1.3 c\n",
        )
        .unwrap();
        let cited = changes.join("archive").join("2026-09-01-outra");
        std::fs::create_dir_all(&cited).unwrap();
        std::fs::write(
            cited.join("tasks.md"),
            "Evidence: .tmp/verify/WT-20260929-x/run-1\n",
        )
        .unwrap();
        std::fs::create_dir_all(changes.join("sem-ligacao")).unwrap();
        let named = changes.join("x");
        std::fs::create_dir_all(&named).unwrap();

        let briefs = tempfile::tempdir().unwrap();
        std::fs::write(
            briefs.path().join("WT-20260929-x.md"),
            format!(
                "ticket: WT-20260929-x\nrefs: {}/proposal.md\n",
                active.display()
            ),
        )
        .unwrap();
        let mut ticket = parse_ticket(TICKET, "p", repo.path().join("t.md")).unwrap();
        ticket.id = "WT-20260929-x".into();
        ticket.cwd = repo.path().to_string_lossy().into_owned();

        let links = RepoIndex::default().link_specs(&ticket, briefs.path());
        let names: Vec<&str> = links.iter().map(|link| link.change.as_str()).collect();
        assert_eq!(names, vec!["memoria-wiki", "outra", "x"]);
        let memoria = &links[0];
        assert_eq!(memoria.sources, vec![SpecSource::Brief]);
        assert!(!memoria.archived);
        assert_eq!((memoria.tasks_done, memoria.tasks_total), (2, 3));
        assert_eq!(memoria.capabilities, vec!["memoria"]);
        assert_eq!(
            memoria.why.as_deref(),
            Some("O time perde contexto entre sessões.")
        );
        assert_eq!(links[1].sources, vec![SpecSource::Citation]);
        assert!(links[1].archived);
        assert_eq!(links[2].sources, vec![SpecSource::Name]);
    }

    #[test]
    fn harness_folder_names_match_display_names() {
        assert_eq!(
            project_key("JK Distribuição"),
            project_key("jk-distribuicao")
        );
        assert_eq!(project_key("denchclaw-crm"), "denchclaw-crm");
        assert_ne!(project_key("jk-erp-hub"), project_key("jk-distribuicao"));
    }

    #[test]
    fn checkout_paths_only_match_on_a_path_boundary() {
        assert!(within("/p/crm", "/p/crm"));
        assert!(within("/p/crm/sub", "/p/crm/"));
        assert!(!within("/p/crm-old", "/p/crm"));
        assert!(!within("/p/crm", ""));
    }
}
