# Nodes

## init

Initialize the current server as a node in OPS. Run this directly on the server you want to register.

```bash
ops init [OPTIONS]
```

**Options:**

| Option          | Default  | Description                                  |
| --------------- | -------- | -------------------------------------------- |
| `--daemon`      | `true`   | Start ops serve daemon                       |
| `--project`     |          | Limit to specific projects (comma-separated) |
| `--app`         |          | Limit to specific apps (comma-separated)     |
| `--region`      |          | Region label (e.g., `us-east`, `eu-west`)    |
| `--port`        | `8377`   | Port for the ops serve daemon                |
| `--hostname`    |          | Custom hostname for this node                |
| `--compose_dir` |          | Docker Compose project directory             |

**What it does:**

1. Verifies you're logged in
2. Cleans up any old OPS configuration residue
3. Reads your local SSH public key
4. Registers the node with the OPS API
5. Adds CI SSH key to `~/.ssh/authorized_keys`
6. Installs `ops serve` as a systemd service
7. Configures nginx reverse proxy for the serve endpoint

**Example:**

```bash
ops init --region us-east --compose_dir /opt/myapp
```

**Output:**

```
Node ID:  42
Domain:   42.node.ops.autos
IP:       203.0.113.1
Region:   us-east
```

After init, you can access this server remotely:

```bash
ops ssh 42
```

## node list

List all nodes owned by the current user.

```bash
ops node list
```

Shows node ID, hostname, status, region, and domain. Status indicators:

- `●` healthy
- `●` unhealthy
- `◐` draining
- `○` offline

## node info

Show detailed information about a specific node.

```bash
ops node info <id>
```

**Arguments:**

| Argument | Description |
| -------- | ----------- |
| `id`     | Node ID     |

## node remove

Remove a node from OPS.

```bash
ops node remove <id> [--force]
```

**Arguments:**

| Argument  | Description                    |
| --------- | ------------------------------ |
| `id`      | Node ID                        |
| `--force` | Skip deletion confirmation     |
