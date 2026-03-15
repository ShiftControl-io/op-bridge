# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in op-bridge, please report it privately using [GitHub Security Advisories](https://github.com/ShiftControl-io/op-bridge/security/advisories/new).

**Do not open a public issue for security vulnerabilities.**

We will acknowledge receipt within 48 hours and provide a timeline for a fix.

## Scope

op-bridge handles sensitive 1Password credentials. The following are in scope:

- Secret material leaking to logs, disk, or network
- Unauthorized access to the Unix socket
- Memory not being properly zeroized
- Vulnerabilities in the `op` CLI subprocess invocation (command injection, etc.)
- File watcher reading or writing unintended files

## Supported Versions

| Version | Supported |
|---------|-----------|
| latest  | Yes       |

## Dependencies

We run `cargo audit` in CI to catch known vulnerabilities in dependencies. If you find a vulnerable dependency, please report it.
