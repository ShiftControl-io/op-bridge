# Contributing to op-bridge

Thank you for your interest in contributing to op-bridge! This document provides guidelines and instructions for contributing.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/<you>/op-bridge.git`
3. Create a feature branch: `git checkout -b my-feature`
4. Make your changes
5. Run the CI checks locally (see below)
6. Commit with a **signed commit** (see below) and push
7. Open a pull request
8. Sign the CLA on your first PR (one-time, comment on the PR)

## Commit Signing (Required)

All commits must be cryptographically signed. This is enforced by branch protection rules on all branches. Since op-bridge is a security tool, we require provenance verification for every contribution.

### SSH signing (recommended — simplest)

```bash
# Configure git to use your SSH key for signing
git config --global gpg.format ssh
git config --global user.signingkey ~/.ssh/id_ed25519  # or your key path
git config --global commit.gpgsign true
```

### GPG signing

```bash
git config --global commit.gpgsign true
git config --global user.signingkey <YOUR_GPG_KEY_ID>
```

If you haven't set up commit signing before, GitHub has a good guide: [Signing commits](https://docs.github.com/en/authentication/managing-commit-signature-verification/signing-commits).

## Development Setup

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [1Password CLI](https://developer.1password.com/docs/cli/) (`op`) — for integration testing only
- `cargo-audit` — `cargo install cargo-audit`

### Building

```bash
cargo build
```

### Running Tests

```bash
cargo test
```

### CI Checks

Before submitting a PR, ensure all checks pass locally:

```bash
cargo fmt --check        # formatting
cargo clippy -- -D warnings  # lints
cargo check              # type checking
cargo test               # tests
cargo audit              # security audit
```

## Code Style

- Run `cargo fmt` before committing — the CI enforces formatting.
- All public functions and types must have doc comments (`///`).
- Use `tracing` macros for logging (`info!`, `debug!`, `trace!`, `error!`, `warn!`).
- Secrets must always be wrapped in `secrecy::SecretString`. Never log or print secret values.
- Prefer returning `Result` over panicking.

## Architecture

| Module | Purpose |
|--------|---------|
| `store` | In-memory secret storage with zeroize-on-drop |
| `resolver` | 1Password CLI wrapper (`op read` / `op item edit`) |
| `socket` | Unix socket server and wire protocol |
| `client` | Unix socket client for CLI subcommands |
| `watcher` | File system watcher for credential auto-sync |

## Security

This project handles sensitive credentials. Please keep these guidelines in mind:

- Never log secret values. Use `debug!("resolved {} ({} chars)", name, value.len())` — not the value itself.
- All secret material must use `SecretString` from the `secrecy` crate.
- The `DELETE` command removes from memory only — op-bridge intentionally cannot delete items from 1Password.
- Report security vulnerabilities privately via GitHub Security Advisories, not public issues.

## Pull Request Guidelines

- Keep PRs focused — one feature or fix per PR.
- Include tests for new functionality.
- Update documentation if the public API or CLI changes.
- Ensure all CI checks pass before requesting review.
- Use conventional commit messages when possible (e.g., `feat:`, `fix:`, `docs:`).

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
