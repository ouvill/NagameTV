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
    names = {}
    cookies = {}
    cookie_counter = 0
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

    class ScreenSaver(dbus.service.Object):
        @dbus.service.method("org.freedesktop.ScreenSaver", in_signature="ss", out_signature="u", sender_keyword="sender")
        def Inhibit(self, who, why, sender):
            nonlocal cookie_counter
            mode_path = directory / "screensaver-mode"
            mode = mode_path.read_text() if mode_path.exists() else "success"
            record("screensaver-acquire", mode=mode)
            if mode == "failed":
                raise dbus.exceptions.DBusException("screensaver refused", name="org.freedesktop.DBus.Error.AccessDenied")
            cookie_counter += 1
            cookies[cookie_counter] = sender
            return dbus.UInt32(cookie_counter)

        @dbus.service.method("org.freedesktop.ScreenSaver", in_signature="u")
        def UnInhibit(self, cookie):
            record("screensaver-release", cookie=int(cookie))
            cookies.pop(int(cookie))

    class Logind(dbus.service.Object):
        @dbus.service.method("org.freedesktop.login1.Manager", in_signature="ssss", out_signature="h")
        def Inhibit(self, what, who, why, mode):
            record("logind-acquire", what=str(what), who=str(who), mode=str(mode))
            mode_path = directory / "logind-mode"
            if mode_path.exists() and mode_path.read_text() == "failed":
                raise dbus.exceptions.DBusException("logind refused", name="org.freedesktop.DBus.Error.AccessDenied")
            read_fd, write_fd = os.pipe()
            result = dbus.types.UnixFd(write_fd)
            os.close(write_fd)

            def released(fd, condition):
                assert os.read(fd, 1) == b""
                record("logind-release")
                os.close(fd)
                return False

            GLib.io_add_watch(read_fd, GLib.IO_HUP, released)
            return result

    def owner_changed(sender, old, new):
        if not new:
            for request in list(requests.values()):
                if request.sender == sender:
                    record("disconnected", path=request.path)
                    requests.pop(request.path)
                    request.remove_from_connection()
            for cookie, owner in list(cookies.items()):
                if owner == sender:
                    record("screensaver-disconnected", cookie=cookie)
                    cookies.pop(cookie)

    def update_services():
        config = directory / "services"
        selected = config.read_text().split() if config.exists() else ["portal"]
        for label, service_name in (("portal", "org.freedesktop.portal.Desktop"),
                                    ("screensaver", "org.freedesktop.ScreenSaver"),
                                    ("logind", "org.freedesktop.login1")):
            if label in selected and label not in names:
                names[label] = dbus.service.BusName(service_name, bus)
            elif label not in selected and label in names:
                names.pop(label)
        active = directory / "services-active"
        pending = directory / "services-active.tmp"
        pending.write_text(" ".join(selected))
        pending.replace(active)
        return True

    bus.add_signal_receiver(owner_changed, signal_name="NameOwnerChanged",
                            dbus_interface="org.freedesktop.DBus")
    portal = Portal(bus, "/org/freedesktop/portal/desktop")
    screensaver = ScreenSaver(bus, "/org/freedesktop/ScreenSaver")
    logind = Logind(bus, "/org/freedesktop/login1")
    update_services()
    GLib.timeout_add(10, update_services)
    (directory / "ready").touch()
    GLib.MainLoop().run()
    # Keep service owners alive until the loop ends.
    del portal, screensaver, logind, names


if __name__ == "__main__":
    main()
