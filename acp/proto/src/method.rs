//! Every method name in ACP v1, in one place.
//!
//! Both sides need both lists: a server routes on the agent names and calls
//! the client names, and a client does the reverse.

/// Cancels an in-flight request by id, in either direction.
pub const CANCEL_REQUEST: &str = "$/cancel_request";

/// Methods the agent serves.
pub mod agent {
    pub const INITIALIZE: &str = "initialize";
    pub const AUTHENTICATE: &str = "authenticate";
    pub const LOGOUT: &str = "logout";
    pub const SESSION_NEW: &str = "session/new";
    pub const SESSION_LOAD: &str = "session/load";
    pub const SESSION_RESUME: &str = "session/resume";
    pub const SESSION_FORK: &str = "session/fork";
    pub const SESSION_CLOSE: &str = "session/close";
    pub const SESSION_LIST: &str = "session/list";
    pub const SESSION_DELETE: &str = "session/delete";
    pub const SESSION_SET_MODE: &str = "session/set_mode";
    pub const SESSION_SET_CONFIG_OPTION: &str = "session/set_config_option";
    pub const SESSION_PROMPT: &str = "session/prompt";
    /// Notification.
    pub const SESSION_CANCEL: &str = "session/cancel";

    pub const PROVIDERS_LIST: &str = "providers/list";
    pub const PROVIDERS_SET: &str = "providers/set";
    pub const PROVIDERS_DISABLE: &str = "providers/disable";

    pub const NES_START: &str = "nes/start";
    pub const NES_SUGGEST: &str = "nes/suggest";
    pub const NES_CLOSE: &str = "nes/close";
    /// Notification.
    pub const NES_ACCEPT: &str = "nes/accept";
    /// Notification.
    pub const NES_REJECT: &str = "nes/reject";

    /// Notification.
    pub const DOCUMENT_DID_OPEN: &str = "document/didOpen";
    /// Notification.
    pub const DOCUMENT_DID_CHANGE: &str = "document/didChange";
    /// Notification.
    pub const DOCUMENT_DID_CLOSE: &str = "document/didClose";
    /// Notification.
    pub const DOCUMENT_DID_SAVE: &str = "document/didSave";
    /// Notification.
    pub const DOCUMENT_DID_FOCUS: &str = "document/didFocus";

    /// Also a client method: MCP traffic tunnels in both directions, as a
    /// request when it wants an answer and as a notification when it does not.
    pub const MCP_MESSAGE: &str = "mcp/message";
}

/// Methods the client serves.
pub mod client {
    /// Notification.
    pub const SESSION_UPDATE: &str = "session/update";
    pub const SESSION_REQUEST_PERMISSION: &str = "session/request_permission";
    pub const FS_READ_TEXT_FILE: &str = "fs/read_text_file";
    pub const FS_WRITE_TEXT_FILE: &str = "fs/write_text_file";
    pub const TERMINAL_CREATE: &str = "terminal/create";
    pub const TERMINAL_OUTPUT: &str = "terminal/output";
    pub const TERMINAL_RELEASE: &str = "terminal/release";
    pub const TERMINAL_WAIT_FOR_EXIT: &str = "terminal/wait_for_exit";
    pub const TERMINAL_KILL: &str = "terminal/kill";
    pub const ELICITATION_CREATE: &str = "elicitation/create";
    /// Notification.
    pub const ELICITATION_COMPLETE: &str = "elicitation/complete";
    pub const MCP_CONNECT: &str = "mcp/connect";
    pub const MCP_DISCONNECT: &str = "mcp/disconnect";
    /// Also an agent method — see [`agent::MCP_MESSAGE`](super::agent::MCP_MESSAGE).
    pub const MCP_MESSAGE: &str = "mcp/message";
}
