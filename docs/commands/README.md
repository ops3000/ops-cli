# Commands Overview

All commands are invoked as `ops <command>`.

## Authentication

| Command                         | Description                          |
| ------------------------------- | ------------------------------------ |
| [`register`](auth.md#register) | Create a new account                 |
| [`login`](auth.md#login)       | Log in and save token                |
| [`logout`](auth.md#logout)     | Clear saved credentials              |
| [`whoami`](auth.md#whoami)     | Show current user info               |
| [`token`](auth.md#token)       | Print session token to stdout        |

## Projects

| Command                                        | Description                    |
| ---------------------------------------------- | ------------------------------ |
| [`project create`](projects.md#project-create) | Create a new project           |
| [`project list`](projects.md#project-list)     | List projects with tree view   |

## Nodes

| Command                                 | Description                      |
| --------------------------------------- | -------------------------------- |
| [`init`](nodes.md#init)                | Initialize server as a node      |
| [`node list`](nodes.md#node-list)      | List all your nodes              |
| [`node info`](nodes.md#node-info)      | Show node details                |
| [`node remove`](nodes.md#node-remove)  | Remove a node                    |

## Node Groups

| Command                                                  | Description                       |
| -------------------------------------------------------- | --------------------------------- |
| [`node-group create`](node-groups.md#node-group-create) | Create a node group               |
| [`node-group list`](node-groups.md#node-group-list)     | List node groups                  |
| [`node-group show`](node-groups.md#node-group-show)     | Show group details and members    |
| [`node-group nodes`](node-groups.md#node-group-nodes)   | List nodes in an environment      |

## Deployment

| Command                              | Description                        |
| ------------------------------------ | ---------------------------------- |
| [`set`](deployment.md#set)          | Bind a server to an app            |
| [`deploy`](deployment.md#deploy)    | Deploy services from ops.toml      |
| [`build`](build.md#build)          | Remote build on a build node       |
| [`status`](deployment.md#status)    | Show deployed service status       |
| [`logs`](deployment.md#logs)        | View service logs                  |

## Custom Domains

| Command                                    | Description                        |
| ------------------------------------------ | ---------------------------------- |
| [`domain add`](domain.md#domain-add)      | Add a custom domain                |
| [`domain list`](domain.md#domain-list)    | List custom domains                |
| [`domain remove`](domain.md#domain-remove)| Remove a custom domain             |

## Resource Pool

| Command                                        | Description                        |
| ---------------------------------------------- | ---------------------------------- |
| [`pool status`](pool.md#pool-status)          | Show pool status                   |
| [`pool strategy`](pool.md#pool-strategy)      | Change load balancing strategy     |
| [`pool drain`](pool.md#pool-drain)            | Drain a node                       |
| [`pool undrain`](pool.md#pool-undrain)        | Restore a drained node             |

## SSH & File Transfer

| Command                           | Description                        |
| --------------------------------- | ---------------------------------- |
| [`ssh`](ssh.md#ssh)              | SSH into a server                  |
| [`push`](ssh.md#push)            | Push files to a server (SCP)       |
| [`ci-keys`](ssh.md#ci-keys)     | Get CI private key                 |

## Network

| Command                        | Description                        |
| ------------------------------ | ---------------------------------- |
| [`ip`](network.md#ip)         | Get server public IP               |
| [`ping`](network.md#ping)     | Ping a server                      |

## Environment Variables

| Command                            | Description                        |
| ---------------------------------- | ---------------------------------- |
| [`env upload`](env.md#env-upload)     | Upload .env file to server      |
| [`env download`](env.md#env-download) | Download .env file from server  |

## Server & Daemon

| Command                                       | Description                        |
| --------------------------------------------- | ---------------------------------- |
| [`serve`](server.md#serve)                   | Start monitoring daemon            |
| [`server whoami`](server.md#server-whoami)   | Show current server info           |
| [`update`](server.md#update)                 | Update OPS to latest version       |
| [`version`](server.md#version)               | Show version info                  |
