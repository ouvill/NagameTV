use super::test_fixture as fixture;
use super::*;
use serde_json::{Value, json};

#[test]
fn synthetic_carousel_uses_broadcast_identifiers_and_matching_block_data() {
    // STD-B24 fascicle 3 §§6.2.2, 6.3.2, 6.5: the DII transaction is
    // network-originated, while the DDB header identifies the download itself.
    let mut receiver = arib_b24::transport::SectionPackets::new(0x1f00).unwrap();
    let raw: Vec<_> = fixture::carousel(&mut 0)
        .as_chunks::<188>()
        .0
        .iter()
        .flat_map(|packet| receiver.push(packet).unwrap())
        .collect();
    let sections: Vec<_> = raw
        .iter()
        .map(|section| arib_b24::Section::parse(section).unwrap())
        .collect();
    let [
        arib_b24::Section::Info(info),
        arib_b24::Section::Block(block),
    ] = sections.as_slice()
    else {
        panic!("fixture must contain one DII and one DDB");
    };
    assert_eq!(info.transaction_id >> 30, 0b10);
    assert_eq!(info.download_id, block.download_id);
    assert_ne!(info.transaction_id, info.download_id);
    assert_eq!(
        u16::from_be_bytes(raw[0][3..5].try_into().unwrap()),
        info.transaction_id as u16
    );
    assert_eq!(info.modules.len(), 1);
    let module = &info.modules[0];
    assert_eq!(module.id, block.module_id);
    assert_eq!(module.version, block.module_version);
    assert_eq!(module.size as usize, block.data.len());
    assert_eq!(block.block_number, 0);
    assert_eq!(
        u16::from_be_bytes(raw[1][3..5].try_into().unwrap()),
        block.module_id
    );
    assert_eq!((raw[1][5] >> 1) & 31, block.module_version & 31);
    // DDB syntax has 27 bytes after section_length in addition to block data.
    assert!(usize::from(info.block_size) + 27 <= 4093);
    assert!(block.data.len() <= usize::from(info.block_size));
    assert_eq!(
        block.data.as_slice(),
        include_bytes!("../../../../tests/fixtures/bml/overlay.bml")
    );
}

#[test]
fn synthetic_pmt_only_advertises_the_carousel_services_it_provides() {
    for automatic in [false, true] {
        let mut receiver = arib_b24::transport::SectionPackets::new(fixture::PMT_PID).unwrap();
        let sections: Vec<_> = fixture::tables(Some(automatic), 0)
            .as_chunks::<188>()
            .0
            .iter()
            .flat_map(|packet| receiver.push(packet).unwrap())
            .collect();
        assert_eq!(sections.len(), 1);
        let components =
            arib_b24::transport::data_components_from_pmt(&sections[0], fixture::SERVICE).unwrap();
        assert_eq!(components.len(), 1);
        let info = components[0].bxml_info().unwrap();
        assert_eq!(info.entry_point.as_ref().unwrap().auto_start, automatic);
        assert_eq!(info.entry_point.as_ref().unwrap().document_resolution, 3);
        let carousel = info.carousel.as_ref().unwrap();
        assert!(!carousel.event_sections);
        assert!(!carousel.ondemand_retrieval);
        assert!(!carousel.file_storable);
    }
}

#[test]
fn monitor_discovers_startup_changes_without_receiving_modules() {
    let mut decoder = Decoder::new(fixture::SERVICE, None).unwrap();
    decoder.collecting = false;
    assert_eq!(decoder.entry(), Entry::Unknown);
    let mut counter = 0;
    decoder.consume(&fixture::tables(Some(false), 0));
    assert_eq!(decoder.entry(), Entry::Manual);
    assert!(decoder.consume(&fixture::carousel(&mut counter)).is_empty());
    decoder.consume(&fixture::tables(Some(true), 1));
    assert_eq!(decoder.entry(), Entry::Automatic);
    decoder.collecting = true;
    let updates = decoder.consume(&fixture::carousel(&mut counter));
    assert!(
        updates
            .iter()
            .any(|update| matches!(update, Update::Broadcast(ServiceUpdate::Module { .. })))
    );
    decoder.consume(&fixture::tables(None, 2));
    assert_eq!(decoder.entry(), Entry::Absent);
}

#[tokio::test]
async fn monitor_publishes_startup_policy_and_can_be_promoted_to_display() {
    let session = Session::start(fixture::SERVICE, None).unwrap();
    session.configure(Demand::Monitor).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !session.tap.active() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!session.receiving());
    session.tap.push(&fixture::tables(Some(true), 0));
    tokio::time::timeout(Duration::from_secs(2), async {
        while session.entry() != Entry::Automatic {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let generation = session.capture.generation.load(Ordering::Acquire);
    session.configure(Demand::Display).unwrap();
    wait_receiving(&session, true).await;
    let mut socket = browser(&session).await;
    let (_, messages) = replay(&mut socket).await;
    assert!(messages.iter().any(|message| message["type"] == "pmt"));
    assert_eq!(
        session.capture.generation.load(Ordering::Acquire),
        generation
    );
}

#[test]
fn fragmented_ts_has_the_same_updates_as_contiguous_input() {
    fn messages(updates: Vec<Update>) -> Vec<Value> {
        updates
            .into_iter()
            .map(|update| match update {
                Update::Broadcast(update) => viewer_web_bml::encode(update).unwrap(),
                Update::Metadata(metadata::Update::Program(program)) => {
                    viewer_web_bml::program(&program)
                }
                Update::Metadata(metadata::Update::Time(time)) => {
                    viewer_web_bml::current_time(time)
                }
                Update::Metadata(metadata::Update::Clock { base, extension }) => {
                    viewer_web_bml::pcr(base, extension)
                }
            })
            .collect()
    }
    let bytes = include_bytes!("../../../../tests/fixtures/recording-seek.ts");
    let expected = messages(Decoder::new(1, None).unwrap().consume(bytes));
    assert!(!expected.is_empty());
    for size in [
        1,
        PACKET_BYTES - 1,
        PACKET_BYTES,
        PACKET_BYTES + 1,
        CHUNK_BYTES,
    ] {
        let mut decoder = Decoder::new(1, None).unwrap();
        let mut actual = Vec::new();
        for chunk in bytes.chunks(size) {
            assert!(decoder.consume(&[]).is_empty());
            actual.extend(messages(decoder.consume(chunk)));
            assert!(decoder.pending_len < PACKET_BYTES);
        }
        assert_eq!(actual, expected, "chunk size {size}");
        assert_eq!(decoder.pending_len, 0);
    }
}

type Socket = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;
async fn read(socket: &mut Socket) -> Value {
    let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    serde_json::from_str(message.to_text().unwrap()).unwrap()
}
async fn connect(port: u16) -> Socket {
    let stream = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))
        .await
        .unwrap();
    tokio_tungstenite::client_async(format!("ws://127.0.0.1:{port}/data/secret"), stream)
        .await
        .unwrap()
        .0
}
async fn replay(socket: &mut Socket) -> (String, Vec<Value>) {
    let begin = read(socket).await;
    replay_after_begin(socket, begin).await
}
async fn replay_after_begin(socket: &mut Socket, begin: Value) -> (String, Vec<Value>) {
    assert_eq!(begin["type"], "begin");
    assert_eq!(begin["version"], 1);
    let epoch = begin["epoch"].as_str().unwrap().to_owned();
    let mut messages = Vec::new();
    loop {
        let frame = read(socket).await;
        assert_eq!(frame["epoch"], epoch);
        if frame["type"] == "ready" {
            return (epoch, messages);
        }
        assert_eq!(frame["type"], "update");
        messages.push(frame["message"].clone());
    }
}

#[test]
fn slow_browser_detaches_without_stopping_reception() {
    let capture = Capture::default();
    capture.active.store(true, Ordering::Release);
    capture.connected.store(true, Ordering::Release);
    let (sender, _receiver) = mpsc::channel(1);
    let mut client = Some(Client {
        id: 1,
        sender,
        budget: Arc::new(Semaphore::new(OUTPUT_RESOURCE_BYTES)),
    });
    publish(&mut client, &capture, delivery::Delivery::Fault("test"));
    assert!(client.is_some());
    publish(&mut client, &capture, delivery::Delivery::Fault("test"));
    assert!(client.is_none());
    assert!(capture.active.load(Ordering::Acquire));
    assert!(!capture.connected.load(Ordering::Acquire));
}

#[tokio::test(flavor = "current_thread")]
async fn reconnect_replays_received_state_and_only_explicit_reset_discards_it() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let capture = Arc::new(Capture::default());
    let (input_tx, input_rx) = mpsc::channel(INPUT_CAPACITY);
    let (control_tx, control_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(run(
        listener,
        "secret".into(),
        ServiceIdentity {
            service_id: 1,
            original_network_id: None,
        },
        capture.clone(),
        input_rx,
        control_rx,
        control_tx.clone(),
    ));
    let bad_stream = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))
        .await
        .unwrap();
    assert!(
        tokio_tungstenite::client_async(format!("ws://127.0.0.1:{port}/data/wrong"), bad_stream)
            .await
            .is_err()
    );

    control_tx
        .send(Command::Configure(Demand::Display))
        .unwrap();
    let mut first = connect(port).await;
    let (epoch, messages) = replay(&mut first).await;
    let generation = capture.generation.load(Ordering::Acquire);
    assert_eq!(messages.len(), 1); // No invented empty PMT to reset the browser.
    assert_eq!(messages[0]["type"], "programInfo");
    assert!(messages[0]["transportStreamId"].is_null());
    first.close(None).await.unwrap();
    // A disconnect has no bearing on TS intake; receive actual SI with no browser.
    for bytes in include_bytes!("../../../../tests/fixtures/recording-seek.ts").chunks(CHUNK_BYTES)
    {
        input_tx
            .send(Ingress {
                generation,
                payload: Payload::Bytes(bytes.to_vec()),
            })
            .await
            .unwrap();
    }
    // A command is prioritized over input. Observe live metadata once all queued
    // input has drained rather than relying on a scheduler sleep.
    let mut second = connect(port).await;
    let (second_epoch, snapshot) = replay(&mut second).await;
    assert_eq!(second_epoch, epoch);
    let mut identified = snapshot
        .iter()
        .any(|m| m["type"] == "programInfo" && m["eventId"] == 2);
    while !identified {
        let frame = read(&mut second).await;
        identified = frame["message"]["type"] == "programInfo" && frame["message"]["eventId"] == 2;
    }
    second.close(None).await.unwrap();
    let mut third = connect(port).await;
    let (third_epoch, snapshot) = replay(&mut third).await;
    assert_eq!(third_epoch, epoch);
    assert_eq!(snapshot[0]["transportStreamId"], 1);
    assert_eq!(snapshot[0]["eventId"], 2);
    assert!(capture.active.load(Ordering::Acquire));
    // A late disconnect from the old subscriber cannot detach the new one.
    control_tx.send(Command::Disconnected(2)).unwrap();
    input_tx
        .send(Ingress {
            generation: generation.wrapping_sub(1),
            payload: Payload::Reset,
        })
        .await
        .unwrap();
    input_tx
        .send(Ingress {
            generation,
            payload: Payload::Reset,
        })
        .await
        .unwrap();
    let mut frame = read(&mut third).await;
    while frame["type"] == "update" {
        assert_eq!(frame["epoch"], epoch); // Already queued updates precede reset.
        frame = read(&mut third).await;
    }
    let (reset_epoch, reset) = replay_after_begin(&mut third, frame).await;
    assert_ne!(reset_epoch, epoch);
    assert_eq!(reset.len(), 1);
    assert!(reset[0]["transportStreamId"].is_null());
    assert!(capture.connected.load(Ordering::Acquire));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), third.next())
            .await
            .is_err()
    );

    control_tx
        .send(Command::Configure(Demand::Stopped))
        .unwrap();
    assert_eq!(
        read(&mut third).await,
        json!({"type":"fault", "reason":"closed"})
    );
    assert!(!capture.active.load(Ordering::Acquire));
    control_tx.send(Command::Shutdown).unwrap();
    task.await.unwrap().unwrap();
}

async fn wait_receiving(session: &Session, receiving: bool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while session.receiving() != receiving {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
}

async fn browser(session: &Session) -> Socket {
    let url = url::Url::parse(session.url()).unwrap();
    let stream = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, url.port().unwrap()))
        .await
        .unwrap();
    tokio_tungstenite::client_async(session.url(), stream)
        .await
        .unwrap()
        .0
}

#[tokio::test(flavor = "current_thread")]
async fn prefetch_survives_open_close_and_stopped_discards_the_cache() {
    let session = Session::start(1, None).unwrap();
    session.configure(Demand::Prefetch).unwrap();
    wait_receiving(&session, true).await;
    let generation = session.capture.generation.load(Ordering::Acquire);
    // Prefetch grants no browser subscription, including a stale page's URL.
    let mut denied = browser(&session).await;
    assert_eq!(
        read(&mut denied).await,
        json!({"type":"fault", "reason":"closed"})
    );
    assert!(!session.connected());
    assert!(session.receiving());
    for bytes in include_bytes!("../../../../tests/fixtures/recording-seek.ts").chunks(CHUNK_BYTES)
    {
        session
            .tap
            .sender
            .send(Ingress {
                generation,
                payload: Payload::Bytes(bytes.to_vec()),
            })
            .await
            .unwrap();
    }
    // Drain the bounded input before opening. SI is repeated throughout the
    // fixture; the final chunk contains no change to its last program identity.
    tokio::time::timeout(Duration::from_secs(2), async {
        while session.tap.sender.capacity() != INPUT_CAPACITY {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    session.configure(Demand::Display).unwrap();
    let mut first = browser(&session).await;
    let (epoch, snapshot) = replay(&mut first).await;
    assert_eq!(snapshot[0]["transportStreamId"], 1);
    assert_eq!(snapshot[0]["eventId"], 2);
    assert_eq!(
        session.capture.generation.load(Ordering::Acquire),
        generation
    );
    session.configure(Demand::Display).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), first.next())
            .await
            .is_err()
    );

    session.configure(Demand::Prefetch).unwrap();
    assert_eq!(
        read(&mut first).await,
        json!({"type":"fault", "reason":"closed"})
    );
    assert!(session.receiving());
    assert!(!session.connected());
    session.configure(Demand::Display).unwrap();
    let mut second = browser(&session).await;
    let (same_epoch, cached) = replay(&mut second).await;
    assert_eq!(same_epoch, epoch);
    assert_eq!(cached[0]["eventId"], 2);

    session.configure(Demand::Stopped).unwrap();
    assert_eq!(
        read(&mut second).await,
        json!({"type":"fault", "reason":"closed"})
    );
    wait_receiving(&session, false).await;
    session.configure(Demand::Display).unwrap();
    let mut third = browser(&session).await;
    let (new_epoch, empty) = replay(&mut third).await;
    assert_ne!(new_epoch, epoch);
    assert!(empty[0]["transportStreamId"].is_null());
    assert_ne!(
        session.capture.generation.load(Ordering::Acquire),
        generation
    );
}

#[tokio::test(flavor = "current_thread")]
async fn failed_prefetch_can_be_retried_by_opening_without_a_background_retry_loop() {
    let session = Session::start(1, None).unwrap();
    session.configure(Demand::Prefetch).unwrap();
    wait_receiving(&session, true).await;
    let generation = session.capture.generation.load(Ordering::Acquire);
    session.capture.overflow.store(true, Ordering::Release);
    wait_receiving(&session, false).await;
    session.configure(Demand::Prefetch).unwrap();
    assert!(!session.receiving());
    assert_eq!(
        session.capture.generation.load(Ordering::Acquire),
        generation
    );
    session.configure(Demand::Display).unwrap();
    let mut client = browser(&session).await;
    replay(&mut client).await;
    assert!(session.receiving());
    assert_ne!(
        session.capture.generation.load(Ordering::Acquire),
        generation
    );
}

#[tokio::test(flavor = "current_thread")]
async fn overflow_reported_after_stop_does_not_poison_the_next_prefetch() {
    let session = Session::start(1, None).unwrap();
    session.configure(Demand::Prefetch).unwrap();
    wait_receiving(&session, true).await;
    let generation = session.capture.generation.load(Ordering::Acquire);
    session.configure(Demand::Stopped).unwrap();
    let mut denied = browser(&session).await;
    assert_eq!(
        read(&mut denied).await,
        json!({"type":"fault", "reason":"closed"})
    );
    assert!(!session.receiving());
    // Simulate an old enqueue finishing after the stop command was handled.
    session.capture.overflow.store(true, Ordering::Release);
    session.configure(Demand::Prefetch).unwrap();
    wait_receiving(&session, true).await;
    assert_ne!(
        session.capture.generation.load(Ordering::Acquire),
        generation
    );
    assert!(!session.capture.overflow.load(Ordering::Acquire));
}
