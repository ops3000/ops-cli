# Launch

Scan the current project, auto-detect the framework, and generate deployment files (`Dockerfile`, `docker-compose.yml`, `.dockerignore`, `ops.toml`).

```bash
ops launch [OPTIONS]
```

**Options:**

| Option         | Default    | Description                           |
| -------------- | ---------- | ------------------------------------- |
| `-o, --output` | `ops.toml` | Output file path for ops.toml         |
| `-y, --yes`    |            | Accept all defaults without prompting |

## What It Does

`ops launch` uses the project scanner (`scanner/` module) to detect your framework and generate production-ready deployment files. It runs a priority-ordered scan across 13 supported frameworks and generates four files:

| Generated File      | Description                                  |
| ------------------- | -------------------------------------------- |
| `Dockerfile`        | Multi-stage production build for your framework |
| `docker-compose.yml`| Service definition with port mappings        |
| `.dockerignore`     | Framework-appropriate excludes               |
| `ops.toml`          | Project-mode deployment config (`[[apps]]`)  |

Each file is skipped if it already exists in the project directory.

## Supported Frameworks

The scanner detects frameworks in priority order:

| Category   | Framework     | Detection               | Docker Strategy              |
| ---------- | ------------- | ----------------------- | ---------------------------- |
| **Node.js**| Next.js       | `next` in package.json  | Standalone output mode       |
| **Node.js**| Nuxt          | `nuxt` in package.json  | Nitro server build           |
| **Node.js**| Remix         | `remix` in package.json | Production build             |
| **Node.js**| Vite SPA      | `vite` in package.json  | nginx static serving         |
| **Node.js**| Generic Node  | `package.json` exists   | npm start                    |
| **Python** | Django        | `django` in requirements| gunicorn WSGI                |
| **Python** | Flask         | `flask` in requirements | gunicorn WSGI                |
| **Python** | FastAPI       | `fastapi` in requirements| uvicorn ASGI                |
| **Python** | Generic Python| `requirements.txt`      | python main                  |
| **Go**     | Go            | `go.mod` exists         | 2-stage alpine static binary |
| **Rust**   | Rust          | `Cargo.toml` exists     | 2-stage with dep caching     |
| **Static** | Static HTML   | `index.html` exists     | nginx:alpine                 |
| —          | Dockerfile    | `Dockerfile` exists     | Uses existing Dockerfile     |

## Interactive Flow

```
$ ops launch

  OPS Launch
  ══════════

  Scanning project...
  ✔ Detected: Next.js (package.json)

  Project name [my-app]:
  App name [web]:
  Deploy path [/opt/my-app]:

  Generating files...
  ✔ Dockerfile (Next.js standalone)
  ✔ docker-compose.yml
  ✔ .dockerignore
  ✔ ops.toml

  Ready to deploy:
    ops deploy
```

With `--yes`, all prompts accept defaults and files are generated non-interactively.

## Generated Output

Example output for a Next.js project:

**ops.toml:**
```toml
[project]
name = "my-app"

[[apps]]
name = "web"
deploy_path = "/opt/my-app"

[apps.deploy]
source = "push"
compose_files = ["docker-compose.yml"]
```

**Dockerfile** (framework-specific multi-stage build), **docker-compose.yml** (service definition), and **.dockerignore** (framework-appropriate excludes) are also generated with production-ready defaults.

## Examples

```bash
# Interactive mode (recommended)
ops launch

# Accept all defaults (CI-friendly)
ops launch --yes

# Generate ops.toml to a custom path
ops launch -o ops.prod.toml
```

After generating, deploy with:

```bash
ops deploy
```
