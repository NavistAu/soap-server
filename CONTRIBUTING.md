# Contributing to soap-server

Thank you for your interest in contributing. Please read this document before opening a PR.

---

## Building

```sh
cargo build
```

## Testing

Run the full test suite, including workspace members, with all features enabled:

```sh
cargo test --workspace --all-features
```

## Linting

Both of the following must pass cleanly before submitting a PR:

```sh
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Run `cargo fmt` (without `--check`) to apply formatting in place.

---

## Branching model (Gitflow)

| Branch | Purpose |
|---|---|
| `main` | Stable, released code. Only release branches are merged here. |
| `develop` | Integration branch. All feature work targets here. |
| `feature/<name>` | New features and non-release fixes. Branch from `develop`. |
| `release/vX.Y.Z` | Release preparation. Branch from `develop`, merge into `main`. |
| `hotfix/<name>` | Critical fixes against `main`. Merge back into both `main` and `develop`. |

**Normal workflow:**

1. Branch from `develop`: `git checkout -b feature/my-feature develop`
2. Make changes, commit using Conventional Commits (see below).
3. Open a PR targeting `develop`.
4. CI must be green before merge.

**Releases** are made by a maintainer who cuts a `release/vX.Y.Z` branch from `develop`,
bumps the version in `Cargo.toml`, and opens a PR into `main`. Merging that PR
auto-tags and publishes to crates.io via Trusted Publishing.

---

## Commit messages

This project uses [Conventional Commits](https://www.conventionalcommits.org/).

Examples:

```
feat: add support for WS-Addressing headers
fix: reject empty nonce in PasswordDigest token
docs: document auth_bypass usage
chore: update axum to 0.9
```

Breaking changes must include a `BREAKING CHANGE:` footer or a `!` after the type:

```
feat!: remove deprecated from_wsdl_bytes constructor
```

---

## CI requirements

All PRs must pass:

- `cargo test --workspace --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`

Do not open a PR with known CI failures.
