use artifact::stats::{Stats, Tokens};
use cydonia_gui::model::statistics::Summary;

#[test]
fn a_summary_lays_rows_out_by_day() {
    let dir = std::env::temp_dir().join(format!("cydonia-statistics-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let stats = Stats::open(&dir).unwrap();
    stats.words("2026-10-05", "a", 10, 2).unwrap();
    stats.message("2026-10-06", "s").unwrap();
    stats
        .usage(
            "2026-10-06",
            "s",
            "m",
            Tokens {
                input: 1,
                ..Tokens::default()
            },
        )
        .unwrap();
    // Outside a 7-day range ending on the 6th.
    stats.words("2026-09-01", "a", 99, 0).unwrap();

    let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 6).unwrap();
    let summary = Summary::compute(&dir, 7, today).unwrap();
    assert_eq!(summary.days.len(), 7);
    assert_eq!(summary.days[0].date, "2026-09-30");
    assert_eq!(summary.days[5].words_added, 10);
    assert_eq!(summary.days[6].messages, 1);
    assert_eq!(summary.grew, vec![("a".to_owned(), 8)]);
    assert_eq!(summary.spend.len(), 1);
    assert_eq!(summary.spend[0].agent, "unknown");
    let _ = std::fs::remove_dir_all(&dir);
}
