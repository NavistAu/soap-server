# Installation

## Adding the dependency

```
cargo add soap-server
```

Or add it manually to your `Cargo.toml`:

```toml
[dependencies]
soap-server = "0.1.0"
```

## Minimum Supported Rust Version (MSRV)

`soap-server` requires **Rust 1.88.0** or later.

## Transitive dependencies

The library pulls in `axum` (HTTP routing) and `tokio` (async runtime) as direct
dependencies. A complete async Tokio runtime is needed at the application level. The
quickest way to satisfy this is:

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
soap-server = "0.1.0"
```
