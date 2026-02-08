# Build

## build

Remote build on a persistent build node. Connects to a dedicated build server via SSH, syncs code, runs your build command, and optionally builds & pushes Docker images to a registry.

```bash
ops build [OPTIONS]
```

**Options:**

| Option          | Default    | Description                              |
| --------------- | ---------- | ---------------------------------------- |
| `-f, --file`    | `ops.toml` | Path to config file                      |
| `--ref`         |            | Git ref to build (commit SHA, branch, or tag) |
| `-s, --service` |            | Only build a specific service image      |
| `-t, --tag`     | `latest`   | Docker image tag                         |
| `--no-push`     |            | Skip pushing images to registry          |
| `-j, --jobs`    | `5`        | Number of parallel image builds          |

**Build steps:**

1. Read `[build]` section from `ops.toml`
2. Connect to build node via SSH
3. Sync code to build node (`git clone/pull` or `rsync`)
4. Run build command (e.g., `cargo build --release`)
5. Build Docker images (parallel when `-j > 1`)
6. Push images to container registry

**Build node resolution:**

The build node is resolved in this order:
1. `build.node` in ops.toml (explicit node ID)
2. `target` in ops.toml
3. Auto-lookup via API using `project` name

### ops.toml configuration

```toml
[build]
node = 4                           # Build node ID (optional)
path = "/opt/builds/my-project"    # Remote build directory
command = "cargo build --release"  # Build command
source = "git"                     # "git" or "push"
branch = "main"                    # Default branch (optional)

[build.git]
repo = "git@github.com:org/repo"   # Git repository URL
ssh_key = "~/.ssh/deploy_key"      # SSH key for git auth (optional)
token = "$GITHUB_TOKEN"            # HTTPS token, supports $ENV_VAR (optional)

[build.image]
dockerfile = "Dockerfile.prod"     # Dockerfile path
registry = "ghcr.io"              # Container registry
token = "$GHCR_PAT"               # Registry auth token (supports $ENV_VAR)
username = "oauth2"                # Registry username
prefix = "ghcr.io/org/project"    # Image name prefix
binary_arg = "SERVICE_BINARY"     # Dockerfile ARG name for service binary
services = ["api", "worker", "web"] # Services to build
```

**Examples:**

```bash
# Build everything (default)
ops build

# Build a specific git ref
ops build --ref v1.2.0

# Build only one service
ops build --service api

# Build with custom tag, don't push
ops build --tag abc123 --no-push

# Build with 3 parallel jobs
ops build -j 3

# Use custom config file
ops build -f ops.prod.toml
```
