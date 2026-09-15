use cydonia_artifact::session::{chat::ChatItem, record::Record};

#[test]
fn message_times_survive_round_trip_and_legacy_records_stay_readable() {
    let legacy = serde_json::json!({
        "agent": "claude",
        "title": "Hello",
        "name": null,
        "updated": 1757000000_u64,
        "items": [{"User": "Hello"}]
    });
    let mut record: Record = serde_json::from_value(legacy).unwrap();
    assert!(record.sent_at.is_empty());
    record.sent_at.insert(0, 1756999900);

    let restored: Record = serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
    assert_eq!(restored.sent_at.get(&0), Some(&1756999900));
    assert!(matches!(&restored.items[0], ChatItem::User(text) if text == "Hello"));
}
