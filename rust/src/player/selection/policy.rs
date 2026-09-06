/// `playing` includes connection establishment, so repeated clicks do not
/// interrupt a pending request. Errors and stop both clear it in Player.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SelectionAction {
    Keep,
    Start,
}

impl SelectionAction {
    pub(super) fn for_request(current: Option<u64>, requested: u64, playing: bool) -> Self {
        if playing && current == Some(requested) {
            Self::Keep
        } else {
            Self::Start
        }
    }
}

pub(super) fn adjacent_index(ids: &[u64], current: Option<u64>, offset: i32) -> Option<i32> {
    let count = i64::try_from(ids.len()).ok().filter(|count| *count > 0)?;
    let current = current.and_then(|id| ids.iter().position(|candidate| *candidate == id));
    let next = match current {
        Some(index) => (i64::try_from(index).ok()? + i64::from(offset)).rem_euclid(count),
        None if offset < 0 => count - 1,
        None => 0,
    };
    i32::try_from(next).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_active_selection_but_allows_retry_and_other_channels() {
        assert_eq!(
            SelectionAction::for_request(Some(10), 10, true),
            SelectionAction::Keep
        );
        assert_eq!(
            SelectionAction::for_request(Some(10), 10, false),
            SelectionAction::Start
        );
        assert_eq!(
            SelectionAction::for_request(Some(10), 20, true),
            SelectionAction::Start
        );
        assert_eq!(
            SelectionAction::for_request(None, 10, false),
            SelectionAction::Start
        );
    }

    #[test]
    fn unselected_or_missing_service_starts_at_the_boundary() {
        for current in [None, Some(99)] {
            assert_eq!(adjacent_index(&[10, 20, 30], current, 1), Some(0));
            assert_eq!(adjacent_index(&[10, 20, 30], current, -1), Some(2));
        }
        assert_eq!(adjacent_index(&[], None, 1), None);
        assert_eq!(adjacent_index(&[10], None, -1), Some(0));
    }

    #[test]
    fn wraps_existing_selection_and_handles_large_offsets() {
        assert_eq!(adjacent_index(&[10, 20, 30], Some(30), 1), Some(0));
        assert_eq!(adjacent_index(&[10, 20, 30], Some(10), -1), Some(2));
        assert_eq!(adjacent_index(&[10, 20, 30], Some(20), 0), Some(1));
        assert_eq!(adjacent_index(&[10, 20, 30], Some(20), i32::MAX), Some(2));
        assert_eq!(adjacent_index(&[10, 20, 30], Some(20), i32::MIN), Some(2));
    }
}
