# Resource Pool

Manage the multi-node resource pool for an app. Target format is `app.project` (e.g., `api.RedQ`).

Pool commands require the app to be in multi-node mode (at least two nodes bound).

## pool status

Show the resource pool status for an app, including all bound nodes, their health, and the current load balancing strategy.

```bash
ops pool status <target>
```

**Arguments:**

| Argument | Description                                |
| -------- | ------------------------------------------ |
| `target` | Target in `app.project` format             |

**Example:**

```bash
ops pool status api.RedQ
# Output:
#   Pool status for api.RedQ
#   Mode:     multi
#   Strategy: round-robin
#   Group ID: 5
#
#   ID       Domain                       IP               Region         Status     Primary
#   --------------------------------------------------------------------------------
#   3        3.node.ops.autos             1.2.3.4          hk             healthy    yes
#   4        4.node.ops.autos             5.6.7.8          jp             healthy    -
#
#   2/2 nodes healthy
```

## pool strategy

Change the load balancing strategy for the app's node pool.

```bash
ops pool strategy <target> <strategy>
```

**Arguments:**

| Argument   | Description                                                |
| ---------- | ---------------------------------------------------------- |
| `target`   | Target in `app.project` format                             |
| `strategy` | One of: `round-robin`, `geo`, `weighted`, `failover`       |

**Strategies:**

| Strategy      | Description                                      |
| ------------- | ------------------------------------------------ |
| `round-robin` | Distribute requests evenly across all nodes      |
| `geo`         | Route to the geographically closest node         |
| `weighted`    | Distribute based on node weights                 |
| `failover`    | Use primary node, failover to others if unhealthy |

**Example:**

```bash
ops pool strategy api.RedQ geo
```

## pool drain

Drain a node from the pool. A drained node stops receiving new traffic but continues serving existing connections.

```bash
ops pool drain <target> --node <id>
```

**Arguments:**

| Argument | Description                    |
| -------- | ------------------------------ |
| `target` | Target in `app.project` format |

**Options:**

| Option   | Description     |
| -------- | --------------- |
| `--node` | Node ID to drain |

**Example:**

```bash
ops pool drain api.RedQ --node 4
```

## pool undrain

Restore a drained node back to active rotation.

```bash
ops pool undrain <target> --node <id>
```

**Arguments:**

| Argument | Description                    |
| -------- | ------------------------------ |
| `target` | Target in `app.project` format |

**Options:**

| Option   | Description       |
| -------- | ----------------- |
| `--node` | Node ID to restore |

**Example:**

```bash
ops pool undrain api.RedQ --node 4
```
