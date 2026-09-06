// unwrap below asserts successful decoding of the checked-in fixture.
// A decode/setup error must fail this test rather than be ignored.
#[path = "fixtures/sample.rs"]
mod fixture;
use libaribcaption::Decoder;

#[test]
fn decodes_upstream_sample_and_owns_result_after_decoder_drop() {
    let caption = {
        let mut decoder = Decoder::new().unwrap();
        let caption = decoder.decode(fixture::SAMPLE, 1234).unwrap().unwrap();
        for pts in 0..32 {
            decoder.flush();
            assert!(decoder.decode(fixture::SAMPLE, pts).unwrap().is_some());
        }
        caption
    };
    assert_eq!(caption.pts_ms, 1234);
    assert!(caption.clear_screen);
    assert_eq!(caption.text, "♬〜");
    assert_eq!(caption.duration_ms, None);
    assert_eq!((caption.plane_width, caption.plane_height), (960, 540));
    let characters: Vec<_> = caption.regions.iter().flat_map(|r| &r.characters).collect();
    assert_eq!(characters.len(), 2);
    assert_eq!((characters[0].x, characters[0].y), (170, 449));
    assert_eq!((characters[1].x, characters[1].y), (210, 449));
    assert_eq!(characters[0].background.alpha, 128);
    assert_eq!(characters[0].foreground.alpha, 255);
    for ch in characters {
        assert!(!ch.text.is_empty());
        assert!(ch.section_width > 0 && ch.section_height > 0);
    }
}
