use cydonia_cupola::client::platform::{Arch, Os, Platform};

#[test]
fn uname_names_the_os_and_arch() {
    let cases = [
        ("Darwin arm64\n", Os::MacOs, Arch::Aarch64),
        ("Darwin x86_64", Os::MacOs, Arch::X86_64),
        ("Linux aarch64\n", Os::Linux, Arch::Aarch64),
        ("Linux armv8l", Os::Linux, Arch::Aarch64),
        ("Linux x86_64\n", Os::Linux, Arch::X86_64),
    ];
    for (output, os, arch) in cases {
        assert_eq!(
            Platform::from_uname(output).unwrap(),
            Platform { os, arch },
            "{output:?}"
        );
    }
}

#[test]
fn what_the_shell_printed_before_uname_is_skipped() {
    let output = "Welcome to box\nLast login: today\nLinux x86_64\n";
    assert_eq!(
        Platform::from_uname(output).unwrap(),
        Platform {
            os: Os::Linux,
            arch: Arch::X86_64
        }
    );
}

#[test]
fn an_unsupported_platform_is_refused() {
    for output in ["FreeBSD amd64", "Linux armv7l", "Linux i686", "garbage"] {
        assert!(Platform::from_uname(output).is_err(), "{output:?}");
    }
}

#[test]
fn a_platform_displays_as_os_and_arch() {
    let platform = Platform {
        os: Os::MacOs,
        arch: Arch::Aarch64,
    };
    assert_eq!(platform.to_string(), "macos-aarch64");
}
