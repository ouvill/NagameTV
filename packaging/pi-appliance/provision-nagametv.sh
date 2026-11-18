#!/usr/bin/env bash
set -euo pipefail

fail() {
    echo "NagameTV appliance: $*" >&2
    exit 1
}

# shellcheck source=/dev/null
. /etc/os-release
[[ $ID == ubuntu && $VERSION_ID == 26.04 && $(dpkg --print-architecture) == arm64 ]] ||
    fail "Ubuntu 26.04 arm64 is required."

boot=/boot/firmware
mountpoint -q "$boot" || fail "$boot is not mounted."
for file in nagametv.deb nagametv-kiosk.service start-kiosk; do
    [[ -s $boot/$file ]] || fail "Missing $boot/$file."
done

# Stage the local package on a path the _apt sandbox can read.
deb=$(mktemp /var/tmp/nagametv.XXXXXX.deb)
trap 'rm -f -- "$deb"' EXIT
install -m 0644 "$boot/nagametv.deb" "$deb"
[[ $(dpkg-deb -f "$deb" Package) == nagametv && $(dpkg-deb -f "$deb" Architecture) == arm64 ]] ||
    fail "nagametv.deb must contain the NagameTV arm64 package."
case "$(dpkg-deb -f "$deb" Version)" in
    *-*"ubuntu26.04"*) ;;
    *) fail "nagametv.deb must target Ubuntu 26.04." ;;
esac

export DEBIAN_FRONTEND=noninteractive
apt-get -o APT::Update::Error-Mode=any update
# The stock server image does not include the Mesa DRI modules used by Cage.
apt-get -o DPkg::Lock::Timeout=300 install -y --no-install-recommends \
    cage wlr-randr libgl1-mesa-dri kbd libpam-systemd dbus-user-session openssh-server \
    pipewire pipewire-pulse wireplumber pulseaudio-utils fonts-noto-cjk "$deb"

install -D -m 0755 "$boot/start-kiosk" /usr/local/lib/nagametv-appliance/start-kiosk
install -m 0644 "$boot/nagametv-kiosk.service" /etc/systemd/system/nagametv-kiosk.service
systemctl --global enable pipewire.socket pipewire-pulse.socket wireplumber.service

# Keep the other virtual terminals and the Plymouth quit action available.
systemctl mask getty@tty1.service apt-daily.timer apt-daily-upgrade.timer \
    snapd.service snapd.socket snapd.seeded.service snapd.autoimport.service \
    fwupd.service fwupd-refresh.timer plymouth-quit-wait.service ModemManager.service
systemctl stop apt-daily.timer apt-daily-upgrade.timer
systemctl daemon-reload
systemctl enable nagametv-kiosk.service
# cloud-final already starts after multi-user.target. Queuing Cage here also
# covers the first boot without adding an ordering cycle on subsequent boots.
systemctl start --no-block nagametv-kiosk.service
# Leave provisioning available if any earlier operation fails.
touch /etc/cloud/cloud-init.disabled
