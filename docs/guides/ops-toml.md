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

| Field         | Type   | Description                                    |
| ------------- | ------ | ---------------------------------------------- |
| `app`         | string | App name (used for backend record sync)        |
| `target`      | string | Target in `app.project` format                 |
| `deploy_path` | string | Remote directory for deployment                |

### `[deploy]`

| Field    | Default | Description                          |
| -------- | ------- | ------------------------------------ |
| `source` | `"git"` | Deployment source: `"git"` or `"push"` |
| `branch` | `"main"` | Git branch to deploy                |

- **`git`**: Clones the repo on first deploy, runs `git pull` on subsequent deploys.
- **`push`**: Uses rsync to sync the local directory to the server. Excludes `target/`, `node_modules/`, `.git/`, `.env`, and `.env.deploy` automatically.

### `[deploy.git]`

| Field     | Description                          |
| --------- | ------------------------------------ |
| `repo`    | Git repository URL                   |
| `ssh_key` | Path to deploy key (optional, supports `~`) |

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
