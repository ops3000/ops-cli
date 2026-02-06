# Projects

## project create

Create a new project.

```bash
ops project create <name>
```

**Arguments:**

| Argument | Description  |
| -------- | ------------ |
| `name`   | Project name |

**Example:**

```bash
ops project create my-saas
```

## project list

List all projects with a tree view showing environments and nodes.

```bash
ops project list [name]
```

**Arguments:**

| Argument | Description                | Required |
| -------- | -------------------------- | -------- |
| `name`   | Filter by project name     | No       |

**Example output:**

```
my-saas
├── api
│   ├── 203.0.113.1  api.my-saas.ops.autos
│   └── 203.0.113.2  api.my-saas.ops.autos
└── web
    └── 203.0.113.3  web.my-saas.ops.autos
```
