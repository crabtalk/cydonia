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

fn source() -> Record {
    serde_json::from_value(serde_json::json!({
        "id": "original", "agent": "claude", "session": "agent-original",
        "title": "Project", "name": null, "updated": 1, "closed": true,
        "items": [{"User": "first"}, {"Agent": "answer"}, {"User": "edit me"}, {"Agent": "excluded"}],
        "sent_at": {"0": 10, "2": 20}
    })).unwrap()
}

#[test]
fn fork_retains_only_prior_history_and_does_not_reuse_agent_identity() {
    let original = source();
    let before = serde_json::to_value(&original).unwrap();
    let fork = original.fork_at(2).unwrap();
    assert_eq!(fork.items.len(), 2);
    assert_eq!(fork.draft, "edit me");
    assert_eq!(fork.session, None);
    assert_eq!(fork.id, "");
    assert_eq!(fork.number, None);
    assert!(!fork.closed);
    assert_eq!(fork.sent_at.keys().copied().collect::<Vec<_>>(), vec![0]);
    assert_eq!(serde_json::to_value(&original).unwrap(), before);
    let restored: Record = serde_json::from_str(&serde_json::to_string(&fork).unwrap()).unwrap();
    let origin = restored.fork.unwrap();
    assert_eq!(origin.session, "original");
    assert_eq!(origin.before, 2);
    assert!(origin.pending);
    assert_eq!(restored.draft, "edit me");
}

#[test]
fn first_message_can_fork_and_non_user_positions_are_rejected() {
    let original = source();
    let fork = original.fork_at(0).unwrap();
    assert!(fork.items.is_empty());
    assert!(fork.sent_at.is_empty());
    assert_eq!(fork.draft, "first");
    assert!(original.fork_at(1).is_none());
    assert!(original.fork_at(4).is_none());
    assert!(original.fork_at(usize::MAX).is_none());
}
