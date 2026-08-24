use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Subcommand;
use zeron_workers_unpeel::LocalWorkersClient;
use zeron_workers_unpeel::resources::{ResourceSupport, WorkersResourceSnapshot};

#[derive(Debug, Subcommand)]
pub(crate) enum WorkersCommand {
    /// Show CPU, memory, and process totals for hosted worker sessions.
    Top {
        /// Emit the typed resource snapshot as JSON.
        #[arg(long)]
        json: bool,
        /// Include bounded per-process rows.
        #[arg(long)]
        processes: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkersTopIdentity {
    session_id: String,
    title: String,
    project: String,
}

pub(crate) fn run(command: WorkersCommand) -> anyhow::Result<()> {
    match command {
        WorkersCommand::Top { json, processes } => run_top(json, processes),
    }
}

fn run_top(json: bool, include_processes: bool) -> anyhow::Result<()> {
    let client = LocalWorkersClient::new();
    let warmup = client
        .resource_snapshot(include_processes)
        .map_err(anyhow::Error::new)?;
    let snapshot = if warmup.support == ResourceSupport::Supported && !warmup.sessions.is_empty() {
        std::thread::sleep(std::time::Duration::from_secs(1));
        client
            .resource_snapshot(include_processes)
            .map_err(anyhow::Error::new)?
    } else {
        warmup
    };
    if json {
        println!("{}", render_json(&snapshot)?);
        return Ok(());
    }
    let identities = client
        .bootstrap()
        .map(identities_from_bootstrap)
        .unwrap_or_default();
    print!(
        "{}",
        render_human(&snapshot, &identities, include_processes)
    );
    Ok(())
}

fn identities_from_bootstrap(
    bootstrap: zeron_workers_unpeel::WorkersBootstrap,
) -> Vec<WorkersTopIdentity> {
    let projects = bootstrap
        .projects
        .into_iter()
        .map(|project| (project.id, project.name))
        .collect::<HashMap<_, _>>();
    bootstrap
        .sessions
        .into_iter()
        .map(|session| WorkersTopIdentity {
            project: projects
                .get(&session.project_id)
                .cloned()
                .unwrap_or(session.project_id),
            session_id: session.id,
            title: session.title,
        })
        .collect()
}

fn render_json(snapshot: &WorkersResourceSnapshot) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(snapshot)
}

fn render_human(
    snapshot: &WorkersResourceSnapshot,
    identities: &[WorkersTopIdentity],
    include_processes: bool,
) -> String {
    let now_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    render_human_at(snapshot, identities, include_processes, now_unix_ms)
}

fn render_human_at(
    snapshot: &WorkersResourceSnapshot,
    identities: &[WorkersTopIdentity],
    include_processes: bool,
    now_unix_ms: u64,
) -> String {
    if snapshot.support == ResourceSupport::Unsupported {
        return format!(
            "Workers resources unavailable: {}\n",
            snapshot.error.as_deref().unwrap_or("unsupported platform")
        );
    }
    let identities = identities
        .iter()
        .map(|identity| (identity.session_id.as_str(), identity))
        .collect::<HashMap<_, _>>();
    let sample_age_seconds = now_unix_ms.saturating_sub(snapshot.sampled_at_unix_ms) / 1_000;
    let mut output = format!(
        "Sample age: {sample_age_seconds}s\nCPU      MEMORY    PROC  PROJECT              SESSION\n"
    );
    for session in &snapshot.sessions {
        let identity = identities.get(session.session_id.as_str()).copied();
        let project = identity.map(|value| value.project.as_str()).unwrap_or("-");
        let title = identity
            .map(|value| value.title.as_str())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(session.session_id.as_str());
        let completeness = if session.attribution_complete {
            ""
        } else {
            " ~"
        };
        output.push_str(&format!(
            "{:>6.1}%  {:>9}  {:>4}  {:<20} {}{}\n",
            session.cpu_percent,
            format_bytes(session.physical_footprint_bytes),
            session.process_count,
            truncate(project, 20),
            truncate(title, 72),
            completeness,
        ));
        if include_processes {
            for process in &session.top_processes {
                output.push_str(&format!(
                    "          {:>9}        pid {:<6} {:>6.1}% {}\n",
                    format_bytes(process.physical_footprint_bytes),
                    process.pid,
                    process.cpu_percent,
                    process.name,
                ));
            }
        }
    }
    if let Some(error) = snapshot.error.as_deref() {
        output.push_str(&format!("\nLast sampling error: {error}\n"));
    }
    output
}

fn format_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GiB", bytes as f64 / GIB)
    } else {
        format!("{:.1} MiB", bytes as f64 / MIB)
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }
    value
        .chars()
        .take(max_chars.saturating_sub(1))
        .chain(['…'])
        .collect()
}

#[cfg(test)]
mod tests {
    use zeron_workers_unpeel::resources::{
        ResourceSupport, WorkersMemorySource, WorkersProcessResource, WorkersResourceSnapshot,
        WorkersSessionResource,
    };

    use super::*;

    fn snapshot() -> WorkersResourceSnapshot {
        WorkersResourceSnapshot {
            support: ResourceSupport::Supported,
            sampled_at_unix_ms: 10_000,
            error: None,
            sessions: vec![WorkersSessionResource {
                session_id: "session-a".into(),
                sampled_at_unix_ms: 10_000,
                root_pid: Some(42),
                root_pid_started_at: Some(1_000),
                cpu_percent: 12.5,
                physical_footprint_bytes: 2 * 1024 * 1024 * 1024,
                resident_bytes: 1024 * 1024 * 1024,
                process_count: 3,
                attribution_complete: true,
                top_processes: vec![WorkersProcessResource {
                    pid: 42,
                    parent_pid: 1,
                    name: "omp".into(),
                    cpu_percent: 10.0,
                    physical_footprint_bytes: 2 * 1024 * 1024 * 1024,
                    resident_bytes: 1024 * 1024 * 1024,
                    memory_source: WorkersMemorySource::PhysicalFootprint,
                }],
            }],
        }
    }

    #[test]
    fn human_output_is_compact_and_contains_no_process_details_by_default() {
        let output = render_human(
            &snapshot(),
            &[WorkersTopIdentity {
                session_id: "session-a".into(),
                title: "Investigate build".into(),
                project: "comet".into(),
            }],
            false,
        );

        assert!(output.contains("Investigate build"));
        assert!(output.contains("comet"));
        assert!(output.contains("12.5%"));
        assert!(output.contains("2.00 GiB"));
        assert!(output.contains("3"));
        assert!(output.contains("Sample age:"));
        assert!(!output.contains("pid 42"));
    }

    #[test]
    fn process_flag_adds_bounded_process_rows() {
        let output = render_human(&snapshot(), &[], true);

        assert!(output.contains("omp"));
        assert!(output.contains("pid 42"));
    }

    #[test]
    fn json_output_preserves_typed_snapshot() {
        let output = render_json(&snapshot()).expect("serialize snapshot");
        let decoded: WorkersResourceSnapshot =
            serde_json::from_str(&output).expect("decode rendered snapshot");

        assert_eq!(decoded, snapshot());
    }

    #[test]
    fn unsupported_snapshot_is_reported_explicitly() {
        let output = render_human(
            &WorkersResourceSnapshot {
                support: ResourceSupport::Unsupported,
                sampled_at_unix_ms: 10_000,
                sessions: Vec::new(),
                error: Some("unsupported".into()),
            },
            &[],
            false,
        );

        assert!(output.contains("unsupported"));
        assert!(!output.contains("0.00 GiB"));
    }
}
