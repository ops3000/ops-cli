# Network

## ip

Get the public IP address of a server.

```bash
ops ip <target>
```

**Arguments:**

| Argument | Description                      |
| -------- | -------------------------------- |
| `target` | Node ID or `app.project` format  |

Resolves the server's DNS domain to its IPv4 address.

**Example:**

```bash
ops ip api.my-saas
# 203.0.113.1
```

## ping

Ping a server to check reachability.

```bash
ops ping <target>
```

**Arguments:**

| Argument | Description                      |
| -------- | -------------------------------- |
| `target` | Node ID or `app.project` format  |

**Example:**

```bash
ops ping 42
```
