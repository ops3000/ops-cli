# Launch

Generate an `ops.toml` configuration file by scanning the current project directory.

```bash
ops launch [OPTIONS]
```

**Options:**

| Option         | Default    | Description                           |
| -------------- | ---------- | ------------------------------------- |
| `-o, --output` | `ops.toml` | Output file path                      |
| `-y, --yes`    |            | Accept all defaults without prompting |

## What It Does

`ops launch` scans your project directory and interactively generates an `ops.toml` deployment config with sensible defaults. It detects:

| File                    | Detection                                       |
| ----------------------- | ----------------------------------------------- |
| `docker-compose*.yml`   | Added to `compose_files`, suggests `source = "image"` |
| `Dockerfile`            | Suggests `source = "push"`                      |
| `.git/` + origin remote | Suggests `source = "git"`, fills `git.repo`     |
| `Cargo.toml`            | Language: Rust                                  |
| `package.json`          | Language: Node.js                               |
| `go.mod`                | Language: Go                                    |
| `requirements.txt`      | Language: Python                                |
| `.env`                  | Auto-adds to `[[env_files]]`                    |
| `Config.toml`, etc.     | Suggests adding to `[[sync]]`                   |

## Interactive Flow

```
$ ops launch

  OPS Launch
  ══════════

  Scanning project...
  ✔ Detected: docker-compose.yml, docker-compose.prod.yml
  ✔ Detected: .env
  ✔ Language: Rust (Cargo.toml)

  App name [my-project]:
  Deploy source (git/push/image) [image]:
  Deploy path [/opt/my-project]:
  Target (e.g. prod.myproject, enter to skip):

  Found docker-compose files:
    1. docker-compose.yml
    2. docker-compose.prod.yml
  Use these compose files? [Y/n]:

  Found .env file. Sync to remote? [Y/n]:

  Health check URL (enter to skip):

  ✔ Generated ops.toml
```

## Source Detection Logic

The default deploy source is inferred from your project:

- **`image`** — docker-compose files found, no Dockerfile (pull pre-built images)
- **`push`** — Dockerfile found (rsync + build on server)
- **`git`** — git remote configured (clone/pull on server)
- **`push`** — fallback when nothing specific is detected

## Examples

```bash
# Interactive mode (recommended)
ops launch

# Accept all defaults (CI-friendly)
ops launch --yes

# Generate to a custom file
ops launch -o ops.prod.toml
```

## Generated Output

Example output for a project with docker-compose and .env:

```toml
app = "my-api"
deploy_path = "/opt/my-api"

[deploy]
source = "image"
compose_files = ["docker-compose.yml", "docker-compose.prod.yml"]

# [deploy.registry]
# url = "ghcr.io"
# token = "$GHCR_PAT"

[[env_files]]
local = ".env"
remote = ".env"

[[sync]]
local = "./docker-compose.yml"
remote = "docker-compose.yml"

[[sync]]
local = "./docker-compose.prod.yml"
remote = "docker-compose.prod.yml"

# [[healthchecks]]
# name = "Health"
# url = "http://localhost:8000/health"
```

After generating, deploy with:

```bash
ops deploy
```
