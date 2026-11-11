use super::*;

fn sample(tick: u64, video: u64, audio: u64) -> Observation {
    Observation::Live(Sample {
        generation: 1,
        video_expected: true,
        audio_expected: true,
        received_bytes: tick * 188,
        video_input: tick,
        audio_input: tick,
        video_output: Some(video),
        audio_output: Some(audio),
        ..Default::default()
    })
}
fn feed(recovery: &mut Recovery, start: Instant, seconds: u64, video: u64, audio: u64) -> Action {
    recovery.observe(
        start + Duration::from_secs(seconds),
        sample(seconds + 1, video, audio),
    )
}

#[test]
fn either_output_stops_while_the_other_keeps_advancing() {
    for stopped_video in [true, false] {
        let start = Instant::now();
        let mut recovery = Recovery::default();
        for second in 0..STALL_TIMEOUT.as_secs() {
            let (video, audio) = if stopped_video {
                (1, second + 1)
            } else {
                (second + 1, 1)
            };
            assert!(matches!(
                feed(&mut recovery, start, second, video, audio),
                Action::None
            ));
        }
        let elapsed = STALL_TIMEOUT.as_secs();
        let (video, audio) = if stopped_video {
            (1, elapsed + 1)
        } else {
            (elapsed + 1, 1)
        };
        let Action::Restart(permit) = feed(&mut recovery, start, elapsed, video, audio) else {
            panic!("stall was missed");
        };
        assert_eq!(permit.attempt(), 1);
        assert_eq!(recovery.status(), Status::Recovering);
        // PLAYING alone is insufficient; both newly installed outputs must advance.
        feed(&mut recovery, start, elapsed + 1, 0, 0);
        feed(&mut recovery, start, elapsed + 2, 1, 0);
        assert_eq!(recovery.status(), Status::Recovering);
        feed(&mut recovery, start, elapsed + 3, 2, 1);
        assert_eq!(recovery.status(), Status::Idle);
    }
}

#[test]
fn paused_seeking_history_buffering_and_no_signal_never_spend_a_retry() {
    let start = Instant::now();
    for exempt in [
        Observation::Suspended,
        Observation::Buffering,
        Observation::Live(Sample {
            video_input: 1,
            audio_input: 1,
            ..Default::default()
        }),
        Observation::Live(Sample::default()),
    ] {
        let mut recovery = Recovery::default();
        for second in 0..120 {
            assert!(matches!(
                recovery.observe(start + Duration::from_secs(second), exempt),
                Action::None
            ));
        }
        assert_eq!(recovery.status(), Status::Idle);
        for second in 120..128 {
            feed(&mut recovery, start, second, 1, 1);
        }
        assert!(matches!(
            feed(&mut recovery, start, 128, 1, 1),
            Action::Restart(_)
        ));
    }
}

#[test]
fn no_signal_resumption_and_counter_reset_get_a_fresh_watch_window() {
    let start = Instant::now();
    let mut recovery = Recovery::default();
    for second in 0..6 {
        feed(&mut recovery, start, second, 20, 20);
    }
    for second in 6..30 {
        recovery.observe(start + Duration::from_secs(second), sample(6, 20, 20));
    }
    assert!(matches!(
        feed(&mut recovery, start, 30, 20, 20),
        Action::None
    ));
    assert!(matches!(feed(&mut recovery, start, 31, 0, 0), Action::None));
    assert!(matches!(feed(&mut recovery, start, 38, 0, 0), Action::None));
    assert!(matches!(
        feed(&mut recovery, start, 39, 0, 0),
        Action::Restart(_)
    ));
}

#[test]
fn retry_count_spacing_deadline_and_manual_reset_are_bounded() {
    let start = Instant::now();
    let mut recovery = Recovery::default();
    let mut restarts = Vec::new();
    for second in 0..120 {
        let outputs = match recovery.state {
            State::Starting { .. } => second,
            _ => 0,
        };
        if let Action::Restart(permit) = feed(&mut recovery, start, second, outputs, outputs) {
            restarts.push((second, permit.attempt()));
        }
    }
    assert_eq!(restarts.len(), usize::from(MAX_RESTARTS));
    assert!(
        restarts
            .windows(2)
            .all(|pair| pair[1].0 - pair[0].0 >= RESTART_INTERVAL.as_secs())
    );
    assert_eq!(recovery.status(), Status::Failed);
    assert!(matches!(
        feed(&mut recovery, start, 200, 5, 5),
        Action::None
    ));
    recovery.reset();
    for second in 200..208 {
        feed(&mut recovery, start, second, 5, 5);
    }
    assert!(matches!(
        feed(&mut recovery, start, 208, 5, 5),
        Action::Restart(_)
    ));
    let deadline = start + Duration::from_secs(208) + RECOVERY_TIMEOUT;
    assert!(matches!(
        recovery.observe(deadline, Observation::Buffering),
        Action::Failed
    ));
}

#[test]
fn reception_of_an_audio_only_service_does_not_require_video() {
    let start = Instant::now();
    let mut recovery = Recovery::default();
    for second in 0..60 {
        let observation = Observation::Live(Sample {
            generation: 7,
            audio_expected: true,
            received_bytes: second * 188,
            audio_input: second,
            audio_output: Some(second),
            ..Default::default()
        });
        assert!(matches!(
            recovery.observe(start + Duration::from_secs(second), observation),
            Action::None
        ));
    }
}

#[test]
fn missing_video_pes_is_still_a_stall_when_audio_and_the_video_pmt_remain() {
    let start = Instant::now();
    let mut recovery = Recovery::default();
    for second in 0..8 {
        let Observation::Live(mut observed) = sample(second + 1, 5, second + 1) else {
            unreachable!()
        };
        observed.video_input = 1;
        assert!(matches!(
            recovery.observe(
                start + Duration::from_secs(second),
                Observation::Live(observed)
            ),
            Action::None
        ));
    }
    assert!(matches!(
        feed(&mut recovery, start, 8, 5, 9),
        Action::Restart(_)
    ));
}

proptest::proptest! {
    #[test]
    fn arbitrary_operations_never_exceed_retry_count_or_spacing(operations in proptest::collection::vec(0u8..5, 0..300)) {
        let start = Instant::now();
        let mut recovery = Recovery::default();
        let mut restarts = Vec::new();
        let mut input = 0;
        let mut output = 0;
        for (second, operation) in operations.into_iter().enumerate() {
            let observation = match operation {
                0 => Observation::Suspended,
                1 => Observation::Buffering,
                2 => { input += 1; output += 1; sample(input, output, output) },
                3 => { input += 1; sample(input, output, output) },
                4 => sample(input, output, output),
                _ => unreachable!(),
            };
            if let Action::Restart(_) = recovery.observe(start + Duration::from_secs(second as u64), observation) {
                restarts.push(second);
            }
        }
        proptest::prop_assert!(restarts.len() <= usize::from(MAX_RESTARTS));
        proptest::prop_assert!(restarts.windows(2).all(|pair| pair[1] - pair[0] >= RESTART_INTERVAL.as_secs() as usize));
    }
}

#[test]
fn output_returning_with_reset_counters_cancels_a_spaced_retry() {
    let start = Instant::now();
    let mut recovery = Recovery::default();
    for second in 0..8 {
        feed(&mut recovery, start, second, 5, 5);
    }
    assert!(matches!(
        feed(&mut recovery, start, 8, 5, 5),
        Action::Restart(_)
    ));
    feed(&mut recovery, start, 9, 10, 10);
    feed(&mut recovery, start, 10, 11, 11);
    assert_eq!(recovery.status(), Status::Idle);
    for second in 11..=19 {
        feed(&mut recovery, start, second, 1000, 1000);
    }
    assert_eq!(recovery.status(), Status::Recovering);
    assert!(matches!(feed(&mut recovery, start, 20, 0, 0), Action::None));
    assert!(matches!(feed(&mut recovery, start, 21, 1, 1), Action::None));
    assert_eq!(recovery.status(), Status::Idle);
}

#[test]
fn a_changed_input_generation_requires_its_own_output_confirmation() {
    let start = Instant::now();
    let mut recovery = Recovery::default();
    for second in 0..8 {
        feed(&mut recovery, start, second, 5, 5);
    }
    assert!(matches!(
        feed(&mut recovery, start, 8, 5, 5),
        Action::Restart(_)
    ));
    feed(&mut recovery, start, 9, 10, 10);
    let Observation::Live(mut newer) = sample(11, 11, 11) else {
        unreachable!()
    };
    newer.generation = 2;
    recovery.observe(start + Duration::from_secs(10), Observation::Live(newer));
    assert_eq!(recovery.status(), Status::Recovering);
    newer.received_bytes += 188;
    newer.video_output = Some(12);
    newer.audio_output = Some(12);
    recovery.observe(start + Duration::from_secs(11), Observation::Live(newer));
    assert_eq!(recovery.status(), Status::Idle);
}
