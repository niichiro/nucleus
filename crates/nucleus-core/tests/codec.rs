//! Тесты кодека: кадрирование, версионирование, roundtrip.

use nucleus_core::{codec, Ack, Action, Command, FrameError, Message, Response};

fn sample_message() -> Message {
    Message::Command(Command {
        id: 42,
        device_id: "lamp:living".into(),
        action: Action::OnOff(true),
        timestamp: 1_234_567,
        ttl: 8,
    })
}

#[test]
fn roundtrip_command() {
    let frame = codec::encode(&sample_message()).unwrap();
    assert_eq!(&frame[..codec::FRAME_MAGIC.len()], b"NUC");
    assert_eq!(frame[codec::FRAME_MAGIC.len()], codec::PROTOCOL_VERSION);
    let decoded: Message = codec::decode(&frame).unwrap();
    assert_eq!(decoded, sample_message());
}

#[test]
fn roundtrip_all_variants() {
    let messages = vec![
        Message::Ack(Ack {
            command_id: 7,
            result_code: 0,
            new_state: None,
        }),
        Message::Query(nucleus_core::Query {
            device_ids: None,
            request_id: 3,
        }),
        Message::Response(Response {
            request_id: 3,
            devices: Vec::new(),
        }),
        Message::Announce(nucleus_core::Announce {
            node_id: [7u8; 32],
            node_type: nucleus_core::NodeType::OrangePi,
            flags: 0,
            devices: vec!["lamp:living".into()],
            protocol_version: codec::PROTOCOL_VERSION,
        }),
    ];
    for msg in messages {
        let decoded: Message = codec::decode(&codec::encode(&msg).unwrap()).unwrap();
        assert_eq!(decoded, msg);
    }
}

#[test]
fn rejects_bad_magic() {
    let mut frame = codec::encode(&sample_message()).unwrap();
    frame[0] = b'X';
    let err = codec::decode::<Message>(&frame).unwrap_err();
    assert!(matches!(err, FrameError::BadMagic));
}

#[test]
fn rejects_short_frame() {
    assert!(matches!(
        codec::decode::<Message>(b""),
        Err(FrameError::BadMagic)
    ));
    assert!(matches!(
        codec::decode::<Message>(b"NUC"),
        Err(FrameError::BadMagic)
    ));
}

#[test]
fn rejects_unsupported_version() {
    let mut frame = codec::encode(&sample_message()).unwrap();
    let foreign = codec::PROTOCOL_VERSION + 1;
    frame[codec::FRAME_MAGIC.len()] = foreign;

    let err = codec::decode::<Message>(&frame).unwrap_err();
    assert!(matches!(err, FrameError::UnsupportedVersion(v) if v == foreign));

    // Узел с совпадающей версией декодирует тот же кадр без проблем.
    let decoded: Message = codec::decode_versioned(&frame, foreign).unwrap();
    assert_eq!(decoded, sample_message());
}
