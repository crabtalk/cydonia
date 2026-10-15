//! The nodes an app draws, with the messages they send back typed as the
//! app's own `M`.
//!
//! ```ignore
//! column([
//!     text("Hello").style(TextStyle::Title).into(),
//!     input("name").placeholder("Your name").on_input(Msg::Name).into(),
//!     button("Greet").on_click(Msg::Greet).into(),
//! ])
//! .gap(Space::Medium)
//! ```

use crate::wire::{
    self, Align, Axis, ButtonStyle, Choice, Justify, Source, Space, Surface, TextStyle, Tone,
};
use std::{collections::HashMap, rc::Rc};

pub enum Node<M> {
    Layout(Layout<M>),
    Button(Button<M>),
    Input(Input<M>),
    Select(Select<M>),
    /// A node that sends nothing back.
    Plain(wire::Node),
}

pub struct Layout<M> {
    wire: wire::Layout,
    children: Vec<Node<M>>,
}

pub struct Button<M> {
    wire: wire::Button,
    on_click: Option<Rc<dyn Fn() -> M>>,
}

pub struct Input<M> {
    wire: wire::Input,
    on_input: Option<Rc<dyn Fn(String) -> M>>,
}

pub struct Select<M> {
    wire: wire::Select,
    on_select: Option<Rc<dyn Fn(String) -> M>>,
}

/// Children in a column.
pub fn column<M>(children: impl IntoIterator<Item = Node<M>>) -> Layout<M> {
    Layout {
        wire: wire::Layout::default(),
        children: children.into_iter().collect(),
    }
}

/// Children in a row.
pub fn row<M>(children: impl IntoIterator<Item = Node<M>>) -> Layout<M> {
    column(children).axis(Axis::Row)
}

pub fn text(content: impl Into<String>) -> wire::Text {
    wire::Text {
        content: content.into(),
        ..wire::Text::default()
    }
}

pub fn button<M>(label: impl Into<String>) -> Button<M> {
    Button {
        wire: wire::Button {
            label: label.into(),
            ..wire::Button::default()
        },
        on_click: None,
    }
}

/// A text field. `key` names it across draws: the text Cydonia holds for it
/// is the text of the field with this key.
pub fn input<M>(key: impl Into<String>) -> Input<M> {
    Input {
        wire: wire::Input {
            key: key.into(),
            ..wire::Input::default()
        },
        on_input: None,
    }
}

/// A choice among `options`, each a value and its label.
pub fn select<M>(
    key: impl Into<String>,
    options: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
) -> Select<M> {
    Select {
        wire: wire::Select {
            key: key.into(),
            options: options
                .into_iter()
                .map(|(value, label)| Choice {
                    value: value.into(),
                    label: label.into(),
                })
                .collect(),
            ..wire::Select::default()
        },
        on_select: None,
    }
}

pub fn link(label: impl Into<String>, url: impl Into<String>) -> wire::Link {
    wire::Link {
        label: label.into(),
        url: url.into(),
        ..wire::Link::default()
    }
}

/// An image Cydonia fetches from `url`, drawn `width` by `height`.
pub fn image(url: impl Into<String>, width: f32, height: f32) -> wire::Image {
    wire::Image {
        key: None,
        source: Source::Url(url.into()),
        width,
        height,
        round: false,
    }
}

/// An image from encoded bytes of the MIME type `mime`.
pub fn image_bytes(mime: impl Into<String>, data: Vec<u8>, width: f32, height: f32) -> wire::Image {
    wire::Image {
        key: None,
        source: Source::Bytes {
            mime: mime.into(),
            data,
        },
        width,
        height,
        round: false,
    }
}

/// A virtualized list: Cydonia calls [`crate::App::rows`] with `key` for the
/// rows on screen.
pub fn list(key: impl Into<String>, count: u32, row_height: f32) -> wire::List {
    wire::List {
        key: key.into(),
        count,
        row_height,
    }
}

impl<M> Layout<M> {
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.wire.key = Some(key.into());
        self
    }

    pub fn axis(mut self, axis: Axis) -> Self {
        self.wire.axis = axis;
        self
    }

    pub fn gap(mut self, gap: Space) -> Self {
        self.wire.gap = gap;
        self
    }

    pub fn padding(mut self, padding: Space) -> Self {
        self.wire.padding = padding;
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.wire.align = align;
        self
    }

    pub fn justify(mut self, justify: Justify) -> Self {
        self.wire.justify = justify;
        self
    }

    pub fn grow(mut self) -> Self {
        self.wire.grow = true;
        self
    }

    pub fn scroll(mut self) -> Self {
        self.wire.scroll = true;
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.wire.surface = Some(surface);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.wire.width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.wire.height = Some(height);
        self
    }

    pub fn child(mut self, child: impl Into<Node<M>>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Node<M>>) -> Self {
        self.children.extend(children);
        self
    }
}

impl<M> Button<M> {
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.wire.key = Some(key.into());
        self
    }

    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.wire.style = style;
        self
    }

    pub fn on_click(mut self, msg: M) -> Self
    where
        M: Clone + 'static,
    {
        self.on_click = Some(Rc::new(move || msg.clone()));
        self
    }
}

impl<M> Input<M> {
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.wire.placeholder = placeholder.into();
        self
    }

    pub fn on_input(mut self, msg: impl Fn(String) -> M + 'static) -> Self {
        self.on_input = Some(Rc::new(msg));
        self
    }
}

impl<M> Select<M> {
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.wire.placeholder = placeholder.into();
        self
    }

    pub fn selected(mut self, value: Option<impl Into<String>>) -> Self {
        self.wire.selected = value.map(Into::into);
        self
    }

    pub fn on_select(mut self, msg: impl Fn(String) -> M + 'static) -> Self {
        self.on_select = Some(Rc::new(msg));
        self
    }
}

impl wire::Text {
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn lines(mut self, lines: u32) -> Self {
        self.lines = Some(lines);
        self
    }
}

impl wire::Link {
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }
}

impl wire::Image {
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn round(mut self) -> Self {
        self.round = true;
        self
    }
}

macro_rules! plain {
    ($($kind:ident),*) => {$(
        impl<M> From<wire::$kind> for Node<M> {
            fn from(node: wire::$kind) -> Self {
                Node::Plain(wire::Node::$kind(node))
            }
        }
    )*};
}

plain!(Text, Link, Image, List);

impl<M> From<Layout<M>> for Node<M> {
    fn from(node: Layout<M>) -> Self {
        Node::Layout(node)
    }
}

impl<M> From<Button<M>> for Node<M> {
    fn from(node: Button<M>) -> Self {
        Node::Button(node)
    }
}

impl<M> From<Input<M>> for Node<M> {
    fn from(node: Input<M>) -> Self {
        Node::Input(node)
    }
}

impl<M> From<Select<M>> for Node<M> {
    fn from(node: Select<M>) -> Self {
        Node::Select(node)
    }
}

pub(crate) enum Handle<M> {
    Click(Rc<dyn Fn() -> M>),
    Text(Rc<dyn Fn(String) -> M>),
}

/// The handlers of the last draw, by the number the wire carries. A `view`
/// replaces all of them; a list's `rows` replaces that list's.
pub(crate) struct Handlers<M> {
    next: wire::Handler,
    live: HashMap<wire::Handler, Handle<M>>,
    lists: HashMap<String, Vec<wire::Handler>>,
    fresh: Vec<wire::Handler>,
}

impl<M> Default for Handlers<M> {
    fn default() -> Self {
        Self {
            next: 0,
            live: HashMap::new(),
            lists: HashMap::new(),
            fresh: Vec::new(),
        }
    }
}

impl<M> Handlers<M> {
    pub(crate) fn view(&mut self, node: Node<M>) -> wire::Node {
        self.live.clear();
        self.lists.clear();
        let node = self.lower(node);
        self.fresh.clear();
        node
    }

    pub(crate) fn rows(&mut self, list: &str, rows: Vec<Node<M>>) -> Vec<wire::Node> {
        for handler in self.lists.remove(list).unwrap_or_default() {
            self.live.remove(&handler);
        }
        let rows = rows.into_iter().map(|row| self.lower(row)).collect();
        self.lists
            .insert(list.to_owned(), std::mem::take(&mut self.fresh));
        rows
    }

    pub(crate) fn get(&self, handler: wire::Handler) -> Option<&Handle<M>> {
        self.live.get(&handler)
    }

    fn register(&mut self, handle: Handle<M>) -> wire::Handler {
        let handler = self.next;
        self.next = self.next.wrapping_add(1);
        self.live.insert(handler, handle);
        self.fresh.push(handler);
        handler
    }

    fn lower(&mut self, node: Node<M>) -> wire::Node {
        match node {
            Node::Layout(Layout { mut wire, children }) => {
                wire.children = children
                    .into_iter()
                    .map(|child| self.lower(child))
                    .collect();
                wire::Node::Layout(wire)
            }
            Node::Button(Button { mut wire, on_click }) => {
                wire.on_click = on_click.map(|msg| self.register(Handle::Click(msg)));
                wire::Node::Button(wire)
            }
            Node::Input(Input { mut wire, on_input }) => {
                wire.on_input = on_input.map(|msg| self.register(Handle::Text(msg)));
                wire::Node::Input(wire)
            }
            Node::Select(Select {
                mut wire,
                on_select,
            }) => {
                wire.on_select = on_select.map(|msg| self.register(Handle::Text(msg)));
                wire::Node::Select(wire)
            }
            Node::Plain(node) => node,
        }
    }
}
