# CI/CD Integration

OPS can be integrated into CI/CD pipelines for automated deployments.

## Authentication

Set the `OPS_TOKEN` environment variable in your pipeline. Get the token:

```bash
ops token
```

Add it as a secret in your CI/CD provider (e.g., `OPS_TOKEN` in GitHub Actions secrets).

## GitHub Actions Example

```yaml
name: Deploy
on:
  push:
    branches: [main]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install OPS
        run: curl -fsSL https://get.ops.autos | sh

      - name: Deploy
        env:
          OPS_TOKEN: ${{ secrets.OPS_TOKEN }}
        run: ops deploy
```

## SSH Access in CI

Use `ops ci-keys` to get the SSH private key for direct server access:

```yaml
- name: Get SSH Key
  env:
    OPS_TOKEN: ${{ secrets.OPS_TOKEN }}
  run: |
    ops ci-keys api.my-saas > /tmp/deploy_key
    chmod 600 /tmp/deploy_key

- name: Run Remote Command
  run: |
    ssh -i /tmp/deploy_key -o StrictHostKeyChecking=no root@api.my-saas.ops.autos "docker ps"
```

## Deploy Specific Services

```yaml
- name: Deploy API only
  env:
    OPS_TOKEN: ${{ secrets.OPS_TOKEN }}
  run: ops deploy --service api
```

## Restart Without Rebuilding

```yaml
- name: Restart services
  env:
    OPS_TOKEN: ${{ secrets.OPS_TOKEN }}
  run: ops deploy --restart-only
```

## Custom Config File

```yaml
- name: Deploy staging
  env:
    OPS_TOKEN: ${{ secrets.OPS_TOKEN }}
  run: ops deploy -f staging.ops.toml
```

## Deployment Status

OPS automatically tracks deployment status in the backend. Each `ops deploy` creates a deployment record with status (`success` or `failed`), visible in the web dashboard.
