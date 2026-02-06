# OPS CLI

OPS is a command-line tool for managing cloud deployments. It handles server initialization, app deployment via Docker Compose, SSH access, multi-region node groups, and more.

## Quick Install

```bash
curl -fsSL https://get.ops.autos | sh
```

## What OPS Does

- **Register servers as nodes** with automatic DNS (`<id>.node.ops.autos`)
- **Deploy apps** using `ops.toml` configuration and Docker Compose
- **SSH into servers** without managing keys manually
- **Multi-region deployments** with load balancing (round-robin, geo, weighted, failover)
- **Environment variable management** across servers
- **Monitoring daemon** with container metrics and log streaming

## Getting Started

1. [Installation](getting-started/installation.md)
2. [Quickstart](getting-started/quickstart.md)
3. [Configuration](getting-started/configuration.md)

## Command Reference

See the full [command reference](commands/) for all available commands.
