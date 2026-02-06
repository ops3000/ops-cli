# ops.toml Schema Reference

Complete schema for the `ops.toml` deployment configuration file.

```toml
# Required: App name
app = "string"

# Required: Deployment target in "app.project" format
target = "string"

# Required: Remote directory to deploy to
deploy_path = "string"

# Required: Deployment configuration
[deploy]
# Source type: "git" or "push"
# Default: "git"
source = "git"

# Git branch to deploy
# Default: "main"
branch = "main"

# Git configuration (required when source = "git")
[deploy.git]
# Git repository URL
repo = "git@github.com:org/repo.git"

# Path to SSH deploy key (optional)
# Supports ~ expansion
ssh_key = "~/.ssh/deploy_key"

# Environment file mappings (optional, repeatable)
[[env_files]]
# Local file path
local = ".env.production"
# Remote path relative to deploy_path
remote = ".env"

# Directory sync mappings (optional, repeatable)
[[sync]]
# Local directory
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

## Nginx Configuration

Each `[[routes]]` entry generates an nginx server block with:

- WebSocket upgrade support
- `X-Real-IP` and `X-Forwarded-For` headers
- Proxy buffering disabled
- Chunked transfer encoding
- 86400s read timeout

The config file is written to `/etc/nginx/sites-available/ops-<app>.conf` and symlinked to `sites-enabled/`.

When `ssl = true`, certbot is invoked with `--nginx --non-interactive`.
