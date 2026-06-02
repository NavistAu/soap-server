# build stage — multi-stage Rust build from the repo root context
FROM rust:1.88@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0 AS build
WORKDIR /src
COPY . .
RUN cargo build -p crossref --bin controlled_server --release

# runtime stage
FROM debian:bookworm-slim@sha256:0104b334637a5f19aa9c983a91b54c89887c0984081f2068983107a6f6c21eeb
COPY --from=build /src/target/release/controlled_server /usr/local/bin/controlled_server
EXPOSE 8080
ENTRYPOINT ["controlled_server"]
