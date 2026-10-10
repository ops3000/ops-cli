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

A login lasts 30 days. Every `ops` command renews it once it is inside its last
week, so a machine you use never gets logged out; one left idle past that has to
`ops login` again.

### token create

Issue a named token for CI or another machine (1–365 days, default 90). The token
goes to stdout, everything else to stderr:

```bash
ops token create --name gh:owner/repo --ttl-days 365 | gh secret set OPS_TOKEN --repo owner/repo
```

Don't put your own login token (`ops token`) into CI: it expires in 30 days and
CI has nowhere to save a renewed one.

### token revoke-all

Revoke **every** token of your account at once — other machines, CI secrets, a
laptop you sold. The machine you run it on gets a fresh token in the same
request and stays logged in; everything else needs `ops login` or a new
`ops token create`.

```bash
ops token revoke-all          # asks for confirmation
ops token revoke-all --force  # scripts
```
