# ops.toml Configuration

The `ops.toml` file defines how your app is deployed. Place it in the root of your project.

## Minimal Example

```toml
app = "api"
target = "api.my-saas"
deploy_path = "/opt/api"

[deploy]
source = "git"

[deploy.git]
repo = "git@github.com:myorg/api.git"
```

## Full Example

```toml
app = "api"
target = "api.my-saas"
deploy_path = "/opt/api"

[deploy]
source = "git"
branch = "main"

[deploy.git]
repo = "git@github.com:myorg/api.git"
ssh_key = "~/.ssh/deploy_key"

[[env_files]]
local = ".env.production"
remote = ".env"

[[env_files]]
local = ".env.db"
remote = "db/.env"

[[sync]]
local = "./configs"
remote = "configs"

[[routes]]
domain = "api.example.com"
port = 3000
ssl = true

[[routes]]
domain = "ws.example.com"
port = 3001
ssl = true

[[healthchecks]]
name = "API"
url = "http://localhost:3000/health"

[[healthchecks]]
name = "WebSocket"
url = "http://localhost:3001/health"
```

## Fields

### Top-level

| Field         | Type   | Required | Description                                    |
| ------------- | ------ | -------- | ---------------------------------------------- |
| `app`         | string | *        | App name (legacy mode)                         |
| `project`     | string | *        | Project name (project mode)                    |
| `target`      | string |          | Target in `app.project` format (optional in project mode) |
| `deploy_path` | string | yes      | Remote directory for deployment                |

> One of `app` or `project` is required. Use `app` for single-app deployments, `project` for multi-app projects.
>
> When `target` is omitted (project mode), the CLI auto-resolves the deployment node via the API based on bound apps.

### `[deploy]`

| Field           | Default  | Description                                   |
| --------------- | -------- | --------------------------------------------- |
| `source`        | `"git"`  | Deployment source: `"git"`, `"push"`, or `"image"` |
| `branch`        | `"main"` | Git branch to deploy                          |
| `compose_files` |          | List of docker-compose files (e.g., `["-f a.yml", "-f b.yml"]`) |

- **`git`**: Clones the repo on first deploy, runs `git pull` on subsequent deploys.
- **`push`**: Uses rsync to sync the local directory to the server. Excludes `target/`, `node_modules/`, `.git/`, `.env`, and `.env.deploy` automatically.
- **`image`**: Pulls pre-built images from a container registry. No local build. Use with `compose_files` and optionally `[deploy.registry]`.

### `[deploy.git]`

| Field     | Description                          |
| --------- | ------------------------------------ |
| `repo`    | Git repository URL                   |
| `ssh_key` | Path to deploy key (optional, supports `~`) |

### `[deploy.registry]`

Container registry authentication for `source = "image"`.

| Field      | Default    | Description                                     |
| ---------- | ---------- | ----------------------------------------------- |
| `url`      |            | Registry URL (e.g., `ghcr.io`)                  |
| `token`    |            | Auth token or PAT. Supports `$ENV_VAR` syntax   |
| `username` | `"oauth2"` | Registry username (optional for most registries) |

Values starting with `$` are resolved from environment variables at deploy time.

### `[[apps]]`

Define app groups for multi-service projects. Each group maps to a set of docker-compose services that can be deployed independently with `ops deploy --app <name>`.

| Field      | Description                              |
| ---------- | ---------------------------------------- |
| `name`     | App group name (used with `--app` flag)  |
| `services` | List of docker-compose service names     |

### `[[env_files]]`

Map local env files to remote paths (relative to `deploy_path`).

| Field    | Description              |
| -------- | ------------------------ |
| `local`  | Local file path          |
| `remote` | Remote file path         |

Files are only uploaded if they exist locally. Missing files are silently skipped.

### `[[sync]]`

Sync additional directories to the server via SCP.

| Field    | Description                          |
| -------- | ------------------------------------ |
| `local`  | Local directory path                 |
| `remote` | Remote path (relative to deploy_path) |

### `[[routes]]`

Configure nginx reverse proxy routes.

| Field    | Type    | Description                     |
| -------- | ------- | ------------------------------- |
| `domain` | string  | Domain name                     |
| `port`   | number  | Backend port to proxy to        |
| `ssl`    | boolean | Enable SSL via certbot          |

Each route generates an nginx server block. When `ssl = true`, certbot is run automatically.

### `[[healthchecks]]`

Post-deployment health checks.

| Field  | Description                     |
| ------ | ------------------------------- |
| `name` | Display name for the check      |
| `url`  | URL to check (retries 10 times) |

Health checks retry up to 10 times with 2-second intervals.

---

## Project Mode

For projects with multiple microservices, use `project` instead of `app` and define `[[apps]]` groups:

```toml
project = "RedQ"
deploy_path = "/opt/exchange"

[deploy]
source = "image"
compose_files = ["docker-compose.base.yml", "docker-compose.prod.yml"]

[deploy.registry]
url = "ghcr.io"
token = "$GHCR_PAT"

[[apps]]
name = "api"
services = ["api_server"]

[[apps]]
name = "core"
services = ["user_service", "order_service", "wallet_service"]

[[apps]]
name = "infra"
services = ["db", "redis", "kafka"]

[[env_files]]
local = ".env"
remote = ".env"

[[sync]]
local = "./docker-compose.base.yml"
remote = "docker-compose.base.yml"

[[sync]]
local = "./docker-compose.prod.yml"
remote = "docker-compose.prod.yml"

[[healthchecks]]
name = "API"
url = "http://localhost:8000/health"
```

### Deploying in Project Mode

```bash
# Deploy everything
ops deploy --set IMAGE_TAG=abc123

# Deploy only the api group
ops deploy --app api --set IMAGE_TAG=abc123

# Deploy a single service (bypass groups)
ops deploy --service api_server --set IMAGE_TAG=abc123

# Restart only
ops deploy --restart-only --app api
```

### Target Resolution

In project mode (no `target` field), the CLI resolves the deployment node automatically:

- `ops deploy --app api` finds the primary node bound to the `api` app via the API
- `ops deploy` (no `--app`) finds the first node bound to the project

You can still set `target` explicitly to override automatic resolution.
