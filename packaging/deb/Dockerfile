# Native Ubuntu 26.04 debs use the distribution's Qt and GStreamer ABI.
# Ubuntu 24.04 debs reuse packaging/appimage/Dockerfile instead.
FROM ubuntu:26.04@sha256:f144425ff09be612d6d9ad965196e9cdc23dae1f42110a8a11a3e9a8198759f7 AS distribution

COPY runtime-dependencies-26.04.txt /tmp/runtime-dependencies.txt
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    build-essential binutils ca-certificates ccache cmake curl desktop-file-utils dpkg-dev git \
    libclang-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
    libgstreamer-plugins-bad1.0-dev libass-dev lld pkg-config python3 qmake6 \
    qt6-base-dev qt6-declarative-dev qt6-declarative-dev-tools qt6-l10n-tools qt6-webengine-dev util-linux \
    $(sed '/^#/d; /^$/d' /tmp/runtime-dependencies.txt) \
    && rm -rf /var/lib/apt/lists/* /tmp/runtime-dependencies.txt
RUN ld.lld --version && pkg-config --atleast-version=6.8.0 Qt6WebEngineQuick

ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo
ENV PATH="/opt/cargo/bin:${PATH}"
RUN case "$(dpkg --print-architecture)" in \
        amd64) target=x86_64-unknown-linux-gnu; checksum=20a06e644b0d9bd2fbdbfd52d42540bdde820ea7df86e92e533c073da0cdd43c ;; \
        arm64) target=aarch64-unknown-linux-gnu; checksum=e3853c5a252fca15252d07cb23a1bdd9377a8c6f3efa01531109281ae47f841c ;; \
        *) echo 'Unsupported build architecture' >&2; exit 1 ;; \
    esac \
    && curl --fail --location --retry 3 -o /tmp/rustup-init \
        "https://static.rust-lang.org/rustup/archive/1.28.2/$target/rustup-init" \
    && echo "$checksum  /tmp/rustup-init" | sha256sum --check \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain 1.98.1 \
    && chmod -R a+rX /opt/rustup /opt/cargo \
    && rm /tmp/rustup-init
WORKDIR /project

# ARM CI exercises the same hardware-free suites as the x86_64 AppImage job.
FROM distribution AS ci
RUN apt-get update && apt-get install -y --no-install-recommends \
        dbus-daemon python3-dbus python3-gi nodejs npm gnupg \
    && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt && chmod -R a+rX /opt/rustup /opt/cargo
RUN case "$(dpkg --print-architecture)" in \
        amd64) target=x86_64-unknown-linux-gnu; checksum=682c21b777c333e96fd532e114d3a5a894e0729ab88d94c0a9f20f8419695428 ;; \
        arm64) target=aarch64-unknown-linux-gnu; checksum=b2e33d7c72de7ade0ff7b3a948ac37516b24f8a836b7a8870c1f634a94be9de9 ;; \
        *) echo 'Unsupported test architecture' >&2; exit 1 ;; \
    esac \
    && curl --fail --location --retry 3 -o /tmp/nextest.tar.gz \
        "https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-0.9.146/cargo-nextest-0.9.146-$target.tar.gz" \
    && echo "$checksum  /tmp/nextest.tar.gz" | sha256sum --check \
    && tar -xzf /tmp/nextest.tar.gz -C /opt/cargo/bin cargo-nextest \
    && chmod a+rx /opt/cargo/bin/cargo-nextest \
    && cargo nextest --version \
    && rm /tmp/nextest.tar.gz
