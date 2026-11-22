#ifndef NAGAMETV_QT_QUIT_SIGNALS_H
#define NAGAMETV_QT_QUIT_SIGNALS_H

#include <QtCore/QtGlobal>

#ifdef Q_OS_LINUX
#include <QtCore/QCoreApplication>
#include <QtCore/QSocketNotifier>
#include <QtGui/QGuiApplication>
#include <rust/cxx.h>

#include <atomic>
#include <cerrno>
#include <csignal>
#include <fcntl.h>
#include <system_error>
#include <unistd.h>

namespace viewer::quit_signals {

constexpr int signalsToCatch[] = {SIGTERM, SIGINT, SIGHUP};

static_assert(std::atomic<int>::is_always_lock_free);
inline std::atomic<int> signalWriter{-1};
inline std::atomic_flag requested = ATOMIC_FLAG_INIT;

inline void receive(int signal) {
    // A second signal can interrupt a stalled GUI or resource teardown.
    if (requested.test_and_set(std::memory_order_relaxed))
        ::_exit(128 + signal);

    const int savedErrno = errno;
    const auto byte = static_cast<unsigned char>(signal);
    const int fd = signalWriter.load(std::memory_order_relaxed);
    ssize_t size;
    do {
        size = ::write(fd, &byte, sizeof(byte));
    } while (size < 0 && errno == EINTR);
    if (size != 1)
        ::_exit(128 + signal);
    errno = savedErrno;
}

} // namespace viewer::quit_signals

inline int openQuitSignalPipe() {
    using namespace viewer::quit_signals;
    int pipeEnds[2];
    if (::pipe2(pipeEnds, O_NONBLOCK | O_CLOEXEC) < 0)
        throw std::system_error(errno, std::generic_category(), "pipe2");

    // Keep both ends until process exit. A handler on another thread could
    // still be writing while Qt or Rust owners are being destroyed.
    signalWriter.store(pipeEnds[1], std::memory_order_relaxed);
    struct sigaction action {};
    action.sa_handler = receive;
    action.sa_flags = SA_RESTART;
    ::sigemptyset(&action.sa_mask);
    for (const int signal : signalsToCatch)
        ::sigaddset(&action.sa_mask, signal);
    for (const int signal : signalsToCatch) {
        if (::sigaction(signal, &action, nullptr) < 0)
            throw std::system_error(errno, std::generic_category(), "sigaction");
    }
    return pipeEnds[0];
}

inline void watchQuitSignals(QGuiApplication &app, int readFd, rust::Fn<void(int)> received) {
    auto *notifier = new QSocketNotifier(readFd, QSocketNotifier::Read, &app);
    QObject::connect(notifier, &QSocketNotifier::activated, &app, [notifier, readFd, received] {
        unsigned char signal;
        const auto size = ::read(readFd, &signal, sizeof(signal));
        const int readError = errno;
        if (size < 0 && (readError == EINTR || readError == EAGAIN))
            return;
        notifier->setEnabled(false);
        if (size == 1)
            received(signal);
        else
            qErrnoWarning(size < 0 ? readError : EIO, "Could not read the application quit signal");

        // QGuiApplication handles Quit by closing the top-level windows,
        // which runs Main.qml's existing Player shutdown and settings save.
        QCoreApplication::quit();
    });
}
#endif

#endif
