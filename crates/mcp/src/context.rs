use crate::tools::workspace;
use std::path::Path;

pub fn session(
    cwd: &Path,
    session: Option<u64>,
    article: Option<u64>,
    mcp_available: bool,
    hidden: &[&str],
) -> String {
    let mut context = prompts::session_context(cwd, session, article, mcp_available, hidden);
    if let Some(shown) = workspace::on_screen() {
        context.push_str("\n\nOn screen in cydonia:\n");
        context.push_str(&shown);
    }
    context
}
