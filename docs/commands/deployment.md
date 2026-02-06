# Deployment

## set

Bind a server to an app. Supports two modes:

### Remote binding (recommended)

Bind an existing node to an app:

```bash
ops set <target> --node <node-id> [OPTIONS]
```

**Options:**

| Option       | Description                                 |
| ------------ | ------------------------------------------- |
| `--node`     | Node ID to bind                             |
| `--primary`  | Set as primary node                         |
| `--region`   | Region (e.g., `us-east`, `eu-west`)         |
| `--zone`     | Availability zone (e.g., `a`, `b`, `c`)     |
| `--hostname` | Custom hostname                             |
| `--weight`   | Load balancing weight (1-100)               |

**Example:**

```bash
ops set api.my-saas --node 42 --primary
```

### Local binding (legacy)

Run directly on the server to bind it:

```bash
ops set api.my-saas
```

This prompts for confirmation and optionally regenerates CI/CD SSH keys.

**Target format:** `app.project` (e.g., `api.my-saas`)

## deploy

Deploy services defined in `ops.toml`.

```bash
ops deploy [OPTIONS]
```

**Options:**

| Option           | Default    | Description                                  |
| ---------------- | ---------- | -------------------------------------------- |
| `-f, --file`     | `ops.toml` | Path to config file                          |
| `--service`      |            | Deploy only a specific docker-compose service |
| `--app`          |            | Deploy only services in this app group       |
| `--restart-only` |            | Skip build/pull, only restart containers     |
| `--set`          |            | Set env variable (`KEY=VALUE`), repeatable   |

**Deployment steps:**

1. Parse `ops.toml` configuration
2. Resolve target (from `target` field, or auto-lookup via API in project mode)
3. Sync app record to backend API
4. Sync code based on `source`:
   - **`git`**: clone or pull from remote repository
   - **`push`**: rsync local directory to server
   - **`image`**: docker login (if registry configured) + `docker compose pull`
5. Upload env files to remote paths
6. Sync additional directories/files
7. Build & start:
   - **`git`/`push`**: `docker compose build && docker compose up -d`
   - **`image`**: `docker compose up -d` (no build) + `docker image prune`
8. Generate and upload nginx config (if routes defined)
9. Configure SSL via certbot (if `ssl = true` in routes)
10. Run health checks

**Examples:**

```bash
# Deploy all services
ops deploy

# Deploy specific service
ops deploy --service api_server

# Deploy by app group (uses [[apps]] in ops.toml)
ops deploy --app api

# Pass environment variables to docker compose
ops deploy --set IMAGE_TAG=abc123 --set ENV=production

# Restart without rebuilding
ops deploy --restart-only

# Restart specific app group
ops deploy --restart-only --app api

# Use custom config file
ops deploy -f ops.prod.toml
```

## status

Show status of deployed services.

```bash
ops status [-f <file>]
```

Reads `ops.toml` to determine the target server and runs `docker compose ps` remotely.

## logs

View logs of a deployed service.

```bash
ops logs <service> [OPTIONS]
```

**Arguments:**

| Argument  | Description                          |
| --------- | ------------------------------------ |
| `service` | Service name (e.g., `api`, `web`)    |

**Options:**

| Option         | Default    | Description              |
| -------------- | ---------- | ------------------------ |
| `--file`       | `ops.toml` | Path to config file      |
| `-n, --tail`   | `100`      | Number of lines to show  |
| `-f, --follow` |            | Stream logs in real-time |

**Examples:**

```bash
ops logs api
ops logs api -n 500
ops logs api --follow
```
