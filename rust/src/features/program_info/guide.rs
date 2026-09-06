//! A calendar-day range validated at the QML boundary (milliseconds since epoch).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayWindow {
    start: u64,
    end: u64,
}
#[derive(Debug, thiserror::Error)]
#[error("番組表の日付範囲が不正です")]
pub struct InvalidDayWindow;
impl DayWindow {
    pub fn new(start: f64, end: f64) -> Result<Self, InvalidDayWindow> {
        // JavaScript Date's range is inside the exact integer range of f64.
        let timestamp = |value: f64| {
            value.is_finite()
                && value.fract() == 0.0
                && (0.0..=8_640_000_000_000_000.0).contains(&value)
        };
        if !timestamp(start)
            || !timestamp(end)
            || end <= start
            || end - start > 26.0 * 60.0 * 60.0 * 1000.0
        {
            return Err(InvalidDayWindow);
        }
        Ok(Self {
            start: start as u64,
            end: end as u64,
        })
    }
    pub(super) fn overlaps(self, start: u64, duration: u64) -> bool {
        duration > 0 && start < self.end && start.saturating_add(duration) > self.start
    }
}

#[derive(Default)]
pub enum Guide {
    #[default]
    Closed,
    AwaitingDay,
    Showing(DayWindow),
}
