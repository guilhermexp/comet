use std::path::Path;
use unpeel_core::runtime_catalog::{RuntimeCatalog, RuntimeDescriptor};
use unpeel_core::session_host::HostedSessionManifest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdmissionDecision {
    Accepted,
    RejectedMismatch,
    RejectedNested,
    RejectedStaleGeneration,
}

impl AdmissionDecision {
    pub(crate) fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    pub(crate) fn is_ignored(&self) -> bool {
        !self.is_accepted()
    }
}

fn first_json_string<'a>(json: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        json.get(*key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

fn resolve_runtime_descriptor<'a>(
    catalog: &'a RuntimeCatalog,
    raw: &str,
) -> Option<&'a RuntimeDescriptor> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    catalog
        .by_legacy_slug(raw)
        .or_else(|| catalog.by_slug(raw))
        .or_else(|| catalog.by_id(raw))
        .or_else(|| catalog.by_command_alias_for_current_platform(raw))
        .or_else(|| catalog.by_executable_alias(raw))
}

fn resolve_manifest_runtime<'a>(
    catalog: &'a RuntimeCatalog,
    manifest: &HostedSessionManifest,
) -> Option<&'a RuntimeDescriptor> {
    if let Some(active_id) = manifest
        .runtime
        .as_ref()
        .and_then(|r| r.current_observation.as_ref())
        .map(|obs| obs.runtime_id.trim())
        .filter(|s| !s.is_empty())
    {
        if let Some(descriptor) = resolve_runtime_descriptor(catalog, active_id) {
            return Some(descriptor);
        }
    }
    let command_head = unpeel_core::integrations::command_head(&manifest.session.command);
    resolve_runtime_descriptor(catalog, command_head)
}

pub(crate) fn check_hook_admission(
    manifest: Option<&HostedSessionManifest>,
    payload: &serde_json::Value,
) -> AdmissionDecision {
    // 1. Source runtime check
    let reported_runtime_id = first_json_string(
        payload,
        &[
            "unpeel_runtime_id",
            "unpeelRuntimeId",
            "runtime_id",
            "runtimeId",
        ],
    );

    let Some(manifest) = manifest else {
        // Without an owned manifest, we cannot verify any reported runtime identity.
        if reported_runtime_id.is_some() {
            return AdmissionDecision::RejectedMismatch;
        }
        // Explicit sub-kind must be rejected even if legacy.
        if first_json_string(payload, &["unpeel_agent_kind", "agent_kind", "agentKind"])
            == Some("sub")
        {
            return AdmissionDecision::RejectedNested;
        }
        // Preserve unidentified legacy primary seed handling per existing spec.
        return AdmissionDecision::Accepted;
    };

    let catalog = unpeel_core::runtime_catalog::builtin_runtime_catalog();

    if let Some(reported_str) = reported_runtime_id {
        let mismatched = resolve_manifest_runtime(&catalog, manifest)
            .zip(resolve_runtime_descriptor(&catalog, reported_str))
            .is_some_and(|(expected, reported)| expected.id != reported.id);

        if mismatched {
            return AdmissionDecision::RejectedMismatch;
        }
    }

    // 2. Nested role / transcript check
    match first_json_string(payload, &["unpeel_agent_kind", "agent_kind", "agentKind"]) {
        Some("sub") => return AdmissionDecision::RejectedNested,
        Some("main") => {}
        _ => {
            if first_json_string(
                payload,
                &[
                    "transcript_path",
                    "transcriptPath",
                    "provider_transcript_path",
                    "providerTranscriptPath",
                ],
            )
            .is_some_and(|path| {
                unpeel_core::integrations::is_nested_provider_transcript(manifest, Path::new(path))
            }) {
                return AdmissionDecision::RejectedNested;
            }
        }
    }

    // 3. Generation check
    let runtime_generation = payload
        .get("unpeel_runtime_generation")
        .or_else(|| payload.get("unpeelRuntimeGeneration"))
        .and_then(serde_json::Value::as_u64);

    if runtime_generation.is_some_and(|generation| generation < manifest.runtime_launch_generation)
    {
        return AdmissionDecision::RejectedStaleGeneration;
    }

    AdmissionDecision::Accepted
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use unpeel_core::runtime_observer::ActiveRuntimeObservation;
    use unpeel_core::session_host::{
        HostedSessionManifest, HostedSessionRuntime, HostedSessionState,
    };
    use unpeel_core::state::SessionInfo;

    fn sample_manifest(
        command: &str,
        active_runtime: Option<&str>,
        generation: u64,
    ) -> HostedSessionManifest {
        HostedSessionManifest {
            session: SessionInfo {
                id: "worker-admission-test".into(),
                project_id: "project-1".into(),
                label: "OMP".into(),
                custom_title: false,
                command: command.into(),
                created_at: 1,
                tag_id: None,
                worktree_path: None,
                worktree_branch: None,
                parent_session_id: None,
                spawned_by: None,
                role: None,
                task: None,
            },
            cwd: "/tmp".into(),
            state: HostedSessionState::Running,
            pid: None,
            pid_started_at: None,
            exit_code: None,
            host_build_id: None,
            host_protocol_version: None,
            has_been_written_to: true,
            provider_session_id: None,
            provider_transcript_path: None,
            managed_storage_path: None,
            resume_failure_markers: Vec::new(),
            runtime: active_runtime.map(|id| HostedSessionRuntime {
                current_observation: Some(ActiveRuntimeObservation {
                    runtime_id: id.into(),
                    pid: 1234,
                    pid_started_at: Some(1),
                    process_group_id: 1234,
                    process_name: id.into(),
                    argv: None,
                }),
            }),
            runtime_launch_generation: generation,
            runtime_launch_pending: false,
            runtime_launched_at: Some(1),
            runtime_launch_output_offset: 0,
            mcp_enabled: None,
            browser_mcp_enabled: None,
            computer_mcp_enabled: None,
            mcp_client_registered: false,
            browser_client_registered: false,
            computer_client_registered: false,
            menu_prompt_active: false,
            turn_settled_by_interrupt: false,
            screen_changed_at: None,
            detected_local_urls: Vec::new(),
            heartbeat_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn runtime_mismatch_rejected_even_if_main_agent_kind_declared() {
        let manifest = sample_manifest("omp --session-dir /tmp/sessions", None, 1);
        let payload = json!({
            "hook_event_name": "PermissionRequest",
            "unpeel_runtime_id": "claude",
            "unpeel_agent_kind": "main",
            "unpeel_runtime_generation": 1
        });
        assert_eq!(
            check_hook_admission(Some(&manifest), &payload),
            AdmissionDecision::RejectedMismatch
        );
    }

    #[test]
    fn matching_runtime_accepted_for_main() {
        let manifest = sample_manifest("omp --session-dir /tmp/sessions", None, 1);
        let payload = json!({
            "hook_event_name": "Start",
            "unpeel_runtime_id": "omp",
            "unpeel_agent_kind": "main",
            "unpeel_runtime_generation": 1
        });
        assert_eq!(
            check_hook_admission(Some(&manifest), &payload),
            AdmissionDecision::Accepted
        );
    }

    #[test]
    fn active_runtime_observation_overrides_launch_command() {
        let manifest = sample_manifest("bash", Some("claude"), 1);
        let claude_payload = json!({
            "hook_event_name": "PermissionRequest",
            "unpeel_runtime_id": "claude",
            "unpeel_agent_kind": "main",
            "unpeel_runtime_generation": 1
        });
        assert_eq!(
            check_hook_admission(Some(&manifest), &claude_payload),
            AdmissionDecision::Accepted
        );

        let omp_payload = json!({
            "hook_event_name": "Start",
            "unpeel_runtime_id": "omp",
            "unpeel_agent_kind": "main",
            "unpeel_runtime_generation": 1
        });
        assert_eq!(
            check_hook_admission(Some(&manifest), &omp_payload),
            AdmissionDecision::RejectedMismatch
        );
    }

    #[test]
    fn unresolved_manifest_runtime_falls_through_to_role_and_generation() {
        for command in ["env FOO=1 claude", "npx some-wrapper", "./run-agent.sh"] {
            let manifest = sample_manifest(command, None, 2);
            let main_payload = json!({
                "hook_event_name": "PermissionRequest",
                "unpeel_runtime_id": "claude",
                "unpeel_agent_kind": "main",
                "unpeel_runtime_generation": 2
            });
            assert_eq!(
                check_hook_admission(Some(&manifest), &main_payload),
                AdmissionDecision::Accepted,
                "{command}"
            );

            let sub_payload = json!({
                "hook_event_name": "Stop",
                "unpeel_runtime_id": "claude",
                "unpeel_agent_kind": "sub",
                "unpeel_runtime_generation": 2
            });
            assert_eq!(
                check_hook_admission(Some(&manifest), &sub_payload),
                AdmissionDecision::RejectedNested,
                "{command}"
            );

            let stale_payload = json!({
                "hook_event_name": "Stop",
                "unpeel_runtime_id": "claude",
                "unpeel_agent_kind": "main",
                "unpeel_runtime_generation": 1
            });
            assert_eq!(
                check_hook_admission(Some(&manifest), &stale_payload),
                AdmissionDecision::RejectedStaleGeneration,
                "{command}"
            );
        }
    }

    #[test]
    fn nested_subagent_rejected_even_with_matching_runtime() {
        let manifest = sample_manifest("omp", None, 1);
        let payload = json!({
            "hook_event_name": "Stop",
            "unpeel_runtime_id": "omp",
            "unpeel_agent_kind": "sub",
            "unpeel_runtime_generation": 1
        });
        assert_eq!(
            check_hook_admission(Some(&manifest), &payload),
            AdmissionDecision::RejectedNested
        );
    }

    #[test]
    fn legacy_missing_runtime_preserves_main_and_rejects_sub() {
        let manifest = sample_manifest("omp", None, 1);
        let legacy_main = json!({
            "hook_event_name": "Start",
            "unpeel_agent_kind": "main",
            "unpeel_runtime_generation": 1
        });
        assert_eq!(
            check_hook_admission(Some(&manifest), &legacy_main),
            AdmissionDecision::Accepted
        );

        let legacy_sub = json!({
            "hook_event_name": "Stop",
            "unpeel_agent_kind": "sub",
            "unpeel_runtime_generation": 1
        });
        assert_eq!(
            check_hook_admission(Some(&manifest), &legacy_sub),
            AdmissionDecision::RejectedNested
        );
    }

    #[test]
    fn stale_generation_rejected() {
        let manifest = sample_manifest("omp", None, 3);
        let stale_payload = json!({
            "hook_event_name": "Start",
            "unpeel_runtime_id": "omp",
            "unpeel_agent_kind": "main",
            "unpeel_runtime_generation": 2
        });
        assert_eq!(
            check_hook_admission(Some(&manifest), &stale_payload),
            AdmissionDecision::RejectedStaleGeneration
        );

        let current_payload = json!({
            "hook_event_name": "Start",
            "unpeel_runtime_id": "omp",
            "unpeel_agent_kind": "main",
            "unpeel_runtime_generation": 3
        });
        assert_eq!(
            check_hook_admission(Some(&manifest), &current_payload),
            AdmissionDecision::Accepted
        );
    }

    #[test]
    fn manifest_none_rejects_stamped_runtime_id() {
        let stamped_omp = json!({
            "hook_event_name": "Start",
            "unpeel_runtime_id": "omp",
            "unpeel_agent_kind": "main",
            "unpeel_runtime_generation": 1
        });
        assert_eq!(
            check_hook_admission(None, &stamped_omp),
            AdmissionDecision::RejectedMismatch
        );

        let stamped_claude = json!({
            "hook_event_name": "PermissionRequest",
            "unpeel_runtime_id": "claude",
            "unpeel_agent_kind": "main",
            "unpeel_runtime_generation": 1
        });
        assert_eq!(
            check_hook_admission(None, &stamped_claude),
            AdmissionDecision::RejectedMismatch
        );
    }

    #[test]
    fn manifest_none_rejects_sub_kind_even_if_unmarked() {
        let unmarked_sub = json!({
            "hook_event_name": "Stop",
            "unpeel_agent_kind": "sub",
            "unpeel_runtime_generation": 1
        });
        assert_eq!(
            check_hook_admission(None, &unmarked_sub),
            AdmissionDecision::RejectedNested
        );
    }

    #[test]
    fn manifest_none_preserves_unidentified_legacy_primary() {
        let unmarked_main = json!({
            "hook_event_name": "Stop",
            "unpeel_runtime_generation": 1
        });
        assert_eq!(
            check_hook_admission(None, &unmarked_main),
            AdmissionDecision::Accepted
        );

        let unmarked_explicit_main = json!({
            "hook_event_name": "Start",
            "unpeel_agent_kind": "main",
            "unpeel_runtime_generation": 1
        });
        assert_eq!(
            check_hook_admission(None, &unmarked_explicit_main),
            AdmissionDecision::Accepted
        );
    }
}
