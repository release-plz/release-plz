FROM lukemathwalker/cargo-chef:0.1.73-rust-1.93-slim-trixie AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release --locked --bin release-plz

FROM debian:trixie-slim AS runner

WORKDIR /app

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

# copied from https://github.com/rust-lang/docker-rust/blob/c606c2424f5341d20708910cd6f1fa51e9cf4e50/stable/bookworm/slim/Dockerfile#L10
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        wget \
        gcc \
        # release-plz doesn't need installing openssl, but the user might need it
        pkg-config \
        libssl-dev \
        libc6-dev \
        libssl3 \
        ssh-client \
        libcurl4 \
        git \
        ; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
        'amd64') \
            rustArch='x86_64-unknown-linux-gnu'; \
            rustupSha256='20a06e644b0d9bd2fbdbfd52d42540bdde820ea7df86e92e533c073da0cdd43c'; \
            ;; \
        'armhf') \
            rustArch='armv7-unknown-linux-gnueabihf'; \
            rustupSha256='3b8daab6cc3135f2cd4b12919559e6adaee73a2fbefb830fadf0405c20231d61'; \
            ;; \
        'arm64') \
            rustArch='aarch64-unknown-linux-gnu'; \
            rustupSha256='e3853c5a252fca15252d07cb23a1bdd9377a8c6f3efa01531109281ae47f841c'; \
            ;; \
        'i386') \
            rustArch='i686-unknown-linux-gnu'; \
            rustupSha256='a5db2c4b29d23e9b318b955dd0337d6b52e93933608469085c924e0d05b1df1f'; \
            ;; \
        'ppc64el') \
            rustArch='powerpc64le-unknown-linux-gnu'; \
            rustupSha256='acd89c42b47c93bd4266163a7b05d3f26287d5148413c0d47b2e8a7aa67c9dc0'; \
            ;; \
        's390x') \
            rustArch='s390x-unknown-linux-gnu'; \
            rustupSha256='726b7fd5d8805e73eab4a024a2889f8859d5a44e36041abac0a2436a52d42572'; \
            ;; \
        *) \
            echo >&2 "unsupported architecture: $arch"; \
            exit 1; \
            ;; \
    esac; \
    \
    url="https://static.rust-lang.org/rustup/archive/1.28.2/${rustArch}/rustup-init"; \
    wget --progress=dot:giga "$url"; \
    echo "${rustupSha256} *rustup-init" | sha256sum -c -; \
    \
    chmod +x rustup-init; \
    ./rustup-init -y --no-modify-path --profile minimal --default-host ${rustArch}; \
    rm rustup-init; \
    chmod -R a+w $RUSTUP_HOME $CARGO_HOME; \
    cargo --version; \
    apt-get remove -y --auto-remove \
        wget \
        ; \
    rm -rf /var/lib/apt/lists/*; \
    mv /usr/local/cargo/bin/cargo ~; \
    mv /usr/local/cargo/bin/rustc ~; \
    rm /usr/local/cargo/bin/*; \
    mv ~/cargo /usr/local/cargo/bin/; \
    mv ~/rustc /usr/local/cargo/bin/;
COPY --from=builder /app/target/release/release-plz /usr/local/bin
ENTRYPOINT ["release-plz"]
