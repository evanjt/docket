# Build context is this directory. Build with:
#   docker build -t docket-server .

# ---- Stage 1: dependency cache -------------------------------------------
# Compiles stub crates so the dependency build lands in a layer that only
# invalidates when a Cargo.toml or Cargo.lock changes. Every workspace member's
# Cargo.toml is copied, or cargo refuses to load the workspace.
FROM rust:1.98-alpine AS cacher
WORKDIR /build
# musl-dev and gcc are for ring, which rustls pulls in.
RUN apk --no-cache upgrade && \
    apk add --no-cache musl-dev gcc make perl
COPY Cargo.toml Cargo.lock ./
COPY docket/Cargo.toml docket/Cargo.toml
COPY docket-client/Cargo.toml docket-client/Cargo.toml
COPY docket-core/Cargo.toml docket-core/Cargo.toml
COPY docket-dump/Cargo.toml docket-dump/Cargo.toml
COPY docket-migration/Cargo.toml docket-migration/Cargo.toml
COPY docket-server/Cargo.toml docket-server/Cargo.toml
COPY docket-tui/Cargo.toml docket-tui/Cargo.toml
RUN mkdir -p docket/src docket-client/src docket-core/src docket-dump/src docket-migration/src \
             docket-server/src docket-tui/src && \
    echo 'fn main() {}' > docket/src/main.rs && \
    : > docket-client/src/lib.rs && \
    : > docket-core/src/lib.rs && \
    echo 'fn main() {}' > docket-dump/src/main.rs && \
    : > docket-migration/src/lib.rs && \
    : > docket-tui/src/lib.rs && \
    echo 'fn main() {}' > docket-tui/src/main.rs && \
    : > docket-server/src/lib.rs && \
    echo 'fn main() {}' > docket-server/src/main.rs && \
    cargo build --release --locked -p docket-server && \
    rm -rf target/release/deps/*docket_core* \
           target/release/deps/*docket_migration* \
           target/release/deps/*docket_server* \
           target/release/.fingerprint/docket-core-* \
           target/release/.fingerprint/docket-migration-* \
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
COPY docket docket
COPY docket-client docket-client
COPY docket-core docket-core
COPY docket-dump docket-dump
COPY docket-migration docket-migration
COPY docket-server docket-server
COPY docket-tui docket-tui
RUN cargo build --release --locked -p docket-server

# ---- Stage 3: web client -------------------------------------------------
# The page the server serves at /ui/. Its dependencies land in a layer that only invalidates when the
# lockfile changes.
FROM node:24-alpine AS web
WORKDIR /web
COPY docket-web/package.json docket-web/package-lock.json ./
RUN npm ci
COPY docket-web ./
RUN npm run build

# ---- Stage 4: runtime ----------------------------------------------------
FROM alpine:3.21
RUN apk --no-cache upgrade && \
    apk add --no-cache ca-certificates tzdata libgcc && \
    adduser -D -u 10001 docket && \
    install -d -o docket -g docket /etc/docket
COPY --from=builder /build/target/release/docket-server /usr/local/bin/docket-server
COPY --from=web /web/dist /usr/share/docket/web
# The database is the Postgres DATABASE_URL names; the keys file is mounted at /etc/docket.
ENV DOCKET_KEYS=/etc/docket/keys \
    DOCKET_LISTEN=0.0.0.0:7878 \
    DOCKET_WEB=/usr/share/docket/web
USER 10001
EXPOSE 7878
ENTRYPOINT ["/usr/local/bin/docket-server"]
