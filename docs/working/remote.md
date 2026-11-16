# Remote projects

Remote projects use SSH to reach `cydonia-server`. The transport uses the system
OpenSSH client, including SSH config aliases, keys, agent, and known hosts. The
remote shell must understand POSIX shell syntax. Supported hosts are Linux and
macOS on x86_64 or aarch64.

## Testing without a desktop

Run the store, files, spawner, and recovery integration tests with:

```sh
cargo nextest run -p cydonia-gui -E 'binary(/^remote_/)'
```

For actual SSH, install OpenSSH client and server, Python 3, and cargo-nextest,
then run:

```sh
scripts/test-remote-ssh
```

The script starts an unprivileged SSH server on loopback with a temporary host
key, client identity, known-hosts file, and home directory. It never edits your
SSH configuration or authorized keys. The test uses the actual ControlMaster,
server upload, proxy, and daemon. It checks files, git, project writes, process
execution, and an MCP call from a process on the host. The daemon and SSH server
are stopped and their temporary files removed afterward. On Linux, the system
OpenSSH privilege-separation directory (`/run/sshd`) must already exist.

Reopening a project after its host session has ended refreshes the existing
store and replaces the cached spawner. Ordinary link loss reconnects within the
transport; daemon replacement refills project replicas. Processes from a replaced
daemon cannot be resumed.
