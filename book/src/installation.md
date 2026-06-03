# Installation

## Adding the dependency

```sh
cargo add soap-server
```

## Transitive dependencies

`soap-server` builds on `axum` (HTTP routing) and `tokio` (async runtime). Your
application needs its own Tokio runtime; the simplest way to add one is:

```sh
cargo add tokio --features full
```

## Minimum Supported Rust Version (MSRV)

The MSRV is the `rust-version` declared in the crate's `Cargo.toml`, shown on the
crate's [crates.io page](https://crates.io/crates/soap-server).
