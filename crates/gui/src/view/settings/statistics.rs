//! The statistics section: a year of activity across the open projects, and
//! what their agents' tokens would cost at API prices while sessions are on.

use crate::{
    model::{
        prices::{self, Prices},
        statistics::{Logs, RANGES, Spend, Summary, Tokens},
    },
    view::settings::{GROUP_GAP, LABEL_GAP, SettingsWindow},
};
use bezel::{
    gpui::{self, AnyElement, App, Context, Global, Hsla, SharedString, Task, div, prelude::*, px},
    theme::{Appearance, TextStyle, Theme, Typeset},
    ui::{
        tooltip::Tooltip,
        widgets::{Controls, Scaffolding},
    },
};
use std::rc::Rc;

/// SwiftUI's system colours, in the order a series takes them: blue, orange,
/// green, purple, pink, teal, indigo, yellow, red, brown. Light, then dark.
const SERIES: [[u32; 10]; 2] = [
    [
        0x007AFF, 0xFF9500, 0x34C759, 0xAF52DE, 0xFF2D55, 0x30B0C7, 0x5856D6, 0xFFCC00, 0xFF3B30,
        0xA2845E,
    ],
    [
        0x0A84FF, 0xFF9F0A, 0x30D158, 0xBF5AF2, 0xFF375F, 0x40C8E0, 0x5E5CE6, 0xFFD60A, 0xFF453A,
        0xAC8E68,
    ],
];

/// SwiftUI's green, which the heatmap is drawn in. Light, then dark.
const GREEN: [u32; 2] = [0x34C759, 0x30D158];

const CHART_HEIGHT: f32 = 56.;
/// Models drawn as their own series; past one more, the rest are "Others".
const SHOWN: usize = 5;
const CELL_GAP: f32 = 2.;

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

/// What the section holds between frames.
pub(super) struct Statistics {
    days: u32,
    summary: Option<Summary>,
    _load: Option<Task<()>>,
}

impl Default for Statistics {
    fn default() -> Self {
        Self {
            days: RANGES[1],
            summary: None,
            _load: None,
        }
    }
}

impl SettingsWindow {
    /// Read the open projects again, and while sessions are on the agents'
    /// logs and, the first time, the price list.
    pub(super) fn load_statistics(&mut self, cx: &mut Context<Self>) {
        let projects: Vec<_> = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .map(|project| project.path.clone())
            .collect();
        let days = self.statistics.days;
        let sessions = self.workspace.read(cx).settings.features.sessions;
        let logs = match sessions {
            true => Logs::local(),
            false => Logs {
                claude: None,
                codex: None,
            },
        };
        self.statistics._load = Some(cx.spawn(async move |this, cx| {
            let today = chrono::Local::now().date_naive();
            let summary = cx
                .background_executor()
                .spawn(async move { Summary::compute(&projects, &logs, days, today) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.statistics.summary = Some(summary.unwrap_or_default());
                cx.notify();
            });
        }));
        if sessions {
            self.fetch_prices(cx);
        }
    }

    /// Query the range again from the indexes the last load refreshed.
    fn load_spend(&mut self, cx: &mut Context<Self>) {
        if self.statistics.summary.is_none() {
            return self.load_statistics(cx);
        }
        let projects: Vec<_> = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .map(|project| project.path.clone())
            .collect();
        let days = self.statistics.days;
        self.statistics._load = Some(cx.spawn(async move |this, cx| {
            let today = chrono::Local::now().date_naive();
            let spend = cx
                .background_executor()
                .spawn(async move { Summary::spend(&projects, days, today) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let (Some(summary), Ok((days, spend))) = (&mut this.statistics.summary, spend) {
                    summary.days = days;
                    summary.spend = spend;
                }
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

    pub(super) fn statistics_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let Some(summary) = &self.statistics.summary else {
            return div()
                .text_style(TextStyle::Body)
                .text_color(theme.text_muted)
                .child("Reading…")
                .into_any_element();
        };
        let unavailable = matches!(cx.try_global::<PriceTable>(), Some(PriceTable::Unavailable));
        let prices = loaded_prices(cx);
        let sessions = self.workspace.read(cx).settings.features.sessions;

        div()
            .flex()
            .flex_col()
            .gap(px(GROUP_GAP))
            .child(heatmap(&theme, summary))
            .when(sessions, |body| {
                body.child(self.spend(&theme, summary, prices.as_deref(), unavailable, cx))
            })
            .into_any_element()
    }

    fn spend(
        &self,
        theme: &Theme,
        summary: &Summary,
        prices: Option<&Prices>,
        unavailable: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let cost = |spend: &Spend| {
            prices
                .and_then(|prices| prices.rates(&spend.model))
                .map(|rates| rates.cost(&spend.tokens))
        };
        // Models by what they cost over the range, most first.
        let mut models: Vec<(String, f64)> = Vec::new();
        let mut sum = Tokens::default();
        for spend in &summary.spend {
            sum += spend.tokens;
            let spent = cost(spend).unwrap_or(0.);
            match models.iter_mut().find(|(model, _)| *model == spend.model) {
                Some((_, total)) => *total += spent,
                None => models.push((spend.model.clone(), spent)),
            }
        }
        models.sort_by(|a, b| b.1.total_cmp(&a.1));
        let total: f64 = models.iter().map(|(_, cost)| cost).sum();
        // The costliest models each take a series; the rest share one.
        let named = match models.len() > SHOWN + 1 {
            true => SHOWN,
            false => models.len(),
        };
        let mut groups: Vec<(String, f64, Hsla)> = models[..named]
            .iter()
            .enumerate()
            .map(|(ix, (model, cost))| (model.clone(), *cost, series(theme, ix)))
            .collect();
        if named < models.len() {
            let rest = models[named..].iter().map(|(_, cost)| cost).sum();
            groups.push(("Others".to_owned(), rest, theme.text_faint));
        }
        let group_of = |model: &str| {
            models
                .iter()
                .position(|(name, _)| name == model)
                .map_or(named, |ix| ix.min(named))
        };

        let mut about = vec![format!("{} tokens", tokens(processed(&sum)))];
        if let Some(cached) = cached(&sum) {
            about.push(format!("{cached:.0}% cached"));
        }
        match (prices, unavailable) {
            (None, true) => about.push("prices unavailable".to_owned()),
            (None, false) => about.push("fetching prices…".to_owned()),
            (Some(_), _) => {}
        }
        let header = theme
            .card_row(true)
            .child(theme.row_title(dollars(total)).flex_1())
            .child(
                div()
                    .flex_none()
                    .text_style(TextStyle::Subheadline)
                    .text_color(theme.text_muted)
                    .child(about.join(" · ")),
            );

        let bars: Vec<Bar> = summary
            .days
            .iter()
            .map(|date| {
                let spent: Vec<&Spend> = summary
                    .spend
                    .iter()
                    .filter(|spend| &spend.day == date)
                    .collect();
                let day_cost: f64 = spent.iter().filter_map(|spend| cost(spend)).sum();
                let mut lines = vec![format!("{date} · {}", dollars(day_cost))];
                let mut parts = vec![0f64; groups.len()];
                for (model, _) in &models {
                    let by: f64 = spent
                        .iter()
                        .filter(|spend| &spend.model == model)
                        .filter_map(|spend| cost(spend))
                        .sum();
                    if by > 0. {
                        lines.push(format!("{model} · {}", dollars(by)));
                        parts[group_of(model)] += by;
                    }
                }
                let parts = parts
                    .into_iter()
                    .zip(&groups)
                    .filter(|(by, _)| *by > 0.)
                    .map(|(by, (_, _, color))| (by, *color))
                    .collect();
                Bar {
                    tip: lines.join("\n"),
                    parts,
                }
            })
            .collect();
        let key = theme.meta_line(
            groups
                .iter()
                .map(|(name, cost, color)| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .child(div().size(px(8.)).rounded(px(2.)).bg(*color))
                        .child(name.clone())
                        .child(div().text_color(theme.text).child(dollars(*cost)))
                        .into_any_element()
                })
                .collect(),
        );
        let chart = theme
            .card_row(false)
            .flex_col()
            .items_stretch()
            .gap(px(4.))
            .child(bar_chart(bars, theme))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_style(TextStyle::Caption)
                    .text_color(theme.text_faint)
                    .children(summary.days.first().cloned())
                    .children(summary.days.last().cloned()),
            )
            .child(key);

        let selected = RANGES
            .iter()
            .position(|days| *days == self.statistics.days)
            .unwrap_or(1);
        div()
            .flex()
            .flex_col()
            .gap(px(LABEL_GAP))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(theme.field_label("Tokens"))
                    .child(theme.segmented(
                        "statistics-range",
                        RANGES.map(|days| format!("{days} days")),
                        selected,
                        cx.listener(|this, ix: &usize, _, cx| {
                            this.statistics.days = RANGES[*ix];
                            this.load_spend(cx);
                        }),
                    )),
            )
            .child(theme.group_box().child(header).child(chart))
            .child(footnote(
                theme,
                "Estimated API-equivalent cost: what these tokens would cost at each \
                 model's API price. Not your subscription bill."
                    .to_owned(),
            ))
    }
}

fn loaded_prices(cx: &App) -> Option<Rc<Prices>> {
    match cx.try_global::<PriceTable>() {
        Some(PriceTable::Ready(prices)) => Some(prices.clone()),
        _ => None,
    }
}

fn footnote(theme: &Theme, text: String) -> gpui::Div {
    div()
        .text_style(TextStyle::Footnote)
        .text_color(theme.text_muted)
        .child(text)
}

fn swift(theme: &Theme, colors: [u32; 2]) -> Hsla {
    let dark = usize::from(theme.appearance == Appearance::Dark);
    gpui::rgb(colors[dark]).into()
}

fn series(theme: &Theme, ix: usize) -> Hsla {
    let at = ix % SERIES[0].len();
    swift(theme, [SERIES[0][at], SERIES[1][at]])
}

/// A year of days worked, Monday on top, oldest week first.
fn heatmap(theme: &Theme, summary: &Summary) -> gpui::Div {
    let max = summary
        .heat
        .iter()
        .map(|day| day.total())
        .max()
        .unwrap_or(0)
        .max(1);
    let green = swift(theme, GREEN);
    let lead = summary
        .heat
        .first()
        .and_then(|day| chrono::NaiveDate::parse_from_str(&day.date, "%Y-%m-%d").ok())
        .map_or(0, |date| {
            chrono::Datelike::weekday(&date).num_days_from_monday() as usize
        });
    let mut cells: Vec<Option<&crate::model::statistics::Day>> = vec![None; lead];
    cells.extend(summary.heat.iter().map(Some));
    // The weeks share the width, and the cells stay square.
    div()
        .w_full()
        .flex()
        .flex_row()
        .gap(px(CELL_GAP))
        .children(cells.chunks(7).map(|week| {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(CELL_GAP))
                .children((0..7).map(|row| {
                    let cell = div().w_full().aspect_square().rounded(px(2.));
                    let Some(day) = week.get(row).copied().flatten() else {
                        return cell.into_any_element();
                    };
                    let fill = match day.total() {
                        0 => theme.ink(0.06),
                        n => green.opacity(0.25 + 0.75 * n as f32 / max as f32),
                    };
                    let tip = SharedString::from(format!(
                        "{}\n{} sessions · {} articles · {} cards",
                        day.date, day.sessions, day.articles, day.cards
                    ));
                    cell.id(SharedString::from(format!("heat-{}", day.date)))
                        .bg(fill)
                        .tooltip(move |window, cx| Tooltip::text(tip.clone(), window, cx))
                        .into_any_element()
                }))
        }))
}

/// One day's bar: what its hover says, and its stack from the bottom up.
struct Bar {
    tip: String,
    parts: Vec<(f64, Hsla)>,
}

fn bar_chart(bars: Vec<Bar>, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    let max = bars
        .iter()
        .map(|bar| bar.parts.iter().map(|(value, _)| value).sum::<f64>())
        .fold(0f64, f64::max);
    let scale = match max > 0. {
        true => CHART_HEIGHT / max as f32,
        false => 0.,
    };
    div()
        .id("statistics-bars")
        .h(px(CHART_HEIGHT))
        .flex()
        .flex_row()
        .items_end()
        .gap(px(2.))
        .border_b_1()
        .border_color(theme.border)
        .children(bars.into_iter().enumerate().map(|(ix, bar)| {
            let tip = SharedString::from(bar.tip);
            div()
                .id(ix)
                .flex_1()
                .h_full()
                .flex()
                .flex_col_reverse()
                .rounded(px(2.))
                .hover(|el| el.bg(theme.element_hover))
                .children(
                    bar.parts.into_iter().map(|(value, color)| {
                        div().flex_none().h(px(value as f32 * scale)).bg(color)
                    }),
                )
                .tooltip(move |window, cx| Tooltip::text(tip.clone(), window, cx))
        }))
}

fn processed(t: &Tokens) -> u64 {
    t.input + t.output + t.cache_read + t.cache_write
}

/// Cache reads as a share of input, in percent.
fn cached(t: &Tokens) -> Option<f64> {
    let input = t.input + t.cache_read + t.cache_write;
    (input > 0).then(|| t.cache_read as f64 * 100. / input as f64)
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
