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

| Option           | Default    | Description                    |
| ---------------- | ---------- | ------------------------------ |
| `-f, --file`     | `ops.toml` | Path to config file            |
| `--service`      |            | Deploy only a specific service |
| `--restart-only` |            | Skip build, only restart       |

**Deployment steps:**

1. Parse `ops.toml` configuration
2. Sync app record to backend API
3. Sync code via git clone/pull or rsync
4. Upload env files to remote paths
5. Sync additional directories
6. Run `docker compose build && docker compose up -d`
7. Generate and upload nginx config
8. Configure SSL via certbot (if `ssl = true` in routes)
9. Run health checks

**Examples:**

```bash
# Deploy all services
ops deploy

# Deploy specific service
ops deploy --service api

# Restart without rebuilding
ops deploy --restart-only

# Use custom config file
ops deploy -f production.ops.toml
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
