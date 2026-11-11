use crate::{
    model::{
        project::ProjectId,
        workspace::{Showing, Workspace, references::Part},
    },
    view::{
        kit::{
            icon::kind_icon,
            link::mention::{self, Linkables},
            text::CardTitle,
        },
        search::Found,
        sidebar::Row,
    },
};
use artifact::{kind::Kind, reference};
use bezel::{
    gpui::{App, Div, HighlightStyle, SharedString, Stateful, StyledText, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset as _},
    ui::icons,
};
use std::ops::Range;

const LEAD: usize = 32;

/// One entry or board card found by search.
#[derive(Clone)]
pub(crate) struct Hit {
    pub(crate) row: Row,
    pub(crate) reference: Option<String>,
    /// Where the query was found, when it was found in the entry's body: the
    /// line holding it, and where in the line.
    pub(crate) snippet: Option<(String, Range<usize>)>,
    /// A card on the board `row` names.
    card: Option<HitCard>,
}

#[derive(Clone)]
struct HitCard {
    /// `DEV-12`.
    handle: String,
    title: String,
}

impl Hit {
    /// What a search found, as rows: title matches first, then the rest, each
    /// most recently touched first.
    pub(crate) fn ranked(found: Vec<Found>, workspace: &Workspace, cx: &App) -> Vec<Hit> {
        let mut hits: Vec<(bool, u128, Hit)> = found
            .into_iter()
            .filter_map(|found| {
                let open = workspace
                    .projects
                    .iter()
                    .find(|open| open.path == found.root)?;
                let showing = open.stored(found.kind, &found.id, cx)?;
                let touched = open.facts(&showing, cx)?.touched;
                let row = Row::Entry {
                    project: open.id,
                    showing,
                };
                Some((
                    found.in_title,
                    touched,
                    Hit {
                        row,
                        reference: None,
                        snippet: found.snippet,
                        card: None,
                    },
                ))
            })
            .collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        hits.into_iter().map(|(.., hit)| hit).collect()
    }

    /// The entries and cards the field's query names — see
    /// [`mention::Query::rank_named`].
    pub(crate) fn named(
        text: &str,
        workspace: &Workspace,
        home: Option<ProjectId>,
        cx: &App,
    ) -> Vec<Hit> {
        let held = cx
            .try_global::<Linkables>()
            .map_or(&[][..], |Linkables(held)| held.as_slice());
        let cards = mention::carded(workspace, cx);
        let (named, _) = mention::Query::parse(text).rank_named(held, &cards);
        let mut hits: Vec<Hit> = named
            .into_iter()
            .filter_map(|named| match named {
                mention::Named::Entry(ix) => Some(Hit {
                    row: held[ix].row.clone(),
                    reference: None,
                    snippet: None,
                    card: None,
                }),
                mention::Named::Card(ix) => {
                    let carded = &cards[ix];
                    let project = workspace
                        .projects
                        .iter()
                        .find(|open| reference::names(&open.path, &carded.project))?;
                    Some(Hit {
                        row: Row::Entry {
                            project: project.id,
                            showing: Showing::Board(carded.board.id.clone()),
                        },
                        reference: Some(carded.reference().link()),
                        snippet: None,
                        card: Some(HitCard {
                            handle: carded.handle.clone(),
                            title: carded.card.title(),
                        }),
                    })
                }
            })
            .collect();
        if home.is_some() {
            let text = text.trim();
            if let Some(mut reference) = reference::parse(text)
                && let Ok(resolved) = workspace.resolve_from(text, home, cx)
                && let Some(project) = workspace.project(resolved.project)
                && let Some(name) = reference::project_name(&project.path)
            {
                reference.project = Some(&name);
                let row = Row::Entry {
                    project: resolved.project,
                    showing: resolved.showing,
                };
                hits.retain(|hit| hit.row != row);
                let card = match resolved.part {
                    Some(Part::Card(card)) => Some(HitCard {
                        handle: card.handle,
                        title: markdown::parse(&card.text).title(),
                    }),
                    _ => None,
                };
                hits.insert(
                    0,
                    Hit {
                        row,
                        reference: Some(reference.link()),
                        snippet: None,
                        card,
                    },
                );
            }
        }
        hits
    }

    pub(crate) fn link(&self, workspace: &Workspace, cx: &App) -> Option<String> {
        let url = if let Some(reference) = &self.reference {
            reference.clone()
        } else {
            let Row::Entry { project, showing } = &self.row else {
                return None;
            };
            workspace.link_of(*project, showing, cx)?
        };
        Some(url)
    }

    pub(crate) fn matching(
        text: &str,
        found: Vec<Found>,
        workspace: &Workspace,
        home: Option<ProjectId>,
        cx: &App,
    ) -> Vec<Self> {
        let mut hits = Self::named(text, workspace, home, cx);
        let label = mention::Query::parse(text).label;
        let ranked = Self::ranked(found, workspace, cx)
            .into_iter()
            .filter(|hit| {
                !hits
                    .iter()
                    .any(|held| held.card.is_none() && held.row == hit.row)
                    && label.as_ref().is_none_or(|label| {
                        let Row::Entry { project, showing } = &hit.row else {
                            return false;
                        };
                        workspace
                            .project_ix(*project)
                            .and_then(|at| workspace.labels_of(at, showing, cx))
                            .is_some_and(|labels| labels.iter().any(|held| held == label))
                    })
            })
            .collect::<Vec<_>>();
        hits.extend(ranked);
        hits
    }

    pub(crate) fn recent(rows: &[Row]) -> Vec<Self> {
        rows.iter()
            .map(|row| Self {
                row: row.clone(),
                reference: None,
                snippet: None,
                card: None,
            })
            .collect()
    }

    pub(crate) fn render(
        &self,
        ix: usize,
        selected: bool,
        workspace: &Workspace,
        cx: &App,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx).clone();
        let project = self
            .row
            .located(workspace)
            .and_then(|(project, _)| workspace.projects.get(project).map(|open| open.name()));
        let icon = kind_icon(self.row.kind().unwrap_or(Kind::Article));
        let title = match &self.card {
            Some(card) => format!("{} · {}", card.handle, card.title),
            None => self.row.label(workspace, cx),
        };
        let snippet = self.snippet.as_ref().map(|(line, at)| {
            let (text, at) = clipped(line, at.clone());
            StyledText::new(text).with_highlights([(
                at,
                HighlightStyle {
                    color: Some(theme.text),
                    background_color: Some(theme.accent.opacity(0.22)),
                    ..Default::default()
                },
            )])
        });
        div()
            .id(("search-hit", ix))
            .flex()
            .flex_row()
            .items_start()
            .gap(px(10.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(Theme::control_radius()))
            .cursor_pointer()
            .when(selected, |el| el.bg(theme.element_active))
            .when(!selected, |el| el.hover(|el| el.bg(theme.element_hover)))
            .child(
                div()
                    .pt(px(2.))
                    .child(icons::icon(icon).size(px(14.)).text_color(theme.text_muted)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_style(TextStyle::Body)
                                    .text_color(theme.text)
                                    .child(title),
                            )
                            .children(project.map(|project| {
                                div()
                                    .flex_none()
                                    .text_style(TextStyle::Subheadline)
                                    .text_color(theme.text_faint)
                                    .child(project)
                            })),
                    )
                    .children(snippet.map(|snippet| {
                        div()
                            .truncate()
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(snippet)
                    })),
            )
    }
}

/// The line cut to start a little ahead of the match and trimmed, and the
/// match's range in what is left.
pub(crate) fn clipped(line: &str, at: Range<usize>) -> (SharedString, Range<usize>) {
    let start = line[..at.start]
        .char_indices()
        .rev()
        .nth(LEAD)
        .map_or(0, |(ix, _)| ix);
    let head = &line[start..];
    let trimmed = head.trim_start();
    let cut = start + (head.len() - trimmed.len());
    let lead = if start > 0 { "…" } else { "" };
    let text = format!("{lead}{}", trimmed.trim_end());
    let from = (lead.len() + at.start.saturating_sub(cut)).min(text.len());
    let to = (lead.len() + at.end.saturating_sub(cut)).min(text.len());
    (text.into(), from..to)
}

#[cfg(test)]
#[path = "../../../tests/unit/entry_search.rs"]
mod tests;
