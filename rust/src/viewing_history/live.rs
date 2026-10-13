//! Count sustained playback separately from the two-channel return operation.
use std::time::{Duration, Instant};
pub(super) const QUALIFY_AFTER: Duration = Duration::from_secs(10);
pub(super) const MAX_RECENT: usize = 20;

#[derive(Default)]
enum Visit {
    #[default]
    Idle,
    Timing {
        service: u64,
        since: Instant,
    },
    Recorded(u64),
}
#[derive(Default)]
pub(super) struct Live {
    visit: Visit,
    current: Option<u64>,
    previous: Option<u64>,
}
impl Live {
    pub fn observe(&mut self, service: Option<u64>, now: Instant) -> Option<u64> {
        let Some(service) = service else {
            self.visit = Visit::Idle;
            return None;
        };
        if self.current != Some(service) {
            self.previous = self.current;
            self.current = Some(service);
        }
        match self.visit {
            Visit::Timing {
                service: old,
                since,
            } if old == service => {
                if now.saturating_duration_since(since) >= QUALIFY_AFTER {
                    self.visit = Visit::Recorded(service);
                    Some(service)
                } else {
                    None
                }
            }
            Visit::Recorded(old) if old == service => None,
            Visit::Idle | Visit::Timing { .. } | Visit::Recorded(_) => {
                self.visit = Visit::Timing {
                    service,
                    since: now,
                };
                None
            }
        }
    }
    pub fn previous(
        &self,
        current: u64,
        recent: &[u64],
        exists: impl Fn(u64) -> bool,
    ) -> Option<u64> {
        self.previous
            .into_iter()
            .chain(recent.iter().copied())
            .find(|service| *service != current && exists(*service))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zapping_does_not_enter_history_but_last_channel_remains_available() {
        let start = Instant::now();
        let mut live = Live::default();
        assert_eq!(live.observe(Some(1), start), None);
        assert_eq!(
            live.observe(Some(1), start + QUALIFY_AFTER - Duration::from_millis(1)),
            None
        );
        assert_eq!(live.observe(Some(2), start + QUALIFY_AFTER), None);
        assert_eq!(live.previous(2, &[], |_| true), Some(1));
        assert_eq!(live.observe(Some(1), start + QUALIFY_AFTER), None);
        assert_eq!(live.previous(1, &[], |_| true), Some(2));
        assert_eq!(live.observe(Some(1), start + QUALIFY_AFTER * 2), Some(1));
        assert_eq!(live.observe(Some(1), start + QUALIFY_AFTER * 3), None);
        assert_eq!(live.previous(1, &[1, 3, 2], |id| id != 2), Some(3));
    }
    #[test]
    fn interrupted_playback_restarts_qualification() {
        let start = Instant::now();
        let mut live = Live::default();
        live.observe(Some(1), start);
        live.observe(None, start + QUALIFY_AFTER);
        assert_eq!(live.observe(Some(1), start + QUALIFY_AFTER * 2), None);
        assert_eq!(live.observe(Some(1), start + QUALIFY_AFTER * 3), Some(1));
    }
}
