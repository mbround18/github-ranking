# syntax=docker/dockerfile:1

# --- Rust: server binary and the wasm bundle -------------------------------
#
# Statically linked against musl so the runtime image needs no libc. rusqlite
# bundles SQLite, so a C toolchain is needed here but nothing at runtime.
FROM rust:1.98-alpine AS rust-builder

RUN apk add --no-cache musl-dev curl

# Prebuilt wasm-pack rather than `cargo install`, which would rebuild it from
# source on every cache miss.
ARG WASM_PACK_VERSION=0.13.1
RUN curl -sSfL \
      "https://github.com/rustwasm/wasm-pack/releases/download/v${WASM_PACK_VERSION}/wasm-pack-v${WASM_PACK_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
    | tar -xz --strip-components=1 -C /usr/local/bin \
      "wasm-pack-v${WASM_PACK_VERSION}-x86_64-unknown-linux-musl/wasm-pack" \
 && rustup target add wasm32-unknown-unknown

# Cargo features for the server build. Set to `pat-in-production` for a
# single-tenant self-hosted image, which permits authenticating with a personal
# access token instead of a GitHub App:
#
#     docker build --build-arg CARGO_FEATURES=pat-in-production .
#
# Empty by default, so a stock image cannot run production traffic on a PAT.
ARG CARGO_FEATURES=""

WORKDIR /build

# Manifests first, so editing our own source doesn't invalidate the dependency
# cache below it.
COPY Cargo.toml Cargo.lock ./
COPY crates/core/Cargo.toml crates/core/
COPY crates/server/Cargo.toml crates/server/
COPY crates/wasm/Cargo.toml crates/wasm/
COPY tools/fontgen/Cargo.toml tools/fontgen/
RUN mkdir -p crates/core/src crates/server/src crates/wasm/src tools/fontgen/src \
 && echo 'fn main() {}' > crates/server/src/main.rs \
 && echo 'fn main() {}' > tools/fontgen/src/main.rs \
 && touch crates/core/src/lib.rs crates/wasm/src/lib.rs \
 && cargo build --release -p github-ranked ${CARGO_FEATURES:+--features "$CARGO_FEATURES"} 2>/dev/null || true

COPY crates/ crates/
COPY fixtures/ fixtures/

# cargo skips the rebuild if the placeholders look newer than the real sources.
RUN touch crates/core/src/lib.rs crates/server/src/main.rs \
 && cargo build --release -p github-ranked ${CARGO_FEATURES:+--features "$CARGO_FEATURES"} \
 && strip target/release/github-ranked

# Built from source in the image rather than trusting a committed artifact, so
# the frontend can never ship ranking logic that has drifted from the server's.
RUN wasm-pack build crates/wasm \
      --target web --out-dir /wasm --out-name github_ranked --release --no-pack \
 && rm -f /wasm/.gitignore


# --- Node: frontend --------------------------------------------------------
FROM node:24-alpine AS web-builder

WORKDIR /web

COPY web/package.json web/package-lock.json ./
RUN npm ci

COPY web/ ./
COPY --from=rust-builder /wasm ./src/wasm

RUN npm run build


# --- Runtime ---------------------------------------------------------------
FROM alpine:3.21 AS runtime

# TLS roots for the GitHub API; tini so PID 1 reaps and forwards signals.
RUN apk add --no-cache ca-certificates tini wget \
 && adduser -D -u 10001 -h /app app

WORKDIR /app

COPY --from=rust-builder /build/target/release/github-ranked /usr/local/bin/github-ranked
COPY --from=web-builder /web/dist /app/web

RUN mkdir -p /data && chown -R app:app /app /data

USER app

# 10090, not 10080: browsers refuse to connect to 10080 (ERR_UNSAFE_PORT).
ENV HOST=0.0.0.0 \
    PORT=10090 \
    APP_ENV=production \
    WEB_ROOT=/app/web \
    CACHE_PATH=/data/cache.db

EXPOSE 10090
VOLUME ["/data"]

# The Kubernetes liveness probe is the real check; this only catches a process
# that has stopped accepting connections entirely.
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD wget -qO- http://127.0.0.1:10090/healthz || exit 1

ENTRYPOINT ["/sbin/tini", "--"]
CMD ["github-ranked"]
