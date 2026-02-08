# Custom Domains

Manage custom domains for your app. All commands read `ops.toml` to determine the project and app context.

## domain add

Add a custom domain to your app. Returns the CNAME record you need to configure in your DNS provider.

```bash
ops domain add <domain> [-f <file>]
```

**Arguments:**

| Argument | Description                          |
| -------- | ------------------------------------ |
| `domain` | Custom domain (e.g., `api.example.com`) |

**Options:**

| Option       | Default    | Description         |
| ------------ | ---------- | ------------------- |
| `-f, --file` | `ops.toml` | Path to config file |

**Example:**

```bash
ops domain add api.example.com
# Output:
#   ✔ Domain added
#   CNAME: api.example.com → api.my-saas.ops.autos
#   SSL: pending
#   Add a CNAME record in your DNS provider pointing to the target above.
```

## domain list

List all custom domains configured for your app.

```bash
ops domain list [-f <file>]
```

**Options:**

| Option       | Default    | Description         |
| ------------ | ---------- | ------------------- |
| `-f, --file` | `ops.toml` | Path to config file |

**Example:**

```bash
ops domain list
# Output:
#   Domains for api.my-saas:
#   api.my-saas.ops.autos (default)
#   api.example.com [active]
#   staging.example.com [pending]
```

## domain remove

Remove a custom domain from your app.

```bash
ops domain remove <domain> [-f <file>]
```

**Arguments:**

| Argument | Description              |
| -------- | ------------------------ |
| `domain` | Custom domain to remove  |

**Options:**

| Option       | Default    | Description         |
| ------------ | ---------- | ------------------- |
| `-f, --file` | `ops.toml` | Path to config file |

**Example:**

```bash
ops domain remove staging.example.com
```
