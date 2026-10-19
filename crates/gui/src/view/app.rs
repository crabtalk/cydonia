//! An app's view: one running instance of a mini-app and the nodes it draws,
//! painted with bezel's widgets in the theme's tokens.
//!
//! Cydonia holds what the app's nodes carry between draws: an input's text,
//! caret, selection and composition, a select's open menu, scroll, focus and
//! hover. The app hears a committed change as an event.

use crate::{
    model::{
        apps::{
            Answer, Build, Call, CallId, Command, Event, Fetcher as _, Http, Manifest, Node,
            Origin, Program, Runner as _, Runtime,
        },
        workspace::Workspace,
    },
    view::{
        kit::link::Links,
        window::root::{Cydonia, LoadApp},
    },
};
use artifact::{app::Loaded, kind::Kind, space::Member};
use bezel::{
    gpui::{
        self, AnyElement, Context, ElementId, Entity, EntityInputHandler as _, ImageSource,
        IntoElement, ObjectFit, PathPromptOptions, PromptLevel, SharedString, Subscription, Task,
        UniformListScrollHandle, Window, div, img, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        combobox::{Combobox, ComboboxEvent},
        input::{FieldEvent, Shape, TextField},
        list::virtual_list,
        widgets::{ButtonStyle, Buttons},
    },
};
use guest::{
    manifest::calls,
    wire::{self, Align, Axis, Choice, Handler, Justify, Source, Space, Surface, Tone},
};
use markdown::Links as _;
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    rc::Rc,
    sync::Arc,
};

/// The corner of a layout drawn on a surface.
const RADIUS: f32 = 8.;

impl Cydonia {
    /// Open the app `id`: the space holding it, or the pane in front.
    pub(crate) fn open_app(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        if self.enter_member(Some(Member::app(id)), window, cx) {
            return;
        }
        self.workspace
            .update(cx, |workspace, cx| workspace.open_app(id, cx));
        self.leaf().update(cx, |pane, _| pane.kind = Kind::App);
        cx.notify();
    }

    /// List the app just recorded as `id`, and open it.
    pub(crate) fn open_recorded(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.app_recorded(cx));
        self.open_app(id, window, cx);
    }

    /// Load the crate in a folder the person picks, and open it.
    pub(crate) fn load_app_action(
        &mut self,
        _: &LoadApp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if cfg!(not(feature = "desktop")) {
            self.desktop_only("Loading an app", window, cx);
            return;
        }
        if !self.workspace.read(cx).settings.developer_mode {
            return;
        }
        self.commit(cx);
        self.close_menu(cx);
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Load".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(dir) = paths.into_iter().next() else {
                return;
            };
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    let app = Loaded::read(&dir)?;
                    app.save()?;
                    anyhow::Ok(app)
                })
                .await;
            let refused = this.update_in(cx, |this, window, cx| match loaded {
                Ok(app) => {
                    this.open_recorded(&app.id, window, cx);
                    None
                }
                Err(error) => Some(window.prompt(
                    PromptLevel::Critical,
                    "The app could not be loaded",
                    Some(&format!("{error:#}")),
                    &["OK"],
                    cx,
                )),
            });
            if let Ok(Some(answer)) = refused {
                let _ = answer.await;
            }
        })
        .detach();
    }
}

pub struct AppView {
    /// The app's id.
    pub(crate) app: String,
    /// Its program. `None` once the app is gone, or not listed.
    program: Option<Entity<Program>>,
    /// The build the instance was started from.
    running: Option<Arc<[u8]>>,
    /// How many instances this view has started. An answer goes only to the
    /// instance whose call it answers.
    started: u64,
    state: State,
    lists: HashMap<String, Listed>,
    inputs: HashMap<String, Field>,
    /// Text the app set for an input not drawn yet.
    pending: HashMap<String, String>,
    selects: HashMap<String, Picker>,
    /// The inputs and selects drawn since the last render.
    drawn: HashSet<String>,
    _program: Option<Subscription>,
}

enum State {
    /// No build to run yet.
    Idle,
    Starting {
        _task: Task<()>,
    },
    Running {
        runtime: Box<Runtime>,
        tree: Rc<Node>,
    },
    Failed(SharedString),
}

/// A virtualized list's scroll, and the rows on screen as the app last drew
/// them.
#[derive(Default)]
struct Listed {
    scroll: UniformListScrollHandle,
    rows: Option<(Range<usize>, Rc<Vec<Node>>)>,
}

struct Field {
    field: Entity<TextField>,
    handler: Option<Handler>,
    placeholder: String,
    /// Text Cydonia put in the field for the app, whose change the app does
    /// not hear.
    echo: Option<String>,
    _events: Subscription,
}

struct Picker {
    combobox: Entity<Combobox>,
    options: Vec<Choice>,
    handler: Option<Handler>,
    _events: Subscription,
}

impl AppView {
    /// A view of `app`, which asks for a build and runs each new one.
    pub fn new(app: String, workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        let program = workspace.read(cx).program(&app, cx).cloned();
        let observing = program
            .as_ref()
            .map(|program| cx.observe(program, |this, _, cx| this.sync(cx)));
        if let Some(program) = &program {
            program.update(cx, |program, cx| program.want(cx));
        }
        let mut this = Self {
            app,
            program,
            running: None,
            started: 0,
            state: State::Idle,
            lists: HashMap::new(),
            inputs: HashMap::new(),
            pending: HashMap::new(),
            selects: HashMap::new(),
            drawn: HashSet::new(),
            _program: observing,
        };
        this.sync(cx);
        this
    }

    /// Start the program's latest build, unless it is the one running.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let Some(program) = &self.program else {
            return;
        };
        if !program.read(cx).origin.allowed() {
            cx.notify();
            return;
        }
        let Some(wasm) = program.read(cx).wasm.clone() else {
            cx.notify();
            return;
        };
        if self
            .running
            .as_ref()
            .is_some_and(|running| Arc::ptr_eq(running, &wasm))
        {
            cx.notify();
            return;
        }
        self.running = Some(wasm.clone());
        self.started += 1;
        self.lists.clear();
        self.inputs.clear();
        self.selects.clear();
        self.pending.clear();
        let starting = cx.spawn(async move |this, cx| {
            let started = cx
                .background_executor()
                .spawn(async move { Runtime::start(&wasm) })
                .await;
            let _ = this.update(cx, |this, cx| match started {
                Ok((runtime, commands)) => {
                    this.state = State::Running {
                        runtime: Box::new(runtime),
                        tree: Rc::new(Node::Layout(wire::Layout::default())),
                    };
                    this.apply(commands, cx);
                    this.refresh(cx);
                }
                Err(error) => this.fail(error, cx),
            });
        });
        self.state = State::Starting { _task: starting };
        cx.notify();
    }

    /// Deliver `event`, and draw what the app draws after it.
    pub fn dispatch(&mut self, event: Event, cx: &mut Context<Self>) {
        let State::Running { runtime, .. } = &mut self.state else {
            return;
        };
        match runtime.update(&event) {
            Ok(commands) => {
                self.apply(commands, cx);
                self.refresh(cx);
            }
            Err(error) => self.fail(error, cx),
        }
    }

    /// Ask the app what it draws now. The rows of its lists are asked again
    /// as they are drawn.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let State::Running { runtime, tree } = &mut self.state else {
            return;
        };
        match runtime.view() {
            Ok(node) => {
                *tree = Rc::new(node);
                for listed in self.lists.values_mut() {
                    listed.rows = None;
                }
                cx.notify();
            }
            Err(error) => self.fail(error, cx),
        }
    }

    fn apply(&mut self, commands: Vec<Command>, cx: &mut Context<Self>) {
        for command in commands {
            match command {
                Command::SetInput { key, value } => match self.inputs.get_mut(&key) {
                    Some(held) => {
                        held.echo = Some(value.clone());
                        held.field
                            .update(cx, |field, cx| field.set_content(value, cx));
                    }
                    None => {
                        self.pending.insert(key, value);
                    }
                },
                Command::Call { id, call } => self.call(id, call, cx),
            }
        }
    }

    /// Do one of the app's calls, and deliver its answer to the instance that
    /// made it once it is in.
    fn call(&mut self, id: CallId, call: Call, cx: &mut Context<Self>) {
        let manifest = self
            .program
            .as_ref()
            .and_then(|program| program.read(cx).manifest.clone());
        let started = self.started;
        let answered: Task<Answer> = match call {
            Call::Fetch(request) => {
                let reaches = manifest.is_some_and(|manifest| manifest.reaches(&request.url));
                match reaches {
                    true => cx.background_executor().spawn(async move {
                        Http::fetch(&request).map_or_else(
                            |error| Answer::Error(format!("{error:#}")),
                            Answer::Response,
                        )
                    }),
                    false => Task::ready(Answer::Error(format!(
                        "{} is not on a host the app's manifest names",
                        request.url
                    ))),
                }
            }
            call => {
                let declared = manifest.is_some_and(|manifest| manifest.calls(calls::TABLES));
                match (declared, &self.program) {
                    (true, Some(program)) => {
                        let answer = program.update(cx, |program, _| program.table_call(call));
                        cx.spawn(async move |_, _| answer.await)
                    }
                    (true, None) => Task::ready(Answer::Error("The app is gone".into())),
                    (false, _) => Task::ready(Answer::Error(format!(
                        "The app's manifest does not declare `{}` among its calls",
                        calls::TABLES
                    ))),
                }
            }
        };
        cx.spawn(async move |this, cx| {
            let answer = answered.await;
            let _ = this.update(cx, |this, cx| {
                if this.started == started {
                    this.dispatch(Event::Answer { call: id, answer }, cx)
                }
            });
        })
        .detach();
    }

    fn fail(&mut self, error: anyhow::Error, cx: &mut Context<Self>) {
        self.state = State::Failed(format!("{error:#}").into());
        self.lists.clear();
        cx.notify();
    }

    /// The question an installed app asks before it first runs a digest:
    /// what its manifest says it reaches.
    fn ask(&self, program: &Entity<Program>, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let held = program.read(cx);
        let Origin::Installed(app) = &held.origin else {
            return div().into_any_element();
        };
        let reaches = held.manifest.reach();
        let line = |text: String, tone: gpui::Hsla| {
            div()
                .text_style(TextStyle::Callout)
                .text_color(tone)
                .child(text)
                .into_any_element()
        };
        let program = program.clone();
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(Space::Medium.value(theme)))
            .p(px(Space::Large.value(theme)))
            .child(
                div()
                    .text_style(TextStyle::Headline)
                    .text_color(theme.text)
                    .child(format!("Run {}?", held.label())),
            )
            .children(reaches.into_iter().map(|text| line(text, theme.text)))
            .child(line(
                format!("{}:{} at {}", app.reference, app.tag, app.digest),
                theme.text_muted,
            ))
            .child(
                div().flex().child(
                    theme
                        .button("Run", ButtonStyle::Prominent, None)
                        .id("app-allow")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Err(error) = program.update(cx, |program, cx| program.allow(cx))
                            {
                                this.fail(error, cx);
                            }
                        })),
                ),
            )
            .into_any_element()
    }

    /// What stands in the app's place while it is not running: a headline,
    /// what the app reaches, and what cargo or the app said.
    fn notice(
        headline: SharedString,
        reach: Vec<String>,
        detail: SharedString,
        theme: &Theme,
    ) -> AnyElement {
        div()
            .id("app-notice")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(Space::Small.value(theme)))
            .p(px(Space::Large.value(theme)))
            .child(
                div()
                    .text_style(TextStyle::Headline)
                    .text_color(theme.text)
                    .child(headline),
            )
            .children(reach.into_iter().map(|line| {
                div()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text)
                    .child(line)
            }))
            .child(
                div()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text_muted)
                    .font_family(theme.font_mono.clone())
                    .child(detail),
            )
            .into_any_element()
    }

    /// A node's identity among its siblings: its key, or its place.
    fn id(key: Option<&str>, ix: usize) -> ElementId {
        match key {
            Some(key) => ElementId::Name(SharedString::from(key.to_owned())),
            None => ElementId::NamedInteger("node".into(), ix as u64),
        }
    }

    fn draw(
        &mut self,
        node: &Node,
        ix: usize,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match node {
            Node::Layout(layout) => self.layout(layout, ix, false, theme, window, cx),
            Node::Text(text) => div()
                .text_style(text.style.value(theme))
                .text_color(text.tone.value(theme))
                .when_some(text.lines, |el, lines| el.line_clamp(lines as usize))
                .child(text.content.clone())
                .into_any_element(),
            Node::Button(button) => {
                let style = match button.style {
                    wire::ButtonStyle::Ghost => ButtonStyle::Ghost,
                    wire::ButtonStyle::Prominent => ButtonStyle::Prominent,
                    wire::ButtonStyle::Destructive => ButtonStyle::Destructive,
                };
                let drawn = theme
                    .button(button.label.clone(), style, None)
                    .id(Self::id(button.key.as_deref(), ix));
                match button.on_click {
                    Some(handler) => drawn
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.dispatch(Event::Click { handler }, cx)
                        }))
                        .into_any_element(),
                    None => drawn.opacity(0.5).into_any_element(),
                }
            }
            Node::Input(input) => self.input(input, window, cx),
            Node::Select(select) => self.select(select, window, cx),
            Node::Link(link) => {
                let url = link.url.clone();
                div()
                    .id(Self::id(link.key.as_deref(), ix))
                    .cursor_pointer()
                    .text_style(link.style.value(theme))
                    .text_color(theme.accent)
                    .hover(|el| el.underline())
                    .child(link.label.clone())
                    .on_click(move |_, window, cx| {
                        Links::open(markdown::Link::new(&url, None), window, cx)
                    })
                    .into_any_element()
            }
            Node::Image(image) => {
                let frame = div().flex_none().w(px(image.width)).h(px(image.height));
                let manifest = self
                    .program
                    .as_ref()
                    .and_then(|program| program.read(cx).manifest.as_ref());
                let Some(source) = image.source.paint(manifest) else {
                    return frame.into_any_element();
                };
                frame
                    .child(
                        img(source)
                            .size_full()
                            .object_fit(ObjectFit::Cover)
                            .when(image.round, |el| el.rounded_full()),
                    )
                    .into_any_element()
            }
            Node::List(list) => self.list(list, theme, cx),
            Node::Unknown(unknown) => div()
                .flex_none()
                .w(px(unknown.width.unwrap_or_default()))
                .h(px(unknown.height.unwrap_or_default()))
                .into_any_element(),
        }
    }

    /// A layout and its children. The root fills the view.
    fn layout(
        &mut self,
        layout: &wire::Layout,
        ix: usize,
        root: bool,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let children: Vec<AnyElement> = layout
            .children
            .iter()
            .enumerate()
            .map(|(ix, child)| self.draw(child, ix, theme, window, cx))
            .collect();
        let el = div()
            .flex()
            .min_w_0()
            .min_h_0()
            .map(|el| match layout.axis {
                Axis::Column => el.flex_col(),
                Axis::Row => el.flex_row(),
            })
            .gap(px(layout.gap.value(theme)))
            .p(px(layout.padding.value(theme)))
            .map(|el| match layout.align {
                Align::Stretch => el,
                Align::Start => el.items_start(),
                Align::Center => el.items_center(),
                Align::End => el.items_end(),
            })
            .map(|el| match layout.justify {
                Justify::Start => el.justify_start(),
                Justify::Center => el.justify_center(),
                Justify::End => el.justify_end(),
                Justify::Between => el.justify_between(),
            })
            .when(layout.grow || root, |el| el.flex_1())
            .when_some(layout.width, |el, width| el.flex_none().w(px(width)))
            .when_some(layout.height, |el, height| el.h(px(height)))
            .when_some(layout.surface, |el, surface| {
                el.bg(match surface {
                    Surface::Card => theme.surface_card,
                    Surface::Raised => theme.surface_raised,
                    Surface::Sunken => theme.surface,
                })
                .border_1()
                .border_color(theme.border)
                .rounded(px(RADIUS))
                .overflow_hidden()
            })
            .children(children);
        match layout.scroll {
            false => el.into_any_element(),
            true => el
                .id(Self::id(layout.key.as_deref(), ix))
                .map(|el| match layout.axis {
                    Axis::Column => el.overflow_y_scroll(),
                    Axis::Row => el.overflow_x_scroll(),
                })
                .into_any_element(),
        }
    }

    fn input(
        &mut self,
        input: &wire::Input,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.drawn.insert(input.key.clone());
        if !self.inputs.contains_key(&input.key) {
            let field = cx.new(|cx| TextField::new(cx).with_shape(Shape::Line));
            let echo = self.pending.remove(&input.key);
            if let Some(text) = echo.clone() {
                field.update(cx, |field, cx| field.set_content(text, cx));
            }
            let key = input.key.clone();
            let events = cx.subscribe_in(
                &field,
                window,
                move |this, field, event: &FieldEvent, window, cx| {
                    let FieldEvent::Changed(_) = event else {
                        return;
                    };
                    let composing = field.update(cx, |field, cx| {
                        field.marked_text_range(window, cx).is_some()
                    });
                    if composing {
                        return;
                    }
                    let text = field.read(cx).content().to_string();
                    let Some(held) = this.inputs.get_mut(&key) else {
                        return;
                    };
                    if held.echo.take().is_some_and(|echo| echo == text) {
                        return;
                    }
                    if let Some(handler) = held.handler {
                        this.dispatch(Event::Input { handler, text }, cx);
                    }
                },
            );
            self.inputs.insert(
                input.key.clone(),
                Field {
                    field,
                    handler: None,
                    placeholder: String::new(),
                    echo,
                    _events: events,
                },
            );
        }
        let held = self
            .inputs
            .get_mut(&input.key)
            .expect("the input was just held");
        held.handler = input.on_input;
        if held.placeholder != input.placeholder {
            held.placeholder = input.placeholder.clone();
            let placeholder = SharedString::from(input.placeholder.clone());
            held.field
                .update(cx, |field, cx| field.set_placeholder(placeholder, cx));
        }
        div().w_full().child(held.field.clone()).into_any_element()
    }

    fn select(
        &mut self,
        select: &wire::Select,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.drawn.insert(select.key.clone());
        let chosen = select
            .selected
            .as_ref()
            .and_then(|value| select.options.iter().position(|o| &o.value == value));
        let current = self.selects.get(&select.key).is_some_and(|held| {
            held.options == select.options && held.combobox.read(cx).selection() == chosen
        });
        if !current {
            let labels = select
                .options
                .iter()
                .map(|option| SharedString::from(option.label.clone()))
                .collect();
            let placeholder = select.placeholder.clone();
            let combobox = cx.new(|cx| {
                let combobox = Combobox::new(labels, placeholder, cx);
                match chosen {
                    Some(ix) => combobox.with_selection(ix),
                    None => combobox,
                }
            });
            let key = select.key.clone();
            let events = cx.subscribe_in(
                &combobox,
                window,
                move |this, _, event: &ComboboxEvent, _, cx| {
                    let ComboboxEvent::Selected(ix) = event;
                    let Some(held) = this.selects.get(&key) else {
                        return;
                    };
                    let (Some(handler), Some(option)) = (held.handler, held.options.get(*ix))
                    else {
                        return;
                    };
                    let value = option.value.clone();
                    this.dispatch(Event::Select { handler, value }, cx);
                },
            );
            self.selects.insert(
                select.key.clone(),
                Picker {
                    combobox,
                    options: select.options.clone(),
                    handler: None,
                    _events: events,
                },
            );
        }
        let held = self
            .selects
            .get_mut(&select.key)
            .expect("the select was just held");
        held.handler = select.on_select;
        held.combobox.clone().into_any_element()
    }

    fn list(&mut self, list: &wire::List, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let scroll = self
            .lists
            .entry(list.key.clone())
            .or_default()
            .scroll
            .clone();
        let key = list.key.clone();
        let theme = theme.clone();
        div()
            .flex_1()
            .min_h_0()
            .size_full()
            .child(virtual_list(
                SharedString::from(format!("list-{}", list.key)),
                list.count as usize,
                px(list.row_height),
                &scroll,
                cx.processor(move |this, range: Range<usize>, window, cx| {
                    this.rows(&key, range, &theme, window, cx)
                }),
            ))
            .into_any_element()
    }

    /// The rows `range` of the list `key`, asked of the app when they are not
    /// the ones it drew last.
    fn rows(
        &mut self,
        key: &str,
        range: Range<usize>,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let held = self
            .lists
            .get(key)
            .and_then(|listed| listed.rows.as_ref())
            .filter(|(drawn, _)| *drawn == range)
            .map(|(_, rows)| rows.clone());
        let rows = match held {
            Some(rows) => rows,
            None => {
                let State::Running { runtime, .. } = &mut self.state else {
                    return Vec::new();
                };
                match runtime.rows(key, range.start as u32..range.end as u32) {
                    Ok(rows) => {
                        let rows = Rc::new(rows);
                        self.lists.entry(key.to_owned()).or_default().rows =
                            Some((range.clone(), rows.clone()));
                        rows
                    }
                    Err(error) => {
                        self.fail(error, cx);
                        return Vec::new();
                    }
                }
            }
        };
        rows.iter()
            .enumerate()
            .map(|(ix, row)| {
                div()
                    .id(("row", range.start + ix))
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(self.draw(row, ix, theme, window, cx))
                    .into_any_element()
            })
            .collect()
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let drawn = std::mem::take(&mut self.drawn);
        self.inputs.retain(|key, _| drawn.contains(key));
        self.selects.retain(|key, _| drawn.contains(key));
        let theme = Theme::of(cx).clone();
        let build = self
            .program
            .as_ref()
            .map(|program| program.read(cx).build.clone());
        let notice = match (&build, &self.state) {
            (None, _) => Some((
                "The app is gone".into(),
                Vec::new(),
                SharedString::default(),
            )),
            (Some(Build::Failed(error)), _) => Some((
                "The app did not build".into(),
                Vec::new(),
                error.clone().into(),
            )),
            (Some(Build::Running), State::Idle) => {
                let reach = self
                    .program
                    .as_ref()
                    .map(|program| program.read(cx).manifest.reach())
                    .unwrap_or_default();
                Some(("Building…".into(), reach, SharedString::default()))
            }
            (_, State::Failed(error)) => {
                Some(("The app stopped".into(), Vec::new(), error.clone()))
            }
            _ => None,
        };
        let asking = self
            .program
            .as_ref()
            .filter(|program| !program.read(cx).origin.allowed())
            .cloned();
        let body = match (notice, &self.state) {
            (Some((headline, reach, detail)), _) => Self::notice(headline, reach, detail, &theme),
            (None, _) if let Some(program) = asking => self.ask(&program, &theme, cx),
            (None, State::Running { tree, .. }) => {
                let tree = tree.clone();
                match &*tree {
                    Node::Layout(layout) => self.layout(layout, 0, true, &theme, window, cx),
                    node => self.draw(node, 0, &theme, window, cx),
                }
            }
            (None, _) => div().into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg)
            .text_color(theme.text)
            .child(body)
    }
}

/// What an app's manifest says it reaches, a sentence each.
trait Reach {
    fn reach(&self) -> Vec<String>;
}

impl Reach for Option<Manifest> {
    fn reach(&self) -> Vec<String> {
        let mut reach = Vec::new();
        if let Some(manifest) = self {
            if manifest.calls(calls::TABLES) {
                reach.push("It keeps tables of its own.".to_owned());
            }
            if !manifest.hosts.is_empty() {
                reach.push(format!("It fetches from {}.", manifest.hosts.join(", ")));
            }
        }
        if reach.is_empty() {
            reach.push("It reaches nothing outside its own view.".to_owned());
        }
        reach
    }
}

/// A wire token as the theme paints it.
trait Token {
    type Value;

    fn value(self, theme: &Theme) -> Self::Value;
}

impl Token for Space {
    type Value = f32;

    fn value(self, _: &Theme) -> f32 {
        match self {
            Space::None => 0.,
            Space::Small => 4.,
            Space::Medium => 8.,
            Space::Large => 16.,
        }
    }
}

impl Token for wire::TextStyle {
    type Value = TextStyle;

    fn value(self, _: &Theme) -> TextStyle {
        match self {
            wire::TextStyle::Title => TextStyle::Title,
            wire::TextStyle::Headline => TextStyle::Headline,
            wire::TextStyle::Body => TextStyle::Body,
            wire::TextStyle::Callout => TextStyle::Callout,
            wire::TextStyle::Caption => TextStyle::Caption,
        }
    }
}

impl Token for Tone {
    type Value = gpui::Hsla;

    fn value(self, theme: &Theme) -> gpui::Hsla {
        match self {
            Tone::Default => theme.text,
            Tone::Muted => theme.text_muted,
            Tone::Faint => theme.text_faint,
            Tone::Accent => theme.accent,
            Tone::Danger => theme.danger,
        }
    }
}

trait Paint {
    /// What an image node paints: a URL on a host `manifest` names, fetched,
    /// or bytes of a format gpui decodes. Anything else paints nothing.
    fn paint(&self, manifest: Option<&Manifest>) -> Option<ImageSource>;
}

impl Paint for Source {
    fn paint(&self, manifest: Option<&Manifest>) -> Option<ImageSource> {
        match self {
            Source::Url(url) => manifest
                .is_some_and(|manifest| manifest.reaches(url))
                .then(|| SharedString::from(url.clone()).into()),
            Source::Bytes { mime, data } => {
                let format = gpui::ImageFormat::from_mime_type(mime)?;
                Some(Arc::new(gpui::Image::from_bytes(format, data.clone())).into())
            }
        }
    }
}
