# syntax=docker/dockerfile:1
# Override both image arguments with immutable digests for a deployment build.
ARG RUST_IMAGE=rust:1.95.0-bookworm
ARG RUNTIME_IMAGE=debian:bookworm-slim
FROM ${RUST_IMAGE} AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 clang cmake pkg-config libssl-dev libasound2-dev libudev-dev libcap-dev protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
WORKDIR /src/codex-rs
ENV CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_DEBUG=0 CARGO_BUILD_JOBS=2
# Keep the official CLI login and local runtime companions available. The CPA
# inference path does not call these execution components.
RUN cargo build --locked --release -p codex-bwrap --bin bwrap \
    && bwrap_sha256="$(sha256sum target/release/bwrap | cut -d ' ' -f 1)" \
    && target="$(rustc -vV | sed -n 's/^host: //p')" \
    && python3 /src/deploy/cpa-runtime/fetch-v8.py --target "$target" --output-dir /opt/rusty-v8 \
    && CODEX_BWRAP_SHA256="$bwrap_sha256" \
       RUSTY_V8_ARCHIVE="/opt/rusty-v8/librusty_v8_ptrcomp_sandbox_release_${target}.a.gz" \
       RUSTY_V8_SRC_BINDING_PATH="/opt/rusty-v8/src_binding_ptrcomp_sandbox_release_${target}.rs" \
       cargo build --locked --release \
    -p codex-cli --bin codex \
    -p codex-app-server --bin codex-app-server --bin exec-server \
    -p codex-code-mode-host --bin codex-code-mode-host
RUN mkdir /out && cp target/release/codex target/release/codex-app-server \
    target/release/exec-server target/release/codex-code-mode-host target/release/bwrap /out/

FROM ${RUNTIME_IMAGE}
ARG FORK_REVISION=unrecorded-local-build
LABEL org.opencontainers.image.revision=${FORK_REVISION} \
    io.codex.cpa.protocol-version="3" \
    io.codex.cpa.upstream-revision="afb436df8b70bb5bc57b86d9a3e829968988cd21"
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 libasound2 libudev1 libcap2 git ripgrep bash \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --create-home codex \
    && install -d -o codex -g codex -m 0700 /var/lib/codex /run/codex
COPY --from=build /out/ /usr/local/bin/
COPY deploy/cpa-runtime/entrypoint.sh /usr/local/bin/codex-cpa-entrypoint
ENV CODEX_HOME=/var/lib/codex CODEX_CPA_PORT=38317 CODEX_CPA_AUTH_DIR=/cpa-auth
USER codex
WORKDIR /home/codex
ENTRYPOINT ["/usr/local/bin/codex-cpa-entrypoint"]
