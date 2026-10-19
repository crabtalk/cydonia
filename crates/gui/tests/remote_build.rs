//! How cydonia-server is built for a remote host from a checkout, and how
//! the delegate names and runs it. Nothing is built here.

#![cfg(unix)]

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use cupola::client::{
    platform::{Arch, Os, Platform},
    session::Mode,
    ssh::Delegate as _,
};
use cydonia_gui::model::remote::{Build, Delegate};
use futures::StreamExt as _;

fn line(command: &Command) -> Vec<String> {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|part| part.to_string_lossy().into_owned())
        .collect()
}

fn checkout() -> &'static Path {
    Path::new("/src/cydonia")
}

#[test]
fn linux_is_built_for_musl_with_zigbuild() {
    for (arch, triple) in [
        (Arch::X86_64, "x86_64-unknown-linux-musl"),
        (Arch::Aarch64, "aarch64-unknown-linux-musl"),
    ] {
        let build = Build::new(
            checkout(),
            Platform {
                os: Os::Linux,
                arch,
            },
        )
        .unwrap();
        let command = build.command();
        assert_eq!(command.get_current_dir(), Some(checkout()));
        assert_eq!(
            line(&command),
            [
                "cargo",
                "zigbuild",
                "--release",
                "--target",
                triple,
                "-p",
                "cydonia-server",
                "--target-dir",
                "/src/cydonia/target/cydonia-server",
            ]
        );
        assert_eq!(
            build.binary(),
            PathBuf::from("/src/cydonia/target/cydonia-server")
                .join(triple)
                .join("release/cydonia-server")
        );

        let setup = build.setup();
        assert_eq!(setup.len(), 2);
        assert_eq!(
            line(setup[0].unless.as_ref().unwrap()),
            ["cargo", "zigbuild", "--version"]
        );
        assert_eq!(
            line(&setup[0].run),
            ["cargo", "install", "--locked", "cargo-zigbuild"]
        );
        assert!(setup[1].unless.is_none());
        assert_eq!(line(&setup[1].run), ["rustup", "target", "add", triple]);
    }
}

#[test]
fn a_mac_of_this_arch_gets_this_machines_build() {
    let Some(local) = Platform::local().filter(|platform| platform.os == Os::MacOs) else {
        return;
    };
    let build = Build::new(checkout(), local).unwrap();
    assert_eq!(
        line(&build.command()),
        [
            "cargo",
            "build",
            "-p",
            "cydonia-server",
            "--target-dir",
            "/src/cydonia/target/cydonia-server",
        ]
    );
    assert_eq!(
        build.binary(),
        Path::new("/src/cydonia/target/cydonia-server/debug/cydonia-server")
    );
    assert!(build.setup().is_empty());

    let other = Platform {
        os: Os::MacOs,
        arch: match local.arch {
            Arch::X86_64 => Arch::Aarch64,
            Arch::Aarch64 => Arch::X86_64,
        },
    };
    assert!(Build::new(checkout(), other).is_err());
}

#[test]
fn the_delegate_runs_the_server_by_its_own_commands() {
    let (delegate, _prompts) = Delegate::new();
    assert_eq!(delegate.name(), server::NAME);
    assert_eq!(delegate.version(), env!("CARGO_PKG_VERSION"));
    for (args, command) in [
        (delegate.version_args(), server::Command::Version),
        (
            delegate.proxy_args(Mode::Start),
            server::Command::Proxy(Mode::Start),
        ),
        (
            delegate.proxy_args(Mode::Reconnect),
            server::Command::Proxy(Mode::Reconnect),
        ),
    ] {
        assert_eq!(server::Command::parse(args), Some(command));
    }
}

#[test]
fn a_prompt_is_answered_through_the_channel() {
    let (delegate, mut prompts) = Delegate::new();
    let asked = delegate.ask("Password:".into());
    let answered = std::thread::spawn(move || futures::executor::block_on(asked));
    let prompt = futures::executor::block_on(prompts.next()).unwrap();
    assert_eq!(prompt.text, "Password:");
    prompt.answer.send(Some("hunter2".into())).unwrap();
    assert_eq!(answered.join().unwrap().as_deref(), Some("hunter2"));

    let cancelled = delegate.ask("Verification code:".into());
    drop(futures::executor::block_on(prompts.next()).unwrap());
    assert_eq!(futures::executor::block_on(cancelled), None);

    drop(prompts);
    assert_eq!(
        futures::executor::block_on(delegate.ask("Password:".into())),
        None
    );
}
