#[cfg(not(target_os = "linux"))]
use browser::ConsoleLevel;
use browser::ConsoleMessage;
#[cfg(not(target_os = "linux"))]
use mcp::rail::Level;
use std::collections::VecDeque;
#[cfg(not(target_os = "linux"))]
use std::fmt::Write as _;

pub(crate) const LOG_LIMIT: usize = 500;

#[derive(Default)]
pub(crate) struct ConsoleLog {
    cursor: u64,
    entries: VecDeque<(u64, ConsoleMessage)>,
}

impl ConsoleLog {
    pub(crate) fn push(&mut self, message: ConsoleMessage) {
        self.cursor += 1;
        if self.entries.len() == LOG_LIMIT {
            self.entries.pop_front();
        }
        self.entries.push_back((self.cursor, message));
    }

    #[cfg(not(target_os = "linux"))]
    pub(crate) fn read(&self, levels: &[Level], since: u64) -> String {
        let mut out = format!("cursor: {}", self.cursor);
        if let Some((first, _)) = self.entries.front()
            && since < first - 1
        {
            let _ = write!(out, "\n[messages before {first} are no longer retained]");
        }
        let mut matched = false;
        for (id, message) in &self.entries {
            let level = match message.level {
                ConsoleLevel::Debug => Level::Debug,
                ConsoleLevel::Log => Level::Log,
                ConsoleLevel::Info => Level::Info,
                ConsoleLevel::Warn => Level::Warn,
                ConsoleLevel::Error => Level::Error,
            };
            if *id <= since || (!levels.is_empty() && !levels.contains(&level)) {
                continue;
            }
            matched = true;
            let _ = write!(
                out,
                "\n[{id}] [{level}] {} ({})",
                message.text, message.source
            );
        }
        if !matched {
            out.push_str("\nno matching console messages");
        }
        out
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/browser_console.rs"]
mod tests;
