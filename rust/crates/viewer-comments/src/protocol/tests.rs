use super::*;

#[test]
fn history_boundary_and_reconnection_preserve_comment_classification() -> Result<(), Error> {
    let mut decoder = Decoder::default();
    let bytes = br#"{"chat":{"content":"first","user_id":"nicolive:42","date":0}}"#;
    let Event::Comment(history) = decoder.decode(bytes)? else {
        panic!("expected comment");
    };
    assert_eq!(history.phase, Phase::History);
    assert_eq!(history.origin, Origin::Niconico);
    assert_eq!(history.japan_time(), "09:00:00");
    assert_eq!(
        decoder.decode(br#"{"ping":{"content":"rf:0"}}"#)?,
        Event::HistoryComplete
    );
    let Event::Comment(live) = decoder.decode(bytes)? else {
        panic!("expected comment");
    };
    assert_eq!(live.phase, Phase::Live);
    assert_eq!(
        decoder.decode(br#"{"ping":{"content":"rf:0"}}"#)?,
        Event::Ignored
    );
    let Event::Comment(reconnected) = Decoder::default().decode(bytes)? else {
        panic!("expected comment");
    };
    assert_eq!(reconnected.phase, Phase::History);
    Ok(())
}

#[test]
fn protocol_metadata_is_ignored_and_invalid_messages_do_not_advance_phase() -> Result<(), Error> {
    let mut decoder = Decoder::default();
    for bytes in [
        b"{}".as_slice(),
        br#"{"thread":{"resultcode":0}}"#,
        br#"{"ping":{"content":"pf:0"}}"#,
    ] {
        assert_eq!(decoder.decode(bytes)?, Event::Ignored);
    }
    for bytes in [
        br#"{"ping":{"content":"rf:0"}"#.as_slice(),
        br#"{"chat":{"content":2}}"#,
        br#"{"chat":{"content":"x","date":-1}}"#,
        br#"{"chat":{"content":"x","date":18446744073709551616}}"#,
    ] {
        assert!(matches!(decoder.decode(bytes), Err(Error::Json(_))));
    }
    let Event::Comment(comment) = decoder.decode(br#"{"chat":{"content":"still history"}}"#)?
    else {
        panic!("expected comment");
    };
    assert_eq!(comment.phase, Phase::History);
    Ok(())
}

#[test]
fn limits_apply_to_frame_bytes_and_decoded_utf8_content() -> Result<(), Error> {
    let mut decoder = Decoder::default();
    assert!(matches!(
        decoder.decode(&vec![b' '; MAX_MESSAGE_BYTES + 1]),
        Err(Error::TooLarge { .. })
    ));
    for (text, accepted) in [
        ("x".repeat(4096), true),
        ("x".repeat(4097), false),
        ("あ".repeat(1366), false),
        (String::new(), false),
    ] {
        let bytes = serde_json::to_vec(&serde_json::json!({"chat":{"content":text}}))?;
        assert_eq!(
            matches!(decoder.decode(&bytes)?, Event::Comment(_)),
            accepted
        );
    }
    // The post-unescaping limit also applies to ASCII represented with JSON escapes.
    let bytes = format!(
        "{{\"chat\":{{\"content\":\"{}\"}}}}",
        "\\u0061".repeat(4097)
    );
    assert_eq!(decoder.decode(bytes.as_bytes())?, Event::Ignored);
    Ok(())
}

#[test]
fn timestamps_cannot_overflow_and_text_is_preserved_verbatim() -> Result<(), Error> {
    let mut decoder = Decoder::default();
    for (user, origin) in [("rekari:42", Origin::Niconico), ("random-user", Origin::Nx)] {
        let input = serde_json::to_vec(&serde_json::json!({"chat":{
            "content":"<b>字幕ではない</b>\n実況", "user_id":user, "date":u64::MAX,
            "unused":{"large":[1,2,3]}
        }}))?;
        let Event::Comment(comment) = decoder.decode(&input)? else {
            panic!("expected comment");
        };
        assert_eq!(comment.japan_time(), "16:00:15");
        assert_eq!(comment.origin, origin);
        assert_eq!(&*comment.text, "<b>字幕ではない</b>\n実況");
    }
    Ok(())
}

#[test]
fn thread_selection_and_subscription_keep_large_identifiers_as_strings() -> Result<(), Error> {
    let thread = ThreadId::active_in(
        br#"[
        {"id":1,"status":"PAST"},
        {"id":18446744073709551615,"status":"ACTIVE"},
        {"id":3,"status":"FUTURE"}
    ]"#,
    )?;
    let request: serde_json::Value = serde_json::from_str(&thread.subscription()?)?;
    assert_eq!(request[2]["thread"]["thread"], "18446744073709551615");
    assert_eq!(request[2]["thread"]["res_from"], -100);
    assert_eq!(request[4]["ping"]["content"], "rf:0");
    assert!(matches!(
        ThreadId::active_in(b"[]"),
        Err(Error::NoActiveThread)
    ));
    assert!(matches!(
        ThreadId::active_in(br#"[{"id":1,"status":"FUTURE"}]"#),
        Err(Error::NoActiveThread)
    ));
    assert!(matches!(
        ThreadId::active_in(&vec![0; MAX_THREAD_LIST_BYTES + 1]),
        Err(Error::TooLarge { .. })
    ));
    assert!(matches!(
        ThreadId::active_in(br#"[{"id":-1,"status":"ACTIVE"}]"#),
        Err(Error::Json(_))
    ));
    Ok(())
}

#[test]
fn mail_position_and_color_reach_the_comment_without_interpreting_text() -> Result<(), Error> {
    for (mail, position, color) in [
        ("184 ue red", Position::Top, 0xff0000),
        ("shita #12AbEF", Position::Bottom, 0x12abef),
        ("ue shita naka black", Position::Right, 0x000000),
        ("blue2", Position::Right, 0x3399ff),
        ("ue unknown #gggggg #123 #12345678", Position::Top, 0xffffff),
        ("", Position::Right, 0xffffff),
    ] {
        let input = serde_json::to_vec(&serde_json::json!({"chat": {
            "content": "<b>comment</b>", "mail": mail
        }}))?;
        let Event::Comment(comment) = Decoder::default().decode(&input)? else {
            panic!("expected comment");
        };
        assert_eq!(comment.style, Style { position, color });
        assert_eq!(&*comment.text, "<b>comment</b>");
    }
    Ok(())
}
