use crate::services::error::{AppError, Result};
use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::process::Command;

pub const GATEWAY_LISTENER_FD_ENV: &str = "LAM_GATEWAY_LISTENER_FD";
const GATEWAY_CHILD_LISTENER_FD: i32 = 3;

pub fn validate_gateway_listener(listener: &TcpListener, expected_port: u16) -> Result<()> {
    let address = listener.local_addr().map_err(|_| listener_invalid())?;
    if address.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) || address.port() != expected_port {
        return Err(listener_invalid());
    }
    Ok(())
}

#[cfg(unix)]
pub fn configure_listener_handoff(command: &mut Command, listener: &TcpListener) -> Result<()> {
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;

    let source_fd = listener.as_raw_fd();
    if source_fd < 0 {
        return Err(listener_invalid());
    }
    command.env(
        GATEWAY_LISTENER_FD_ENV,
        GATEWAY_CHILD_LISTENER_FD.to_string(),
    );
    unsafe {
        command.pre_exec(move || install_child_listener_fd(source_fd));
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn configure_listener_handoff(_command: &mut Command, _listener: &TcpListener) -> Result<()> {
    Err(listener_unsupported())
}

#[cfg(unix)]
pub fn take_inherited_gateway_listener(expected_port: u16) -> Result<TcpListener> {
    use std::os::fd::FromRawFd;

    let fd = inherited_listener_fd()?;
    validate_stream_socket(fd)?;
    let listener = unsafe { TcpListener::from_raw_fd(fd) };
    validate_gateway_listener(&listener, expected_port)?;
    listener
        .set_nonblocking(true)
        .map_err(|_| listener_invalid())?;
    Ok(listener)
}

#[cfg(not(unix))]
pub fn take_inherited_gateway_listener(_expected_port: u16) -> Result<TcpListener> {
    Err(listener_unsupported())
}

#[cfg(unix)]
fn install_child_listener_fd(source_fd: i32) -> std::io::Result<()> {
    if source_fd != GATEWAY_CHILD_LISTENER_FD
        && unsafe { libc::dup2(source_fd, GATEWAY_CHILD_LISTENER_FD) } < 0
    {
        return Err(std::io::Error::last_os_error());
    }
    let flags = unsafe { libc::fcntl(GATEWAY_CHILD_LISTENER_FD, libc::F_GETFD) };
    if flags < 0
        || unsafe {
            libc::fcntl(
                GATEWAY_CHILD_LISTENER_FD,
                libc::F_SETFD,
                flags & !libc::FD_CLOEXEC,
            )
        } < 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn inherited_listener_fd() -> Result<i32> {
    let value = std::env::var(GATEWAY_LISTENER_FD_ENV).map_err(|_| listener_missing())?;
    let fd = value.parse::<i32>().map_err(|_| listener_invalid())?;
    if fd < 3 || unsafe { libc::fcntl(fd, libc::F_GETFD) } < 0 {
        return Err(listener_invalid());
    }
    Ok(fd)
}

#[cfg(unix)]
fn validate_stream_socket(fd: i32) -> Result<()> {
    let mut socket_type = 0;
    let mut length = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&mut socket_type as *mut libc::c_int).cast(),
            &mut length,
        )
    };
    if result != 0 || socket_type != libc::SOCK_STREAM {
        return Err(listener_invalid());
    }
    Ok(())
}

fn listener_missing() -> AppError {
    AppError::new(
        "GATEWAY_LISTENER_MISSING",
        "Gateway inherited listener is unavailable",
    )
}

fn listener_invalid() -> AppError {
    AppError::new(
        "GATEWAY_LISTENER_IDENTITY_INVALID",
        "Gateway inherited listener identity is invalid",
    )
}

#[cfg(not(unix))]
fn listener_unsupported() -> AppError {
    AppError::new(
        "UNSUPPORTED_REMOTE_PROVIDER_PLATFORM",
        "Gateway listener handoff requires a supported Unix platform",
    )
}
