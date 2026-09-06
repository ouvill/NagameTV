//! Main's simulcast rule, using explicit broadcast metadata rather than endpoint IDs.
use super::model::Program;
use crate::channels::{Channel, PhysicalChannel};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Signature {
    event: u16,
    start: u64,
    duration: u64,
}

pub(super) fn indices<'a>(
    channels: &[Channel],
    current: impl Fn(&Channel) -> Option<&'a Program>,
) -> Vec<usize> {
    let mut first: HashMap<&PhysicalChannel, Option<Signature>> = HashMap::new();
    let mut seen = HashSet::new();
    channels
        .iter()
        .enumerate()
        .filter_map(|(index, channel)| {
            // Older/minimal services without a physical channel cannot be grouped safely.
            let Some(physical) = channel.physical.as_ref() else {
                return Some(index);
            };
            let signature = current(channel).and_then(|program| {
                program.event_id.map(|event| Signature {
                    event,
                    start: program.start_at,
                    duration: program.duration,
                })
            });
            let Some(primary) = first.get(physical) else {
                first.insert(physical, signature);
                seen.insert((physical, signature));
                return Some(index);
            };
            (matches!((primary, signature), (Some(main), Some(sub)) if *main != sub)
                && seen.insert((physical, signature)))
            .then_some(index)
        })
        .collect()
}

/// Indices retain catalog order, including when the playing service has become hidden.
pub(super) fn adjacent(
    visible: &[usize],
    selected: Option<usize>,
    step: crate::channels::Step,
) -> Option<usize> {
    use crate::channels::Step;
    match (selected, step) {
        (None, Step::Next) => visible.first().copied(),
        (None, Step::Previous) => visible.last().copied(),
        (Some(current), Step::Next) => visible
            .get(visible.partition_point(|index| *index <= current))
            .or_else(|| visible.first())
            .copied(),
        (Some(current), Step::Previous) => visible
            .partition_point(|index| *index < current)
            .checked_sub(1)
            .and_then(|index| visible.get(index))
            .or_else(|| visible.last())
            .copied(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::program_info::model;

    #[test]
    fn navigation_skips_hidden_services_and_wraps_in_both_directions() {
        use crate::channels::Step::{Next, Previous};
        let visible = [0, 2, 5];
        assert_eq!(adjacent(&visible, Some(0), Next), Some(2));
        assert_eq!(adjacent(&visible, Some(2), Previous), Some(0));
        assert_eq!(adjacent(&visible, Some(5), Next), Some(0));
        assert_eq!(adjacent(&visible, Some(0), Previous), Some(5));
        assert_eq!(adjacent(&visible, Some(3), Next), Some(5));
        assert_eq!(adjacent(&visible, Some(3), Previous), Some(2));
        assert_eq!(adjacent(&visible, None, Next), Some(0));
        assert_eq!(adjacent(&visible, None, Previous), Some(5));
        assert_eq!(adjacent(&[], Some(0), Next), None);
        assert_eq!(adjacent(&[2], Some(2), Previous), Some(2));
    }

    #[test]
    fn broadcast_identity_distinguishes_split_programs_without_endpoint_arithmetic()
    -> Result<(), Box<dyn std::error::Error>> {
        let channels = crate::channels::parse(br#"[
            {"id":99,"name":"A","type":1,"networkId":10,"serviceId":1,"channel":{"type":"GR","channel":"27"}},
            {"id":123,"name":"B","type":1,"networkId":10,"serviceId":2,"channel":{"type":"GR","channel":"27"}},
            {"id":18446744073709551615,"name":"C","type":1,"networkId":10,"serviceId":3,"channel":{"type":"GR","channel":"27"}}
        ]"#)?;
        let snapshot = model::parse(br#"[
            {"id":900,"eventId":7,"networkId":10,"serviceId":1,"name":"Main","startAt":100,"duration":100},
            {"id":999,"eventId":7,"networkId":10,"serviceId":2,"name":"Different title, same event","startAt":100,"duration":100},
            {"id":1001,"eventId":8,"networkId":10,"serviceId":3,"name":"Main","startAt":100,"duration":100}
        ]"#)?;
        assert_eq!(
            indices(&channels, |c| snapshot.current(c.broadcast, 100)),
            [0, 2]
        );
        assert_eq!(
            indices(&channels, |c| snapshot.current(c.broadcast, 200)),
            [0]
        );
        assert_eq!(indices(&channels, |_| None), [0]);
        Ok(())
    }

    #[test]
    fn missing_physical_metadata_keeps_services_and_distinct_carriers_do_not_merge()
    -> Result<(), Box<dyn std::error::Error>> {
        let channels = crate::channels::parse(br#"[
            {"id":1,"name":"A","type":1,"networkId":10,"serviceId":1,"channel":{"type":"GR","channel":"27"}},
            {"id":2,"name":"B","type":1,"networkId":10,"serviceId":2,"channel":{"type":"GR","channel":"28"}},
            {"id":3,"name":"C","type":1,"networkId":11,"serviceId":3,"channel":{"type":"GR","channel":"27"}},
            {"id":4,"name":"D","type":1,"networkId":10,"serviceId":4},
            {"id":5,"name":"E","type":1,"networkId":10,"serviceId":5}
        ]"#)?;
        assert_eq!(indices(&channels, |_| None), [0, 1, 2, 3, 4]);
        Ok(())
    }
}
