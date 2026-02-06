# Multi-Region Deployment

OPS supports deploying apps across multiple regions using node groups with configurable load balancing.

## Concepts

- **Node**: A single server registered with OPS
- **Node Group**: A collection of nodes serving the same app environment
- **Load Balancing Strategy**: How traffic is distributed across nodes in a group

## Workflow

### 1. Initialize Nodes in Different Regions

On each server:

```bash
# US East server
ops init --region us-east

# EU West server
ops init --region eu-west

# AP Northeast server
ops init --region ap-northeast
```

### 2. Create a Node Group

```bash
ops node-group create --project my-saas --env api --strategy geo
```

### 3. Bind Nodes to the App

```bash
ops set api.my-saas --node 42 --primary --region us-east
ops set api.my-saas --node 43 --region eu-west
ops set api.my-saas --node 44 --region ap-northeast
```

### 4. Verify the Setup

```bash
ops node-group nodes api.my-saas
```

## Load Balancing Strategies

### round-robin

Distributes requests evenly across all healthy nodes. Default strategy.

```bash
ops node-group create --project my-saas --env api --strategy round-robin
```

### geo

Routes requests to the nearest region based on client location.

```bash
ops node-group create --project my-saas --env api --strategy geo
```

### weighted

Distributes traffic based on node weights (1-100). Higher weight = more traffic.

```bash
ops set api.my-saas --node 42 --weight 70
ops set api.my-saas --node 43 --weight 30
```

### failover

Sends all traffic to the primary node. Fails over to secondary nodes if the primary becomes unhealthy.

```bash
ops node-group create --project my-saas --env api --strategy failover
ops set api.my-saas --node 42 --primary
ops set api.my-saas --node 43  # secondary
```

## Availability Zones

Within a region, you can assign nodes to availability zones:

```bash
ops set api.my-saas --node 42 --region us-east --zone a
ops set api.my-saas --node 43 --region us-east --zone b
```

## Monitoring

View all nodes in a group:

```bash
ops node-group show <group-id>
```

Check individual node status:

```bash
ops node list
ops node info <node-id>
```
