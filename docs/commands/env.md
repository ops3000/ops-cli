# Environment Variables

## env upload

Upload a local `.env` file to the target server.

```bash
ops env upload <target>
```

**Arguments:**

| Argument | Description                      |
| -------- | -------------------------------- |
| `target` | `app.project` format             |

Uploads the local `.env` file to `/opt/judge/.env` on the remote server (with sudo).

**Example:**

```bash
ops env upload api.my-saas
```

## env download

Download the `.env` file from the target server to the current directory.

```bash
ops env download <target>
```

**Arguments:**

| Argument | Description                      |
| -------- | -------------------------------- |
| `target` | `app.project` format             |

**Example:**

```bash
ops env download api.my-saas
# Downloads remote .env to ./
```
