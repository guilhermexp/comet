use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui::{Context, Entity, Global, Task};
use zeron_workers_unpeel::resources::{WorkersResourceSnapshot, WorkersSessionResource};
use zeron_workers_unpeel::{LocalWorkersClient, WorkersResourceSettings};

use super::model::WorkersModel;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryPressureLevel {
    #[default]
    Normal,
    Warning,
    Critical,
}

impl MemoryPressureLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Warning => "Warning",
            Self::Critical => "Critical",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PressureAction {
    #[default]
    None,
    TrimCaches,
    TrimAggressively,
}

#[derive(Debug, Default)]
pub struct MemoryPressureReducer {
    level: MemoryPressureLevel,
}

impl MemoryPressureReducer {
    pub fn observe(&mut self, level: MemoryPressureLevel) -> PressureAction {
        let action = match (self.level, level) {
            (previous, MemoryPressureLevel::Critical)
                if previous < MemoryPressureLevel::Critical =>
            {
                PressureAction::TrimAggressively
            }
            (MemoryPressureLevel::Normal, MemoryPressureLevel::Warning) => {
                PressureAction::TrimCaches
            }
            _ => PressureAction::None,
        };
        self.level = level;
        action
    }
}

#[cfg(target_os = "macos")]
mod pressure_source {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, Ordering};

    use block2::RcBlock;
    use dispatch2::{
        _dispatch_source_type_memorypressure, DispatchObject, DispatchQoS, DispatchQueue,
        DispatchRetained, DispatchSource, GlobalQueueIdentifier,
        dispatch_source_memorypressure_flags_t,
    };

    use super::MemoryPressureLevel;

    pub struct MemoryPressureSource {
        level: Arc<AtomicU8>,
        source: DispatchRetained<DispatchSource>,
        _handler: RcBlock<dyn Fn()>,
    }

    impl MemoryPressureSource {
        pub fn new() -> Self {
            let mask = dispatch_source_memorypressure_flags_t::DISPATCH_MEMORYPRESSURE_NORMAL.0
                | dispatch_source_memorypressure_flags_t::DISPATCH_MEMORYPRESSURE_WARN.0
                | dispatch_source_memorypressure_flags_t::DISPATCH_MEMORYPRESSURE_CRITICAL.0;
            let queue = DispatchQueue::global_queue(GlobalQueueIdentifier::QualityOfService(
                DispatchQoS::Utility,
            ));
            // SAFETY: libdispatch owns this process-wide source type and the source has no handle.
            let source = unsafe {
                DispatchSource::new(
                    std::ptr::addr_of!(_dispatch_source_type_memorypressure).cast_mut(),
                    0,
                    mask as usize,
                    Some(&queue),
                )
            };
            let source_ptr: *const DispatchSource = &*source;
            let level = Arc::new(AtomicU8::new(0));
            let callback_level = level.clone();
            let handler: RcBlock<dyn Fn()> = RcBlock::new(move || {
                // SAFETY: the handler is retained by `MemoryPressureSource`, which also retains
                // the source for the entire callback lifetime.
                let data = unsafe { (&*source_ptr).data() };
                let critical =
                    dispatch_source_memorypressure_flags_t::DISPATCH_MEMORYPRESSURE_CRITICAL.0
                        as usize;
                let warning =
                    dispatch_source_memorypressure_flags_t::DISPATCH_MEMORYPRESSURE_WARN.0 as usize;
                let next = if data & critical != 0 {
                    2
                } else if data & warning != 0 {
                    1
                } else {
                    0
                };
                callback_level.store(next, Ordering::Release);
            });
            // SAFETY: libdispatch copies the valid heap block and invokes it with no arguments.
            unsafe {
                source.set_event_handler_with_block(RcBlock::as_ptr(&handler).cast());
            }
            source.activate();
            Self {
                level,
                source,
                _handler: handler,
            }
        }

        pub fn level(&self) -> MemoryPressureLevel {
            match self.level.load(Ordering::Acquire) {
                2 => MemoryPressureLevel::Critical,
                1 => MemoryPressureLevel::Warning,
                _ => MemoryPressureLevel::Normal,
            }
        }
    }

    impl Drop for MemoryPressureSource {
        fn drop(&mut self) {
            self.source.cancel();
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod pressure_source {
    use super::MemoryPressureLevel;

    pub struct MemoryPressureSource;

    impl MemoryPressureSource {
        pub fn new() -> Self {
            Self
        }

        pub fn level(&self) -> MemoryPressureLevel {
            MemoryPressureLevel::Normal
        }
    }
}

use pressure_source::MemoryPressureSource;

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
    pressure_source: MemoryPressureSource,
    pressure_reducer: MemoryPressureReducer,
    pressure_level: MemoryPressureLevel,
    pressure_action: PressureAction,
    pressure_generation: u64,
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
            pressure_source: MemoryPressureSource::new(),
            pressure_reducer: MemoryPressureReducer::default(),
            pressure_level: MemoryPressureLevel::Normal,
            pressure_action: PressureAction::None,
            pressure_generation: 0,
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

    pub fn pressure_level(&self) -> MemoryPressureLevel {
        self.pressure_level
    }

    pub fn pressure_action(&self) -> PressureAction {
        self.pressure_action
    }

    pub fn pressure_generation(&self) -> u64 {
        self.pressure_generation
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
        self.observe_memory_pressure(cx);
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
        let include_processes =
            self.details_requested && self.pressure_level == MemoryPressureLevel::Normal;
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

    fn observe_memory_pressure(&mut self, cx: &mut Context<Self>) {
        let level = self.pressure_source.level();
        let action = self.pressure_reducer.observe(level);
        let changed = level != self.pressure_level;
        self.pressure_level = level;
        if action != PressureAction::None {
            self.pressure_action = action;
            if let Some(snapshot) = self.snapshot.as_mut() {
                for session in &mut snapshot.sessions {
                    session.top_processes.clear();
                }
            }
            self.pressure_generation = self.pressure_generation.wrapping_add(1);
            post_pressure_alert(level, self.snapshot.as_ref());
        }
        if changed || action != PressureAction::None {
            self.generation = self.generation.wrapping_add(1);
            cx.notify();
        }
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

fn post_pressure_alert(level: MemoryPressureLevel, snapshot: Option<&WorkersResourceSnapshot>) {
    let heaviest = snapshot
        .into_iter()
        .flat_map(|snapshot| snapshot.sessions.iter())
        .filter(|session| session.attribution_complete)
        .max_by_key(|session| session.physical_footprint_bytes)
        .map(|session| session.session_id.as_str());
    let severity = match level {
        MemoryPressureLevel::Normal => return,
        MemoryPressureLevel::Warning => "Memory pressure is elevated",
        MemoryPressureLevel::Critical => "Memory pressure is critical",
    };
    let detail = heaviest
        .map(|session_id| format!(" Heaviest hosted worker: {session_id}."))
        .unwrap_or_default();
    crate::notify::post(
        "Workers memory pressure",
        &format!(
            "{severity}. Comet released local caches without stopping any worker.{detail} Open Settings -> Resources for details."
        ),
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

    #[test]
    fn pressure_reducer_reacts_only_to_escalation_and_resets_on_normal() {
        let mut reducer = MemoryPressureReducer::default();
        assert_eq!(
            reducer.observe(MemoryPressureLevel::Normal),
            PressureAction::None
        );
        assert_eq!(
            reducer.observe(MemoryPressureLevel::Warning),
            PressureAction::TrimCaches
        );
        assert_eq!(
            reducer.observe(MemoryPressureLevel::Warning),
            PressureAction::None
        );
        assert_eq!(
            reducer.observe(MemoryPressureLevel::Critical),
            PressureAction::TrimAggressively
        );
        assert_eq!(
            reducer.observe(MemoryPressureLevel::Normal),
            PressureAction::None
        );
        assert_eq!(
            reducer.observe(MemoryPressureLevel::Warning),
            PressureAction::TrimCaches
        );
    }

    fn gib(value: f64) -> u64 {
        (value * GIB as f64) as u64
    }
}
