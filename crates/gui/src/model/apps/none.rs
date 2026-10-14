//! No app runtime. [`Absent`] has no values: [`Runner::start`] refuses every
//! app, so nothing else here can be called.

use super::{Command, Event, Node, Runner};
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
