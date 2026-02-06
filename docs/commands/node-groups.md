# Node Groups

Node groups enable multi-region deployments by grouping multiple nodes under a single app environment with load balancing.

## node-group create

Create a new node group.

```bash
ops node-group create --project <project> --env <env> [OPTIONS]
```

**Options:**

| Option       | Default        | Description                                                      |
| ------------ | -------------- | ---------------------------------------------------------------- |
| `--project`  | (required)     | Project name                                                     |
| `--env`      | (required)     | Environment name (e.g., `prod`, `staging`)                       |
| `--name`     |                | Custom name for the group                                        |
| `--strategy` | `round-robin`  | Load balancing strategy: `round-robin`, `geo`, `weighted`, `failover` |

**Example:**

```bash
ops node-group create --project my-saas --env api --strategy geo
```

## node-group list

List node groups, optionally filtered by project.

```bash
ops node-group list [--project <project>]
```

**Options:**

| Option      | Description                                   |
| ----------- | --------------------------------------------- |
| `--project` | Filter by project name (lists all if omitted) |

## node-group show

Show detailed information about a node group including member nodes.

```bash
ops node-group show <id>
```

**Arguments:**

| Argument | Description   |
| -------- | ------------- |
| `id`     | Node group ID |

Displays:

- Group name, environment, load balancing strategy
- Member nodes with IP, region, zone, weight, status, and health check info

## node-group nodes

List nodes in a specific environment.

```bash
ops node-group nodes <target>
```

**Arguments:**

| Argument | Description                              |
| -------- | ---------------------------------------- |
| `target` | Target in format `environment.project`   |

**Example:**

```bash
ops node-group nodes api.my-saas
```
