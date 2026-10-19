//! The operating system and architecture a server binary is built for.

use std::fmt;

use anyhow::{Context as _, bail};

/// A remote host's operating system and architecture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Os {
    MacOs,
    Linux,
}

/// 64-bit architectures only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Arch {
    X86_64,
    Aarch64,
}

impl Platform {
    /// This machine's, unless it is one no server is built for.
    pub fn local() -> Option<Self> {
        let os = [Os::MacOs, Os::Linux]
            .into_iter()
            .find(|os| os.as_str() == std::env::consts::OS)?;
        let arch = [Arch::X86_64, Arch::Aarch64]
            .into_iter()
            .find(|arch| arch.as_str() == std::env::consts::ARCH)?;
        Some(Self { os, arch })
    }

    /// Reads the last line of `uname -sm` output; lines before it are what
    /// the remote shell printed on start.
    pub fn from_uname(output: &str) -> anyhow::Result<Self> {
        let output = output.trim();
        let uname = output.rsplit_once('\n').map_or(output, |(_, last)| last);
        let (os, arch) = uname
            .trim()
            .split_once(' ')
            .with_context(|| format!("unknown uname: {uname:?}"))?;
        let os = match os {
            "Darwin" => Os::MacOs,
            "Linux" => Os::Linux,
            _ => bail!("no server for the operating system {os:?}"),
        };
        let arch = if ["arm64", "aarch64", "armv8", "armv9"]
            .iter()
            .any(|prefix| arch.starts_with(prefix))
        {
            Arch::Aarch64
        } else if arch.starts_with("x86_64") || arch == "amd64" {
            Arch::X86_64
        } else {
            bail!("no server for the architecture {arch:?}")
        };
        Ok(Self { os, arch })
    }
}

impl Os {
    /// As `std::env::consts::OS` spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MacOs => "macos",
            Self::Linux => "linux",
        }
    }
}

impl Arch {
    /// As Rust's target triples and `std::env::consts::ARCH` spell it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.os.as_str(), self.arch.as_str())
    }
}
