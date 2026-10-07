use artifact::{project::Project as _, session::record::Record};
use chrono::TimeZone as _;
use cydonia_gui::model::statistics::{Logs, Summary, Tokens};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("cydonia-statistics-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

#[test]
fn activity_comes_from_the_project_files() {
    let dir = scratch("activity");
    let store = artifact::project::fs::Project::new(&dir);
    let noon = chrono::Local
        .with_ymd_and_hms(2026, 10, 5, 12, 0, 0)
        .unwrap();
    let id = store.create_session().unwrap();
    store
        .save_session(&Record {
            number: None,
            id,
            agent: "Claude Agent".into(),
            agent_id: Some("claude-acp".into()),
            session: None,
            title: "t".into(),
            name: None,
            updated: 0,
            closed: false,
            fork: None,
            replay: None,
            draft: String::new(),
            items: Vec::new(),
            sent_at: [(0, noon.timestamp() as u64)].into(),
        })
        .unwrap();
    let mut board = store.create_board("Work", "WORK").unwrap();
    let column = board.add_column("Todo").id.clone();
    board.add_card(&column, "new".into());
    board.columns[0].cards[0].id = noon.timestamp_millis().to_string();
    store.save_board(&mut board).unwrap();

    let logs = Logs {
        claude: None,
        codex: None,
    };
    let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 6).unwrap();
    let summary = Summary::compute(std::slice::from_ref(&dir), &logs, 7, today).unwrap();
    let day = summary
        .heat
        .iter()
        .find(|day| day.date == "2026-10-05")
        .unwrap();
    assert_eq!((day.sessions, day.cards), (1, 1));
    assert_eq!(summary.days.len(), 7);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn spend_comes_from_the_agents_logs() {
    let project = scratch("project");
    let home = scratch("home");
    let today = chrono::Local::now().date_naive();
    let at = chrono::Utc::now().to_rfc3339();

    let folder: String = project
        .to_string_lossy()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let claude = home.join("claude");
    std::fs::create_dir_all(claude.join(&folder)).unwrap();
    let message = format!(
        r#"{{"type":"assistant","timestamp":"{at}","requestId":"r1","message":{{"id":"m1","model":"claude-opus-5-5","usage":{{"input_tokens":2,"output_tokens":3,"cache_read_input_tokens":40,"cache_creation_input_tokens":5}}}}}}"#
    );
    // The same message twice, as a split message and a resumed copy write it.
    std::fs::write(
        claude.join(&folder).join("s.jsonl"),
        format!("{message}\n{message}\n"),
    )
    .unwrap();

    let codex = home.join("codex").join("2026").join("10").join("07");
    std::fs::create_dir_all(&codex).unwrap();
    let total = |input: u64, cached: u64, output: u64| {
        format!(
            r#"{{"timestamp":"{at}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{input},"cached_input_tokens":{cached},"output_tokens":{output}}}}}}}}}"#
        )
    };
    std::fs::write(
        codex.join("rollout-a.jsonl"),
        [
            format!(
                r#"{{"type":"session_meta","payload":{{"cwd":"{}"}}}}"#,
                project.display()
            ),
            r#"{"type":"turn_context","payload":{"model":"gpt-5.5"}}"#.to_owned(),
            total(100, 60, 10),
            total(250, 160, 30),
        ]
        .join("\n")
            + "\n",
    )
    .unwrap();

    let logs = Logs {
        claude: Some(claude),
        codex: Some(home.join("codex")),
    };
    let summary = Summary::compute(std::slice::from_ref(&project), &logs, 7, today).unwrap();
    let tokens = |model: &str| {
        summary
            .spend
            .iter()
            .find(|spend| spend.model == model)
            .unwrap()
            .tokens
    };
    assert_eq!(
        tokens("claude-opus-5-5"),
        Tokens {
            input: 2,
            output: 3,
            cache_read: 40,
            cache_write: 5,
        }
    );
    assert_eq!(
        tokens("gpt-5.5"),
        Tokens {
            input: 90,
            output: 30,
            cache_read: 160,
            cache_write: 0,
        }
    );

    // Read again, nothing is counted twice; appended, only the new line is.
    let again = Summary::compute(std::slice::from_ref(&project), &logs, 7, today).unwrap();
    assert_eq!(again.spend, summary.spend);
    let log = codex.join("rollout-a.jsonl");
    let mut text = std::fs::read_to_string(&log).unwrap();
    text.push('\n');
    text.push_str(&total(300, 200, 40));
    text.push('\n');
    std::fs::write(&log, text).unwrap();
    let grown = Summary::compute(std::slice::from_ref(&project), &logs, 7, today).unwrap();
    let gpt = grown
        .spend
        .iter()
        .find(|spend| spend.model == "gpt-5.5")
        .unwrap()
        .tokens;
    assert_eq!((gpt.input, gpt.output, gpt.cache_read), (100, 40, 200));
    let _ = std::fs::remove_dir_all(&project);
    let _ = std::fs::remove_dir_all(&home);
}
