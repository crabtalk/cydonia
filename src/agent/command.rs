//! The command a settings entry names, as this platform can run it.
//!
//! `command = "npx"` is what every default entry says, and on macOS and Linux
//! that is the whole story: `execvp` walks `PATH` and finds the executable.
//! Windows has no such lookup behind `CreateProcess`. npm installs `npx` as
//! `npx.cmd`, a batch script, and given the bare word Windows looks for
//! `npx.exe` and nothing else — so the settings file that starts an agent on
//! a Mac reports "program not found" here. What this module does is the
//! lookup the shell would have done: `PATH` walked with `PATHEXT`, the way
//! `cmd.exe` resolves a word, so an entry stays a word rather than a path with
//! an extension nobody on macOS would write.
//!
//! Two more things a Windows spawn needs and a Unix one does not:
//!
//! * no console. A GUI process has none, and a console child started from one
//!   is given a fresh window of its own — a black box flashing up behind the
//!   app for every agent. `CREATE_NO_WINDOW` keeps it off screen.
//! * a way to end the agent. Killing `npx.cmd` kills `cmd.exe`, and the node
//!   process it started carries on without it: Windows has no process groups
//!   to signal. A job object is the mechanism it does have — every process
//!   started under the agent joins it, and closing the job's handle ends all
//!   of them at once. See [`Job`].

use crate::model::settings;
use anyhow::{Result, anyhow};
use std::path::PathBuf;
use tokio::process::{Child, Command};

/// `entry` as a command ready to spawn: its arguments and environment on, and
/// — on Windows — its program resolved and its console kept off screen.
pub fn build(entry: &settings::Agent) -> Result<Command> {
    let program = resolve(&entry.command)?;
    let mut command = Command::new(program);
    command.args(&entry.args).envs(&entry.env);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    Ok(command)
}

/// What to hand `Command::new` for `program`.
///
/// Unchanged off Windows: `Command` already searches `PATH` there, and an
/// entry that names a path is run as written.
#[cfg(not(windows))]
pub fn resolve(program: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(program))
}

/// What to hand `Command::new` for `program`: the file `PATH` and `PATHEXT`
/// resolve it to, which is how `npx` becomes `npx.cmd`.
#[cfg(windows)]
pub fn resolve(program: &str) -> Result<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    find(program, &dirs, &extensions()).ok_or_else(|| missing(program))
}

/// `CreateProcess`'s flag for a child that gets no console window of its own.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The error for a program that is nowhere on `PATH`, saying what to install
/// where the program is one of node's — which is where every default entry
/// starts, and the one thing a fresh Windows machine is missing.
#[cfg(any(windows, test))]
fn missing(program: &str) -> anyhow::Error {
    let stem = std::path::Path::new(program)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match stem.as_str() {
        "npx" | "npm" | "node" => anyhow!("{program} was not found on PATH — install Node.js"),
        _ => anyhow!("{program} was not found on PATH"),
    }
}

/// The lookup itself, over the directories and extensions it is given. Split
/// from [`resolve`] so it can be exercised without touching the process
/// environment.
///
/// A program named with a directory in it is looked for where it says and
/// nowhere else. A bare word is looked for in each directory in turn, and the
/// first directory that has it wins, whichever extension it has it under —
/// the order `cmd.exe` uses, so `npx` finds the same `npx.cmd` a terminal
/// would.
///
/// Extensions are tried before the bare name in both cases. A file on disk
/// under the bare name is a script for a shell this machine may not have:
/// npm's `.bin/<name>` is a POSIX one, kept beside the `<name>.cmd` meant for
/// here, and the installer records the bare path on every platform.
#[cfg(any(windows, test))]
pub(crate) fn find(program: &str, dirs: &[PathBuf], exts: &[String]) -> Option<PathBuf> {
    let given = std::path::Path::new(program);
    if given.is_absolute() || given.components().count() > 1 {
        return found(given, exts);
    }
    dirs.iter().find_map(|dir| found(&dir.join(program), exts))
}

/// `base` on disk under one of `exts`, or under its own name.
#[cfg(any(windows, test))]
fn found(base: &std::path::Path, exts: &[String]) -> Option<PathBuf> {
    let name = base.as_os_str().to_string_lossy().to_ascii_lowercase();
    if exts.iter().any(|ext| name.ends_with(ext.as_str())) && base.is_file() {
        return Some(base.to_path_buf());
    }
    exts.iter()
        .map(|ext| {
            let mut with = base.as_os_str().to_owned();
            with.push(ext);
            PathBuf::from(with)
        })
        .chain(std::iter::once(base.to_path_buf()))
        .find(|candidate| candidate.is_file())
}

/// `PATHEXT` as the shell reads it — `.COM;.EXE;.BAT;.CMD;…` — or that
/// default where it is unset. Lowercased: the filesystem keeps no case, and
/// neither does the comparison in [`found`].
#[cfg(windows)]
fn extensions() -> Vec<String> {
    std::env::var("PATHEXT")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_owned())
        .split(';')
        .filter(|ext| ext.starts_with('.'))
        .map(str::to_ascii_lowercase)
        .collect()
}

/// The agent's process tree, ended together when this is dropped — see the
/// module note. Off Windows there is nothing to hold: the process is killed
/// under `kill_on_drop`, and whatever it started reads end-of-file on the
/// pipe that went with it.
#[cfg_attr(not(windows), allow(dead_code))]
pub struct Job {
    /// Held for its `Drop`, which is what ends the job; never read.
    #[cfg(windows)]
    _handle: job::Handle,
}

/// Put `child`, and everything it goes on to start, in a job of its own.
///
/// `None` where the platform has no jobs, or where Windows would not give
/// one: the agent still runs, and the fallback is the `kill_on_drop` cacp
/// already arms on the process it spawned.
pub fn adopt(child: &Child) -> Option<Job> {
    #[cfg(windows)]
    {
        let process = child.raw_handle()?;
        job::Handle::adopt(process).map(|handle| Job { _handle: handle })
    }
    #[cfg(not(windows))]
    {
        let _ = child;
        None
    }
}

/// The four kernel32 calls a kill-on-close job takes, declared here rather
/// than through a bindings crate: cydonia's own dependencies name none, and
/// what a job needs is four functions and one struct whose layout has not
/// moved since Windows 2000.
#[cfg(windows)]
mod job {
    use std::{ffi::c_void, os::windows::io::RawHandle, ptr};

    type Raw = *mut c_void;

    /// `JobObjectExtendedLimitInformation`, the class the kill flag lives in.
    const EXTENDED_LIMIT_INFORMATION: i32 = 9;
    /// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`: closing the last handle to the
    /// job terminates every process in it.
    const KILL_ON_JOB_CLOSE: u32 = 0x2000;

    /// `JOBOBJECT_BASIC_LIMIT_INFORMATION`.
    #[repr(C)]
    struct BasicLimit {
        per_process_user_time: i64,
        per_job_user_time: i64,
        limit_flags: u32,
        minimum_working_set: usize,
        maximum_working_set: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    /// `IO_COUNTERS`.
    #[repr(C)]
    struct IoCounters {
        read_operations: u64,
        write_operations: u64,
        other_operations: u64,
        read_bytes: u64,
        write_bytes: u64,
        other_bytes: u64,
    }

    /// `JOBOBJECT_EXTENDED_LIMIT_INFORMATION`.
    #[repr(C)]
    struct ExtendedLimit {
        basic: BasicLimit,
        io: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Raw;
        fn SetInformationJobObject(job: Raw, class: i32, info: *const c_void, len: u32) -> i32;
        fn AssignProcessToJobObject(job: Raw, process: Raw) -> i32;
        fn CloseHandle(handle: Raw) -> i32;
    }

    /// An owned job handle. Closed on drop, which is what ends the job.
    pub struct Handle(Raw);

    // A job handle is a kernel object with no thread affinity: any thread may
    // hold it, and any thread may close it.
    unsafe impl Send for Handle {}
    unsafe impl Sync for Handle {}

    impl Handle {
        /// A kill-on-close job with `process` in it, or nothing if any of the
        /// three steps was refused — a process already in a job that forbids
        /// nesting, on a Windows older than 8, is the usual reason.
        pub fn adopt(process: RawHandle) -> Option<Self> {
            // SAFETY: every pointer handed across is either null where the
            // API allows null, or points at a struct that lives for the call.
            // The handle returned is owned by `Self` and closed exactly once.
            unsafe {
                let raw = CreateJobObjectW(ptr::null(), ptr::null());
                if raw.is_null() {
                    return None;
                }
                let job = Self(raw);
                let mut info: ExtendedLimit = std::mem::zeroed();
                info.basic.limit_flags = KILL_ON_JOB_CLOSE;
                let set = SetInformationJobObject(
                    job.0,
                    EXTENDED_LIMIT_INFORMATION,
                    ptr::from_ref(&info).cast(),
                    std::mem::size_of::<ExtendedLimit>() as u32,
                );
                if set == 0 {
                    return None;
                }
                if AssignProcessToJobObject(job.0, process.cast()) == 0 {
                    return None;
                }
                Some(job)
            }
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a handle this struct owns and has not closed.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of files under the names given, gone when the guard is.
    fn scratch(tag: &str, files: &[&str]) -> (PathBuf, impl Drop) {
        let dir =
            std::env::temp_dir().join(format!("cydonia-command-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for file in files {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"").unwrap();
        }
        struct Guard(PathBuf);
        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        (dir.clone(), Guard(dir))
    }

    fn exts() -> Vec<String> {
        [".com", ".exe", ".bat", ".cmd"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    /// The case the module exists for: `npx` in settings, `npx.cmd` on disk.
    #[test]
    fn a_word_finds_its_cmd_on_path() {
        let (dir, _guard) = scratch("word", &["npx.cmd", "npx"]);
        let found = find("npx", std::slice::from_ref(&dir), &exts()).unwrap();
        assert_eq!(found, dir.join("npx.cmd"));
    }

    /// `PATHEXT` order decides between two extensions in one directory, as it
    /// does for `cmd.exe`.
    #[test]
    fn pathext_order_wins_within_a_directory() {
        let (dir, _guard) = scratch("order", &["tool.cmd", "tool.exe"]);
        let found = find("tool", std::slice::from_ref(&dir), &exts()).unwrap();
        assert_eq!(found, dir.join("tool.exe"));
    }

    /// The first directory with any match wins over a later one with a
    /// "better" extension — the search is by directory, then by extension.
    #[test]
    fn the_first_directory_wins() {
        let (dir, _guard) = scratch("dirs", &["first/tool.cmd", "second/tool.exe"]);
        let dirs = [dir.join("first"), dir.join("second")];
        let found = find("tool", &dirs, &exts()).unwrap();
        assert_eq!(found, dirs[0].join("tool.cmd"));
    }

    /// An installed agent's recorded path is npm's POSIX shim. Next to it is
    /// the `.cmd` that runs here, and that is what the path resolves to.
    #[test]
    fn a_path_prefers_the_cmd_beside_the_bare_script() {
        let (dir, _guard) = scratch(
            "shim",
            &["node_modules/.bin/agent", "node_modules/.bin/agent.cmd"],
        );
        let bin = dir.join("node_modules").join(".bin");
        let bare = bin.join("agent");
        let found = find(bare.to_str().unwrap(), &[], &exts()).unwrap();
        assert_eq!(found, bin.join("agent.cmd"));
    }

    /// A path with its extension already on it is taken as written.
    #[test]
    fn an_explicit_extension_is_kept() {
        let (dir, _guard) = scratch("explicit", &["agent.exe", "agent.exe.cmd"]);
        let given = dir.join("agent.exe");
        let found = find(given.to_str().unwrap(), &[], &exts()).unwrap();
        assert_eq!(found, given);
    }

    /// A path is looked for where it says, never on `PATH`.
    #[test]
    fn a_path_is_not_searched_for_on_path() {
        let (dir, _guard) = scratch("path", &["bin/tool.exe"]);
        let elsewhere = dir.join("other").join("tool");
        let on_path = [dir.join("bin")];
        assert_eq!(find(elsewhere.to_str().unwrap(), &on_path, &exts()), None);
        assert_eq!(find("missing", &on_path, &exts()), None);
    }

    /// The hint names Node where that is what is missing, and nothing else
    /// where it is not.
    #[test]
    fn missing_node_says_so() {
        assert!(missing("npx").to_string().contains("Node.js"));
        assert!(missing("npx.cmd").to_string().contains("Node.js"));
        assert!(!missing("gemini").to_string().contains("Node.js"));
    }
}
