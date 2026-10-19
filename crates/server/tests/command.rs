//! The binary's arguments, read back as the command they were written for.

use std::ffi::OsString;

use artifact::location::Location;
use cupola::client::session::Mode;
use cydonia_server::{Command, mcp::Binding};

#[test]
fn every_command_reads_back_from_its_arguments() {
    for command in [
        Command::Version,
        Command::Proxy(Mode::Start),
        Command::Proxy(Mode::Reconnect),
        Command::Daemon,
        Command::Mcp(Binding::default()),
        Command::Mcp(Binding {
            project: Some(Location::parse("/home/me/my project")),
            session: Some("1700000000000".into()),
        }),
        Command::Mcp(Binding {
            project: Some(Location::parse("cupola://7/home/me/my project")),
            session: None,
        }),
        Command::Mcp(Binding {
            project: None,
            session: Some("1700000000000".into()),
        }),
    ] {
        assert_eq!(Command::parse(command.args()), Some(command));
    }
}

#[test]
fn anything_else_is_no_command() {
    for args in [
        &[][..],
        &["serve"],
        &["proxy"],
        &["proxy", "start", "now"],
        &["mcp", "--project"],
        &["mcp", "--project", "/a", "--project", "/b"],
        &["mcp", "--cwd", "/a"],
    ] {
        assert_eq!(Command::parse(args.iter().map(OsString::from)), None);
    }
}
