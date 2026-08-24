use std::ffi::CStr;
use std::mem::MaybeUninit;
use std::ptr;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{PlatformProcess, ProcessPlatform, WorkersMemorySource};

pub(super) struct MacProcessPlatform {
    processes: Vec<PlatformProcess>,
    sampled_at_unix_ms: u64,
    sampled_at_ns: u64,
}

impl MacProcessPlatform {
    pub(super) fn capture() -> Result<Self, String> {
        let sampled_at_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        let sampled_at_ns = monotonic_time_ns()?;
        let pids = all_pids()?;
        let processes = pids.into_iter().filter_map(read_process).collect();
        Ok(Self {
            processes,
            sampled_at_unix_ms,
            sampled_at_ns,
        })
    }
}

impl ProcessPlatform for MacProcessPlatform {
    fn processes(&self) -> Result<Vec<PlatformProcess>, String> {
        Ok(self.processes.clone())
    }

    fn process_started_at_unix_ms(&self, pid: u32) -> Option<u64> {
        read_bsd_info(pid).map(|info| process_started_at_unix_ms(&info))
    }

    fn sampled_at_unix_ms(&self) -> u64 {
        self.sampled_at_unix_ms
    }

    fn sampled_at_ns(&self) -> u64 {
        self.sampled_at_ns
    }
}

fn all_pids() -> Result<Vec<u32>, String> {
    let estimated = unsafe { libc::proc_listallpids(ptr::null_mut(), 0) };
    if estimated <= 0 {
        return Err("proc_listallpids could not estimate the process table".into());
    }
    let mut capacity = usize::try_from(estimated).unwrap_or(256).saturating_add(64);
    for _ in 0..3 {
        let mut pids = vec![0 as libc::pid_t; capacity];
        let byte_len = pids
            .len()
            .checked_mul(std::mem::size_of::<libc::pid_t>())
            .and_then(|bytes| i32::try_from(bytes).ok())
            .ok_or("process table allocation is too large")?;
        let returned =
            unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast::<libc::c_void>(), byte_len) };
        if returned < 0 {
            return Err(format!(
                "proc_listallpids failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        let returned = usize::try_from(returned).unwrap_or_default();
        if returned < pids.len() {
            pids.truncate(returned);
            return Ok(pids
                .into_iter()
                .filter_map(|pid| u32::try_from(pid).ok())
                .filter(|pid| *pid > 1)
                .collect());
        }
        capacity = pids.len().saturating_mul(2);
    }
    Err("process table changed too quickly to capture safely".into())
}

fn read_process(pid: u32) -> Option<PlatformProcess> {
    let bsd = read_bsd_info(pid)?;
    let live_pid = u32::try_from(bsd.pbi_pid).ok()?;
    if live_pid != pid {
        return None;
    }
    let kernel_session_id = unsafe { libc::getsid(i32::try_from(pid).ok()?) };
    let kernel_session_id = u32::try_from(kernel_session_id).ok()?;
    let task = read_task_info(pid);
    let usage = read_rusage(pid);
    let resident_bytes = task
        .as_ref()
        .map(|info| info.pti_resident_size)
        .or_else(|| usage.as_ref().map(|info| info.ri_resident_size))
        .unwrap_or_default();
    let (physical_footprint_bytes, memory_source) = usage
        .as_ref()
        .map(|info| {
            (
                info.ri_phys_footprint,
                WorkersMemorySource::PhysicalFootprint,
            )
        })
        .unwrap_or_else(|| {
            if task.is_some() {
                (resident_bytes, WorkersMemorySource::ResidentFallback)
            } else {
                (0, WorkersMemorySource::Unavailable)
            }
        });
    let total_cpu_time_ns = usage
        .as_ref()
        .map(|info| info.ri_user_time.saturating_add(info.ri_system_time))
        .unwrap_or_default();

    Some(PlatformProcess {
        pid,
        parent_pid: bsd.pbi_ppid,
        kernel_session_id,
        started_at_unix_ms: process_started_at_unix_ms(&bsd),
        name: process_name(&bsd),
        total_cpu_time_ns,
        physical_footprint_bytes,
        resident_bytes,
        memory_source,
    })
}

fn read_bsd_info(pid: u32) -> Option<libc::proc_bsdinfo> {
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let expected = i32::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).ok()?;
    let read = unsafe {
        libc::proc_pidinfo(
            i32::try_from(pid).ok()?,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast::<libc::c_void>(),
            expected,
        )
    };
    (read == expected).then(|| unsafe { info.assume_init() })
}

fn read_task_info(pid: u32) -> Option<libc::proc_taskinfo> {
    let mut info = MaybeUninit::<libc::proc_taskinfo>::zeroed();
    let expected = i32::try_from(std::mem::size_of::<libc::proc_taskinfo>()).ok()?;
    let read = unsafe {
        libc::proc_pidinfo(
            i32::try_from(pid).ok()?,
            libc::PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr().cast::<libc::c_void>(),
            expected,
        )
    };
    (read == expected).then(|| unsafe { info.assume_init() })
}

fn read_rusage(pid: u32) -> Option<libc::rusage_info_v4> {
    let mut info = MaybeUninit::<libc::rusage_info_v4>::zeroed();
    let result = unsafe {
        libc::proc_pid_rusage(
            i32::try_from(pid).ok()?,
            libc::RUSAGE_INFO_V4,
            info.as_mut_ptr().cast::<libc::rusage_info_t>(),
        )
    };
    (result == 0).then(|| unsafe { info.assume_init() })
}

fn process_started_at_unix_ms(info: &libc::proc_bsdinfo) -> u64 {
    info.pbi_start_tvsec
        .saturating_mul(1_000)
        .saturating_add(info.pbi_start_tvusec / 1_000)
}

fn process_name(info: &libc::proc_bsdinfo) -> String {
    let bytes = unsafe { CStr::from_ptr(info.pbi_name.as_ptr()) }.to_bytes();
    let name = String::from_utf8_lossy(bytes).trim().to_owned();
    if name.is_empty() {
        format!("pid-{}", info.pbi_pid)
    } else {
        name
    }
}

fn monotonic_time_ns() -> Result<u64, String> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let status = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, &mut time) };
    if status != 0 {
        return Err(format!(
            "clock_gettime failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let seconds = u64::try_from(time.tv_sec).map_err(|_| "negative monotonic seconds")?;
    let nanoseconds = u64::try_from(time.tv_nsec).map_err(|_| "negative monotonic nanoseconds")?;
    Ok(seconds
        .saturating_mul(1_000_000_000)
        .saturating_add(nanoseconds))
}
