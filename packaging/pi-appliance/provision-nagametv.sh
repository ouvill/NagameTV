#!/usr/bin/env bash
set -euo pipefail

fail() {
    echo "NagameTV appliance: $*" >&2
    exit 1
}

# cloud-final copies stdout to the console, so these lines show on the TV
# between kernel messages during the first boot.
step() {
    echo "NagameTV appliance: $*"
}

# shellcheck source=/dev/null
. /etc/os-release
[[ $ID == ubuntu && $VERSION_ID == 26.04 && $(dpkg --print-architecture) == arm64 ]] ||
    fail "Ubuntu 26.04 arm64 is required."

boot=/boot/firmware
mountpoint -q "$boot" || fail "$boot is not mounted."
for file in nagametv.deb nagametv-kiosk.service start-kiosk labwc-rc.xml \
    nagametv-hdmi-audio.service hdmi-audio.py 50-nagametv-hdmi-only.conf 50-nagametv-hdmi-stream.conf; do
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

# The image clock starts at its build date, so the first time sync makes both
# apt timers overdue. Stopping unattended-upgrade mid-run can leave dpkg
# half-configured, so let a job that already started finish instead.
# These oneshot services report "activating" while they run, which is-active
# alone treats as not running.
apt_job_running() {
    { systemctl is-active apt-daily.service apt-daily-upgrade.service || true; } |
        grep -q -x -e active -e activating
}
systemctl mask --now apt-daily.timer apt-daily-upgrade.timer
if apt_job_running; then
    step "Waiting for the running Ubuntu update to finish."
    while apt_job_running; do
        sleep 10
    done
fi

step "Installing packages. This can take a while on a slow connection."
export DEBIAN_FRONTEND=noninteractive
apt-get -o APT::Update::Error-Mode=any update
# The stock server image does not include the Mesa DRI modules used by labwc.
apt-get -o DPkg::Lock::Timeout=300 install -y --no-install-recommends \
    labwc wlr-randr libgl1-mesa-dri kbd libpam-systemd dbus-user-session openssh-server \
    pipewire pipewire-pulse wireplumber pulseaudio-utils python3 fonts-noto-cjk "$deb"

step "Configuring the kiosk."
connection="$boot/nagametv-connection.toml"
if [[ -e $connection ]]; then
    [[ -f $connection ]] || fail "$connection must be a file."
    step "Importing the connection settings."
    # Read FAT as root even when it is mounted with restrictive permissions.
    # runuser writes through the app as the kiosk user, with its normal XDG path.
    if ! runuser -u nagametv -- env -u NAGAMETV_DATA_DIR -u XDG_CONFIG_HOME \
        /usr/bin/nagametv --import-connection < "$connection"; then
        fail "Invalid or unreadable $connection, or the kiosk settings could not be saved."
    fi
fi
install -D -m 0755 "$boot/start-kiosk" /usr/local/lib/nagametv-appliance/start-kiosk
install -D -m 0644 "$boot/labwc-rc.xml" /usr/local/lib/nagametv-appliance/labwc/rc.xml
install -D -m 0644 "$boot/hdmi-audio.py" /usr/local/lib/nagametv-appliance/hdmi-audio.py
install -m 0644 "$boot/nagametv-kiosk.service" /etc/systemd/system/nagametv-kiosk.service
install -m 0644 "$boot/nagametv-hdmi-audio.service" /etc/systemd/system/nagametv-hdmi-audio.service
install -D -m 0644 "$boot/50-nagametv-hdmi-only.conf" \
    /etc/wireplumber/wireplumber.conf.d/50-nagametv-hdmi-only.conf
install -D -m 0644 "$boot/50-nagametv-hdmi-stream.conf" \
    /etc/pipewire/pipewire-pulse.conf.d/50-nagametv-hdmi-stream.conf
systemctl --global enable pipewire.socket pipewire-pulse.socket wireplumber.service

# Keep the other virtual terminals and the Plymouth quit action available.
systemctl mask getty@tty1.service \
    snapd.service snapd.socket snapd.seeded.service snapd.autoimport.service \
    fwupd.service fwupd-refresh.timer plymouth-quit-wait.service ModemManager.service
systemctl daemon-reload
systemctl enable nagametv-kiosk.service
# cloud-final already starts after multi-user.target. Queuing labwc here also
# covers the first boot without adding an ordering cycle on subsequent boots.
step "Starting NagameTV."
systemctl start --no-block nagametv-kiosk.service
# Leave provisioning available if any earlier operation fails.
touch /etc/cloud/cloud-init.disabled
