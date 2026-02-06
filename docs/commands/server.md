# Server & Daemon

## serve

Start the HTTP monitoring daemon that exposes container status, logs, and metrics.

```bash
ops serve --token <token> --compose_dir <dir> [OPTIONS]
```

**Options:**

| Option          | Default | Description                                        |
| --------------- | ------- | -------------------------------------------------- |
| `--token`       | (required) | Bearer token for API authentication             |
| `--port`        | `8377`  | Port to listen on                                  |
| `--compose_dir` | (required) | Docker Compose project directory                |
| `--install`     |         | Install as systemd service + nginx reverse proxy   |
| `--domain`      |         | Domain for nginx (e.g., `api.my-saas.ops.autos`)  |

**REST API endpoints:**

| Method | Endpoint          | Description              |
| ------ | ----------------- | ------------------------ |
| GET    | `/health`         | Health check             |
| GET    | `/containers`     | List containers          |
| GET    | `/logs`           | View container logs      |
| GET    | `/logs/stream`    | Stream logs (SSE)        |
| GET    | `/metrics`        | Container metrics        |
| POST   | `/restart`        | Restart a container      |
| POST   | `/stop`           | Stop a container         |
| POST   | `/start`          | Start a container        |
| POST   | `/deploy`         | Deploy a service         |
| GET    | `/checkupdate`    | Check for updates        |

The daemon checks for updates every 5 minutes and auto-restarts when a new binary is available.

**Install as systemd service:**

```bash
ops serve --token <token> --compose_dir /opt/myapp --install --domain mynode.ops.autos
```

This creates `/etc/systemd/system/ops-serve.service` and configures nginx.

{% hint style="info" %}
You typically don't run `ops serve` manually. It's installed automatically by `ops init`.
{% endhint %}

## server whoami

Show information about the current server based on its public IP.

```bash
ops server whoami
```

**Output includes:**

- IP address
- Status (registered/unregistered)
- Bound domain
- Project name
- Owner
- Permission level

## update

Update OPS to the latest version.

```bash
ops update
```

Downloads the latest binary from GitHub Releases and replaces the current binary. If `ops-serve` is running as a systemd service, it attempts to restart it.

## version

Show current version and check for updates.

```bash
ops version
```

**Example output:**

```
ops-cli version: 0.5.9
You are on the latest version.
```
