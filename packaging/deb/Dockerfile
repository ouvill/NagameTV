# Native Ubuntu 26.04 debs use the distribution's Qt and GStreamer ABI.
# Ubuntu 24.04 debs reuse packaging/appimage/Dockerfile instead.
FROM ubuntu:26.04@sha256:da6fc2be547864451aa253836dd926da33623312df4a9a243e35dc877c378a78

COPY runtime-dependencies-26.04.txt /tmp/runtime-dependencies.txt
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    build-essential binutils ca-certificates cmake curl desktop-file-utils dpkg-dev git \
    libclang-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
    libgstreamer-plugins-bad1.0-dev pkg-config python3 qmake6 \
    qt6-base-dev qt6-declarative-dev qt6-declarative-dev-tools qt6-l10n-tools util-linux \
    $(sed '/^#/d; /^$/d' /tmp/runtime-dependencies.txt) \
    && rm -rf /var/lib/apt/lists/* /tmp/runtime-dependencies.txt

ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo
ENV PATH="/opt/cargo/bin:${PATH}"
RUN curl --fail --location --retry 3 -o /tmp/rustup-init \
        https://static.rust-lang.org/rustup/archive/1.28.2/x86_64-unknown-linux-gnu/rustup-init \
    && echo '20a06e644b0d9bd2fbdbfd52d42540bdde820ea7df86e92e533c073da0cdd43c  /tmp/rustup-init' | sha256sum --check \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain 1.98.1 \
    && chmod -R a+rX /opt/rustup /opt/cargo \
    && rm /tmp/rustup-init
WORKDIR /project
