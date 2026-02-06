# ops.toml Schema Reference

Complete schema for the `ops.toml` deployment configuration file.

```toml
# App name (legacy single-app mode)
# One of `app` or `project` is required
app = "string"

# Project name (multi-app project mode)
project = "string"

# Deployment target in "app.project" format
# Optional in project mode (auto-resolved via API)
target = "string"

# Required: Remote directory to deploy to
deploy_path = "string"

# Required: Deployment configuration
[deploy]
# Source type: "git", "push", or "image"
# Default: "git"
source = "git"

# Git branch to deploy
# Default: "main"
branch = "main"

# Docker-compose file paths (optional)
# Used with all source types
compose_files = ["docker-compose.yml", "docker-compose.prod.yml"]

# Git configuration (required when source = "git")
[deploy.git]
# Git repository URL
repo = "git@github.com:org/repo.git"

# Path to SSH deploy key (optional)
# Supports ~ expansion
ssh_key = "~/.ssh/deploy_key"

# Container registry (optional, used with source = "image")
[deploy.registry]
# Registry URL
url = "ghcr.io"

# Auth token (supports $ENV_VAR syntax)
token = "$GHCR_PAT"

# Username (optional, defaults to "oauth2")
username = "oauth2"

# App group definitions (optional, repeatable)
# Used with `ops deploy --app <name>` to deploy a subset of services
[[apps]]
# Group name
name = "api"
# Docker-compose service names in this group
services = ["api_server", "api_worker"]

# Environment file mappings (optional, repeatable)
[[env_files]]
# Local file path
local = ".env.production"
# Remote path relative to deploy_path
remote = ".env"

# Directory sync mappings (optional, repeatable)
[[sync]]
# Local directory or file
local = "./configs"
# Remote path relative to deploy_path
remote = "configs"

# Nginx route definitions (optional, repeatable)
[[routes]]
# Domain name for nginx server_name
domain = "api.example.com"
# Backend port to proxy to
port = 3000
# Enable SSL via certbot
# Default: false
ssl = true

# Health checks (optional, repeatable)
[[healthchecks]]
# Display name
name = "API Health"
# URL to check (retried 10 times, 2s intervals)
url = "http://localhost:3000/health"
```

## Deploy Sources

### `git`

Clones the repository on first deploy, runs `git pull origin <branch>` on subsequent deploys. If `ssh_key` is specified, the key is uploaded to the server and configured in `~/.ssh/config` for GitHub access.

### `push`

Uses rsync to sync the current local directory to the remote server. Automatically excludes:

- `target/`
- `node_modules/`
- `.git/`
- `.env`
- `.env.deploy`

### `image`

Pulls pre-built container images from a registry. No local build step.

1. **Registry login** (if `[deploy.registry]` configured): runs `docker login` on the remote server using the provided token
2. **Pull**: runs `docker compose pull` with any `compose_files` and `--set` env vars
3. **Start**: runs `docker compose up -d --remove-orphans`
4. **Cleanup**: runs `docker image prune -f` to remove old images

Environment variables passed via `--set KEY=VALUE` are prepended to all docker compose commands.

## App Groups

When `[[apps]]` are defined, you can deploy a subset of services:

```bash
# Deploy only services in the "api" group
ops deploy --app api

# Deploy a single service directly (bypasses groups)
ops deploy --service api_server
```

If neither `--app` nor `--service` is specified, all services are deployed.

## Nginx Configuration

Each `[[routes]]` entry generates an nginx server block with:

- WebSocket upgrade support
- `X-Real-IP` and `X-Forwarded-For` headers
- Proxy buffering disabled
- Chunked transfer encoding
- 86400s read timeout

The config file is written to `/etc/nginx/sites-available/ops-<app>.conf` and symlinked to `sites-enabled/`.

When `ssl = true`, certbot is invoked with `--nginx --non-interactive`.
