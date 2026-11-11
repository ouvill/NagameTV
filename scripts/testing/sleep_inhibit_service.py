"""Inhibit wire fixture for scripts/test.py sleep-inhibit, on its private bus only."""
import json
import os
from pathlib import Path

import dbus
import dbus.mainloop.glib
import dbus.service
from gi.repository import GLib


def main():
    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    directory = Path(os.environ["NAGAMETV_INHIBIT_FIXTURE"])
    bus = dbus.SessionBus()
    name = dbus.service.BusName("org.freedesktop.portal.Desktop", bus)
    requests = {}

    def record(kind, **values):
        with (directory / "events.jsonl").open("a") as log:
            log.write(json.dumps({"kind": kind, **values}) + "\n")

    class Request(dbus.service.Object):
        def __init__(self, path, sender):
            super().__init__(bus, path)
            self.path = path
            self.sender = sender
            requests[path] = self

        @dbus.service.signal("org.freedesktop.portal.Request", signature="ua{sv}")
        def Response(self, code, results):
            pass

        @dbus.service.method("org.freedesktop.portal.Request")
        def Close(self):
            record("close", path=self.path)
            requests.pop(self.path)
            self.remove_from_connection()

    class Portal(dbus.service.Object):
        @dbus.service.method("org.freedesktop.portal.Inhibit", in_signature="sua{sv}",
                             out_signature="o", sender_keyword="sender",
                             async_callbacks=("reply", "error"))
        def Inhibit(self, window, flags, options, sender, reply, error):
            mode = (directory / "mode").read_text()
            record("acquire", flags=int(flags), window=str(window), reason=str(options["reason"]))
            if mode in ("failed", "unavailable"):
                code = ("org.freedesktop.portal.Error.Failed" if mode == "failed"
                        else "org.freedesktop.DBus.Error.UnknownMethod")
                error(dbus.exceptions.DBusException("fixture rejected inhibition", name=code))
                return
            if mode == "malformed":
                reply(dbus.ObjectPath("/invalid"))
                return
            path = "/org/freedesktop/portal/desktop/request/" + sender[1:].replace(".", "_") + "/" + str(options["handle_token"])

            def respond():
                # A grant must never outlive its caller, including a late reply.
                if bus.name_has_owner(sender):
                    request = Request(path, sender)
                    record("held", path=path)
                    # Exercise the race where Response precedes the method reply.
                    if mode != "no-response":
                        request.Response(2 if mode == "response-failed" else 0, {})
                    reply(dbus.ObjectPath(path))
                else:
                    record("cancelled")
                return False

            GLib.timeout_add(150 if mode == "delayed" else 1, respond)

    def owner_changed(sender, old, new):
        if not new:
            for request in list(requests.values()):
                if request.sender == sender:
                    record("disconnected", path=request.path)
                    requests.pop(request.path)
                    request.remove_from_connection()

    bus.add_signal_receiver(owner_changed, signal_name="NameOwnerChanged",
                            dbus_interface="org.freedesktop.DBus")
    portal = Portal(bus, "/org/freedesktop/portal/desktop")
    (directory / "ready").touch()
    GLib.MainLoop().run()
    # Keep service owners alive until the loop ends.
    del portal, name


if __name__ == "__main__":
    main()
