use crate::http;
use std::path::Path;

pub const URL_ENV: &str = "CYDONIA_MCP_URL";
pub const PROJECT_ENV: &str = "CYDONIA_MCP_PROJECT";
pub const SESSION_ENV: &str = "CYDONIA_MCP_SESSION";
pub const ENVIRONMENT: [&str; 3] = [URL_ENV, PROJECT_ENV, SESSION_ENV];

pub struct Bootstrap {
    pub env: [(&'static str, String); 3],
    pub prompt: String,
}

impl Bootstrap {
    pub fn new(url: &str, project: &Path, session: &str) -> Self {
        Self {
            env: [
                (URL_ENV, url.to_owned()),
                (PROJECT_ENV, http::encoded(project)),
                (SESSION_ENV, session.to_owned()),
            ],
            prompt: format!(
                "Fetch and follow Cydonia's startup guide before working. Use the inherited environment variables to identify this session. With a POSIX shell:\n\n\
                 curl -fsS \"${URL_ENV}{}\" -H \"{}: ${PROJECT_ENV}\" -H \"{}: ${SESSION_ENV}\"\n\n\
                 Use the equivalent environment-variable syntax in other shells. If the request fails, report that Cydonia tools are unavailable; do not guess their behavior.",
                http::GUIDE_SUFFIX,
                http::PROJECT,
                http::SESSION,
            ),
        }
    }
}
