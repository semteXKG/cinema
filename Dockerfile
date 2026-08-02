# frontend build
FROM node:22-alpine AS frontend
WORKDIR /build
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# backend dependency cache
FROM rust:1-slim-bookworm AS chef
WORKDIR /build
RUN cargo install cargo-chef --locked

FROM chef AS planner
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/src ./src
RUN cargo chef prepare --recipe-path recipe.json

# backend build
FROM chef AS backend
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/migrations ./migrations
COPY backend/src ./src
RUN cargo build --release

# runtime
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 ov \
    && mkdir -p /data \
    && chown ov /data
COPY --from=backend /build/target/release/ov-watcher /usr/local/bin/ov-watcher
COPY --from=frontend /build/dist /srv/static
USER ov
ENV DATA_DIR=/data PORT=8080 STATIC_DIR=/srv/static
EXPOSE 8080
CMD ["ov-watcher"]
