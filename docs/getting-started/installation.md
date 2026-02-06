# Installation

## One-Line Install (Recommended)

```bash
curl -fsSL https://get.ops.autos | sh
```

This detects your OS and architecture automatically and installs the `ops` binary to `/usr/local/bin`.

**Supported platforms:**

| OS    | Architecture       |
| ----- | ------------------ |
| Linux | x86\_64, arm64     |
| macOS | x86\_64 (Intel), arm64 (Apple Silicon) |

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
