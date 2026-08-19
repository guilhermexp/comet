use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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

fn finite_non_negative(value: f64) -> f64 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        0.0
    }
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
