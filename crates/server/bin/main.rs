//! cydonia-server — see the `cydonia_server` library.

use std::process::ExitCode;

use cydonia_server::{Command, NAME};

fn main() -> ExitCode {
    let Some(command) = Command::parse(std::env::args_os().skip(1)) else {
        eprintln!("usage: {NAME} version | proxy start | proxy reconnect | daemon");
        return ExitCode::FAILURE;
    };
    run(command)
}

#[cfg(unix)]
fn run(command: Command) -> ExitCode {
    command.run()
}

#[cfg(not(unix))]
fn run(_: Command) -> ExitCode {
    eprintln!("{NAME} runs on Unix only");
    ExitCode::FAILURE
}
