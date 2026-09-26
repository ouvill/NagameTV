// Local build-performance patch; see ../PATCH.md.
use std::{io, thread};

pub(crate) fn inherited_jobserver() -> Result<Option<jobserver::Client>, jobserver::FromEnvError> {
    // SAFETY: called in Cargo build scripts, whose inherited jobserver handles
    // remain open for the process lifetime. Check that Unix handles are pipes;
    // do not take ownership of arbitrary descriptors from malformed settings.
    let inherited = unsafe { jobserver::Client::from_env_ext(true) };
    match inherited.client {
        Ok(client) => Ok(Some(client)),
        Err(error)
            if matches!(
                error.kind(),
                jobserver::FromEnvErrorKind::NoEnvVar | jobserver::FromEnvErrorKind::NoJobserver
            ) =>
        {
            // Normal when the helper is used outside a jobserver-aware build.
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

enum Task<'scope, T> {
    Complete(T),
    Running(thread::ScopedJoinHandle<'scope, T>),
}

// The caller already owns one implicit Cargo slot. Only spawn a worker after
// acquiring an extra slot; otherwise do useful work on the calling thread.
// Never wait for a token while holding the implicit slot: -j1 must progress.
pub(crate) fn map<T: Sync, R: Send>(
    mut jobserver: Option<&jobserver::Client>,
    items: &[T],
    operation: impl Fn(&T) -> R + Sync,
) -> io::Result<Vec<R>> {
    thread::scope(|scope| {
        let mut tasks = Vec::with_capacity(items.len());
        let operation = &operation;
        for item in items {
            let permit = match jobserver {
                Some(client) => match client.try_acquire() {
                    Ok(permit) => permit,
                    Err(error) if error.kind() == io::ErrorKind::Unsupported => {
                        println!("cargo:warning=qmlcachegen will run sequentially: {error}");
                        jobserver = None;
                        None
                    }
                    Err(error) => return Err(error),
                },
                None => None,
            };
            tasks.push(match permit {
                Some(permit) => Task::Running(scope.spawn(move || {
                    let result = operation(item);
                    drop(permit);
                    result
                })),
                None => Task::Complete(operation(item)),
            });
        }
        Ok(tasks
            .into_iter()
            .map(|task| match task {
                Task::Complete(result) => result,
                Task::Running(worker) => match worker.join() {
                    Ok(result) => result,
                    Err(panic) => std::panic::resume_unwind(panic),
                },
            })
            .collect())
    })
}

#[cfg(test)]
mod tests {
    use super::{inherited_jobserver, map};
    use std::sync::{mpsc, Mutex};
    use std::time::Duration;

    fn with_server(tokens: usize, name: &str, test: impl FnOnce(&jobserver::Client)) {
        if std::env::var("QML_BUILD_TEST_CHILD").as_deref() == Ok(name) {
            // Exercise actual inheritance, including reopening the anonymous
            // pipe in nonblocking mode on Linux, just as in Cargo build scripts.
            let client = inherited_jobserver().unwrap().unwrap();
            test(&client);
            return;
        }
        let server = jobserver::Client::new(tokens).unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", &format!("parallel::tests::{name}")]);
        command.env("QML_BUILD_TEST_CHILD", name);
        server.configure(&mut command);
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(server.available().unwrap(), tokens);
    }

    #[test]
    fn no_jobserver_uses_the_calling_thread() {
        let caller = std::thread::current().id();
        let result = map(None, &[3, 1, 2], |item| {
            assert_eq!(std::thread::current().id(), caller);
            item * 2
        })
        .unwrap();
        assert_eq!(result, [6, 2, 4]);
    }

    #[test]
    fn one_cargo_slot_does_not_wait_for_an_extra_token() {
        with_server(
            0,
            "one_cargo_slot_does_not_wait_for_an_extra_token",
            |server| {
                let caller = std::thread::current().id();
                map(Some(server), &[1, 2, 3], |_| {
                    assert_eq!(std::thread::current().id(), caller);
                })
                .unwrap();
                assert_eq!(server.available().unwrap(), 0);
            },
        );
    }

    #[test]
    fn parallel_completion_preserves_order_and_returns_tokens() {
        with_server(
            1,
            "parallel_completion_preserves_order_and_returns_tokens",
            |server| {
                let (send, receive) = mpsc::channel();
                let receive = Mutex::new(receive);
                let caller = std::thread::current().id();
                let result = map(Some(server), &[0, 1], |item| {
                    if *item == 0 {
                        assert_ne!(std::thread::current().id(), caller);
                        receive
                            .lock()
                            .unwrap()
                            .recv_timeout(Duration::from_secs(5))
                            .unwrap();
                    } else {
                        assert_eq!(std::thread::current().id(), caller);
                        send.send(()).unwrap();
                    }
                    *item
                })
                .unwrap();
                assert_eq!(result, [0, 1]);
                assert_eq!(server.available().unwrap(), 1);
            },
        );
    }

    #[test]
    fn worker_failure_is_propagated_and_releases_its_token() {
        with_server(
            1,
            "worker_failure_is_propagated_and_releases_its_token",
            |server| {
                let result = std::panic::catch_unwind(|| {
                    map(Some(server), &[0], |_| panic!("qmlcachegen failed")).unwrap()
                });
                assert!(result.is_err());
                assert_eq!(server.available().unwrap(), 1);
            },
        );
    }
}
