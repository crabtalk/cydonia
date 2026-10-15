//! No app runtime. [`Absent`] has no values: [`Runner::start`] refuses every
//! app, so nothing else here can be called. [`Offline`] makes no request.

use super::{Command, Event, Fetcher, Node, Request, Response, Runner};
use anyhow::{Result, bail};
use std::ops::Range;

pub enum Absent {}

impl Runner for Absent {
    fn start(_: &[u8]) -> Result<(Self, Vec<Command>)> {
        bail!("Apps do not run in this build")
    }

    fn update(&mut self, _: &Event) -> Result<Vec<Command>> {
        match *self {}
    }

    fn view(&mut self) -> Result<Node> {
        match *self {}
    }

    fn rows(&mut self, _: &str, _: Range<u32>) -> Result<Vec<Node>> {
        match *self {}
    }
}

pub struct Offline;

impl Fetcher for Offline {
    fn fetch(request: &Request) -> Result<Response> {
        bail!("{} is not reachable from this build", request.url)
    }
}
