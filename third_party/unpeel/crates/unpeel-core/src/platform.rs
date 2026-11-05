//! Native primitives shared by the portable metadata/controller paths.
//! The hosted Unix PTY transport has its own explicit platform boundary.

use std::fs::File;
use std::io;
use std::path::Path;

pub fn lock_exclusive(file: &File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        windows::lock(file, false)
    }
}

pub fn unlock(file: &File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        windows::lock(file, true)
    }
}

pub fn create_private_file(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(path)
    }
    #[cfg(windows)]
    {
        windows::create_private_file(path)
    }
}

pub fn secure_private_file(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
    }
    #[cfg(windows)]
    {
        windows::secure_private_file(path)
    }
}

pub fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        let Ok(pid) = i32::try_from(pid) else {
            return true;
        };
        let result = unsafe { libc::kill(pid, 0) };
        result == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(windows)]
    {
        windows::pid_is_alive(pid)
    }
}

/// Open a private advisory lock, optionally refusing a link at the leaf.
pub fn open_lock_file(path: &Path, no_follow: bool) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(if no_follow { libc::O_NOFOLLOW } else { 0 })
            .open(path)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, OPEN_ALWAYS,
        };
        let file = windows::open_private_file(
            path,
            OPEN_ALWAYS,
            if no_follow {
                FILE_FLAG_OPEN_REPARSE_POINT
            } else {
                0
            },
            true,
        )?;
        if no_follow && file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "lock file is a reparse point",
            ));
        }
        Ok(file)
    }
}

pub fn try_lock_exclusive(file: &File) -> io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(true);
        }
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::WouldBlock {
            Ok(false)
        } else {
            Err(error)
        }
    }
    #[cfg(windows)]
    {
        windows::try_lock_exclusive(file)
    }
}

/// Publish an already-synced staging file and durably commit its rename.
pub fn publish_synced_file(temporary: &Path, path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::rename(temporary, path)?;
        File::open(
            path.parent()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent"))?,
        )?
        .sync_all()
    }
    #[cfg(windows)]
    {
        windows::publish_synced_file(temporary, path)
    }
}

/// Kernel creation time for a Windows PID, used to refuse recycled leases.
#[cfg(windows)]
pub fn process_start_time(pid: u32) -> Option<u64> {
    windows::process_start_time(pid)
}

pub(crate) fn terminate_process(pid: i32, force: bool) -> io::Result<()> {
    if pid <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid process id",
        ));
    }
    #[cfg(unix)]
    {
        let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
        if unsafe { libc::kill(pid, signal) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        let _ = force; // Windows has no Unix signal: terminate this native handle.
        windows::terminate_process(pid as u32)
    }
}

#[cfg(windows)]
pub(crate) mod windows {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{
        GetLastError, LocalFree, ERROR_INVALID_PARAMETER, ERROR_LOCK_VIOLATION, FILETIME,
        GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, SetFileSecurityW, TokenUser, DACL_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, LockFileEx, MoveFileExW, UnlockFileEx, CREATE_NEW, FILE_ATTRIBUTE_NORMAL,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, LOCKFILE_EXCLUSIVE_LOCK,
        LOCKFILE_FAIL_IMMEDIATELY, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetProcessTimes, OpenProcess, OpenProcessToken, TerminateProcess,
        WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        PROCESS_TERMINATE,
    };
    use windows_sys::Win32::System::IO::OVERLAPPED;

    pub(super) fn lock(file: &File, release: bool) -> io::Result<()> {
        let mut overlap: OVERLAPPED = unsafe { std::mem::zeroed() };
        let handle = file.as_raw_handle().cast();
        let result = unsafe {
            if release {
                UnlockFileEx(handle, 0, u32::MAX, u32::MAX, &mut overlap)
            } else {
                LockFileEx(
                    handle,
                    LOCKFILE_EXCLUSIVE_LOCK,
                    0,
                    u32::MAX,
                    u32::MAX,
                    &mut overlap,
                )
            }
        };
        if result == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        let mut value: Vec<_> = path.as_os_str().encode_wide().collect();
        if value.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path contains NUL",
            ));
        }
        value.push(0);
        Ok(value)
    }

    pub(crate) fn owner_sid() -> io::Result<String> {
        let mut token = std::ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = unsafe { OwnedHandle::from_raw_handle(token.cast()) };
        let mut bytes = 0;
        unsafe {
            GetTokenInformation(
                token.as_raw_handle().cast(),
                TokenUser,
                std::ptr::null_mut(),
                0,
                &mut bytes,
            )
        };
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        // TOKEN_USER contains pointers: keep the returned buffer aligned.
        let mut buffer = vec![0usize; (bytes as usize).div_ceil(std::mem::size_of::<usize>())];
        if unsafe {
            GetTokenInformation(
                token.as_raw_handle().cast(),
                TokenUser,
                buffer.as_mut_ptr().cast(),
                bytes,
                &mut bytes,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
        let mut sid = std::ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0;
        while unsafe { *sid.add(len) } != 0 {
            len += 1;
        }
        let result = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(sid, len) });
        unsafe { LocalFree(sid.cast()) };
        Ok(result)
    }

    struct PrivateSecurity(PSECURITY_DESCRIPTOR);
    impl Drop for PrivateSecurity {
        fn drop(&mut self) {
            unsafe { LocalFree(self.0) };
        }
    }
    impl PrivateSecurity {
        fn new() -> io::Result<Self> {
            let sddl: Vec<_> = format!("D:P(A;;FA;;;{})\0", owner_sid()?)
                .encode_utf16()
                .collect();
            let mut descriptor = std::ptr::null_mut();
            if unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(descriptor))
        }
    }

    pub(super) fn create_private_file(path: &Path) -> io::Result<File> {
        open_private_file(path, CREATE_NEW, 0, false)
    }

    pub(super) fn open_private_file(
        path: &Path,
        creation: u32,
        flags: u32,
        shared_write: bool,
    ) -> io::Result<File> {
        let security = PrivateSecurity::new()?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security.0,
            bInheritHandle: 0,
        };
        let path = wide(path)?;
        let sharing =
            FILE_SHARE_READ | FILE_SHARE_DELETE | if shared_write { FILE_SHARE_WRITE } else { 0 };
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                sharing,
                &attributes,
                creation,
                FILE_ATTRIBUTE_NORMAL | flags,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(File::from(unsafe {
            OwnedHandle::from_raw_handle(handle.cast())
        }))
    }

    pub(super) fn secure_private_file(path: &Path) -> io::Result<()> {
        let security = PrivateSecurity::new()?;
        let path = wide(path)?;
        if unsafe {
            SetFileSecurityW(
                path.as_ptr(),
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                security.0,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub(super) fn try_lock_exclusive(file: &File) -> io::Result<bool> {
        let mut overlap: OVERLAPPED = unsafe { std::mem::zeroed() };
        if unsafe {
            LockFileEx(
                file.as_raw_handle().cast(),
                LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                0,
                u32::MAX,
                u32::MAX,
                &mut overlap,
            )
        } != 0
        {
            return Ok(true);
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_LOCK_VIOLATION as i32) {
            Ok(false)
        } else {
            Err(error)
        }
    }

    pub(super) fn publish_synced_file(temporary: &Path, path: &Path) -> io::Result<()> {
        let temporary = wide(temporary)?;
        let path = wide(path)?;
        if unsafe {
            MoveFileExW(
                temporary.as_ptr(),
                path.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub(super) fn process_start_time(pid: u32) -> Option<u64> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(handle.cast()) };
        let mut times: [FILETIME; 4] = unsafe { std::mem::zeroed() };
        if unsafe {
            GetProcessTimes(
                handle.as_raw_handle().cast(),
                &mut times[0],
                &mut times[1],
                &mut times[2],
                &mut times[3],
            )
        } == 0
        {
            return None;
        }
        Some((u64::from(times[0].dwHighDateTime) << 32) | u64::from(times[0].dwLowDateTime))
    }

    pub(super) fn pid_is_alive(pid: u32) -> bool {
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            // Only a definitive missing PID permits stale-state cleanup.
            return unsafe { GetLastError() } != ERROR_INVALID_PARAMETER;
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(handle.cast()) };
        match unsafe { WaitForSingleObject(handle.as_raw_handle().cast(), 0) } {
            WAIT_OBJECT_0 => false,
            WAIT_TIMEOUT => true,
            _ => true, // An uncertain liveness probe never authorizes stale-state deletion.
        }
    }

    pub(super) fn terminate_process(pid: u32) -> io::Result<()> {
        let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(handle.cast()) };
        if unsafe { TerminateProcess(handle.as_raw_handle().cast(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn exclusive_locks_exclude_other_handles_and_release_on_close() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.lock");
        let first = open_lock_file(&path, true).unwrap();
        let second = open_lock_file(&path, true).unwrap();
        assert!(try_lock_exclusive(&first).unwrap());
        assert!(!try_lock_exclusive(&second).unwrap());
        unlock(&first).unwrap();
        assert!(try_lock_exclusive(&second).unwrap());
        drop(second);
        assert!(try_lock_exclusive(&first).unwrap());
    }

    #[test]
    fn private_synced_files_publish_and_replace_unicode_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identidade ç.json");
        std::fs::write(&path, "old").unwrap();
        let temporary = dir.path().join("staging ç.tmp");
        let mut file = create_private_file(&temporary).unwrap();
        file.write_all(b"new").unwrap();
        file.sync_all().unwrap();
        drop(file);
        publish_synced_file(&temporary, &path).unwrap();
        secure_private_file(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert!(!temporary.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn native_liveness_recognizes_the_current_process() {
        assert!(pid_is_alive(std::process::id()));
        assert!(!pid_is_alive(0));
        #[cfg(windows)]
        assert!(process_start_time(std::process::id()).is_some());
    }
}
