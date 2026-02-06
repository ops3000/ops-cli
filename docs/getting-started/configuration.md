# Configuration

## Credentials

OPS stores your authentication token in:

```
~/.config/ops/credentials.json
```

The file contains:

```json
{
  "token": "your-jwt-token"
}
```

This file is created automatically when you run `ops login`.

## Environment Variable

You can override the stored token by setting the `OPS_TOKEN` environment variable:

```bash
export OPS_TOKEN="your-jwt-token"
```

When `OPS_TOKEN` is set, it takes precedence over `credentials.json`. This is useful for CI/CD pipelines and automation scripts.

## API Endpoint

All CLI commands communicate with the OPS API at:

```
https://api.ops.autos
```

This is not configurable.

## SSH Keys

OPS uses your local SSH public key during `ops init` and `ops set`. It looks for keys in this order:

1. `~/.ssh/id_ed25519.pub`
2. `~/.ssh/id_rsa.pub`
3. `~/.ssh/id_ecdsa.pub`

If no key is found, you'll be prompted to generate one:

```bash
ssh-keygen -t ed25519
```

## CI/CD Keys

OPS generates and manages CI SSH key pairs for each node. These are used by commands like `ops ssh`, `ops push`, and `ops deploy` to access servers without manual key management.

To retrieve a CI private key for scripting:

```bash
ops ci-keys api.my-project
```

## Auto-Update

OPS checks for updates on every command (except `update`, `version`, and `serve`). If a new version is available, it updates the binary automatically and prompts you to re-run your command.

To disable this, the binary self-updates using the GitHub Releases API from `ops3000/ops-cli`.
