//! The statistics page: the active project's activity over 7, 30 or 90 days.
//!
//! Held by the window rather than by a pane, drawn over the whole detail
//! column — see [`crate::view::root::Cydonia::toggle_statistics`].

use crate::model::{
    prices::{self, Prices},
    statistics::{RANGES, Summary},
    workspace::Workspace,
};
use artifact::stats::Tokens;
use bezel::{
    gpui::{
        self, AnyElement, App, Bounds, Context, Entity, FocusHandle, Focusable, Global, Hsla,
        PathBuilder, Pixels, SharedString, Task, Window, canvas, div, point, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        tooltip::Tooltip,
        widgets::{Controls, Scaffolding},
    },
};
use std::{collections::BTreeMap, path::PathBuf, rc::Rc};

/// The price list, shared by every window and fetched at most once a run.
#[derive(Default)]
enum PriceTable {
    #[default]
    Idle,
    Loading,
    Ready(Rc<Prices>),
    Unavailable,
}

impl Global for PriceTable {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Measure {
    Cost,
    Tokens,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Model,
    Agent,
    Day,
}

/// Esc on the page: the window takes it away.
pub struct Dismiss;

impl gpui::EventEmitter<Dismiss> for Statistics {}

pub struct Statistics {
    workspace: Entity<Workspace>,
    focus: FocusHandle,
    project: Option<PathBuf>,
    days: u32,
    summary: Option<Summary>,
    measure: Measure,
    group: Group,
    _load: Option<Task<()>>,
}

impl Focusable for Statistics {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Statistics {
    pub fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        cx.observe(&workspace, |this: &mut Self, _, cx| {
            if this.active_project(cx) != this.project {
                this.load(cx);
            }
        })
        .detach();
        let mut this = Self {
            workspace,
            focus: cx.focus_handle(),
            project: None,
            days: RANGES[1],
            summary: None,
            measure: Measure::Cost,
            group: Group::Model,
            _load: None,
        };
        this.load(cx);
        this.fetch_prices(cx);
        this
    }

    fn active_project(&self, cx: &App) -> Option<PathBuf> {
        self.workspace
            .read(cx)
            .active_project()
            .map(|open| open.path.clone())
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.project = self.active_project(cx);
        self.summary = None;
        cx.notify();
        let Some(project) = self.project.clone() else {
            return;
        };
        let days = self.days;
        self._load = Some(cx.spawn(async move |this, cx| {
            let today = chrono::Local::now().date_naive();
            let summary = cx
                .background_executor()
                .spawn(async move { Summary::compute(&project, days, today) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.summary = Some(summary.unwrap_or_default());
                cx.notify();
            });
        }));
    }

    fn fetch_prices(&mut self, cx: &mut Context<Self>) {
        if !matches!(cx.default_global::<PriceTable>(), PriceTable::Idle) {
            return;
        }
        cx.set_global(PriceTable::Loading);
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async { prices::load() })
                .await;
            cx.update(|cx| {
                cx.set_global(match loaded {
                    Ok(prices) => PriceTable::Ready(Rc::new(prices)),
                    Err(_) => PriceTable::Unavailable,
                })
            });
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    fn prices(&self, cx: &App) -> Option<Rc<Prices>> {
        match cx.try_global::<PriceTable>() {
            Some(PriceTable::Ready(prices)) => Some(prices.clone()),
            _ => None,
        }
    }
}

impl Render for Statistics {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let features = &self.workspace.read(cx).settings.features;
        let (sessions, boards) = (features.sessions, features.boards);
        let selected = RANGES
            .iter()
            .position(|days| *days == self.days)
            .unwrap_or(1);
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(theme.page_header("Statistics", None))
            .child(theme.segmented(
                "statistics-range",
                RANGES.map(|days| format!("{days} days")),
                selected,
                cx.listener(|this, ix: &usize, _, cx| {
                    this.days = RANGES[*ix];
                    this.load(cx);
                }),
            ));
        let mut column = theme.page_column().gap(px(24.)).child(header);
        match &self.summary {
            None => {
                column = column.child(
                    div()
                        .text_style(TextStyle::Body)
                        .text_color(theme.text_muted)
                        .child(match self.project {
                            Some(_) => "Reading…",
                            None => "Open a project to see its statistics.",
                        }),
                )
            }
            Some(summary) => {
                let prices = self.prices(cx);
                let unavailable =
                    matches!(cx.try_global::<PriceTable>(), Some(PriceTable::Unavailable));
                column = column
                    .child(heatmap(&theme, summary, sessions, boards))
                    .child(articles(&theme, summary));
                if boards {
                    column =
                        column.child(boards_section(&theme, summary, sessions, prices.as_deref()));
                }
                if sessions {
                    column = column.child(self.usage(
                        &theme,
                        summary,
                        prices.as_deref(),
                        unavailable,
                        cx,
                    ));
                }
            }
        }
        div()
            .id("statistics")
            .track_focus(&self.focus)
            .key_context("Statistics")
            .on_key_down(cx.listener(|_, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.emit(Dismiss);
                }
            }))
            .size_full()
            .overflow_y_scroll()
            .child(column)
    }
}

fn section(theme: &Theme, title: &str) -> gpui::Div {
    div().flex().flex_col().gap(px(10.)).child(
        div()
            .text_style(TextStyle::Headline)
            .text_color(theme.text)
            .child(title.to_owned()),
    )
}

fn tile(theme: &Theme, label: &str, value: String) -> gpui::Div {
    theme
        .group_box()
        .flex_1()
        .min_w(px(110.))
        .px(px(12.))
        .py(px(10.))
        .gap(px(2.))
        .child(
            div()
                .text_style(TextStyle::Subheadline)
                .text_color(theme.text_muted)
                .child(label.to_owned()),
        )
        .child(
            div()
                .text_style(TextStyle::Title3)
                .text_color(theme.text)
                .child(value),
        )
}

fn tiles(children: impl IntoIterator<Item = gpui::Div>) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(px(8.))
        .children(children)
}

/// The categorical colours series are drawn in, borrowed from the syntax
/// palette so they follow the theme.
fn series(theme: &Theme, ix: usize) -> Hsla {
    let palette = [
        theme.accent,
        theme.syntax.string,
        theme.syntax.function,
        theme.syntax.type_name,
        theme.syntax.number,
        theme.syntax.keyword,
    ];
    palette[ix % palette.len()]
}

const CHART_HEIGHT: f32 = 72.;

/// One bar per day, each a stack of `(value, colour)` from the bottom up.
fn bars(id: &'static str, days: Vec<(String, Vec<(u64, Hsla)>)>, theme: &Theme) -> AnyElement {
    let max = days
        .iter()
        .map(|(_, parts)| parts.iter().map(|(value, _)| value).sum::<u64>())
        .max()
        .unwrap_or(0)
        .max(1) as f32;
    div()
        .id(id)
        .h(px(CHART_HEIGHT))
        .flex()
        .flex_row()
        .items_end()
        .gap(px(2.))
        .border_b_1()
        .border_color(theme.border)
        .children(days.into_iter().enumerate().map(|(ix, (tip, parts))| {
            let tip = SharedString::from(tip);
            div()
                .id(ix)
                .flex_1()
                .h_full()
                .flex()
                .flex_col_reverse()
                .children(parts.into_iter().map(|(value, color)| {
                    div()
                        .flex_none()
                        .h(px(value as f32 / max * CHART_HEIGHT))
                        .bg(color)
                }))
                .tooltip(move |window, cx| Tooltip::text(tip.clone(), window, cx))
        }))
        .into_any_element()
}

fn heatmap(theme: &Theme, summary: &Summary, sessions: bool, boards: bool) -> gpui::Div {
    let total = |day: &crate::model::statistics::Day| {
        day.words_added
            + if boards {
                day.cards_created + day.cards_done
            } else {
                0
            }
            + if sessions { day.messages } else { 0 }
    };
    let max = summary.days.iter().map(total).max().unwrap_or(0).max(1);
    // Columns are weeks, Monday on top; the first column starts on the
    // weekday of the range's first day.
    let lead = summary
        .days
        .first()
        .and_then(|day| chrono::NaiveDate::parse_from_str(&day.date, "%Y-%m-%d").ok())
        .map_or(0, |date| {
            chrono::Datelike::weekday(&date).num_days_from_monday() as usize
        });
    let mut cells: Vec<Option<&crate::model::statistics::Day>> = vec![None; lead];
    cells.extend(summary.days.iter().map(Some));
    let weeks = cells.chunks(7).map(|week| {
        div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .children((0..7).map(|row| {
                let cell = div().size(px(11.)).rounded(px(2.));
                match week.get(row).copied().flatten() {
                    None => cell.into_any_element(),
                    Some(day) => {
                        let value = total(day);
                        let fill = match value {
                            0 => theme.ink(0.06),
                            _ => theme
                                .accent
                                .opacity(0.25 + 0.75 * value as f32 / max as f32),
                        };
                        let mut parts = vec![format!("{} words", day.words_added)];
                        if boards {
                            parts.push(format!("{} cards", day.cards_created + day.cards_done));
                        }
                        if sessions {
                            parts.push(format!("{} messages", day.messages));
                        }
                        let tip =
                            SharedString::from(format!("{} · {}", day.date, parts.join(" · ")));
                        cell.id(SharedString::from(format!("heat-{}", day.date)))
                            .bg(fill)
                            .tooltip(move |window, cx| Tooltip::text(tip.clone(), window, cx))
                            .into_any_element()
                    }
                }
            }))
    });
    section(theme, "Activity").child(div().flex().flex_row().gap(px(3.)).children(weeks))
}

fn articles(theme: &Theme, summary: &Summary) -> gpui::Div {
    let added: u64 = summary.days.iter().map(|day| day.words_added).sum();
    let removed: u64 = summary.days.iter().map(|day| day.words_removed).sum();
    let daily = summary
        .days
        .iter()
        .map(|day| {
            (
                format!("{} · +{} −{}", day.date, day.words_added, day.words_removed),
                vec![
                    (day.words_added, theme.success),
                    (day.words_removed, theme.danger),
                ],
            )
        })
        .collect();
    let mut body = section(theme, "Articles")
        .child(tiles([
            tile(theme, "Total words", count(summary.words_total)),
            tile(theme, "Words written", count(added)),
            tile(theme, "Words removed", count(removed)),
        ]))
        .child(bars("article-bars", daily, theme));
    if !summary.grew.is_empty() {
        body = body.child(list(
            theme,
            summary
                .grew
                .iter()
                .map(|(title, words)| (title.clone(), format!("+{}", count(*words as u64)))),
        ));
    }
    body
}

fn boards_section(
    theme: &Theme,
    summary: &Summary,
    sessions: bool,
    prices: Option<&Prices>,
) -> gpui::Div {
    let created: u64 = summary.days.iter().map(|day| day.cards_created).sum();
    let done: u64 = summary.days.iter().map(|day| day.cards_done).sum();
    let daily = summary
        .days
        .iter()
        .map(|day| {
            (
                format!(
                    "{} · {} created · {} done",
                    day.date, day.cards_created, day.cards_done
                ),
                vec![
                    (day.cards_done, theme.success),
                    (day.cards_created, theme.accent.opacity(0.5)),
                ],
            )
        })
        .collect();
    let mut body = section(theme, "Boards")
        .child(tiles([
            tile(theme, "Cards created", count(created)),
            tile(theme, "Cards done", count(done)),
        ]))
        .child(bars("card-bars", daily, theme));
    for (board, columns) in &summary.columns {
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .text_style(TextStyle::Subheadline)
                        .text_color(theme.text_muted)
                        .child(board.clone()),
                )
                .child(list(
                    theme,
                    columns
                        .iter()
                        .map(|(column, cards)| (column.clone(), count(*cards as u64))),
                )),
        );
    }
    if sessions && let Some(prices) = prices {
        let by_session = cost_by(summary, prices, |spend| spend.session.clone());
        let mut per_board: BTreeMap<&str, f64> = BTreeMap::new();
        let mut per_card: Vec<(String, f64)> = Vec::new();
        for link in &summary.links {
            if let Some(cost) = by_session.get(&link.session) {
                *per_board.entry(&link.board).or_default() += cost;
                per_card.push((link.card.clone(), *cost));
            }
        }
        if !per_board.is_empty() {
            let mut per_board: Vec<_> = per_board.into_iter().collect();
            per_board.sort_by(|a, b| b.1.total_cmp(&a.1));
            per_card.sort_by(|a, b| b.1.total_cmp(&a.1));
            per_card.truncate(5);
            body = body
                .child(list(
                    theme,
                    per_board
                        .into_iter()
                        .map(|(board, cost)| (board.to_owned(), dollars(cost))),
                ))
                .child(list(
                    theme,
                    per_card
                        .into_iter()
                        .map(|(card, cost)| (card, dollars(cost))),
                ));
        }
    }
    body
}

impl Statistics {
    fn usage(
        &self,
        theme: &Theme,
        summary: &Summary,
        prices: Option<&Prices>,
        unavailable: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let mut total = Tokens::default();
        for spend in &summary.spend {
            total += spend.tokens;
        }
        let cost = |spend: &crate::model::statistics::Spend| {
            prices
                .and_then(|prices| prices.rates(&spend.model))
                .map_or(0.0, |rates| rates.cost(&spend.tokens))
        };
        let full: f64 = summary.spend.iter().map(cost).sum();
        // What cache reads would have cost as plain input.
        let savings: f64 = summary
            .spend
            .iter()
            .filter_map(|spend| {
                let rates = prices?.rates(&spend.model)?;
                Some(spend.tokens.cache_read as f64 * (rates.input - rates.cache_read) / 1e6)
            })
            .sum();

        let mut agents: Vec<String> = summary
            .spend
            .iter()
            .map(|spend| spend.agent.clone())
            .collect();
        agents.sort();
        agents.dedup();
        let measure = |spend: &crate::model::statistics::Spend| match self.measure {
            Measure::Cost => cost(spend),
            Measure::Tokens => {
                let t = spend.tokens;
                (t.input + t.output + t.cache_read + t.cache_write) as f64
            }
        };
        let share: Vec<(usize, f64)> = agents
            .iter()
            .enumerate()
            .map(|(ix, agent)| {
                (
                    ix,
                    summary
                        .spend
                        .iter()
                        .filter(|spend| &spend.agent == agent)
                        .map(cost)
                        .sum(),
                )
            })
            .collect();

        let headline = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .text_style(TextStyle::Subheadline)
                    .text_color(theme.text_muted)
                    .child(match (prices, unavailable) {
                        (Some(_), _) => "RAW TOKEN COST* · if billed at full API rate",
                        (None, true) => "RAW TOKEN COST* · prices unavailable",
                        (None, false) => "RAW TOKEN COST* · fetching prices…",
                    }),
            )
            .child(
                div()
                    .text_style(TextStyle::Title)
                    .text_color(theme.text)
                    .child(dollars(full)),
            );
        let share_bar = div()
            .h(px(6.))
            .rounded(px(3.))
            .overflow_hidden()
            .flex()
            .flex_row()
            .bg(theme.ink(0.06))
            .children(
                share
                    .iter()
                    .filter(|(_, cost)| *cost > 0.)
                    .map(|(ix, cost)| {
                        div()
                            .h_full()
                            .flex_basis(gpui::relative((cost / full.max(f64::EPSILON)) as f32))
                            .bg(series(theme, *ix))
                    }),
            );
        let legend = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(12.))
            .children(share.iter().map(|(ix, cost)| {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text_muted)
                    .child(div().size(px(8.)).rounded(px(2.)).bg(series(theme, *ix)))
                    .child(format!("{} {}", agents[*ix], dollars(*cost)))
            }));

        // Day by agent, for the stacked area.
        let index: BTreeMap<&str, usize> = summary
            .days
            .iter()
            .enumerate()
            .map(|(ix, day)| (day.date.as_str(), ix))
            .collect();
        let mut grid = vec![vec![0f64; summary.days.len()]; agents.len()];
        for spend in &summary.spend {
            if let (Some(&day), Ok(agent)) = (
                index.get(spend.day.as_str()),
                agents.binary_search(&spend.agent),
            ) {
                grid[agent][day] += measure(spend);
            }
        }
        let colors: Vec<Hsla> = (0..agents.len()).map(|ix| series(theme, ix)).collect();
        let area = area_chart(grid, colors, theme.border);

        let switch = theme.segmented(
            "usage-measure",
            ["Cost", "Tokens"],
            match self.measure {
                Measure::Cost => 0,
                Measure::Tokens => 1,
            },
            cx.listener(|this, ix: &usize, _, cx| {
                this.measure = [Measure::Cost, Measure::Tokens][*ix];
                cx.notify();
            }),
        );

        let processed = total.input + total.output + total.cache_read + total.cache_write;
        let token_tiles = tiles([
            tile(theme, "Processed", tokens(processed)),
            tile(theme, "Cached input", tokens(total.cache_read)),
            tile(
                theme,
                "Uncached input",
                tokens(total.input + total.cache_write),
            ),
            tile(theme, "Output", tokens(total.output)),
            tile(theme, "Cache savings", dollars(savings)),
        ]);

        let group = theme.segmented(
            "usage-group",
            ["Model", "Agent", "Day"],
            match self.group {
                Group::Model => 0,
                Group::Agent => 1,
                Group::Day => 2,
            },
            cx.listener(|this, ix: &usize, _, cx| {
                this.group = [Group::Model, Group::Agent, Group::Day][*ix];
                cx.notify();
            }),
        );
        let mut rows: BTreeMap<String, (Tokens, f64, bool)> = BTreeMap::new();
        for spend in &summary.spend {
            let key = match self.group {
                Group::Model => spend.model.clone(),
                Group::Agent => spend.agent.clone(),
                Group::Day => spend.day.clone(),
            };
            let row = rows.entry(key).or_default();
            row.0 += spend.tokens;
            row.1 += cost(spend);
            row.2 |= prices
                .and_then(|prices| prices.rates(&spend.model))
                .is_none();
        }
        let mut rows: Vec<_> = rows.into_iter().collect();
        if self.group == Group::Day {
            rows.reverse();
        } else {
            rows.sort_by(|a, b| b.1.1.total_cmp(&a.1.1));
        }
        let table = list(
            theme,
            rows.into_iter().map(|(key, (t, cost, unpriced))| {
                let processed = t.input + t.output + t.cache_read + t.cache_write;
                let price = match (unpriced && self.group == Group::Model, cost > 0.) {
                    (true, _) => "unpriced".to_owned(),
                    (false, _) => dollars(cost),
                };
                (key, format!("{} · {price}", tokens(processed)))
            }),
        );

        section(theme, "Usage")
            .child(headline)
            .child(share_bar)
            .child(legend)
            .child(switch)
            .child(area)
            .child(token_tiles)
            .child(group)
            .child(table)
    }
}

/// Cost per key over every spend row.
fn cost_by(
    summary: &Summary,
    prices: &Prices,
    key: impl Fn(&crate::model::statistics::Spend) -> String,
) -> std::collections::HashMap<String, f64> {
    let mut out = std::collections::HashMap::new();
    for spend in &summary.spend {
        if let Some(rates) = prices.rates(&spend.model) {
            *out.entry(key(spend)).or_default() += rates.cost(&spend.tokens);
        }
    }
    out
}

/// Series stacked bottom-up, one value per day each.
fn area_chart(series: Vec<Vec<f64>>, colors: Vec<Hsla>, axis: Hsla) -> AnyElement {
    div()
        .h(px(CHART_HEIGHT))
        .w_full()
        .border_b_1()
        .border_color(axis)
        .child(
            canvas(
                |_, _, _| (),
                move |bounds: Bounds<Pixels>, (), window, _| {
                    let days = series.first().map_or(0, Vec::len);
                    if days == 0 {
                        return;
                    }
                    let mut base = vec![0f64; days];
                    let tops: Vec<Vec<f64>> = series
                        .iter()
                        .map(|values| {
                            for (sum, value) in base.iter_mut().zip(values) {
                                *sum += value;
                            }
                            base.clone()
                        })
                        .collect();
                    let max = base.iter().copied().fold(0f64, f64::max);
                    if max <= 0. {
                        return;
                    }
                    let x = |ix: usize| {
                        let step = match days {
                            1 => 0.,
                            n => f32::from(bounds.size.width) / (n - 1) as f32,
                        };
                        bounds.origin.x + px(step * ix as f32)
                    };
                    let y = |value: f64| {
                        bounds.origin.y + bounds.size.height
                            - px((value / max) as f32 * f32::from(bounds.size.height))
                    };
                    let zero = vec![0f64; days];
                    for (layer, top) in tops.iter().enumerate() {
                        let below = match layer {
                            0 => &zero,
                            n => &tops[n - 1],
                        };
                        let mut path = PathBuilder::fill();
                        path.move_to(point(x(0), y(top[0])));
                        for (ix, value) in top.iter().enumerate().skip(1) {
                            path.line_to(point(x(ix), y(*value)));
                        }
                        for ix in (0..days).rev() {
                            path.line_to(point(x(ix), y(below[ix])));
                        }
                        path.close();
                        if let Ok(path) = path.build() {
                            window.paint_path(path, colors[layer % colors.len()].opacity(0.7));
                        }
                    }
                },
            )
            .size_full(),
        )
        .into_any_element()
}

/// Label on the left, value on the right, a hairline between rows.
fn list(theme: &Theme, rows: impl IntoIterator<Item = (String, String)>) -> gpui::Div {
    theme
        .group_box()
        .children(rows.into_iter().enumerate().map(|(ix, (label, value))| {
            theme
                .card_row(ix == 0)
                .justify_between()
                .child(theme.row_title(label).flex_1())
                .child(
                    div()
                        .flex_none()
                        .text_style(TextStyle::Callout)
                        .text_color(theme.text_muted)
                        .child(value),
                )
        }))
}

fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (ix, ch) in digits.chars().enumerate() {
        if ix > 0 && (digits.len() - ix).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn tokens(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}K", n as f64 / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1}M", n as f64 / 1e6),
        _ => format!("{:.2}B", n as f64 / 1e9),
    }
}

fn dollars(cost: f64) -> String {
    let cents = (cost * 100.).round() as u64;
    format!("${}.{:02}", count(cents / 100), cents % 100)
}
