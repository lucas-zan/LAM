use crate::services::error::{AppError, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayProcessIdentity {
    pub executable: PathBuf,
    pub uid: u32,
    /// Parent PID of the gateway process at inspection time. Used to detect
    /// orphaned gateways whose launching LAM process has exited (their parent
    /// becomes launchd, pid 1).
    pub parent_pid: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryOutcome {
    NotRunning,
    Terminated,
}

pub trait GatewayProcessControl {
    fn inspect(&self, pid: u32) -> Result<Option<GatewayProcessIdentity>>;
    fn terminate(&self, pid: u32, timeout: Duration) -> Result<bool>;
}

pub fn recover_verified_gateway_process<C: GatewayProcessControl>(
    control: &C,
    pid: u32,
    expected_executable: &Path,
    expected_uid: u32,
    timeout: Duration,
) -> Result<RecoveryOutcome> {
    let Some(identity) = control.inspect(pid)? else {
        return Ok(RecoveryOutcome::NotRunning);
    };
    if identity.uid != expected_uid {
        return Err(AppError::new(
            "GATEWAY_PROCESS_OWNER_MISMATCH",
            "Gateway process owner does not match the current user",
        ));
    }
    if identity.executable != expected_executable {
        return Err(AppError::new(
            "GATEWAY_PROCESS_EXECUTABLE_MISMATCH",
            "Gateway process executable does not match the verified component",
        ));
    }
    if !control.terminate(pid, timeout)? {
        return Err(AppError::new(
            "GATEWAY_PROCESS_TERMINATION_TIMEOUT",
            "verified Gateway process did not stop within the bounded deadline",
        ));
    }
    Ok(RecoveryOutcome::Terminated)
}

pub struct SystemGatewayProcessControl;

#[cfg(target_os = "macos")]
impl GatewayProcessControl for SystemGatewayProcessControl {
    fn inspect(&self, pid: u32) -> Result<Option<GatewayProcessIdentity>> {
        if !process_is_running(pid)? {
            return Ok(None);
        }
        let Some(uid) = process_uid(pid)? else {
            return Ok(None);
        };
        Ok(Some(GatewayProcessIdentity {
            executable: process_executable(pid)?,
            uid,
            parent_pid: process_parent_pid(pid)?.unwrap_or(1),
        }))
    }

    fn terminate(&self, pid: u32, timeout: Duration) -> Result<bool> {
        if unsafe { libc::kill(pid as i32, libc::SIGTERM) } != 0 {
            return if !process_is_running(pid)? {
                Ok(true)
            } else {
                Err(process_error("GATEWAY_PROCESS_TERMINATION_FAILED"))
            };
        }
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if !process_is_running(pid)? {
                return Ok(true);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(!process_is_running(pid)?)
    }
}

#[cfg(not(target_os = "macos"))]
impl GatewayProcessControl for SystemGatewayProcessControl {
    fn inspect(&self, _pid: u32) -> Result<Option<GatewayProcessIdentity>> {
        Err(unsupported_platform())
    }

    fn terminate(&self, _pid: u32, _timeout: Duration) -> Result<bool> {
        Err(unsupported_platform())
    }
}

#[cfg(target_os = "macos")]
fn process_executable(pid: u32) -> Result<PathBuf> {
    let mut buffer = vec![0_u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let length =
        unsafe { libc::proc_pidpath(pid as i32, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    if length <= 0 {
        return Err(process_error("GATEWAY_PROCESS_INSPECTION_FAILED"));
    }
    buffer.truncate(length as usize);
    Ok(PathBuf::from(std::ffi::OsString::from_vec(buffer)))
}

#[cfg(target_os = "macos")]
fn process_uid(pid: u32) -> Result<Option<u32>> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let expected = std::mem::size_of::<libc::proc_bsdinfo>();
    let length = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            expected as i32,
        )
    };
    if length != expected as i32 {
        if !process_exists(pid) {
            return Ok(None);
        }
        return Err(process_error("GATEWAY_PROCESS_INSPECTION_FAILED"));
    }
    Ok(Some(unsafe { info.assume_init() }.pbi_uid))
}

#[cfg(target_os = "macos")]
fn process_parent_pid(pid: u32) -> Result<Option<u32>> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let expected = std::mem::size_of::<libc::proc_bsdinfo>();
    let length = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            expected as i32,
        )
    };
    if length != expected as i32 {
        if !process_exists(pid) {
            return Ok(None);
        }
        return Err(process_error("GATEWAY_PROCESS_INSPECTION_FAILED"));
    }
    Ok(Some(unsafe { info.assume_init() }.pbi_ppid))
}

#[cfg(target_os = "macos")]
fn process_is_running(pid: u32) -> Result<bool> {
    Ok(process_status(pid)?.is_some_and(|status| status != libc::SZOMB))
}

#[cfg(target_os = "macos")]
fn process_status(pid: u32) -> Result<Option<u32>> {
    let mut mib = [
        libc::CTL_KERN,
        libc::KERN_PROC,
        libc::KERN_PROC_PID,
        pid as i32,
    ];
    let mut buffer = [0_usize; 512];
    let mut length = std::mem::size_of_val(&buffer);
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as u32,
            buffer.as_mut_ptr().cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 {
        return if !process_exists(pid) {
            Ok(None)
        } else {
            Err(process_error("GATEWAY_PROCESS_INSPECTION_FAILED"))
        };
    }
    if length == 0 {
        return Ok(None);
    }
    if length < std::mem::size_of::<MacProcessPrefix>() {
        return Err(process_error("GATEWAY_PROCESS_INSPECTION_FAILED"));
    }
    let process = unsafe { &*buffer.as_ptr().cast::<MacProcessPrefix>() };
    Ok(Some(process.status as u32))
}

#[cfg(target_os = "macos")]
#[repr(C)]
union MacProcessStart {
    links: [*mut libc::c_void; 2],
    started_at: libc::timeval,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct MacProcessPrefix {
    start: MacProcessStart,
    vmspace: *mut libc::c_void,
    signal_actions: *mut libc::c_void,
    flags: libc::c_int,
    status: libc::c_char,
}

#[cfg(target_os = "macos")]
fn process_exists(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(target_os = "macos")]
fn process_error(code: &str) -> AppError {
    AppError::new(code, "Gateway process operation failed [REDACTED]")
}

#[cfg(not(target_os = "macos"))]
fn unsupported_platform() -> AppError {
    AppError::new(
        "UNSUPPORTED_REMOTE_PROVIDER_PLATFORM",
        "Gateway recovery requires exact-tested macOS",
    )
}

#[cfg(target_os = "macos")]
use std::os::unix::ffi::OsStringExt;
