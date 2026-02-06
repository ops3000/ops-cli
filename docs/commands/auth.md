# Authentication

## register

Create a new user account.

```bash
ops register
```

Prompts for username and password interactively. Password must be at least 8 characters and requires confirmation.

## login

Authenticate and save token locally.

```bash
ops login
```

Prompts for username and password. On success, saves the JWT token to `~/.config/ops/credentials.json`.

## logout

Clear saved credentials.

```bash
ops logout
```

Removes the stored token from `~/.config/ops/credentials.json`.

## whoami

Display current logged-in user info.

```bash
ops whoami
```

Output includes:

- User ID
- Username
- Token expiration date

## token

Print the current session token to stdout.

```bash
ops token
```

Useful for piping into other commands or setting up CI/CD:

```bash
export OPS_TOKEN=$(ops token)
```
