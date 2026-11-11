# syntax=docker/dockerfile:1
# Multi-stage: React/Vite first, then Rust (assets are embedded with include_str!),
# then a slim runtime that runs as a non-root user.
FROM node:22-alpine AS ui-build
WORKDIR /src/ui
COPY ui/package*.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

FROM rust:1.93-bookworm AS rust-build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
# Only the manifest is needed for the dependency cache layer.
COPY src ./src
COPY migrations ./migrations
COPY assets ./assets
COPY --from=ui-build /src/ui/dist ./ui/dist
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates wget \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 newsman \
    && useradd --system --uid 10001 --gid newsman --home-dir /app --shell /usr/sbin/nologin newsman
WORKDIR /app
COPY --from=rust-build --chown=10001:10001 /src/target/release/lnc-newsman /app/lnc-newsman
COPY --from=rust-build --chown=10001:10001 /src/ui/dist /app/ui/dist
ENV DATA_DIR=/data PORT=8000 TZ=Europe/Paris REGIONS_DIR=/data/regions
EXPOSE 8000
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD wget -qO- http://127.0.0.1:8000/health || exit 1
# Non-root. The host data directory must be writable by uid/gid 10001; the stack
# creates it with a root init step before this container starts.
USER 10001:10001
ENTRYPOINT ["/app/lnc-newsman"]
