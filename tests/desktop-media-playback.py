"""Actual Player integration; invoked only by the hardware-validated startup suite."""
import sys
import time
import dbus

bus = dbus.SessionBus()
name = "org.mpris.MediaPlayer2.io.github.ouvill.nagametv.instance" + sys.argv[1]
obj = bus.get_object(name, "/org/mpris/MediaPlayer2", introspect=False)
props = dbus.Interface(obj, "org.freedesktop.DBus.Properties")
player = dbus.Interface(obj, "org.mpris.MediaPlayer2.Player")
interface = "org.mpris.MediaPlayer2.Player"

def get(name):
    return props.Get(interface, name)

def wait_for(predicate):
    deadline = time.monotonic() + 4
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.01)
    raise AssertionError("Production MPRIS update timed out")

wait_for(lambda: get("PlaybackStatus") == "Playing" and get("Metadata").get("xesam:title") == "録画 #100%.ts")
assert get("CanPause")
player.Pause()
wait_for(lambda: get("PlaybackStatus") == "Paused")
position = get("Position")
time.sleep(0.1)
assert abs(get("Position") - position) < 200_000
assert get("MinimumRate") == 0.5 and get("MaximumRate") == 2.0
for rate in [0.5, 1.1, 1.5, 2.0, 1.0]:
    props.Set(interface, "Rate", dbus.Double(rate, variant_level=1))
    wait_for(lambda: get("Rate") == rate)
    assert get("PlaybackStatus") == "Paused"
    assert abs(get("Position") - position) < 200_000
original_volume = get("Volume")
props.Set(interface, "Volume", dbus.Double(0.37, variant_level=1))
wait_for(lambda: get("Volume") == 0.37)
props.Set(interface, "Volume", dbus.Double(original_volume, variant_level=1))
wait_for(lambda: get("Volume") == original_volume)
player.Play()
wait_for(lambda: get("PlaybackStatus") == "Playing")
print("Production MPRIS recording title, pause/resume and volume passed")
