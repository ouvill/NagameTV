#!/usr/bin/env python3
"""FileChooser protocol fixture for test-portal-dialogs.sh, on its private bus only."""
import os
from pathlib import Path

import dbus
import dbus.mainloop.glib
import dbus.service
from gi.repository import GLib

dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
directory = Path(os.environ["VIEWER_PORTAL_TEST_DIR"])
bus = dbus.SessionBus()
name = dbus.service.BusName("org.freedesktop.portal.Desktop", bus)


class Request(dbus.service.Object):
    @dbus.service.signal("org.freedesktop.portal.Request", signature="ua{sv}")
    def Response(self, response, results):
        pass

    @dbus.service.method("org.freedesktop.portal.Request")
    def Close(self):
        self.remove_from_connection()


class Portal(dbus.service.Object):
    def __init__(self):
        super().__init__(bus, "/org/freedesktop/portal/desktop")
        self.requests = []

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="ss", out_signature="v")
    def Get(self, interface, prop):
        if interface == "org.freedesktop.portal.FileChooser" and prop == "version":
            return dbus.UInt32(4)
        raise dbus.exceptions.DBusException("Unknown property")

    @dbus.service.method("org.freedesktop.portal.Settings", in_signature="as", out_signature="a{sa{sv}}")
    def ReadAll(self, namespaces):
        return {}

    @dbus.service.method("org.freedesktop.portal.FileChooser", in_signature="ssa{sv}",
                         out_signature="o", sender_keyword="sender")
    def OpenFile(self, parent, title, options, sender):
        number = len(self.requests) + 1
        token = options.get("handle_token", f"fixture{number}")
        sender_path = sender.lstrip(":").replace(".", "_")
        path = f"/org/freedesktop/portal/desktop/request/{sender_path}/{token}"
        request = Request(bus, path)
        self.requests.append(request)
        selected = directory / ("キャプチャ #100%" if options.get("directory") else "録画 #100%.ts")

        def respond():
            if number % 2:
                request.Response(0, {"uris": dbus.Array([selected.as_uri()], signature="s")})
            else:
                request.Response(1, {})
            request.remove_from_connection()
            return False

        GLib.timeout_add(80, respond)
        return dbus.ObjectPath(path)


portal = Portal()
(directory / "ready").touch()
GLib.MainLoop().run()
