use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui::{Context, Entity, Global, Task};
use zeron_workers_unpeel::resources::{WorkersResourceSnapshot, WorkersSessionResource};
use zeron_workers_unpeel::{LocalWorkersClient, WorkersResourceSettings};

use super::model::WorkersModel;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourceAlertLevel {
    #[default]
    Normal,
    Warning,
    Critical,
}

#[derive(Debug, Default)]
pub struct ResourceAlertReducer {
    level: ResourceAlertLevel,
}

impl ResourceAlertReducer {
    pub fn observe(
        &mut self,
        warning_gib: u16,
        critical_gib: u16,
        physical_footprint_bytes: u64,
        attribution_complete: bool,
    ) -> Option<ResourceAlertLevel> {
        const GIB: u64 = 1024 * 1024 * 1024;
        let warning = u64::from(warning_gib).saturating_mul(GIB);
        let critical = u64::from(critical_gib).saturating_mul(GIB);
        let clear_warning = warning.saturating_mul(80) / 100;
        let next = if attribution_complete && physical_footprint_bytes >= critical {
            ResourceAlertLevel::Critical
        } else if physical_footprint_bytes >= warning {
            ResourceAlertLevel::Warning
        } else if self.level >= ResourceAlertLevel::Warning
            && physical_footprint_bytes >= clear_warning
        {
            ResourceAlertLevel::Warning
        } else {
            ResourceAlertLevel::Normal
        };
        let upward = (next > self.level).then_some(next);
        self.level = next;
        upward
    }

    pub fn level(&self) -> ResourceAlertLevel {
        self.level
    }
}

pub struct WorkersResourceMonitor {
    client: LocalWorkersClient,
    model: Entity<WorkersModel>,
    snapshot: Option<WorkersResourceSnapshot>,
    settings: WorkersResourceSettings,
    reducers: HashMap<String, ResourceAlertReducer>,
    sampling: bool,
    details_requested: bool,
    generation: u64,
    last_error: Option<String>,
    _poll_task: Task<()>,
}

impl WorkersResourceMonitor {
    pub fn new(model: Entity<WorkersModel>, cx: &mut Context<Self>) -> Self {
        let poll_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SAMPLE_INTERVAL).await;
                if this.update(cx, |monitor, cx| monitor.refresh(cx)).is_err() {
                    break;
                }
            }
        });
        let mut monitor = Self {
            client: LocalWorkersClient::new(),
            model,
            snapshot: None,
            settings: WorkersResourceSettings::default(),
            reducers: HashMap::new(),
            sampling: false,
            details_requested: false,
            generation: 0,
            last_error: None,
            _poll_task: poll_task,
        };
        monitor.refresh(cx);
        monitor
    }

    pub fn snapshot(&self) -> Option<&WorkersResourceSnapshot> {
        self.snapshot.as_ref()
    }

    pub fn settings(&self) -> &WorkersResourceSettings {
        &self.settings
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_sampling(&self) -> bool {
        self.sampling
    }

    pub fn set_details_requested(&mut self, requested: bool, cx: &mut Context<Self>) {
        if self.details_requested == requested {
            return;
        }
        self.details_requested = requested;
        if requested {
            self.refresh(cx);
        }
    }

    pub fn discard_process_details(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_mut() else {
            return;
        };
        let had_details = snapshot
            .sessions
            .iter()
            .any(|session| !session.top_processes.is_empty());
        for session in &mut snapshot.sessions {
            session.top_processes.clear();
        }
        if had_details {
            self.generation = self.generation.wrapping_add(1);
            cx.notify();
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.sampling {
            return;
        }
        self.settings = self
            .model
            .read(cx)
            .settings
            .as_ref()
            .map(|settings| settings.resources.clone())
            .unwrap_or_else(WorkersResourceSettings::default);
        if !self.settings.monitoring_enabled {
            return;
        }
        self.sampling = true;
        let include_processes = self.details_requested;
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { client.resource_snapshot(include_processes) })
                .await;
            let _ = this.update(cx, |monitor, cx| {
                monitor.sampling = false;
                match result {
                    Ok(snapshot) => monitor.apply_snapshot(snapshot, cx),
                    Err(error) => {
                        monitor.last_error = Some(error.to_string());
                        monitor.generation = monitor.generation.wrapping_add(1);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn apply_snapshot(&mut self, snapshot: WorkersResourceSnapshot, cx: &mut Context<Self>) {
        let metadata = resource_metadata(&self.model.read(cx));
        let live_ids = snapshot
            .sessions
            .iter()
            .map(|session| session.session_id.clone())
            .collect::<HashSet<_>>();
        for session in &snapshot.sessions {
            let transition = self
                .reducers
                .entry(session.session_id.clone())
                .or_default()
                .observe(
                    self.settings.per_worker_warning_gib,
                    self.settings.per_worker_critical_gib,
                    session.physical_footprint_bytes,
                    session.attribution_complete,
                );
            if self.settings.notifications_enabled
                && let Some(level) = transition
            {
                post_resource_alert(level, session, metadata.get(&session.session_id));
            }
        }
        self.reducers
            .retain(|session_id, _| live_ids.contains(session_id));
        self.last_error = snapshot.error.clone();
        self.snapshot = Some(snapshot);
        self.generation = self.generation.wrapping_add(1);
        cx.notify();
    }
}

fn resource_metadata(model: &WorkersModel) -> HashMap<String, (String, String)> {
    let projects = model
        .projects()
        .iter()
        .map(|project| (project.id.as_str(), project.name.as_str()))
        .collect::<HashMap<_, _>>();
    model
        .sessions()
        .iter()
        .map(|session| {
            (
                session.id.clone(),
                (
                    session.title.clone(),
                    projects
                        .get(session.project_id.as_str())
                        .copied()
                        .unwrap_or(session.project_id.as_str())
                        .to_owned(),
                ),
            )
        })
        .collect()
}

fn post_resource_alert(
    level: ResourceAlertLevel,
    session: &WorkersSessionResource,
    metadata: Option<&(String, String)>,
) {
    let (title, project) = metadata
        .map(|(title, project)| (title.as_str(), project.as_str()))
        .unwrap_or((session.session_id.as_str(), "Unknown project"));
    let severity = match level {
        ResourceAlertLevel::Normal => return,
        ResourceAlertLevel::Warning => "high memory use",
        ResourceAlertLevel::Critical => "critical memory use",
    };
    crate::notify::post(
        "Worker resource warning",
        &format!("{title} · {project} has {severity}. Open Settings -> Resources for details."),
    );
}

pub struct WorkersResourceGlobal {
    pub monitor: Entity<WorkersResourceMonitor>,
}

impl Global for WorkersResourceGlobal {}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn alerts_fire_only_on_upward_transitions_and_clear_with_hysteresis() {
        let mut reducer = ResourceAlertReducer::default();

        assert_eq!(reducer.observe(4, 8, gib(3.9), true), None);
        assert_eq!(
            reducer.observe(4, 8, gib(4.1), true),
            Some(ResourceAlertLevel::Warning)
        );
        assert_eq!(reducer.observe(4, 8, gib(5.0), true), None);
        assert_eq!(
            reducer.observe(4, 8, gib(8.1), true),
            Some(ResourceAlertLevel::Critical)
        );
        assert_eq!(reducer.observe(4, 8, gib(3.5), true), None);
        assert_eq!(reducer.level(), ResourceAlertLevel::Warning);
        assert_eq!(reducer.observe(4, 8, gib(3.1), true), None);
        assert_eq!(reducer.level(), ResourceAlertLevel::Normal);
    }

    #[test]
    fn incomplete_attribution_never_emits_critical() {
        let mut reducer = ResourceAlertReducer::default();

        assert_eq!(
            reducer.observe(4, 8, gib(9.0), false),
            Some(ResourceAlertLevel::Warning)
        );
        assert_eq!(reducer.level(), ResourceAlertLevel::Warning);
    }

    #[test]
    fn changing_thresholds_reduces_against_current_usage() {
        let mut reducer = ResourceAlertReducer::default();
        assert_eq!(reducer.observe(4, 8, gib(3.0), true), None);

        assert_eq!(
            reducer.observe(2, 6, gib(3.0), true),
            Some(ResourceAlertLevel::Warning)
        );
    }

    fn gib(value: f64) -> u64 {
        (value * GIB as f64) as u64
    }
}
