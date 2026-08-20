use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

#[cfg(target_os = "macos")]
mod macos;

#[cfg(not(target_os = "macos"))]
mod unsupported;

#[cfg(target_os = "macos")]
use macos::MacProcessPlatform;

const MAX_TOP_PROCESSES: usize = 8;
const MIN_CPU_SAMPLE_WINDOW_NS: u64 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceSupport {
    Supported,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkersMemorySource {
    PhysicalFootprint,
    ResidentFallback,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkersProcessResource {
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    pub cpu_percent: f64,
    pub physical_footprint_bytes: u64,
    pub resident_bytes: u64,
    pub memory_source: WorkersMemorySource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkersSessionResource {
    pub session_id: String,
    pub sampled_at_unix_ms: u64,
    pub root_pid: Option<u32>,
    pub root_pid_started_at: Option<u64>,
    pub cpu_percent: f64,
    pub physical_footprint_bytes: u64,
    pub resident_bytes: u64,
    pub process_count: usize,
    pub attribution_complete: bool,
    pub top_processes: Vec<WorkersProcessResource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkersResourceSnapshot {
    pub support: ResourceSupport,
    pub sampled_at_unix_ms: u64,
    pub sessions: Vec<WorkersSessionResource>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub started_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessMeasurement {
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    pub cpu_percent: f64,
    pub physical_footprint_bytes: u64,
    pub resident_bytes: u64,
    pub memory_source: WorkersMemorySource,
}

#[derive(Debug, Clone, PartialEq)]
struct PlatformProcess {
    pid: u32,
    parent_pid: u32,
    kernel_session_id: u32,
    started_at_unix_ms: u64,
    name: String,
    total_cpu_time_ns: u64,
    physical_footprint_bytes: u64,
    resident_bytes: u64,
    memory_source: WorkersMemorySource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionProcessRoot {
    session_id: String,
    pid: u32,
    pid_started_at_unix_ms: u64,
}

trait ProcessPlatform {
    fn processes(&self) -> Result<Vec<PlatformProcess>, String>;
    fn process_started_at_unix_ms(&self, pid: u32) -> Option<u64>;
    fn sampled_at_unix_ms(&self) -> u64;
    fn sampled_at_ns(&self) -> u64;
}

#[derive(Debug, Clone, Copy)]
struct CpuSample {
    identity: ProcessIdentity,
    total_cpu_time_ns: u64,
    sampled_at_ns: u64,
    last_cpu_percent: f64,
}

#[derive(Debug, Default)]
pub struct CpuTracker {
    samples: HashMap<u32, CpuSample>,
}

#[derive(Debug, Default)]
pub struct ResourceSampler {
    cpu: CpuTracker,
}

impl ResourceSampler {
    pub fn sample(&mut self, include_processes: bool) -> WorkersResourceSnapshot {
        sample_resources(&mut self.cpu, include_processes)
    }
}

#[cfg(target_os = "macos")]
pub fn current_session_process_identities(session_id: &str) -> Result<Vec<(u32, u64)>, String> {
    use unpeel_core::session_host::PidIdentity;

    let manifest = unpeel_core::session_host::load_manifest(session_id)
        .ok_or_else(|| format!("no manifest for worker {session_id}"))?;
    let (root_pid, root_started_at) = manifest
        .pid
        .zip(manifest.pid_started_at)
        .ok_or_else(|| format!("worker {session_id} has no live process identity"))?;
    if unpeel_core::session_host::manifest_pid_identity(&manifest) != PidIdentity::Matches {
        return Err(format!("worker {session_id} process identity is stale"));
    }
    let platform = MacProcessPlatform::capture()?;
    if platform.process_started_at_unix_ms(root_pid) != Some(root_started_at) {
        return Err(format!(
            "worker {session_id} root process changed during inspection"
        ));
    }
    let processes = platform.processes()?;
    let owned = owned_process_ids(&processes, root_pid);
    let mut identities = processes
        .into_iter()
        .filter(|process| owned.contains(&process.pid))
        .map(|process| (process.pid, process.started_at_unix_ms))
        .collect::<Vec<_>>();
    identities.sort_unstable();
    Ok(identities)
}

fn owned_process_ids(processes: &[PlatformProcess], root_pid: u32) -> HashSet<u32> {
    let mut owned = processes
        .iter()
        .filter(|process| process.pid == root_pid || process.kernel_session_id == root_pid)
        .map(|process| process.pid)
        .collect::<HashSet<_>>();
    loop {
        let before = owned.len();
        for process in processes {
            if owned.contains(&process.parent_pid) {
                owned.insert(process.pid);
            }
        }
        if owned.len() == before {
            break;
        }
    }
    owned
}

#[cfg(not(target_os = "macos"))]
pub fn current_session_process_identities(_session_id: &str) -> Result<Vec<(u32, u64)>, String> {
    Err("worker process inspection is unavailable on this platform".into())
}

impl CpuTracker {
    pub fn observe(
        &mut self,
        identity: ProcessIdentity,
        total_cpu_time_ns: u64,
        sampled_at_ns: u64,
    ) -> f64 {
        let Some(previous) = self.samples.get(&identity.pid).copied() else {
            self.samples.insert(
                identity.pid,
                CpuSample {
                    identity,
                    total_cpu_time_ns,
                    sampled_at_ns,
                    last_cpu_percent: 0.0,
                },
            );
            return 0.0;
        };
        if previous.identity != identity
            || sampled_at_ns <= previous.sampled_at_ns
            || total_cpu_time_ns < previous.total_cpu_time_ns
        {
            self.samples.insert(
                identity.pid,
                CpuSample {
                    identity,
                    total_cpu_time_ns,
                    sampled_at_ns,
                    last_cpu_percent: 0.0,
                },
            );
            return 0.0;
        }

        let elapsed_ns = sampled_at_ns - previous.sampled_at_ns;
        if elapsed_ns < MIN_CPU_SAMPLE_WINDOW_NS {
            return previous.last_cpu_percent;
        }
        let cpu_ns = total_cpu_time_ns - previous.total_cpu_time_ns;
        let cpu_percent = cpu_ns as f64 / elapsed_ns as f64 * 100.0;
        let cpu_percent = finite_non_negative(cpu_percent);
        self.samples.insert(
            identity.pid,
            CpuSample {
                identity,
                total_cpu_time_ns,
                sampled_at_ns,
                last_cpu_percent: cpu_percent,
            },
        );
        cpu_percent
    }
}

pub fn aggregate_session_measurements(
    session_id: &str,
    root_pid: u32,
    root_pid_started_at: u64,
    sampled_at_unix_ms: u64,
    measurements: Vec<ProcessMeasurement>,
    attribution_complete: bool,
    include_processes: bool,
) -> WorkersSessionResource {
    let mut cpu_percent = 0.0;
    let mut physical_footprint_bytes = 0_u64;
    let mut resident_bytes = 0_u64;
    let process_count = measurements.len();
    let mut top_processes = Vec::with_capacity(measurements.len().min(MAX_TOP_PROCESSES));

    for measurement in measurements {
        let process_cpu = finite_non_negative(measurement.cpu_percent);
        cpu_percent += process_cpu;
        physical_footprint_bytes =
            physical_footprint_bytes.saturating_add(measurement.physical_footprint_bytes);
        resident_bytes = resident_bytes.saturating_add(measurement.resident_bytes);
        if include_processes {
            top_processes.push(WorkersProcessResource {
                pid: measurement.pid,
                parent_pid: measurement.parent_pid,
                name: measurement.name,
                cpu_percent: process_cpu,
                physical_footprint_bytes: measurement.physical_footprint_bytes,
                resident_bytes: measurement.resident_bytes,
                memory_source: measurement.memory_source,
            });
        }
    }
    cpu_percent = finite_non_negative(cpu_percent);
    top_processes.sort_by(|left, right| {
        right
            .physical_footprint_bytes
            .cmp(&left.physical_footprint_bytes)
            .then_with(|| left.pid.cmp(&right.pid))
    });
    top_processes.truncate(MAX_TOP_PROCESSES);

    WorkersSessionResource {
        session_id: session_id.to_owned(),
        sampled_at_unix_ms,
        root_pid: Some(root_pid),
        root_pid_started_at: Some(root_pid_started_at),
        cpu_percent,
        physical_footprint_bytes,
        resident_bytes,
        process_count,
        attribution_complete,
        top_processes,
    }
}

fn sample_root<P: ProcessPlatform>(
    root: &SessionProcessRoot,
    platform: &P,
    include_processes: bool,
    cpu: &mut CpuTracker,
) -> WorkersSessionResource {
    let sampled_at_unix_ms = platform.sampled_at_unix_ms();
    let empty = || {
        aggregate_session_measurements(
            &root.session_id,
            root.pid,
            root.pid_started_at_unix_ms,
            sampled_at_unix_ms,
            Vec::new(),
            false,
            include_processes,
        )
    };
    if platform.process_started_at_unix_ms(root.pid) != Some(root.pid_started_at_unix_ms) {
        return empty();
    }
    let Ok(processes) = platform.processes() else {
        return empty();
    };
    let root_matches = processes.iter().any(|process| {
        process.pid == root.pid
            && process.started_at_unix_ms == root.pid_started_at_unix_ms
            && process.kernel_session_id == root.pid
    });
    if !root_matches {
        return empty();
    }
    let sampled_at_ns = platform.sampled_at_ns();
    let owned = owned_process_ids(&processes, root.pid);
    let measurements = processes
        .into_iter()
        .filter(|process| owned.contains(&process.pid))
        .map(|process| ProcessMeasurement {
            pid: process.pid,
            parent_pid: process.parent_pid,
            name: process.name,
            cpu_percent: cpu.observe(
                ProcessIdentity {
                    pid: process.pid,
                    started_at_unix_ms: process.started_at_unix_ms,
                },
                process.total_cpu_time_ns,
                sampled_at_ns,
            ),
            physical_footprint_bytes: process.physical_footprint_bytes,
            resident_bytes: process.resident_bytes,
            memory_source: process.memory_source,
        })
        .collect();
    if platform.process_started_at_unix_ms(root.pid) != Some(root.pid_started_at_unix_ms) {
        return empty();
    }

    aggregate_session_measurements(
        &root.session_id,
        root.pid,
        root.pid_started_at_unix_ms,
        sampled_at_unix_ms,
        measurements,
        true,
        include_processes,
    )
}

fn finite_non_negative(value: f64) -> f64 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        0.0
    }
}

#[cfg(target_os = "macos")]
fn sample_resources(cpu: &mut CpuTracker, include_processes: bool) -> WorkersResourceSnapshot {
    use unpeel_core::session_host::{HostedSessionState, PidIdentity};

    let platform = match MacProcessPlatform::capture() {
        Ok(platform) => platform,
        Err(error) => {
            return WorkersResourceSnapshot {
                support: ResourceSupport::Supported,
                sampled_at_unix_ms: unix_time_ms(),
                sessions: Vec::new(),
                error: Some(error),
            };
        }
    };
    let sampled_at_unix_ms = platform.sampled_at_unix_ms();
    let mut sessions = unpeel_core::session_host::list_manifests()
        .into_iter()
        .filter(|manifest| manifest.state == HostedSessionState::Running)
        .map(|manifest| {
            let root = manifest.pid.zip(manifest.pid_started_at).and_then(
                |(pid, pid_started_at_unix_ms)| {
                    (unpeel_core::session_host::manifest_pid_identity(&manifest)
                        == PidIdentity::Matches)
                        .then(|| SessionProcessRoot {
                            session_id: manifest.session.id.clone(),
                            pid,
                            pid_started_at_unix_ms,
                        })
                },
            );
            match root {
                Some(root) => sample_root(&root, &platform, include_processes, cpu),
                None => WorkersSessionResource {
                    session_id: manifest.session.id,
                    sampled_at_unix_ms,
                    root_pid: manifest.pid,
                    root_pid_started_at: manifest.pid_started_at,
                    cpu_percent: 0.0,
                    physical_footprint_bytes: 0,
                    resident_bytes: 0,
                    process_count: 0,
                    attribution_complete: false,
                    top_processes: Vec::new(),
                },
            }
        })
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        right
            .physical_footprint_bytes
            .cmp(&left.physical_footprint_bytes)
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    WorkersResourceSnapshot {
        support: ResourceSupport::Supported,
        sampled_at_unix_ms,
        sessions,
        error: None,
    }
}

#[cfg(not(target_os = "macos"))]
fn sample_resources(_cpu: &mut CpuTracker, _include_processes: bool) -> WorkersResourceSnapshot {
    unsupported::snapshot()
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measurement(
        pid: u32,
        parent_pid: u32,
        name: &str,
        cpu_percent: f64,
        physical_footprint_bytes: u64,
        resident_bytes: u64,
    ) -> ProcessMeasurement {
        ProcessMeasurement {
            pid,
            parent_pid,
            name: name.to_owned(),
            cpu_percent,
            physical_footprint_bytes,
            resident_bytes,
            memory_source: WorkersMemorySource::PhysicalFootprint,
        }
    }

    #[test]
    fn aggregate_sums_processes_and_orders_heaviest_first() {
        let snapshot = aggregate_session_measurements(
            "session-a",
            42,
            1_000,
            2_000,
            vec![
                measurement(43, 42, "mcp", 5.0, 400, 300),
                measurement(42, 0, "agent", 25.0, 600, 500),
            ],
            true,
            true,
        );

        assert_eq!(snapshot.cpu_percent, 30.0);
        assert_eq!(snapshot.physical_footprint_bytes, 1_000);
        assert_eq!(snapshot.resident_bytes, 800);
        assert_eq!(snapshot.process_count, 2);
        assert_eq!(snapshot.top_processes[0].name, "agent");
        assert!(snapshot.attribution_complete);
    }

    #[test]
    fn aggregate_saturates_integer_totals_and_drops_non_finite_cpu() {
        let snapshot = aggregate_session_measurements(
            "session-a",
            42,
            1_000,
            2_000,
            vec![
                measurement(42, 0, "agent", f64::NAN, u64::MAX, u64::MAX),
                measurement(43, 42, "mcp", -5.0, 10, 10),
            ],
            true,
            false,
        );

        assert_eq!(snapshot.cpu_percent, 0.0);
        assert_eq!(snapshot.physical_footprint_bytes, u64::MAX);
        assert_eq!(snapshot.resident_bytes, u64::MAX);
        assert!(snapshot.top_processes.is_empty());
    }

    #[test]
    fn aggregate_limits_process_details_to_eight_rows() {
        let measurements = (1..=12)
            .map(|pid| measurement(pid, 0, &format!("process-{pid}"), 0.0, u64::from(pid), 1))
            .collect();
        let snapshot =
            aggregate_session_measurements("session-a", 1, 1_000, 2_000, measurements, true, true);

        assert_eq!(snapshot.top_processes.len(), 8);
        assert_eq!(snapshot.top_processes[0].pid, 12);
        assert_eq!(snapshot.top_processes[7].pid, 5);
    }

    #[test]
    fn cpu_tracker_requires_one_second_and_resets_on_pid_reuse() {
        let mut tracker = CpuTracker::default();
        let first = ProcessIdentity {
            pid: 7,
            started_at_unix_ms: 100,
        };
        let reused = ProcessIdentity {
            pid: 7,
            started_at_unix_ms: 200,
        };

        assert_eq!(tracker.observe(first, 10, 0), 0.0);
        assert_eq!(tracker.observe(first, 20, 500_000_000), 0.0);
        let cpu = tracker.observe(first, 30, 1_500_000_000);
        assert!((cpu - (20.0 / 1_500_000_000.0 * 100.0)).abs() < f64::EPSILON);
        assert_eq!(tracker.observe(reused, 40, 2_500_000_000), 0.0);
    }
}

#[cfg(test)]
mod sampler_tests {
    use super::*;

    #[derive(Default)]
    struct FakePlatform {
        processes: Vec<PlatformProcess>,
        sampled_at_unix_ms: u64,
        sampled_at_ns: u64,
    }

    impl ProcessPlatform for FakePlatform {
        fn processes(&self) -> Result<Vec<PlatformProcess>, String> {
            Ok(self.processes.clone())
        }

        fn process_started_at_unix_ms(&self, pid: u32) -> Option<u64> {
            self.processes
                .iter()
                .find(|process| process.pid == pid)
                .map(|process| process.started_at_unix_ms)
        }

        fn sampled_at_unix_ms(&self) -> u64 {
            self.sampled_at_unix_ms
        }

        fn sampled_at_ns(&self) -> u64 {
            self.sampled_at_ns
        }
    }

    fn process(
        pid: u32,
        parent_pid: u32,
        kernel_session_id: u32,
        started_at_unix_ms: u64,
    ) -> PlatformProcess {
        PlatformProcess {
            pid,
            parent_pid,
            kernel_session_id,
            started_at_unix_ms,
            name: format!("process-{pid}"),
            total_cpu_time_ns: u64::from(pid),
            physical_footprint_bytes: u64::from(pid) * 10,
            resident_bytes: u64::from(pid) * 5,
            memory_source: WorkersMemorySource::PhysicalFootprint,
        }
    }

    fn root(pid: u32, started_at_unix_ms: u64) -> SessionProcessRoot {
        SessionProcessRoot {
            session_id: "session-a".into(),
            pid,
            pid_started_at_unix_ms: started_at_unix_ms,
        }
    }

    #[test]
    fn sampler_includes_only_processes_owned_by_verified_kernel_session() {
        let platform = FakePlatform {
            processes: vec![
                process(100, 1, 100, 1_000),
                process(101, 100, 100, 1_001),
                process(102, 101, 102, 1_002),
                process(200, 1, 200, 2_000),
            ],
            sampled_at_unix_ms: 3_000,
            sampled_at_ns: 2_000_000_000,
        };
        let mut cpu = CpuTracker::default();

        let sample = sample_root(&root(100, 1_000), &platform, true, &mut cpu);

        assert_eq!(sample.process_count, 3);
        assert!(
            sample
                .top_processes
                .iter()
                .any(|process| process.pid == 102)
        );
        assert!(
            sample
                .top_processes
                .iter()
                .all(|process| process.pid != 200)
        );
        assert!(sample.attribution_complete);
    }

    #[test]
    fn sampler_fails_closed_when_root_start_time_does_not_match() {
        let platform = FakePlatform {
            processes: vec![process(100, 1, 100, 9_999)],
            sampled_at_unix_ms: 3_000,
            sampled_at_ns: 2_000_000_000,
        };
        let mut cpu = CpuTracker::default();

        let sample = sample_root(&root(100, 1_000), &platform, true, &mut cpu);

        assert!(!sample.attribution_complete);
        assert_eq!(sample.process_count, 0);
        assert!(sample.top_processes.is_empty());
    }

    #[test]
    fn sampler_reports_incomplete_when_process_enumeration_fails() {
        struct BrokenPlatform;
        impl ProcessPlatform for BrokenPlatform {
            fn processes(&self) -> Result<Vec<PlatformProcess>, String> {
                Err("process table unavailable".into())
            }

            fn process_started_at_unix_ms(&self, _pid: u32) -> Option<u64> {
                Some(1_000)
            }

            fn sampled_at_unix_ms(&self) -> u64 {
                3_000
            }

            fn sampled_at_ns(&self) -> u64 {
                2_000_000_000
            }
        }
        let mut cpu = CpuTracker::default();

        let sample = sample_root(&root(100, 1_000), &BrokenPlatform, true, &mut cpu);

        assert!(!sample.attribution_complete);
        assert_eq!(sample.process_count, 0);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_sampler_tests {
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    use super::*;

    #[test]
    fn macos_sampler_attributes_only_the_fixture_kernel_session() {
        let mut command = Command::new("/bin/sleep");
        command.arg("10");
        // SAFETY: This runs in the post-fork child before exec. `setsid` has no
        // Rust-visible memory effects and creates the isolated session the
        // sampler must attribute.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().expect("spawn fixture session");
        let fixture_pid = child.id();
        let platform = MacProcessPlatform::capture().expect("capture process table");
        let started_at = platform
            .process_started_at_unix_ms(fixture_pid)
            .expect("fixture start time");
        let mut cpu = CpuTracker::default();

        let sample = sample_root(
            &SessionProcessRoot {
                session_id: "fixture".into(),
                pid: fixture_pid,
                pid_started_at_unix_ms: started_at,
            },
            &platform,
            true,
            &mut cpu,
        );

        let _ = child.kill();
        let _ = child.wait();
        assert!(sample.attribution_complete);
        assert!(
            sample
                .top_processes
                .iter()
                .any(|process| process.pid == fixture_pid)
        );
        assert!(
            sample
                .top_processes
                .iter()
                .all(|process| process.pid != std::process::id())
        );
    }
}
