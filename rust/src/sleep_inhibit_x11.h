#ifndef NAGAMETV_SLEEP_INHIBIT_X11_H
#define NAGAMETV_SLEEP_INHIBIT_X11_H
#include <QtCore/QLibrary>
#include <QtGui/qguiapplication_platform.h>
#include <memory>
#include <stdexcept>

// Linux native X11 only. Resolve optional Xss at runtime so portal/Wayland
// installations need neither X11 development headers nor an extra linked library.
class X11SleepInhibit final {
    QLibrary xlib{QStringLiteral("X11"), 6};
    QLibrary xss{QStringLiteral("Xss"), 1};
    using DisplayPointer = void *;
    using Close = int (*)(DisplayPointer);
    using Suspend = void (*)(DisplayPointer, int);
    using Sync = int (*)(DisplayPointer, int);
    DisplayPointer display = nullptr;
    Close closeDisplay = nullptr;
    Suspend suspend = nullptr;
    Sync sync = nullptr;
    enum class Grant { Empty, Held };
    Grant grant = Grant::Empty;
    template<class T> static T resolve(QLibrary &library, const char *name) {
        auto symbol = library.resolve(name);
        if (!symbol) throw std::runtime_error(library.errorString().toStdString());
        return reinterpret_cast<T>(symbol);
    }
    void acquire(bool testXwayland) {
#if QT_CONFIG(xcb)
        auto app = qobject_cast<QGuiApplication *>(QCoreApplication::instance());
        if (!app || app->platformName() != QStringLiteral("xcb"))
            throw std::runtime_error("native X11 inhibition requires an active Qt xcb application");
        auto native = app->nativeInterface<QNativeInterface::QX11Application>();
        if (!native || !native->display()) throw std::runtime_error("Qt has no validated X11 display");
        auto displayName = resolve<char *(*)(DisplayPointer)>(xlib, "XDisplayString")(native->display());
        if (!displayName || (displayName[0] != ':' && !QByteArray(displayName).startsWith("unix:")))
            throw std::runtime_error("native X11 inhibition requires the application's local display");
        closeDisplay = resolve<Close>(xlib, "XCloseDisplay");
        display = resolve<DisplayPointer (*)(const char *)>(xlib, "XOpenDisplay")(displayName);
        if (!display) throw std::runtime_error("could not connect to the application's X11 display");
        auto query = resolve<int (*)(DisplayPointer, const char *, int *, int *, int *)>(xlib, "XQueryExtension");
        int opcode, event, error;
        const bool xwayland = query(display, "XWAYLAND", &opcode, &event, &error);
        if (!testXwayland && (xwayland || qEnvironmentVariableIsSet("WAYLAND_DISPLAY")))
            throw std::runtime_error("Xwayland cannot inhibit its Wayland compositor's idle display");
        if (testXwayland && !xwayland)
            throw std::runtime_error("the X11 wire check requires the dedicated Xwayland test display");
        if (!query(display, "MIT-SCREEN-SAVER", &opcode, &event, &error))
            throw std::runtime_error("X11 screensaver extension is unavailable");
        int major, minor;
        auto version = resolve<int (*)(DisplayPointer, int *, int *)>(xss, "XScreenSaverQueryVersion");
        if (!version(display, &major, &minor) || major < 1 || (major == 1 && minor < 1))
            throw std::runtime_error("X11 screensaver extension 1.1 is required");
        sync = resolve<Sync>(xlib, "XSync");
        suspend = resolve<Suspend>(xss, "XScreenSaverSuspend");
        suspend(display, 1);
        sync(display, 0);
        grant = Grant::Held;
#else
        Q_UNUSED(testXwayland);
        throw std::runtime_error("Qt was built without X11 support");
#endif
    }
public:
    static std::unique_ptr<X11SleepInhibit> create(bool testXwayland = false) {
        auto owner = std::unique_ptr<X11SleepInhibit>(new X11SleepInhibit);
        owner->acquire(testXwayland);
        return owner;
    }
    void release() {
        if (grant == Grant::Held) {
            suspend(display, 0);
            sync(display, 0);
            grant = Grant::Empty;
        }
    }
    ~X11SleepInhibit() {
        release();
        if (display) closeDisplay(display);
    }
};
#endif
