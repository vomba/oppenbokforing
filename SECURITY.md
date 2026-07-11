# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| `v0.1.x` beta | Yes — best-effort fixes |

## Reporting a vulnerability

**Do not** open public issues for security problems.

Email or DM the maintainer via GitHub (profile: [@vomba](https://github.com/vomba)) with:

- Description and impact
- Steps to reproduce
- Affected version / commit SHA
- Suggested fix (optional)

We aim to acknowledge within **7 days** and publish a fix or mitigation timeline for confirmed issues.

## Scope

In scope:

- Local workspace data exposure to other users/network
- Backup encryption weaknesses
- Path traversal in document import/reveal/export
- SQL injection or command injection in the Rust layer
- Tauri permission misconfiguration

Out of scope:

- User-submitted incorrect tax figures
- Unsigned macOS Gatekeeper friction (documented)
- Issues in third-party dependencies without a practical app-level fix

## Security model

See [`docs/security-privacy.md`](docs/security-privacy.md) and [`docs/legal/en.md`](docs/legal/en.md).
