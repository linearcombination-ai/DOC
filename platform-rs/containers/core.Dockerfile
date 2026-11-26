# syntax=docker/dockerfile:1.7
#
# The Rust shell + core + job runner (`doc-jobd`): one small image, two roles (DOC_JOBD_ROLE=api | worker).
# Build context: the repository root (the HTML header templates come from DOC's own backend/templates).
#
#   docker build -f platform-rs/containers/core.Dockerfile -t doc-rs/core:local .
#
# Stages: kitchen (read-only clone of scripture-kitchen, the core's dependency) -> planner/builder (cargo-chef: the
# dependency layer is cached until Cargo.lock changes) -> runtime (debian slim, git for clones, non-root, health check).

ARG CHEF_IMAGE=lukemathwalker/cargo-chef:latest-rust-1.100.0-bookworm
ARG RUNTIME_IMAGE=debian:bookworm-slim
ARG KITCHEN_REPO=https://github.com/WycliffeAssociates/scripture-kitchen
ARG KITCHEN_REF=v0.1.24

# hadolint ignore=DL3006
FROM ${CHEF_IMAGE} AS chef
WORKDIR /build/platform-rs

FROM chef AS kitchen
ARG KITCHEN_REPO
ARG KITCHEN_REF
RUN git clone --depth 1 --branch "${KITCHEN_REF}" "${KITCHEN_REPO}" /build/platform-rs/_exp/kitchen \
 && git -C /build/platform-rs/_exp/kitchen rev-parse HEAD > /kitchen.rev \
 && rm -rf /build/platform-rs/_exp/kitchen/.git

FROM chef AS planner
COPY platform-rs/Cargo.toml platform-rs/Cargo.lock ./
COPY platform-rs/crates ./crates
COPY --from=kitchen /build/platform-rs/_exp ./_exp
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ENV CARGO_TERM_COLOR=never CARGO_INCREMENTAL=0
COPY --from=planner /build/platform-rs/recipe.json recipe.json
COPY --from=kitchen /build/platform-rs/_exp ./_exp
RUN cargo chef cook --release --recipe-path recipe.json
COPY platform-rs/Cargo.toml platform-rs/Cargo.lock ./
COPY platform-rs/crates ./crates
RUN cargo build --release --locked -p doc-jobd \
 && strip target/release/doc-jobd

# hadolint ignore=DL3006
FROM ${RUNTIME_IMAGE} AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl git \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system --gid 10001 doc \
 && useradd --system --uid 10001 --gid 10001 --home-dir /app --shell /usr/sbin/nologin doc \
 && mkdir -p /app/document_output /app/assets_download /app/templates \
 && chown -R doc:doc /app
COPY --from=builder /build/platform-rs/target/release/doc-jobd /usr/local/bin/doc-jobd
COPY --from=kitchen /kitchen.rev /app/kitchen.rev
COPY --chown=doc:doc backend/templates/html /app/templates/html

ENV DOC_SHELL_ADDR=0.0.0.0:8081 \
    DOC_JOBD_ROLE=all \
    DOC_SHELL_MODE=fixed \
    DOC_SHELL_LOG_FORMAT=json \
    DOC_TEMPLATES_DIR=/app/templates/html \
    DOC_JOBD_OUTPUT_DIR=/app/document_output \
    RESOURCE_ASSETS_DIR=/app/assets_download \
    DOC_HEALTH_URL=http://127.0.0.1:8081/health/status \
    RUST_LOG=info
WORKDIR /app
USER 10001:10001
EXPOSE 8081 8082
HEALTHCHECK --interval=10s --timeout=5s --start-period=15s --retries=6 \
  CMD ["sh", "-c", "curl -fsS \"$DOC_HEALTH_URL\" > /dev/null"]
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/doc-jobd"]
