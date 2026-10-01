# Build context is this directory. Build with:
#   docker build -t docket-server .

# ---- Stage 1: dependency cache -------------------------------------------
# Compiles stub crates so the dependency build lands in a layer that only
# invalidates when a Cargo.toml or Cargo.lock changes. Every workspace member's
# Cargo.toml is copied, or cargo refuses to load the workspace.
FROM rust:1.98-alpine AS cacher
WORKDIR /build
# musl-dev and gcc are for the bundled sqlite and for ring, which rustls pulls in.
RUN apk --no-cache upgrade && \
    apk add --no-cache musl-dev gcc make perl
COPY Cargo.toml Cargo.lock ./
COPY docket-core/Cargo.toml docket-core/Cargo.toml
COPY docket-server/Cargo.toml docket-server/Cargo.toml
RUN mkdir -p docket-core/src docket-server/src && \
    : > docket-core/src/lib.rs && \
    : > docket-server/src/lib.rs && \
    echo 'fn main() {}' > docket-server/src/main.rs && \
    cargo build --release --locked -p docket-server && \
    rm -rf target/release/deps/*docket_core* \
           target/release/deps/*docket_server* \
           target/release/.fingerprint/docket-core-* \
           target/release/.fingerprint/docket-server-* \
           target/release/docket-server

# ---- Stage 2: build ------------------------------------------------------
FROM rust:1.98-alpine AS builder
WORKDIR /build
RUN apk --no-cache upgrade && \
    apk add --no-cache musl-dev gcc make perl
COPY --from=cacher /build/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
COPY Cargo.toml Cargo.lock ./
COPY docket-core docket-core
COPY docket-server docket-server
RUN cargo build --release --locked -p docket-server

# ---- Stage 3: runtime ----------------------------------------------------
FROM alpine:3.21
RUN apk --no-cache upgrade && \
    apk add --no-cache ca-certificates tzdata libgcc && \
    adduser -D -u 10001 docket && \
    install -d -o docket -g docket /data /etc/docket
COPY --from=builder /build/target/release/docket-server /usr/local/bin/docket-server
# The database lives on a volume at /data, the keys file is mounted at /etc/docket.
ENV DOCKET_DB=/data/docket.db \
    DOCKET_KEYS=/etc/docket/keys \
    DOCKET_LISTEN=0.0.0.0:7878
VOLUME /data
USER 10001
EXPOSE 7878
ENTRYPOINT ["/usr/local/bin/docket-server"]
