//! A counter, a field that echoes, a choice and a long list: each node kind
//! once. Build it with
//! `cargo build -p cydonia-guest --example counter --target wasm32-unknown-unknown`.

use cydonia_guest::{
    App, host,
    node::*,
    wire::{ButtonStyle, Space, Surface, TextStyle, Tone},
};
use std::ops::Range;

struct Counter {
    count: u32,
    name: String,
    color: Option<String>,
    picked: Option<u32>,
}

#[derive(Clone)]
enum Msg {
    Add,
    Reset,
    Name(String),
    Color(String),
    Pick(u32),
    Panic,
}

impl App for Counter {
    type Msg = Msg;

    fn init() -> Self {
        host::set_input("name", "world");
        Counter {
            count: 0,
            name: "world".into(),
            color: None,
            picked: None,
        }
    }

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Add => self.count += 1,
            Msg::Reset => {
                self.count = 0;
                self.name.clear();
                host::set_input("name", "");
            }
            Msg::Name(name) => self.name = name,
            Msg::Color(color) => self.color = Some(color),
            Msg::Pick(row) => self.picked = Some(row),
            Msg::Panic => panic!("asked to panic"),
        }
    }

    fn view(&self) -> Node<Msg> {
        column([
            text(format!("Hello, {}", self.name))
                .style(TextStyle::Title)
                .into(),
            row([
                text(self.count.to_string()).key("count").into(),
                button("Add")
                    .style(ButtonStyle::Prominent)
                    .on_click(Msg::Add)
                    .into(),
                button("Reset").on_click(Msg::Reset).into(),
                button("Panic")
                    .style(ButtonStyle::Destructive)
                    .on_click(Msg::Panic)
                    .into(),
            ])
            .gap(Space::Small)
            .into(),
            input("name")
                .placeholder("Your name")
                .on_input(Msg::Name)
                .into(),
            select("color", [("red", "Red"), ("green", "Green")])
                .placeholder("Colour")
                .selected(self.color.clone())
                .on_select(Msg::Color)
                .into(),
            link("Cydonia", "https://cydonia.sh").into(),
            text(match self.picked {
                Some(row) => format!("Picked row {row}"),
                None => "No row picked".into(),
            })
            .tone(Tone::Muted)
            .into(),
            column([list("rows", 1000, 28.).into()])
                .surface(Surface::Card)
                .grow()
                .into(),
        ])
        .gap(Space::Medium)
        .padding(Space::Large)
        .into()
    }

    fn rows(&self, list: &str, range: Range<u32>) -> Vec<Node<Msg>> {
        if list != "rows" {
            return Vec::new();
        }
        range
            .map(|row| {
                button(format!("Row {row}"))
                    .key(format!("row-{row}"))
                    .on_click(Msg::Pick(row))
                    .into()
            })
            .collect()
    }
}

cydonia_guest::app!(Counter, name = "Counter");
