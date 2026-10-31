use super::*;
use std::{
    io::{Cursor, Read, Seek, SeekFrom},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

fn list(entries: &[(u64, &str)]) -> List {
    List::new(
        entries
            .iter()
            .map(|(ms, name)| Chapter::new(*ms, (*name).into()).unwrap())
            .collect(),
    )
    .unwrap()
}
fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut bytes = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    bytes.extend(kind);
    bytes.extend(body);
    bytes
}
fn movie() -> Vec<u8> {
    atom(b"moov", &atom(b"udta", &atom(b"chpl", &mp4::fixture())))
}
fn discover(bytes: &[u8]) -> Result<List, Error> {
    mp4::discover(
        &mut Cursor::new(bytes),
        bytes.len() as u64,
        &AtomicBool::new(false),
    )
}

#[test]
fn nero_v1_preserves_names_milliseconds_and_commercial_prefix() {
    let parsed = discover(&movie()).unwrap();
    assert_eq!(
        parsed,
        list(&[(0, "CM?"), (10_110, "A90Sec"), (109_743, "CM")])
    );
    let values: serde_json::Value = serde_json::from_str(parsed.json()).unwrap();
    assert_eq!(
        values[1],
        serde_json::json!({"startMs":10110,"name":"A90Sec","commercial":false})
    );
    assert_eq!(values[0]["commercial"], true);
    assert!(!Chapter::new(0, "cm".into()).unwrap().commercial);
    assert_eq!(
        list(&[(10, "日本語"), (0, "先頭"), (10, "重複")])
            .entries
            .len(),
        2
    );
    assert_eq!(
        list(&[(10, "日本語"), (10, "重複")]).entries[0].name,
        "日本語"
    );
}

#[test]
fn nero_rejects_all_truncations_invalid_names_versions_and_times() {
    let bytes = mp4::fixture();
    for end in 0..bytes.len() {
        assert!(mp4::parse(&bytes[..end]).is_err(), "cut at {end}");
    }
    let mut invalid = bytes.clone();
    invalid[0] = 0;
    assert!(mp4::parse(&invalid).is_err());
    invalid = bytes.clone();
    invalid[18] = 0xff;
    assert!(matches!(mp4::parse(&invalid), Err(Error::Utf8(_))));
    invalid = bytes.clone();
    invalid[9..17].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(matches!(mp4::parse(&invalid), Err(Error::Limit)));
    invalid = bytes;
    invalid.push(0);
    assert!(mp4::parse(&invalid).is_err());
    assert!(Chapter::new(0, "a".repeat(256)).is_err());
    assert!(List::new(vec![Chapter::new(0, String::new()).unwrap(); 256]).is_err());
    let mut maximum = vec![1, 0, 0, 0, 0, 0, 0, 0, 255];
    for n in 0..255_u64 {
        maximum.extend((n * 10_000).to_be_bytes());
        maximum.push(255);
        maximum.extend(vec![b'a'; 255]);
    }
    assert_eq!(mp4::parse(&maximum).unwrap().entries.len(), 255);
}

#[test]
fn atom_bounds_cover_extended_sizes_duplicates_and_missing_chapters() {
    assert_eq!(
        discover(&atom(b"moov", &atom(b"free", &[]))).unwrap(),
        List::default()
    );
    let movie = movie();
    for end in 1..movie.len() {
        assert!(discover(&movie[..end]).is_err(), "cut at {end}");
    }
    for size in [2_u32, 7, u32::MAX] {
        let mut invalid = movie.clone();
        invalid[..4].copy_from_slice(&size.to_be_bytes());
        assert!(discover(&invalid).is_err());
    }
    let mut extended = 1_u32.to_be_bytes().to_vec();
    extended.extend(b"moov");
    extended.extend((movie.len() as u64 + 8).to_be_bytes());
    extended.extend(&movie[8..]);
    assert_eq!(discover(&extended).unwrap(), discover(&movie).unwrap());
    let mut to_end = movie.clone();
    to_end[..4].fill(0);
    assert_eq!(discover(&to_end).unwrap(), discover(&movie).unwrap());
    let duplicate = [
        atom(b"chpl", &mp4::fixture()),
        atom(b"chpl", &mp4::fixture()),
    ]
    .concat();
    assert!(discover(&atom(b"moov", &atom(b"udta", &duplicate))).is_err());
    assert_eq!(
        discover(&atom(b"free", &[]).repeat(64)).unwrap(),
        List::default()
    );
    assert!(matches!(
        discover(&atom(b"free", &[]).repeat(65)),
        Err(Error::Limit)
    ));
    assert!(matches!(
        discover(&atom(b"moov", &atom(b"free", &[]).repeat(1025))),
        Err(Error::Limit)
    ));
    let mut oversized =
        Cursor::new([((16_u32 * 1024 * 1024) + 1).to_be_bytes(), *b"moov"].concat());
    assert!(matches!(
        mp4::discover(
            &mut oversized,
            16 * 1024 * 1024 + 1,
            &AtomicBool::new(false)
        ),
        Err(Error::Limit)
    ));
    assert_eq!(
        oversized.position(),
        8,
        "reject size before allocation/read"
    );
    assert!(matches!(
        mp4::discover(&mut Cursor::new(movie), 100, &AtomicBool::new(true)),
        Err(Error::Cancelled)
    ));
}

#[test]
fn large_media_payload_is_skipped_without_reading_it() {
    struct Sparse {
        position: u64,
        bytes_read: usize,
        tail: Vec<u8>,
    }
    const PAYLOAD_END: u64 = 8 * 1024 * 1024 * 1024;
    impl Read for Sparse {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            let head = [
                1_u32.to_be_bytes().as_slice(),
                b"mdat",
                &PAYLOAD_END.to_be_bytes(),
            ]
            .concat();
            let source = if self.position < 16 {
                &head[self.position as usize..]
            } else {
                assert!(self.position >= PAYLOAD_END, "mdat payload read");
                &self.tail[(self.position - PAYLOAD_END) as usize..]
            };
            let count = bytes.len().min(source.len());
            bytes[..count].copy_from_slice(&source[..count]);
            self.position += count as u64;
            self.bytes_read += count;
            Ok(count)
        }
    }
    impl Seek for Sparse {
        fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
            let SeekFrom::Start(position) = position else {
                panic!("absolute seeks only")
            };
            self.position = position;
            Ok(position)
        }
    }
    let mut source = Sparse {
        position: 0,
        bytes_read: 0,
        tail: movie(),
    };
    let size = PAYLOAD_END + source.tail.len() as u64;
    assert_eq!(
        mp4::discover(&mut source, size, &AtomicBool::new(false))
            .unwrap()
            .entries
            .len(),
        3
    );
    assert_eq!(source.bytes_read, 16 + source.tail.len());
}

#[test]
fn navigation_visits_every_boundary_and_obeys_restart_threshold_and_edges() {
    let chapters = list(&[
        (0, "CM?"),
        (10_000, "A"),
        (20_000, "CM"),
        (30_000, "B"),
        (40_000, "CM"),
        (50_000, "CM?"),
    ]);
    for (position, offset, expected) in [
        (0, 1, Some(10_000)),
        (10_000, 1, Some(20_000)),
        (20_000, 1, Some(30_000)),
        (30_000, 1, Some(40_000)),
        (40_000, 1, Some(50_000)),
        (50_000, 1, None),
        (50_000, -1, Some(40_000)),
        (30_000, -1, Some(20_000)),
        (33_000, -1, Some(20_000)),
        (33_001, -1, Some(30_000)),
        (23_000, -1, Some(10_000)),
        (23_001, -1, Some(20_000)),
        (10_000, -1, Some(0)),
        (0, -1, None),
        (3000, -1, None),
        (3001, -1, Some(0)),
        (60_000, -1, Some(50_000)),
        (10_000, 0, None),
        (10_000, 2, None),
    ] {
        assert_eq!(
            chapters.target(offset, position),
            expected,
            "{position}, {offset}"
        );
    }
    let delayed = list(&[(10_000, "CM"), (20_000, "A")]);
    assert_eq!(delayed.current(9999), None);
    assert_eq!(delayed.target(1, 0), Some(10_000));
    assert_eq!(delayed.target(-1, 0), None);
    assert_eq!(List::default().target(1, 0), None);
}

fn toc() -> gstreamer::Toc {
    gstreamer::init().unwrap();
    let mut toc = gstreamer::Toc::new(gstreamer::TocScope::Global);
    for (id, title, start) in [("1", "Opening", 0), ("2", "CM", 5_000_000_000_i64)] {
        let mut entry = gstreamer::TocEntry::new(gstreamer::TocEntryType::Chapter, id);
        entry.get_mut().unwrap().set_start_stop_times(start, -1);
        let mut tags = gstreamer::TagList::new();
        tags.get_mut()
            .unwrap()
            .add::<gstreamer::tags::Title>(&title, gstreamer::TagMergeMode::Replace);
        entry.get_mut().unwrap().set_tags(Some(tags));
        toc.get_mut().unwrap().append_entry(entry);
    }
    toc
}

#[test]
fn container_toc_and_controller_publish_only_seekable_recordings_and_reset() {
    let chapters = List::from_toc(&toc()).unwrap();
    assert_eq!(chapters, list(&[(0, "Opening"), (5000, "CM")]));
    let mut controller = Controller::default();
    controller.update(
        Some(Source::transport(1)),
        Some(chapters.clone()),
        true,
        Some(6000),
        None,
    );
    assert_eq!(controller.current(), 1);
    assert_eq!(controller.target(-1, Some(6000)), Some(0));
    assert_eq!(controller.target(1, Some(0)), Some(5000));
    assert_eq!(controller.target(1, None), None);
    let revision = controller.revision();
    controller.update(
        Some(Source::transport(1)),
        Some(chapters.clone()),
        true,
        Some(6001),
        None,
    );
    assert_eq!(
        controller.revision(),
        revision,
        "position-only ticks within a chapter do not notify"
    );
    controller.update(
        Some(Source::transport(1)),
        Some(chapters.clone()),
        true,
        Some(6001),
        Some(5000),
    );
    assert_eq!(
        controller.current(),
        0,
        "ignore entries at or beyond duration"
    );
    controller.update(Some(Source::transport(2)), None, true, Some(0), None);
    assert_eq!(controller.json(), "[]");
    assert_eq!(controller.current(), -1);
    for (source, seekable) in [(Some(Source::transport(3)), false), (None, true)] {
        controller.update(source, Some(chapters.clone()), seekable, Some(0), None);
        assert_eq!(controller.json(), "[]");
        assert_eq!(controller.target(1, Some(0)), None);
    }
}

#[test]
fn seek_flush_keeps_published_chapters_duration_and_index_without_notifications() {
    let chapters = list(&[(0, "Opening"), (5000, "CM"), (10000, "Past duration")]);
    let mut controller = Controller::default();
    controller.update(
        Some(Source::transport(1)),
        Some(chapters.clone()),
        true,
        Some(6000),
        Some(9000),
    );
    let published = controller.json().to_owned();
    let revision = controller.revision();
    assert!(!published.contains("Past duration"));
    for (seekable, position, duration) in [
        (false, None, None),
        (false, Some(6000), None),
        (true, None, None),
        (true, Some(6000), Some(9000)),
    ] {
        controller.update(
            Some(Source::transport(1)),
            None,
            seekable,
            position,
            duration,
        );
        assert_eq!(controller.json(), published);
        assert_eq!(controller.current(), 1);
        assert_eq!(
            controller.revision(),
            revision,
            "a range/position gap must not notify QML"
        );
    }
    controller.update(
        Some(Source::transport(1)),
        Some(chapters.clone()),
        true,
        Some(2000),
        Some(9000),
    );
    assert_eq!(controller.json(), published);
    assert_eq!(controller.current(), 0);
    assert_eq!(
        controller.revision(),
        revision + 1,
        "confirmed chapter changes still notify"
    );

    controller.update(
        Some(Source::transport(2)),
        Some(chapters.clone()),
        false,
        None,
        None,
    );
    assert_eq!(controller.json(), "[]");
    assert_eq!(controller.current(), -1);
    controller.update(
        Some(Source::transport(2)),
        Some(chapters.clone()),
        true,
        Some(6000),
        Some(9000),
    );
    assert_eq!(controller.json(), published);
    controller.update(None, Some(chapters.clone()), true, None, None);
    assert_eq!(
        controller.json(),
        "[]",
        "stop/live/timeshift must clear a retained list"
    );
    assert_eq!(controller.current(), -1);
    controller.update(
        Some(Source::transport(2)),
        Some(chapters),
        false,
        Some(6000),
        Some(9000),
    );
    assert_eq!(
        controller.json(),
        "[]",
        "reopening must establish seekability again"
    );
}

#[test]
fn chapter_discovery_handles_local_and_http_mp4_and_toc_wins() {
    use crate::playback::recording::{Loader, Recording, Request, serve_ts};
    gstreamer::init().unwrap();
    let mut bytes = include_bytes!("../../../../tests/fixtures/media-h264.mp4").to_vec();
    let mut position = 0;
    while position < bytes.len() {
        let length = u32::from_be_bytes(bytes[position..position + 4].try_into().unwrap()) as usize;
        if &bytes[position + 4..position + 8] == b"moov" {
            let mut body = bytes[position + 8..position + length].to_vec();
            body.extend(atom(b"udta", &atom(b"chpl", &mp4::fixture())));
            bytes[position + 4..position + 8].copy_from_slice(b"free");
            bytes.extend(atom(b"moov", &body));
            break;
        }
        position += length;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chapters.mp4");
    std::fs::write(&path, &bytes).unwrap();
    let server = serve_ts(bytes);
    for request in [
        Request::open(path),
        Request::from_url(&format!("{}/api/videos/123", server.uri())).unwrap(),
    ] {
        let mut loader = Loader::default();
        loader.begin(request);
        let deadline = Instant::now() + Duration::from_secs(10);
        let file = loop {
            if let Some((_, result)) = loader.poll() {
                break result.unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        };
        let Recording::Media(file) = file else {
            panic!("MP4 expected")
        };
        let mut controller = Controller::default();
        loop {
            controller.update(
                Some(Source::media(1, file.clone())),
                None,
                true,
                Some(11_000),
                None,
            );
            if !controller.busy() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(controller.current(), 1);
        assert!(controller.json().contains("A90Sec"));
        controller.update(
            Some(Source::media(1, file.clone())),
            Some(List::from_toc(&toc()).unwrap()),
            true,
            Some(0),
            None,
        );
        assert!(controller.json().contains("Opening"));
        assert!(!controller.json().contains("A90Sec"));
        let published = controller.json().to_owned();
        let revision = controller.revision();
        controller.update(Some(Source::media(1, file)), None, false, None, None);
        assert_eq!(
            controller.json(),
            published,
            "a missing TOC during flush must not restore the MP4 fallback"
        );
        assert_eq!(controller.revision(), revision);
        controller.update(None, None, false, None, None);
        assert_eq!(controller.json(), "[]");
        assert_eq!(controller.current(), -1);
    }
}
