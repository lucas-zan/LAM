use localagentmanager_core::gateway::listener_handoff::{
    configure_listener_handoff, take_inherited_gateway_listener, validate_gateway_listener,
    GATEWAY_LISTENER_FD_ENV,
};
use std::net::{Ipv4Addr, TcpListener};
use std::process::{Command, Stdio};

#[test]
fn listener_handoff_accepts_only_the_expected_ipv4_loopback_port() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    validate_gateway_listener(&listener, port).unwrap();
    assert_eq!(
        validate_gateway_listener(&listener, port.saturating_add(1))
            .unwrap_err()
            .code,
        "GATEWAY_LISTENER_IDENTITY_INVALID"
    );

    let wildcard = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
    let wildcard_port = wildcard.local_addr().unwrap().port();
    assert_eq!(
        validate_gateway_listener(&wildcard, wildcard_port)
            .unwrap_err()
            .code,
        "GATEWAY_LISTENER_IDENTITY_INVALID"
    );
}

#[cfg(unix)]
#[test]
fn command_handoff_uses_a_bounded_child_fd_contract() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let mut command = Command::new("/usr/bin/true");
    configure_listener_handoff(&mut command, &listener).unwrap();

    let value = command
        .get_envs()
        .find_map(|(name, value)| (name == GATEWAY_LISTENER_FD_ENV).then_some(value))
        .flatten()
        .unwrap();
    assert_eq!(value, "3");
}

#[cfg(unix)]
#[test]
fn configured_child_process_inherits_the_reserved_listener() {
    const CHILD_ENV: &str = "LAM_LISTENER_HANDOFF_TEST_CHILD";
    const PORT_ENV: &str = "LAM_LISTENER_HANDOFF_TEST_PORT";
    if std::env::var_os(CHILD_ENV).is_some() {
        let port = std::env::var(PORT_ENV).unwrap().parse::<u16>().unwrap();
        let listener = take_inherited_gateway_listener(port).unwrap();
        println!("inherited_port={}", listener.local_addr().unwrap().port());
        return;
    }

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg("configured_child_process_inherits_the_reserved_listener")
        .arg("--nocapture")
        .env(CHILD_ENV, "1")
        .env(PORT_ENV, port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_listener_handoff(&mut command, &listener).unwrap();

    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(&format!("inherited_port={port}")));
}

#[cfg(unix)]
#[test]
fn invalid_inherited_fd_is_rejected_with_a_stable_error() {
    const CHILD_ENV: &str = "LAM_LISTENER_HANDOFF_INVALID_CHILD";
    if std::env::var_os(CHILD_ENV).is_some() {
        assert_eq!(
            take_inherited_gateway_listener(54_321).unwrap_err().code,
            "GATEWAY_LISTENER_IDENTITY_INVALID"
        );
        return;
    }

    let output = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("invalid_inherited_fd_is_rejected_with_a_stable_error")
        .arg("--nocapture")
        .env(CHILD_ENV, "1")
        .env(GATEWAY_LISTENER_FD_ENV, "999999")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
