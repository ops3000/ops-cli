# Installation

## One-Line Install (Recommended)

**Linux / macOS:**

```bash
curl -fsSL https://get.ops.autos | sh
```

This detects your OS and architecture automatically and installs the `ops` binary to `/usr/local/bin`.

**Windows (PowerShell):**

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://get.ops.autos/install.ps1 | iex"
```

Installs `ops.exe` to `%LOCALAPPDATA%\ops\bin` (no admin required) and adds it to your user `PATH`. Requires Windows 10 1809+ (for the built-in OpenSSH client and `tar.exe`).

**Supported platforms:**

| OS      | Architecture       |
| ------- | ------------------ |
| Linux   | x86\_64, arm64     |
| macOS   | x86\_64 (Intel), arm64 (Apple Silicon) |
| Windows | x86\_64 (arm64 via x64 emulation) |

**Windows notes:**

- `ops ssh` / `ops scp` use the built-in Windows OpenSSH client (`ssh.exe` / `scp.exe`), included by default since Windows 10 1809.
- `ops push` and push-mode deploys require `rsync`, which Windows does not ship. Install it (e.g. `scoop install rsync`) or use WSL for push workflows.
- Windows machines can register as nodes (`ops init --tunnel` auto-detects your username as the SSH login user) and run the daemon in the background via `ops serve --install` (creates a Scheduled Task: runs at boot as SYSTEM, auto-restarts). Docker-based app deploys still target Linux nodes only.

## Manual Download

Download the binary from [GitHub Releases](https://github.com/ops3000/ops-cli/releases/latest):

```bash
# Example: macOS arm64
curl -L -o ops.tar.gz https://github.com/ops3000/ops-cli/releases/latest/download/ops-darwin-arm64.tar.gz
tar -xzf ops.tar.gz
sudo mv ops /usr/local/bin/
sudo chmod +x /usr/local/bin/ops
```

## Build from Source

Requires [Rust](https://rustup.rs/) (edition 2021).

```bash
git clone https://github.com/ops3000/ops-cli.git
cd ops-cli
cargo build --release
sudo cp target/release/ops /usr/local/bin/
```

## Verify Installation

```bash
ops version
```

## Update

OPS checks for updates automatically on every command. To update manually:

```bash
ops update
```
