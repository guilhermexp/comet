use std::process::Command;

use tempfile::TempDir;
use zeron_workers_unpeel::{LocalWorkersClient, worker_parent_links_at};

#[test]
fn dev_demo_fixture_exposes_omp_usage_to_real_bootstrap() -> Result<(), Box<dyn std::error::Error>>
{
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/seed-demo-workers.py");

    let output = Command::new("python3")
        .arg(script)
        .args(["--home", home.path().to_str().expect("utf-8 home")])
        .args([
            "--project-path",
            project.path().to_str().expect("utf-8 project"),
        ])
        .args(["--parent-chat-id", "demo-chat"])
        .output()?;
    assert!(
        output.status.success(),
        "fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let state_path = home.path().join("app-state.json");
    let links = worker_parent_links_at(&state_path)?;
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].worker_session_id, "demo-omp-worker");
    assert_eq!(links[0].parent_chat_id, "demo-chat");

    let previous = std::env::var_os("UNPEEL_HOME");
    // SAFETY: this dedicated integration-test binary has one test and restores
    // the process-global override before returning.
    unsafe { std::env::set_var("UNPEEL_HOME", home.path()) };
    let bootstrap = LocalWorkersClient::new().bootstrap();
    unsafe {
        match previous {
            Some(value) => std::env::set_var("UNPEEL_HOME", value),
            None => std::env::remove_var("UNPEEL_HOME"),
        }
    }
    let bootstrap = bootstrap?;

    let worker = bootstrap
        .sessions
        .iter()
        .find(|session| session.id == "demo-omp-worker")
        .expect("seeded OMP Worker");
    assert_eq!(worker.provider_id.as_deref(), Some("omp"));
    assert_eq!(worker.total_tokens, Some(258_700));
    assert_eq!(worker.model_usage.len(), 2);
    assert_eq!(worker.model_usage[0].model, "openai-codex/gpt-5.6-sol:high");
    assert!(worker.model_usage[0].active);
    assert_eq!(worker.model_usage[1].total_tokens, 216_600);
    Ok(())
}
