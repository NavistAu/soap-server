# build stage — multi-stage Rust build from the repo root context
FROM rust:1.88 AS build
WORKDIR /src
COPY . .
RUN cargo build -p crossref --bin controlled_server --release

# runtime stage
FROM debian:bookworm-slim
COPY --from=build /src/target/release/controlled_server /usr/local/bin/controlled_server
EXPOSE 8080
ENTRYPOINT ["controlled_server"]
