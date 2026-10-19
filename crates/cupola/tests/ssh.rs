//! The command lines the ssh transport runs, built without a host.

#![cfg(unix)]

use std::{ffi::OsString, path::Path, process::Command};

use cydonia_cupola::{
    client::ssh::{Control, Host, ServerPath},
    exec,
};

fn control() -> Control {
    Control::new(
        Host::new("me@box").unwrap().port(2222),
        "/tmp/cupola-ssh/ssh.sock",
    )
}

fn line(command: &Command) -> (String, Vec<String>) {
    let program = command.get_program().to_string_lossy().into_owned();
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    (program, args)
}

fn shared() -> Vec<String> {
    [
        "-o",
        "ControlMaster=no",
        "-o",
        "ControlPath=/tmp/cupola-ssh/ssh.sock",
        "-p",
        "2222",
    ]
    .map(String::from)
    .to_vec()
}

#[test]
fn the_master_holds_the_connection_at_the_control_socket() {
    let (program, args) = line(&control().master());
    assert_eq!(program, "ssh");
    assert_eq!(
        args,
        [
            "-N",
            "-o",
            "ControlPersist=no",
            "-o",
            "ControlMaster=yes",
            "-o",
            "ControlPath=/tmp/cupola-ssh/ssh.sock",
            "-p",
            "2222",
            "me@box",
        ]
    );
}

#[test]
fn the_check_asks_the_master_at_the_control_socket() {
    let (program, args) = line(&control().check());
    assert_eq!(program, "ssh");
    assert_eq!(
        args,
        [
            "-o",
            "ControlPath=/tmp/cupola-ssh/ssh.sock",
            "-p",
            "2222",
            "-O",
            "check",
            "me@box",
        ]
    );
}

#[test]
fn build_command_runs_a_program_at_a_directory_with_a_terminal() {
    let mut command = exec::Command::new("git");
    command.args = vec!["log".into(), "--format=%h %s".into()];
    command.cwd = Some("/srv/my project".into());
    command.env = vec![("PAGER".into(), "cat".into())];
    let (program, args) = line(&control().build_command(&command));
    assert_eq!(program, "ssh");
    let mut expected = shared();
    expected.extend(
        [
            "-o",
            "LogLevel=ERROR",
            "-t",
            "me@box",
            "cd && cd '/srv/my project' && exec env PAGER=cat git log '--format=%h %s'",
        ]
        .map(String::from),
    );
    assert_eq!(args, expected);
}

#[test]
fn build_command_without_a_directory_runs_at_home() {
    let command = exec::Command::new("ls");
    let (_, args) = line(&control().build_command(&command));
    assert_eq!(args.last().unwrap(), "cd && exec env ls");
}

#[test]
fn the_login_shell_starts_at_a_directory() {
    let (_, args) = line(&control().login_shell(Path::new("work/it's")));
    let mut expected = shared();
    expected.extend(
        [
            "-o",
            "LogLevel=ERROR",
            "-t",
            "me@box",
            r#"cd && cd 'work/it'\''s' && exec "$SHELL" -l"#,
        ]
        .map(String::from),
    );
    assert_eq!(args, expected);
}

#[test]
fn the_proxy_runs_the_versioned_server_on_stdio_without_a_terminal() {
    let server = ServerPath::new("cydonia-server", "0.2.3");
    for (proxy_args, script) in [
        (
            vec![OsString::from("proxy")],
            "cd && exec .cupola/cydonia-server-0.2.3 proxy",
        ),
        (
            vec![OsString::from("proxy"), OsString::from("--reconnect")],
            "cd && exec .cupola/cydonia-server-0.2.3 proxy --reconnect",
        ),
    ] {
        let (program, args) = line(&control().proxy(&server, &proxy_args));
        assert_eq!(program, "ssh");
        let mut expected = shared();
        expected.extend(["-T", "me@box", script].map(String::from));
        assert_eq!(args, expected);
    }
}

#[test]
fn sftp_and_scp_go_through_the_control_socket_with_their_port_flag() {
    let through = [
        "-o",
        "ControlMaster=no",
        "-o",
        "ControlPath=/tmp/cupola-ssh/ssh.sock",
        "-P",
        "2222",
    ];

    let (program, args) = line(&control().sftp());
    assert_eq!(program, "sftp");
    let mut expected = through.map(String::from).to_vec();
    expected.extend(["-b", "-", "me@box"].map(String::from));
    assert_eq!(args, expected);

    let (program, args) = line(&control().scp(
        Path::new("/build/server"),
        Path::new(".cupola/server.upload"),
    ));
    assert_eq!(program, "scp");
    let mut expected = through.map(String::from).to_vec();
    expected.extend(["/build/server", "me@box:.cupola/server.upload"].map(String::from));
    assert_eq!(args, expected);
}

#[test]
fn a_host_without_a_port_passes_none() {
    let control = Control::new(Host::new("box").unwrap(), "/s");
    let (_, args) = line(&control.master());
    assert!(!args.iter().any(|arg| arg == "-p"));
    assert_eq!(args.last().unwrap(), "box");
}

#[test]
fn a_destination_that_reads_as_an_option_is_refused() {
    assert!(Host::new("-oProxyCommand=evil").is_err());
    assert!(Host::new("").is_err());
}

#[test]
fn the_server_path_names_the_binary_and_its_version() {
    assert_eq!(
        ServerPath::new("cydonia-server", "0.2.3").as_path(),
        Path::new(".cupola/cydonia-server-0.2.3")
    );
    assert_eq!(
        ServerPath::new("cydonia-server", "0.3.0+dev/abc 1").as_path(),
        Path::new(".cupola/cydonia-server-0.3.0+dev_abc_1")
    );
}
