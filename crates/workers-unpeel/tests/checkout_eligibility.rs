mod support;
use std::{fs, process::Command};
use zeron_workers_unpeel::{CheckoutAvailability, LocalWorkersClient};

#[test]
fn recovery_and_unknown_identity_block_new_launches_and_live_ui_capabilities()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    // This integration binary has one test; no concurrent environment mutation.
    unsafe { std::env::set_var("UNPEEL_HOME", home.path()) };
    let repo = home.path().join("repository");
    fs::create_dir_all(&repo)?;
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["init", "-q"])
            .status()?
            .success()
    );
    let client = LocalWorkersClient::new();
    let id = client.add_project(&repo, &support::registry())?.checkout_id;
    let state_path = home.path().join("app-state.json");
    let ready: serde_json::Value = serde_json::from_slice(&fs::read(&state_path)?)?;
    for reason in [
        "conflict",
        "removalInterrupted",
        "removalPending",
        "futureVersion",
        "malformed",
        "malformed-entry",
        "invalid-archived",
        "invalid-availability",
    ] {
        let mut state = ready.clone();
        let identity = &mut state["comet_project_identity"];
        match reason {
            "futureVersion" => identity["version"] = 99.into(),
            "malformed" => identity["checkouts"] = "invalid".into(),
            "malformed-entry" => identity["checkouts"][0] = serde_json::Value::Null,
            "invalid-archived" => identity["checkouts"][0]["archived"] = "true".into(),
            "invalid-availability" => {
                identity["checkouts"][0]["availability"] = serde_json::Value::Null
            }
            "conflict" => identity["checkouts"][0]["conflict"] = "Repository replaced".into(),
            field => identity["checkouts"][0][field] = true.into(),
        }
        fs::write(&state_path, serde_json::to_vec(&state)?)?;
        let snapshot = client.bootstrap()?;
        let checkout = snapshot
            .projects
            .iter()
            .find(|project| project.id == id)
            .unwrap();
        assert_eq!(
            checkout.checkout_availability,
            Some(CheckoutAvailability::ProbeFailed),
            "{reason}"
        );
        assert!(checkout.change_request_branch().is_none(), "{reason}");
        assert!(
            client.create_session(&id, "true").is_err(),
            "{reason} must block new execution"
        );
        assert!(repo.exists());
    }
    Ok(())
}
