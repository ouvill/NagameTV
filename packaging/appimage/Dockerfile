# Linux x86_64 distribution baseline. Never collect libraries from the newer
# Workshop OS: Ubuntu 24.04 supplies glibc 2.39 and GStreamer 1.24.
FROM ubuntu:26.04@sha256:da6fc2be547864451aa253836dd926da33623312df4a9a243e35dc877c378a78 AS appimage

RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    build-essential ca-certificates cmake curl desktop-file-utils git \
    libclang-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
    libgstreamer-plugins-bad1.0-dev libass-dev libegl1-mesa-dev libgl1-mesa-dev \
    libx11-xcb1 libxcb-cursor0 libxcb-icccm4 libxcb-keysyms1 \
    libxcb-image0 libxcb-render-util0 libxkbcommon-x11-0 \
    gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad gstreamer1.0-libav gstreamer1.0-gl \
    gstreamer1.0-pulseaudio gstreamer1.0-tools \
    meson ninja-build pkg-config python3 python3-venv util-linux \
    && rm -rf /var/lib/apt/lists/*

# Ubuntu 24.04's Qt is too old for the application. Use the official Qt SDK,
# built against an older glibc, and build qml6glsink with exactly this Qt.
RUN python3 -m venv /opt/aqt \
    && /opt/aqt/bin/pip install --no-cache-dir aqtinstall==3.3.0 \
    && /opt/aqt/bin/aqt install-qt linux desktop 6.8.3 linux_gcc_64 \
        --outputdir /opt/qt --modules qtimageformats qtshadertools
ENV PATH="/opt/qt/6.8.3/gcc_64/bin:/opt/cargo/bin:${PATH}" \
    QMAKE=/opt/qt/6.8.3/gcc_64/bin/qmake \
    QT_LRELEASE=/opt/qt/6.8.3/gcc_64/bin/lrelease \
    PKG_CONFIG_PATH=/opt/qt/6.8.3/gcc_64/lib/pkgconfig \
    CMAKE_PREFIX_PATH=/opt/qt/6.8.3/gcc_64 \
    LD_LIBRARY_PATH=/opt/qt/6.8.3/gcc_64/lib \
    RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo

RUN curl --fail --location --retry 3 -o /tmp/gst-plugins-good.tar.xz \
        https://gstreamer.freedesktop.org/src/gst-plugins-good/gst-plugins-good-1.24.12.tar.xz \
    && echo 'd0e66e2f935d1575f6adbef7d0a2b3faba7360344383c51bf0233b39e0489a64  /tmp/gst-plugins-good.tar.xz' | sha256sum --check \
    && tar -xJf /tmp/gst-plugins-good.tar.xz -C /tmp \
    && meson setup /tmp/gst-build /tmp/gst-plugins-good-1.24.12 \
        --prefix=/usr --libdir=lib/x86_64-linux-gnu --buildtype=release \
        -Dauto_features=disabled -Dqt6=enabled -Dqt-method=qmake \
        -Dqt-x11=enabled -Dqt-egl=enabled \
    && meson compile -C /tmp/gst-build -j 8 \
    && meson install -C /tmp/gst-build \
    && install -Dm644 /tmp/gst-plugins-good-1.24.12/COPYING /opt/licenses/gst-plugins-good.txt \
    && rm -rf /tmp/gst-build /tmp/gst-plugins-good-1.24.12 /tmp/gst-plugins-good.tar.xz

RUN curl --fail --location --retry 3 -o /tmp/rustup-init \
        https://static.rust-lang.org/rustup/archive/1.28.2/x86_64-unknown-linux-gnu/rustup-init \
    && echo '20a06e644b0d9bd2fbdbfd52d42540bdde820ea7df86e92e533c073da0cdd43c  /tmp/rustup-init' | sha256sum --check \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain 1.98.1 \
    && chmod -R a+rX /opt/rustup /opt/cargo \
    && rm /tmp/rustup-init
# Packaging needs file for appimagetool and libsoup's location for the HTTP
# plugin, which loads libsoup and the GIO TLS backend dynamically.
RUN apt-get update && apt-get install -y --no-install-recommends \
    file libsoup-3.0-dev glib-networking \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /project

# CI uses the distribution toolchain and libraries, plus the formatting tool.
# Neither stage needs display, GPU or audio access.
FROM appimage AS ci
# Hardware-free MPRIS contract tests use a private D-Bus and a Python client.
RUN apt-get update && apt-get install -y --no-install-recommends dbus-daemon python3-dbus python3-gi \
    && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt && chmod -R a+rX /opt/rustup /opt/cargo
RUN cargo install cargo-nextest --locked --version 0.9.146 \
    && chmod -R a+rX /opt/cargo
