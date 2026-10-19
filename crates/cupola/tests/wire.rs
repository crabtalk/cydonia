use cydonia_cupola::{
    Error,
    wire::{Envelope, Kind, MAX_FRAME},
};
use futures::{executor::block_on, io::Cursor};

fn envelope() -> Envelope {
    Envelope {
        id: 3,
        ack_id: 2,
        service: "echo".into(),
        kind: Kind::Response { to: 9 },
        payload: vec![1, 2, 3],
    }
}

#[test]
fn frames_round_trip_in_order() {
    let mut bytes = Vec::new();
    block_on(envelope().write(&mut bytes)).unwrap();
    block_on(Envelope::ack().write(&mut bytes)).unwrap();
    let length = u32::from_le_bytes(bytes[..4].try_into().unwrap());

    let mut reader = Cursor::new(bytes);
    assert!(length > 0);
    assert_eq!(
        block_on(Envelope::read(&mut reader)).unwrap(),
        Some(envelope())
    );
    assert_eq!(
        block_on(Envelope::read(&mut reader)).unwrap(),
        Some(Envelope::ack())
    );
    assert_eq!(block_on(Envelope::read(&mut reader)).unwrap(), None);
}

#[test]
fn a_stream_cut_inside_a_frame_is_an_error() {
    let mut bytes = Vec::new();
    block_on(envelope().write(&mut bytes)).unwrap();
    bytes.truncate(bytes.len() - 1);
    let error = block_on(Envelope::read(&mut Cursor::new(bytes))).unwrap_err();
    assert!(matches!(error, Error::Io(_)));
}

#[test]
fn an_oversized_length_is_refused() {
    let bytes = (MAX_FRAME + 1).to_le_bytes().to_vec();
    let error = block_on(Envelope::read(&mut Cursor::new(bytes))).unwrap_err();
    assert!(matches!(error, Error::Oversized(len) if len == MAX_FRAME as usize + 1));
}
