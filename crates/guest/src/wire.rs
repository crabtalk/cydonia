//! What crosses between an app and Cydonia, as JSON: the node tree an app
//! draws and the events Cydonia sends back. Cydonia reads these types; an app
//! builds them through [`crate::node`].
//!
//! Fields left at their default are not written, and a missing field reads
//! as its default.

use serde::{Deserialize, Serialize};

/// A handler an app registered while drawing, by number. Valid until the next
/// `view`, or for a list's rows until the next `rows` of that list.
pub type Handler = u32;

/// One node of the tree an app draws.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "node", rename_all = "snake_case")]
pub enum Node {
    Layout(Layout),
    Text(Text),
    Button(Button),
    Input(Input),
    Select(Select),
    Link(Link),
    Image(Image),
    List(List),
}

/// Children in a row or a column.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub axis: Axis,
    #[serde(default, skip_serializing_if = "is_default")]
    pub gap: Space,
    #[serde(default, skip_serializing_if = "is_default")]
    pub padding: Space,
    #[serde(default, skip_serializing_if = "is_default")]
    pub align: Align,
    #[serde(default, skip_serializing_if = "is_default")]
    pub justify: Justify,
    /// Takes the room its parent has left along the parent's axis.
    #[serde(default, skip_serializing_if = "is_default")]
    pub grow: bool,
    /// Scrolls along its axis when its children overflow it.
    #[serde(default, skip_serializing_if = "is_default")]
    pub scroll: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<Surface>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    #[default]
    Column,
    Row,
}

/// A spacing step of the theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Space {
    #[default]
    None,
    Small,
    Medium,
    Large,
}

/// Where children sit across the axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Stretch,
    Start,
    Center,
    End,
}

/// Where children sit along the axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    Between,
}

/// A theme surface a layout is painted with, framed by the theme's border.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Card,
    Raised,
    Sunken,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Text {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub content: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub style: TextStyle,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tone: Tone,
    /// At most this many lines, the last cut with an ellipsis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<u32>,
}

/// A text role of the theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextStyle {
    Title,
    Headline,
    #[default]
    Body,
    Callout,
    Caption,
}

/// A text colour of the theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    #[default]
    Default,
    Muted,
    Faint,
    Accent,
    Danger,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Button {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub label: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub style: ButtonStyle,
    /// Unset, the button is drawn disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_click: Option<Handler>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonStyle {
    #[default]
    Ghost,
    Prominent,
    Destructive,
}

/// A one-line text field. Cydonia holds its text; the app hears each committed
/// change through `on_input` and sets the text with
/// [`crate::host::set_input`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Input {
    pub key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub placeholder: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_input: Option<Handler>,
}

/// One of a list of choices, searchable as it opens.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Select {
    pub key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub placeholder: String,
    pub options: Vec<Choice>,
    /// The value of the option shown as chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_select: Option<Handler>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

/// A URL, opened the way Cydonia opens a link.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Link {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub label: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub style: TextStyle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Image {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub source: Source,
    pub width: f32,
    pub height: f32,
    /// Clipped to a circle.
    #[serde(default, skip_serializing_if = "is_default")]
    pub round: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Fetched by Cydonia.
    Url(String),
    /// Encoded image bytes and their MIME type.
    Bytes {
        mime: String,
        #[serde(with = "base64")]
        data: Vec<u8>,
    },
}

/// A virtualized list of `count` rows, each `row_height` tall. Cydonia asks the
/// app for the rows on screen by the list's key.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct List {
    pub key: String,
    pub count: u32,
    pub row_height: f32,
}

/// What Cydonia tells an app, delivered to the handler it names.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Click { handler: Handler },
    Input { handler: Handler, text: String },
    Select { handler: Handler, value: String },
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

mod base64 {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD.decode(text).map_err(D::Error::custom)
    }
}
