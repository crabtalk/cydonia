use futures::{
    SinkExt,
    channel::{mpsc, oneshot},
};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    io::{Read, Write},
    path::Path,
    sync::mpsc as channel,
};

/// Dropping the panel's owner terminates the shell; a waiter reaps it off-thread.
pub struct Process {
    pub pid: Option<u32>,
    pub master: Box<dyn MasterPty + Send>,
    pub input: channel::Sender<Vec<u8>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    pub exit: Option<oneshot::Receiver<anyhow::Result<portable_pty::ExitStatus>>>,
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.killer.kill();
    }
}

impl Process {
    /// Unit tests get no shell: its reader thread would wake the pump from
    /// outside gpui's test scheduler, which fails the test as nondeterministic.
    #[cfg(test)]
    pub(crate) fn shell(_: &Path) -> anyhow::Result<(Self, mpsc::Receiver<Vec<u8>>)> {
        anyhow::bail!("no shell under test")
    }

    #[cfg(not(test))]
    pub(crate) fn shell(cwd: &Path) -> anyhow::Result<(Self, mpsc::Receiver<Vec<u8>>)> {
        let shell = std::env::var_os("SHELL").filter(|s| !s.is_empty());
        // `$SHELL` is unset on Windows unless something like Git Bash put it
        // there, and `-l` is a unix shell's flag.
        #[cfg(windows)]
        let command = CommandBuilder::new(shell.unwrap_or_else(|| "powershell.exe".into()));
        #[cfg(not(windows))]
        let command = {
            let mut command = CommandBuilder::new(shell.unwrap_or_else(|| "/bin/zsh".into()));
            command.arg("-l");
            command
        };
        Self::spawn(cwd, command)
    }

    pub fn spawn(
        cwd: &Path,
        mut command: CommandBuilder,
    ) -> anyhow::Result<(Self, mpsc::Receiver<Vec<u8>>)> {
        let pair = native_pty_system().openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        // What a program reads to know which terminal it is talking to.
        command.env("TERM_PROGRAM", "cydonia");
        command.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
        // The grid draws kitty graphics, and this is the variable programs
        // sniff for them — the emulator also answers the protocol's own `a=q`
        // query, which the ones that ask get a truthful answer from. TERM stays
        // `xterm-256color`: `xterm-kitty` is a terminfo entry that only exists
        // on a machine with kitty installed.
        command.env("KITTY_WINDOW_ID", "1");
        let mut child = pair.slave.spawn_command(command)?;
        let pid = child.process_id();
        let killer = child.clone_killer();
        drop(pair.slave);
        let (exited, exit) = oneshot::channel();
        std::thread::spawn(move || {
            let _ = exited.send(child.wait().map_err(Into::into));
        });
        let (input, pending) = channel::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            for bytes in pending {
                if writer
                    .write_all(&bytes)
                    .and_then(|_| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        // Bound output so a noisy command cannot outgrow the UI's consumer.
        let (mut output, receiver) = mpsc::channel(32);
        std::thread::spawn(move || {
            let mut bytes = [0; 16384];
            loop {
                match reader.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(n) => {
                        if futures::executor::block_on(output.send(bytes[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        });
        Ok((
            Self {
                pid,
                master: pair.master,
                input,
                killer,
                exit: Some(exit),
            },
            receiver,
        ))
    }
}

#[cfg(target_os = "macos")]
pub fn directory(pid: u32) -> Option<std::path::PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let mut info = std::mem::MaybeUninit::<libc::proc_vnodepathinfo>::uninit();
    let size = std::mem::size_of_val(&info) as i32;
    // The kernel initializes the full structure on a successful, full-size read.
    let read = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if read != size {
        return None;
    }
    let info = unsafe { info.assume_init() };
    let bytes: Vec<u8> = info
        .pvi_cdir
        .vip_path
        .iter()
        .flatten()
        .map(|byte| *byte as u8)
        .take_while(|byte| *byte != 0)
        .collect();
    if bytes.is_empty() {
        return None;
    }
    Some(std::ffi::OsString::from_vec(bytes).into())
}

#[cfg(target_os = "linux")]
pub fn directory(pid: u32) -> Option<std::path::PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn directory(_: u32) -> Option<std::path::PathBuf> {
    None
}
