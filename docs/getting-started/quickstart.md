# Quickstart

This guide walks you through the core workflow: register, create a project, initialize a server, and deploy an app.

## 1. Create an Account

```bash
ops register
```

You'll be prompted for a username and password (8+ characters).

## 2. Log In

```bash
ops login
```

Your JWT token is saved to `~/.config/ops/credentials.json`.

## 3. Create a Project

```bash
ops project create my-project
```

## 4. Initialize a Server

SSH into your server and run:

```bash
ops init
```

This:

- Registers the server as a node in OPS
- Assigns a domain: `<node-id>.node.ops.autos`
- Configures SSH access with CI keys
- Installs the `ops serve` daemon as a systemd service

You can specify a region:

```bash
ops init --region us-east
```

## 5. Bind the Node to an App

```bash
ops set api.my-project --node <node-id>
```

This binds the node to the `api` app under `my-project`.

## 6. Deploy

Generate an `ops.toml` by scanning your project:

```bash
ops launch
```

Or create one manually in your project root:

```toml
app = "api"
target = "api.my-project"
deploy_path = "/opt/api"

[deploy]
source = "git"
branch = "main"

[deploy.git]
repo = "git@github.com:yourorg/api.git"

[[routes]]
domain = "api.example.com"
port = 3000
ssl = true
```

Then deploy:

```bash
ops deploy
```

OPS will:

1. Sync code via git (clone or pull)
2. Upload env files
3. Run `docker compose build && docker compose up -d`
4. Configure nginx reverse proxy
5. Set up SSL via certbot
6. Run health checks

## 7. Check Status

```bash
ops status
ops logs api
```

## Next Steps

- [Configuration](configuration.md) - credential storage and environment variables
- [ops.toml Configuration](../guides/ops-toml.md) - full deployment config reference
- [Multi-Region Deployment](../guides/multi-region.md) - node groups and load balancing
