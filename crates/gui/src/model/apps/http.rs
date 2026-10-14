//! An app's HTTP requests, made with ureq.

use super::Fetcher;
use anyhow::{Context as _, Result};
use guest::wire::{Request, Response};
use std::time::Duration;
use ureq::http;

/// How long one request may take, connecting to reading the body.
const TIMEOUT: Duration = Duration::from_secs(30);
/// The most of a body read.
const LIMIT: u64 = 32 << 20;

pub struct Ureq;

impl Fetcher for Ureq {
    fn fetch(request: &Request) -> Result<Response> {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent("Cydonia")
            .build()
            .new_agent();
        let method = http::Method::from_bytes(request.method.as_bytes())
            .with_context(|| format!("{} is not an HTTP method", request.method))?;
        let mut builder = http::Request::builder().method(method).uri(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let mut response = match &request.body {
            Some(body) => agent.run(builder.body(body.clone())?),
            None => agent.run(builder.body(())?),
        }
        .with_context(|| format!("{} {} failed", request.method, request.url))?;
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_owned(),
                    String::from_utf8_lossy(value.as_bytes()).into_owned(),
                )
            })
            .collect();
        let body = response
            .body_mut()
            .with_config()
            .limit(LIMIT)
            .read_to_vec()
            .with_context(|| format!("reading {} failed", request.url))?;
        Ok(Response {
            status: response.status().as_u16(),
            headers,
            body,
        })
    }
}
