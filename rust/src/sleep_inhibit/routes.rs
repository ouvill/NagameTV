//! Portal-first routing. A fallback grant always owns both sleep and display inhibition.
use super::{Error, Inhibitor, Progress, Request};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Portal,
    Logind,
    ScreenSaver,
    X11,
}
impl Kind {
    const ALL: [Self; 4] = [Self::Portal, Self::Logind, Self::ScreenSaver, Self::X11];
    fn index(self) -> usize {
        match self {
            Self::Portal => 0,
            Self::Logind => 1,
            Self::ScreenSaver => 2,
            Self::X11 => 3,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Portal => "portal",
            Self::Logind => "logind",
            Self::ScreenSaver => "screensaver",
            Self::X11 => "x11",
        }
    }
}
pub trait Transport {
    type Lease: Request;
    fn acquire(&mut self, kind: Kind) -> Result<Self::Lease, Error>;
    fn revision(&mut self) -> u64;
    fn revision_for(&mut self, _kind: Kind) -> u64 {
        self.revision()
    }
}
struct Context<B: Transport> {
    backend: B,
    unavailable: BTreeSet<Kind>,
    revision: u64,
    revisions: [u64; 4],
    retry_revision: u64,
}
pub struct Routes<B: Transport>(Rc<RefCell<Context<B>>>);
impl<B: Transport> Routes<B> {
    pub fn new(backend: B) -> Self {
        Self(Rc::new(RefCell::new(Context {
            backend,
            unavailable: BTreeSet::new(),
            revision: 0,
            revisions: [0; 4],
            retry_revision: 0,
        })))
    }
}
type Attempt<B> = Result<<B as Transport>::Lease, Error>;
enum Stage<B: Transport> {
    Portal(Attempt<B>),
    Sleep(Attempt<B>),
    Screen {
        sleep: B::Lease,
        screen: Attempt<B>,
        kind: Kind,
    },
    PortalHeld(B::Lease),
    PairHeld {
        sleep: B::Lease,
        screen: B::Lease,
        kind: Kind,
    },
    Released,
}
pub struct RoutedRequest<B: Transport> {
    context: Rc<RefCell<Context<B>>>,
    stage: Stage<B>,
}
impl<B: Transport> Context<B> {
    fn start(&mut self, kind: Kind) -> Attempt<B> {
        if self.unavailable.contains(&kind) {
            return Err(Error {
                operation: "acquire sleep inhibition",
                unavailable: true,
                cause: format!(
                    "{} is unavailable until the environment changes",
                    kind.as_str()
                )
                .into(),
            });
        }
        self.backend.acquire(kind)
    }
    fn unavailable(&mut self, kind: Kind, error: &Error, switching: bool) {
        if error.unavailable && self.unavailable.insert(kind) && switching {
            tracing::warn!(
                backend = kind.as_str(),
                operation = error.operation,
                error = error as &dyn std::error::Error,
                "Sleep inhibition backend unavailable; trying fallback"
            );
        }
    }
}
impl<B: Transport> Inhibitor for Routes<B> {
    type Request = RoutedRequest<B>;
    fn acquire(&mut self) -> Result<Self::Request, Error> {
        let mut context = self.0.borrow_mut();
        let stage = if context.unavailable.contains(&Kind::Portal) {
            Stage::Sleep(context.start(Kind::Logind))
        } else {
            Stage::Portal(context.start(Kind::Portal))
        };
        Ok(RoutedRequest {
            context: self.0.clone(),
            stage,
        })
    }
    fn revision(&mut self) -> u64 {
        let mut context = self.0.borrow_mut();
        let revision = context.backend.revision();
        if revision != context.revision {
            let mut rearm = false;
            for kind in Kind::ALL {
                let current = context.backend.revision_for(kind);
                if current != context.revisions[kind.index()] {
                    context.revisions[kind.index()] = current;
                    rearm |= context.unavailable.remove(&kind);
                }
            }
            context.retry_revision += u64::from(rearm);
            context.revision = revision;
        }
        context.retry_revision
    }
    fn reset(&mut self) {
        self.0.borrow_mut().unavailable.clear();
    }
}
fn release_results(display: Result<(), Error>, suspend: Result<(), Error>) -> Result<(), Error> {
    match (display, suspend) {
        (Err(first), Err(second)) => {
            tracing::error!(
                operation = second.operation,
                error = &second as &dyn std::error::Error,
                "Additional sleep inhibition release failed"
            );
            Err(first)
        }
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}
enum Step<R> {
    Pending(R),
    Held(R),
    Failed(Error),
}
fn step<R: Request>(attempt: Result<R, Error>) -> Step<R> {
    match attempt {
        Err(error) => Step::Failed(error),
        Ok(mut request) => match request.poll() {
            Ok(Progress::Pending) => Step::Pending(request),
            Ok(Progress::Held) => Step::Held(request),
            Err(error) => Step::Failed(error),
        },
    }
}
impl<B: Transport> Request for RoutedRequest<B> {
    fn poll(&mut self) -> Result<Progress, Error> {
        let stage = std::mem::replace(&mut self.stage, Stage::Released);
        let mut context = self.context.borrow_mut();
        self.stage = match stage {
            Stage::Portal(attempt) => match step(attempt) {
                Step::Pending(request) => Stage::Portal(Ok(request)),
                Step::Held(request) => {
                    tracing::info!(backend = "portal", "Sleep inhibition acquired");
                    Stage::PortalHeld(request)
                }
                Step::Failed(error) if error.unavailable => {
                    context.unavailable(Kind::Portal, &error, true);
                    Stage::Sleep(context.start(Kind::Logind))
                }
                Step::Failed(error) => return Err(error),
            },
            Stage::Sleep(attempt) => match step(attempt) {
                Step::Pending(request) => Stage::Sleep(Ok(request)),
                Step::Held(sleep) => Stage::Screen {
                    sleep,
                    screen: context.start(Kind::ScreenSaver),
                    kind: Kind::ScreenSaver,
                },
                Step::Failed(error) => {
                    context.unavailable(Kind::Logind, &error, false);
                    return Err(error);
                }
            },
            Stage::Screen {
                mut sleep,
                screen,
                kind,
            } => {
                // A sleep provider can disappear while display acquisition is pending.
                // Do not publish a complete grant using an invalid prerequisite.
                if let Err(error) = sleep.poll() {
                    context.unavailable(Kind::Logind, &error, false);
                    return Err(error);
                }
                match step(screen) {
                    Step::Pending(request) => Stage::Screen {
                        sleep,
                        screen: Ok(request),
                        kind,
                    },
                    Step::Held(screen) => {
                        tracing::info!(
                            backend = kind.as_str(),
                            suspend_backend = "logind",
                            "Sleep inhibition fallback acquired"
                        );
                        Stage::PairHeld {
                            sleep,
                            screen,
                            kind,
                        }
                    }
                    Step::Failed(error) if kind == Kind::ScreenSaver && error.unavailable => {
                        context.unavailable(kind, &error, true);
                        Stage::Screen {
                            sleep,
                            screen: context.start(Kind::X11),
                            kind: Kind::X11,
                        }
                    }
                    Step::Failed(error) => {
                        context.unavailable(kind, &error, false);
                        return Err(error);
                    }
                }
            }
            Stage::PortalHeld(mut request) => {
                if let Err(error) = request.poll() {
                    context.unavailable(Kind::Portal, &error, false);
                    return Err(error);
                }
                Stage::PortalHeld(request)
            }
            Stage::PairHeld {
                mut sleep,
                mut screen,
                kind,
            } => {
                if let Err(error) = sleep.poll() {
                    context.unavailable(Kind::Logind, &error, false);
                    return Err(error);
                }
                if let Err(error) = screen.poll() {
                    context.unavailable(kind, &error, false);
                    return Err(error);
                }
                Stage::PairHeld {
                    sleep,
                    screen,
                    kind,
                }
            }
            Stage::Released => unreachable!("released requests cannot be polled"),
        };
        Ok(
            if matches!(self.stage, Stage::PortalHeld(_) | Stage::PairHeld { .. }) {
                Progress::Held
            } else {
                Progress::Pending
            },
        )
    }
    fn release(mut self) -> Result<(), Error> {
        let stage = std::mem::replace(&mut self.stage, Stage::Released);
        match stage {
            Stage::Portal(Ok(request)) | Stage::Sleep(Ok(request)) | Stage::PortalHeld(request) => {
                request.release()
            }
            Stage::Screen { sleep, screen, .. } => {
                let display = match screen {
                    Ok(request) => request.release(),
                    Err(_) => Ok(()),
                };
                let suspend = sleep.release();
                release_results(display, suspend)
            }
            Stage::PairHeld { sleep, screen, .. } => {
                let display = screen.release();
                let suspend = sleep.release();
                release_results(display, suspend)
            }
            // Cancellation can discard an acquisition result that was never observed.
            Stage::Portal(Err(_)) | Stage::Sleep(Err(_)) | Stage::Released => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
