use crate::view::component::browser::{ConsoleLog, LOG_LIMIT};
use browser::{ConsoleLevel, ConsoleMessage};
use mcp::rail::Level;

#[test]
fn filters_preserve_ids_and_advance_the_cursor_past_unmatched_messages() {
    let mut log = ConsoleLog::default();
    for (level, text) in [
        (ConsoleLevel::Log, "started"),
        (ConsoleLevel::Error, "request failed"),
        (ConsoleLevel::Warn, "retrying"),
    ] {
        log.push(ConsoleMessage {
            level,
            text: text.into(),
            source: "https://example.com".into(),
        });
    }
    assert_eq!(
        log.read(&[Level::Error], 1),
        "cursor: 3\n[2] [error] request failed (https://example.com)"
    );
    assert_eq!(
        log.read(&[Level::Error], 2),
        "cursor: 3\nno matching console messages"
    );
    assert!(log.read(&[], 2).contains("[3] [warn] retrying"));
}

#[test]
fn eviction_does_not_reuse_message_ids_and_reports_the_gap() {
    let mut log = ConsoleLog::default();
    for _ in 0..=LOG_LIMIT {
        log.push(ConsoleMessage {
            level: ConsoleLevel::Debug,
            text: "tick".into(),
            source: String::new(),
        });
    }
    let output = log.read(&[], 0);
    assert!(output.starts_with("cursor: 501\n[messages before 2 are no longer retained]"));
    assert!(!output.contains("\n[1] "));
    assert!(output.ends_with("[501] [debug] tick ()"));
    assert!(!log.read(&[], 1).contains("no longer retained"));
}
