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
    assert_eq!(characters[0].text, "♬");
    assert_eq!(characters[1].text, "〜");
    assert_eq!((characters[0].x, characters[0].y), (170, 449));
    assert_eq!((characters[1].x, characters[1].y), (210, 449));
    assert_eq!(characters[0].background.alpha, 128);
    assert_eq!(characters[0].foreground.alpha, 255);
    for ch in characters {
        assert!(!ch.text.is_empty());
        assert!(ch.section_width > 0 && ch.section_height > 0);
    }
}

#[path = "fixtures/drcs.rs"]
mod drcs;

#[test]
fn drcs_redefinition_and_decoder_drop_do_not_change_retained_pixels() {
    use libaribcaption::Glyph;
    let mut decoder = Decoder::new().unwrap();
    decoder.decode(&drcs::management(8, &[]), 0).unwrap();
    let first = decoder
        .decode(&drcs::bitmap_statement(0x90, true), 1000)
        .unwrap()
        .unwrap();
    let second = decoder
        .decode(&drcs::bitmap_statement(0x60, false), 2000)
        .unwrap()
        .unwrap();
    decoder.flush();
    drop(decoder);
    let Glyph::Drcs(mask) = &first.regions[0].characters[0].glyph else {
        panic!("expected bitmap")
    };
    assert_eq!(mask.pixels(), &[255, 0, 0, 255]);
    assert_eq!((mask.width(), mask.height()), (2, 2));
    assert!(
        first
            .regions
            .iter()
            .flat_map(|r| &r.characters)
            .any(|c| matches!(c.glyph, Glyph::Text) && !c.text.is_empty())
    );
    let Glyph::Drcs(mask) = &second.regions[0].characters[0].glyph else {
        panic!("expected bitmap")
    };
    assert_eq!(mask.pixels(), &[0, 255, 255, 0]);
}
