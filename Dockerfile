# syntax=docker/dockerfile:1

# SPDX-FileCopyrightText: ignorefile contributors
#
# SPDX-License-Identifier: MIT OR Apache-2.0

# Multi-stage Dockerfile for multi-architecture builds (linux/amd64, linux/arm64)

# Dependency stage
# Two DISTINCT names on purpose. A global ARG is only usable in a FROM if it is
# declared before the first one, and re-declaring `IMAGE_TAG` later does NOT
# rebind it for a later FROM -- the runtime stage below resolved to
# `debian:<the rust digest>` and failed with "not found". One ARG per image.
ARG IMAGE_TAG=1-slim@sha256:5c6f46a6e4472ab1ca7ba7d494e6677f2f219ebc02f32025d3986f057635ec9c
ARG RUNTIME_IMAGE_TAG=trixie-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd
ARG APP_VERSION=0.1.0

FROM --platform=$BUILDPLATFORM docker.io/library/rust:${IMAGE_TAG} AS dependencies

SHELL ["/bin/bash", "-eou", "pipefail", "-c"]

ARG IMAGE_TAG
ARG APP_VERSION
ARG TARGETPLATFORM
ARG BUILDPLATFORM

ENV BUILDPLATFORM=$BUILDPLATFORM
ENV TARGETPLATFORM=$TARGETPLATFORM
ENV BUILD_PATH=/usr/src/app

# 1. Install system packages and cross-compilation toolchains
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked\
 --mount=type=cache,target=/var/lib/apt,sharing=locked <<EOF
  #!/bin/bash
  set -exou pipefail

  apt-get update
  apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    gcc \
    g++ \
    cmake \
    curl \
    build-essential \
    clang \
    mold

  case "$TARGETPLATFORM" in
    "linux/arm64")
      dpkg --add-architecture arm64
      apt-get update
      apt-get install -y --no-install-recommends \
        gcc-aarch64-linux-gnu \
        g++-aarch64-linux-gnu \
        libc6-dev-arm64-cross \
        libssl-dev:arm64
      ;;
    "linux/amd64")
      echo "Native amd64 platform, no cross-compilation tools needed."
      ;;
    *)
      echo "Unsupported target platform: $TARGETPLATFORM"
      exit 1
      ;;
  esac
EOF

# 2. Add correct Rust targets based on platform
RUN <<EOF
  #!/bin/bash
  set -exou pipefail
  case "$TARGETPLATFORM" in
    "linux/arm64")
      rustup target add aarch64-unknown-linux-gnu
      ;;
    "linux/amd64")
      rustup target add x86_64-unknown-linux-gnu
      ;;
  esac
EOF

WORKDIR ${BUILD_PATH}

# 3. Copy manifests. There is no root `build.rs` in this repository -- the only
# build script is `crates/ignorefile-cli/build.rs`, which arrives with the source
# in the builder stage. Copying a non-existent path made this stage fail outright.
#
# Every workspace member needs its manifest here: the root declares
# `members = ["crates/*"]`, so cargo refuses to load the workspace while any of
# the five is missing. Only two were listed.
COPY Cargo.toml Cargo.lock ./
COPY crates/ignorefile/Cargo.toml crates/ignorefile/
COPY crates/ignorefile-cli/Cargo.toml crates/ignorefile-cli/
COPY crates/ignorefile-lsp/Cargo.toml crates/ignorefile-lsp/
COPY crates/ignorefile-mcp/Cargo.toml crates/ignorefile-mcp/
COPY crates/ignorefile-wasm/Cargo.toml crates/ignorefile-wasm/
COPY .cargo/config.toml ./.cargo/config.toml

# 4. Generate boilerplate and build dependencies caching both target architectures correctly
RUN --mount=type=cache,target=/usr/local/cargo/registry\
 --mount=type=cache,target=/usr/local/cargo/git\
 --mount=type=cache,target=${BUILD_PATH}/target,sharing=locked <<EOF
  #!/bin/bash
  set -exou pipefail

  # Minimal source placeholders, one per target each manifest declares. A lib
  # crate needs src/lib.rs; ignorefile-cli, -lsp and -mcp each declare [[bin]]
  # over src/main.rs AND carry a lib, so both roots must exist.
  mkdir -p crates/{ignorefile,ignorefile-cli,ignorefile-lsp,ignorefile-mcp,ignorefile-wasm}/src
  touch crates/ignorefile/src/lib.rs \
        crates/ignorefile-cli/src/lib.rs \
        crates/ignorefile-lsp/src/lib.rs \
        crates/ignorefile-mcp/src/lib.rs \
        crates/ignorefile-wasm/src/lib.rs
  for c in ignorefile-cli ignorefile-lsp ignorefile-mcp; do
    echo 'fn main() { println!("===> Preparing Cargo Dependencies! <==="); }' > "crates/$c/src/main.rs"
  done

  # No native C in this repository, so nothing to mock. The previous
  # `src/native/ignorefile_native.c` stub was inherited from another project and
  # created a directory cargo never looked at.

  # Build targeting the exact architecture matching the multi-arch flow
  case "$TARGETPLATFORM" in
    "linux/arm64")
      export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
      cargo build --workspace --all-targets --all-features --target aarch64-unknown-linux-gnu
      ;;
    "linux/amd64")
      # Mold needs clang flag inside cargo to match your .cargo/config.toml
      cargo build --workspace --all-targets --all-features --target x86_64-unknown-linux-gnu
      ;;
  esac
EOF

# Build stage
FROM dependencies AS builder

ARG TARGETPLATFORM
ARG BUILDPLATFORM
ARG APP_VERSION

ENV BUILDPLATFORM=$BUILDPLATFORM
ENV TARGETPLATFORM=$TARGETPLATFORM
ENV BUILD_PATH=/usr/src/app
ENV RUSTUP_PROFILE=minimal
ENV APP_VERSION=${APP_VERSION}

WORKDIR ${BUILD_PATH}

# Copy the actual source code over (invalidating the cache from this point forward)
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry\
 --mount=type=cache,target=/usr/local/cargo/git\
 --mount=type=cache,target=${BUILD_PATH}/target,sharing=locked <<EOF
  #!/bin/bash
  set -exou pipefail

  mkdir -p /app
  rm -f rust-toolchain.toml

  # Clean old binary artifacts to ensure fresh build variables
  rm -rf target/*/release/ignorefile* target/*/release/libignorefile* \
         target/*/incremental target/*/build/ignorefile*

  case "$TARGETPLATFORM" in
    "linux/arm64")
      export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
      export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
      export AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar
      export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
      export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
      export PKG_CONFIG_ALLOW_CROSS=1
      export CARGO_BUILD_VERSION=${APP_VERSION}
      export TARGET=aarch64-unknown-linux-gnu
      export CARGO_BUILD_TARGET=aarch64-unknown-linux-gnu

      cargo build --release --target aarch64-unknown-linux-gnu
      # Both [[bin]] targets: the package ships `ignorefile` and the short
      # alias `ign`, and the runtime stage copies both.
      cp -v target/aarch64-unknown-linux-gnu/release/ignorefile /app/ignorefile
      cp -v target/aarch64-unknown-linux-gnu/release/ign /app/ign
      ;;

    "linux/amd64")
      export CARGO_BUILD_VERSION=${APP_VERSION}
      export TARGET=x86_64-unknown-linux-gnu
      export CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu

      # Use mold linker for maximum link speed on amd64
      export RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=mold ${RUSTFLAGS:-}"

      cargo build --release --target x86_64-unknown-linux-gnu
      # Both [[bin]] targets: the package ships `ignorefile` and the short
      # alias `ign`, and the runtime stage copies both.
      cp -v target/x86_64-unknown-linux-gnu/release/ignorefile /app/ignorefile
      cp -v target/x86_64-unknown-linux-gnu/release/ign /app/ign
      ;;
  esac
EOF

# Testing lives in `Dockerfile.Test`, reached by `mise run docker:test`. It is a
# separate file rather than a stage here because it needs no release build and no
# cross-compilation setup: it bakes the pinned toolchain, nightly and
# cargo-tarpaulin into a cached image and runs the gate against a bind mount.
# An empty `FROM builder AS test` used to sit here and produced nothing.

# Runtime stage - minimal image with just the binary and runtime deps
# Braced `${RUNTIME_IMAGE_TAG}`, matching the dependencies stage above:
# Scorecard's pinned-dependencies check resolves the braced form back to the
# ARG's digest but not the bare `$RUNTIME_IMAGE_TAG`, so this line alone read as
# unpinned. The default lives with the other globals at the top of the file.
FROM docker.io/library/debian:${RUNTIME_IMAGE_TAG} AS runtime

# Install only runtime dependencies (SSL certs, libssl, tini for signal handling)
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked\
 --mount=type=cache,target=/var/lib/apt,sharing=locked <<EOF
  #!/bin/bash
  set -euxo pipefail
  apt-get update
  apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    tini
  rm -rf /var/lib/apt/lists/*
EOF

# Create app directory and non-root user/group with fixed UID/GID
RUN groupadd -g 10000 ignorefile &&\
 useradd -u 10000 -g ignorefile -m -s /sbin/nologin ignorefile

WORKDIR /app

# Copy the binary from builder with ownership root:ignorefile
COPY --from=builder --chown=root:ignorefile /app/ignorefile /app/ignorefile
COPY --from=builder --chown=root:ignorefile /app/ign /app/ign

# Security labels for orchestration tools
LABEL org.opencontainers.image.security.no-new-privileges="true" \
      org.opencontainers.image.security.read-only-rootfs="true" \
      org.opencontainers.image.security.capabilities.drop="ALL" \
      org.opencontainers.image.security.run-as-non-root="true" \
      org.opencontainers.image.security.run-as-user="10000" \
      org.opencontainers.image.security.run-as-group="10000"

# Entrypoint wrapper: allows `docker run ignorefile:0.1.0 bash` for debugging.
#
# The heredoc delimiter is QUOTED (<<'SCRIPT'). Unquoted, the outer shell expands
# `${1:-}` and `"$@"` while BUILDING the image, so the script was baked with the
# build-time value of $1 and ignored every argument it was ever given at run
# time. `docker run ignorefile --version` executed `ignorefile pipefail`.
#
# `set -ex pipefail` was also wrong: `pipefail` is an option to `-o`, not a flag,
# so bash took it as a positional argument -- which is exactly where that stray
# $1 came from. `-o pipefail` is the spelling.
RUN <<'EOF'
#!/bin/bash
set -euo pipefail

cat > /entrypoint.sh <<'SCRIPT'
#!/bin/bash
set -euo pipefail

# If first arg is a shell, exec it directly (for debugging)
case "${1:-}" in
  bash|sh|/bin/bash|/bin/sh)
    exec tini -- "$@"
    ;;
esac

# Default: run ignorefile with args
exec tini -- /app/ignorefile "$@"
SCRIPT

chmod +x /entrypoint.sh
chown root:ignorefile /entrypoint.sh
EOF

# Switch to non-root user
USER ignorefile:ignorefile

ENTRYPOINT ["/entrypoint.sh"]

CMD ["--help"]
