//! HTTP from an app, to the hosts its manifest names — `hosts = [...]` under
//! `[package.metadata.cydonia]` in its `Cargo.toml`. A request to any other host is refused without being
//! made. Each answers later, as the message `msg` makes of its result.
//!
//! ```ignore
//! fetch::get("https://yc-oss.github.io/api/meta.json", Msg::Meta);
//! ```

use crate::{
    host,
    wire::{Answer, Call, Request, Response},
};

/// `GET url`.
pub fn get<M: 'static>(
    url: impl Into<String>,
    msg: impl FnOnce(Result<Response, String>) -> M + 'static,
) {
    send(
        Request {
            method: "GET".into(),
            url: url.into(),
            headers: Vec::new(),
            body: None,
        },
        msg,
    );
}

/// Make `request`. A response of any status is `Ok`; a refused host or a
/// failed connection is `Err`.
pub fn send<M: 'static>(
    request: Request,
    msg: impl FnOnce(Result<Response, String>) -> M + 'static,
) {
    host::request(&Call::Fetch(request), move |answer| {
        msg(match answer {
            Answer::Response(response) => Ok(response),
            Answer::Error(error) => Err(error),
            other => Err(format!("Cydonia answered {other:?}")),
        })
    });
}
