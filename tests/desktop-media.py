"""MPRIS wire checks against the hardware-free Qt fixture on a private D-Bus."""
import math
import sys
import time
import dbus
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
name = "org.mpris.MediaPlayer2.io.github.ouvill.nagametv.instance" + sys.argv[1]
if sys.argv[-1] == "removed":
    assert not bus.name_has_owner(name)
    sys.exit(0)
obj = bus.get_object(name, "/org/mpris/MediaPlayer2", introspect=False)
props = dbus.Interface(obj, "org.freedesktop.DBus.Properties")
player = dbus.Interface(obj, "org.mpris.MediaPlayer2.Player")
root = dbus.Interface(obj, "org.mpris.MediaPlayer2")
interface = "org.mpris.MediaPlayer2.Player"
changed, seeked = [], []
bus.add_signal_receiver(lambda *args: changed.append(args), signal_name="PropertiesChanged", dbus_interface="org.freedesktop.DBus.Properties", bus_name=name)
bus.add_signal_receiver(lambda value: seeked.append(value), signal_name="Seeked", dbus_interface=interface, bus_name=name)
context = GLib.MainContext.default()

def wait_for(predicate):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        while context.pending():
            context.iteration(False)
        if predicate():
            return
        time.sleep(0.01)
    raise AssertionError("MPRIS update timed out")

def rejected(call, expected):
    try:
        call()
    except dbus.DBusException as error:
        assert error.get_dbus_name().endswith(expected), error
    else:
        raise AssertionError("Invalid D-Bus call was accepted")

info = props.GetAll(interface)
assert info["PlaybackStatus"] == "Playing"
assert isinstance(info["Metadata"]["mpris:trackid"], dbus.ObjectPath)
assert isinstance(info["Metadata"]["mpris:length"], dbus.Int64)
assert isinstance(info["Position"], dbus.Int64)
assert info["Metadata"]["xesam:title"] == "字幕のある番組"
assert info["Metadata"]["xesam:artist"] == ["試験放送"]
assert props.Get("org.mpris.MediaPlayer2", "DesktopEntry") == "io.github.ouvill.nagametv"
assert not info["CanGoNext"] and not info["CanGoPrevious"]
xml = dbus.Interface(obj, "org.freedesktop.DBus.Introspectable").Introspect()
assert 'name="SetPosition"' in xml
rejected(lambda: props.Set(interface, "Volume", dbus.String("0.2", variant_level=1)), "InvalidArgs")
rejected(lambda: props.Set(interface, "Volume", dbus.Double(math.nan, variant_level=1)), "InvalidArgs")
rejected(lambda: props.Set(interface, "Position", dbus.Int64(10, variant_level=1)), "PropertyReadOnly")
rejected(lambda: props.Get("invalid.interface", "Volume"), "UnknownInterface")
rejected(lambda: props.Get(interface, "Missing"), "UnknownProperty")
props.Set(interface, "Volume", dbus.Double(0.7, variant_level=1))
wait_for(lambda: props.Get(interface, "Volume") == 0.7)
wait_for(lambda: any("Volume" in values for _, values, _ in changed))
assert info["MinimumRate"] == 0.5 and info["MaximumRate"] == 2.0
for invalid in [math.nan, math.inf, -1.0, 0.4, 2.1, 1.25]:
    rejected(lambda: props.Set(interface, "Rate", dbus.Double(invalid, variant_level=1)), "InvalidArgs")
props.Set(interface, "Rate", dbus.Double(1.3, variant_level=1))
wait_for(lambda: props.Get(interface, "Rate") == 1.3)
wait_for(lambda: any("Rate" in values for _, values, _ in changed))
# MPRIS retains its established Rate=0 pause behavior.
props.Set(interface, "Rate", dbus.Double(0.0, variant_level=1))
wait_for(lambda: props.Get(interface, "PlaybackStatus") == "Paused")
player.Play()
wait_for(lambda: props.Get(interface, "PlaybackStatus") == "Playing")
# Stale tracks, negative positions and positions after EOF are ignored.
track = dbus.ObjectPath(str(info["Metadata"]["mpris:trackid"]))
player.SetPosition(dbus.ObjectPath("/stale"), dbus.Int64(3_000_000))
player.SetPosition(track, dbus.Int64(-1))
player.SetPosition(track, dbus.Int64(61_000_000))
player.SetPosition(track, dbus.Int64(3_000_000))
wait_for(lambda: seeked == [3_000_000])
assert all("Position" not in values for _, values, _ in changed)
root.Raise()
player.Stop()
wait_for(lambda: props.Get(interface, "PlaybackStatus") == "Stopped")
assert not props.Get(interface, "Metadata")
print("MPRIS metadata, signatures, commands, signals and stale-seek rejection passed")
