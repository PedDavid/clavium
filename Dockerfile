# syntax=docker/dockerfile:1
FROM rust:1.94-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends cmake clang && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --bin clavium \
 && cp target/release/clavium /clavium

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /clavium /usr/local/bin/clavium
USER nonroot:nonroot
EXPOSE 8080 9090
ENTRYPOINT ["/usr/local/bin/clavium"]
