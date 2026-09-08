//! Caption component selection after wire-format validation.
//!
//! ARIB TR-B15 v4.6, Part 1 Vol.7 §5.1.3: default caption tag is 0x30.
//! Vol.4 §30.3.3.3: hierarchy alternatives are related by reference_PID;
//! a low-layer component without a reference is an ordinary selection candidate.
//! https://www.arib.or.jp/english/html/overview/doc/8-TR-B15v4_6-2p4-E1.pdf
//! https://www.arib.or.jp/english/html/overview/doc/8-TR-B15v4_6-3p4-E1.pdf
use super::wire::{CaptionStream, ComponentTag, Pid, TransmissionLayer};

/// Normal reception policy for this viewer. The HTTP transport provides no RF
/// quality indication, so silence or missing caption statements must NOT cause
/// automatic rain-fade switching. Language selection is inside the chosen ES
/// and remains the decoder's first-language policy.
pub(super) fn select_caption(streams: &[CaptionStream]) -> Option<Pid> {
    let primary = streams
        .iter()
        .find(|stream| stream.component_tag == ComponentTag::DEFAULT_CAPTION)
        .or_else(|| {
            // Application fallback when the default is absent. Prefer ordinary
            // components over referenced low-layer alternatives, then stable tags.
            // An SD-only PMT is still usable when its partner is absent.
            streams.iter().min_by_key(|stream| {
                let alternative = stream.hierarchy.is_some_and(|hierarchy| {
                    hierarchy.layer == TransmissionLayer::Low && hierarchy.reference.is_some()
                });
                (alternative, stream.component_tag, stream.pid)
            })
        })?;

    if let Some(hierarchy) = primary.hierarchy
        && hierarchy.layer == TransmissionLayer::Low
        && let Some(reference) = hierarchy.reference
        && let Some(high) = streams.iter().find(|stream| {
            stream.pid == reference
                && stream.hierarchy.is_some_and(|partner| {
                    partner.layer == TransmissionLayer::High
                        && partner.reference == Some(primary.pid)
                })
        })
    {
        return Some(high.pid);
    }
    Some(primary.pid)
}

#[cfg(test)]
mod tests {
    use super::super::wire::Hierarchy;
    use super::*;

    fn stream(pid: u16, tag: u8, layer: Option<(TransmissionLayer, Option<u16>)>) -> CaptionStream {
        CaptionStream {
            pid: Pid(pid),
            component_tag: ComponentTag(tag),
            hierarchy: layer.map(|(layer, reference)| Hierarchy {
                layer,
                reference: reference.map(Pid),
            }),
        }
    }

    #[test]
    fn default_is_not_stolen_by_an_unrelated_high_layer() {
        let default = stream(0x130, 0x30, None);
        let other = stream(0x132, 0x32, Some((TransmissionLayer::High, Some(0x133))));
        for streams in [[default, other], [other, default]] {
            assert_eq!(select_caption(&streams), Some(default.pid));
        }
    }

    #[test]
    fn only_the_related_high_layer_replaces_a_low_layer_default() {
        let low = stream(0x130, 0x30, Some((TransmissionLayer::Low, Some(0x131))));
        let high = stream(0x131, 0x31, Some((TransmissionLayer::High, Some(0x130))));
        assert_eq!(select_caption(&[low, high]), Some(high.pid));
        assert_eq!(select_caption(&[high, low]), Some(high.pid));
        let unrelated = stream(0x131, 0x31, Some((TransmissionLayer::High, Some(0x132))));
        assert_eq!(select_caption(&[low, unrelated]), Some(low.pid));
    }

    #[test]
    fn unreferenced_low_layer_is_an_ordinary_default() {
        let default = stream(0x130, 0x30, Some((TransmissionLayer::Low, None)));
        let high = stream(0x131, 0x31, Some((TransmissionLayer::High, Some(0x132))));
        assert_eq!(select_caption(&[high, default]), Some(default.pid));
    }

    #[test]
    fn absent_default_has_a_stable_fallback_and_sd_only_remains_usable() {
        let ordinary = stream(0x132, 0x32, None);
        let low = stream(0x131, 0x31, Some((TransmissionLayer::Low, Some(0x130))));
        assert_eq!(select_caption(&[low, ordinary]), Some(ordinary.pid));
        assert_eq!(select_caption(&[low]), Some(low.pid));
        assert_eq!(select_caption(&[]), None);
    }
}
