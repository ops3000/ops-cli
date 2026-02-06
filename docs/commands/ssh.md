# SSH & File Transfer

## ssh

SSH into a server or execute a remote command.

```bash
ops ssh <target> [command]
```

**Arguments:**

| Argument  | Description                                  |
| --------- | -------------------------------------------- |
| `target`  | Node ID or `app.project` format              |
| `command` | Optional command to execute remotely          |

OPS automatically fetches the CI private key from the API and uses it for authentication. No manual key management needed.

**Examples:**

```bash
# Interactive SSH session by node ID
ops ssh 42

# Interactive SSH session by app target
ops ssh api.my-saas

# Execute a remote command
ops ssh 42 "docker ps"
ops ssh api.my-saas "df -h"
```

## push

Push a file or directory to a server via SCP.

```bash
ops push <source> <target>
```

**Arguments:**

| Argument | Description                                              |
| -------- | -------------------------------------------------------- |
| `source` | Local file or directory path                             |
| `target` | `app.project[:/remote/path]`                             |

If no remote path is specified, files are uploaded to `/root/`.

**Examples:**

```bash
# Push a file to default path (/root/)
ops push config.yaml api.my-saas

# Push to a specific remote path
ops push ./dist api.my-saas:/opt/app/dist

# Push a directory
ops push ./configs api.my-saas:/etc/myapp/
```

## ci-keys

Get the CI private key for a target. Useful for CI/CD pipeline setup.

```bash
ops ci-keys <target>
```

Alias: `ops ci-key`

**Arguments:**

| Argument | Description                        |
| -------- | ---------------------------------- |
| `target` | `app.project` format              |

Outputs the private SSH key to stdout. Use in CI/CD pipelines:

```bash
ops ci-keys api.my-saas > /tmp/deploy_key
chmod 600 /tmp/deploy_key
ssh -i /tmp/deploy_key root@api.my-saas.ops.autos
```
