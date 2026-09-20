#!/usr/bin/env bash
set -euo pipefail

# Native Linux development dependencies from Fedora's package repositories.
# This installs clients for the existing PipeWire server; it does not configure
# containers, start services, or change display/audio routing.
source /etc/os-release
if [[ ${ID:-} != fedora ]]; then
    echo "This setup script requires Fedora; see docs/development.md." >&2
    exit 1
fi

# Additional arguments are dnf options, for example --assumeno to review only.
exec sudo dnf install "$@" -- \
    rust cargo rustfmt clippy rust-analyzer \
    cmake ninja-build gcc gcc-c++ make clang-devel lld pkgconf-pkg-config \
    qt6-qtbase-devel qt6-qtdeclarative-devel qt6-qtsvg-devel \
    qt6-linguist qt6-qtimageformats qt6-qtwayland \
    gstreamer1-devel gstreamer1-plugins-base-devel \
    gstreamer1-plugins-bad-free-devel \
    gstreamer1-plugins-base gstreamer1-plugins-good \
    gstreamer1-plugins-good-qt6 gstreamer1-plugins-bad-free \
    gstreamer1-plugins-bad-free-extras gstreamer1-plugin-libav \
    google-noto-sans-cjk-vf-fonts \
    python3 python3-dbus python3-gobject dbus-daemon dbus-tools \
    weston xorg-x11-server-Xwayland openbox glx-utils \
    xdpyinfo xprop xwininfo xdotool pulseaudio-utils ImageMagick
