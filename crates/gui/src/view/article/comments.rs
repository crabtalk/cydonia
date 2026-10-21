//! The comments button the pane's bar draws for an article, and its menu:
//! how the margin shows the comments, then every one of them.

use super::{ArticleView, MENU_MAX_HEIGHT, MENU_MAX_WIDTH};
use crate::{
    model::workspace::Workspace,
    view::{kit::menu, pane::title},
};
use bezel::{
    gpui::{
        AnyElement, App, Context, Entity, Focusable as _, IntoElement, Render, SharedString,
        WeakEntity, Window, div, prelude::*, px,
    },
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        menu::Item,
        tooltip::Tooltip,
    },
};
use std::path::Path;

pub(crate) struct CommentsButton {
    workspace: Entity<Workspace>,
    /// The article view it is the button of.
    article: WeakEntity<ArticleView>,
    menu: ContextMenu<Self>,
}

impl CommentsButton {
    pub(crate) fn new(
        workspace: Entity<Workspace>,
        article: WeakEntity<ArticleView>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        Self {
            workspace,
            article,
            menu: ContextMenu::default(),
        }
    }

    /// The comments menu's rows: how the margin shows them, then every
    /// comment, open ones first.
    fn comments_rows(
        &self,
        project: usize,
        at: usize,
        editor: &Entity<editor::Editor>,
        path: &Path,
        cx: &App,
    ) -> Vec<(Item, menu::Act<Self>)> {
        use crate::model::settings::CommentDisplay;
        let workspace = self.workspace.read(cx);
        let Some(article) = workspace.article_in(project, at) else {
            return Vec::new();
        };
        let comments = &article.comments.comments;
        let shown = article.comments_shown(workspace.settings.appearance.comments);
        let switch = {
            let path = path.to_owned();
            menu::switch::<Self, _>(
                "Show",
                icons::social::MessageCircle,
                vec![
                    (
                        CommentDisplay::Full,
                        icons::layout::PanelRight.into(),
                        CommentDisplay::Full.label(),
                    ),
                    (
                        CommentDisplay::Minimal,
                        icons::social::MessageCircle.into(),
                        CommentDisplay::Minimal.label(),
                    ),
                ],
                shown,
                move |this, mode, _, cx| {
                    this.workspace.update(cx, |workspace, cx| {
                        workspace.set_comment_display(&path, Some(mode), cx)
                    });
                },
            )
        };
        let row = |comment: &artifact::article::comments::Comment| {
            let quote = artifact::text::one_line(&comment.quote);
            let note = comment.headline().to_owned();
            let icon = match comment.resolved {
                true => icons::glyph::Check,
                false => icons::social::MessageCircle,
            };
            let (id, resolved, path, editor) = (
                comment.id,
                comment.resolved,
                path.to_owned(),
                editor.clone(),
            );
            menu::row::<Self>(
                Item::action(format!("“{quote}”"))
                    .with_description(note)
                    .with_icon(icon),
                move |this, window, cx| {
                    this.workspace.update(cx, |workspace, cx| {
                        if resolved {
                            workspace.resolve_comment(&path, id, false, cx);
                        }
                        workspace.activate_comment(&path, Some(id), cx);
                    });
                    let start = editor
                        .read(cx)
                        .document()
                        .read(cx)
                        .anchors()
                        .iter()
                        .find(|anchor| anchor.id.0 == id)
                        .map(|anchor| anchor.range.ordered().0);
                    if let Some(start) = start {
                        editor.update(cx, |editor, cx| {
                            editor.select_to_top(markdown::Selection::at(start), cx)
                        });
                        window.focus(&editor.focus_handle(cx), cx);
                    }
                },
            )
        };
        let open: Vec<_> = comments.iter().filter(|c| !c.resolved).map(row).collect();
        let mut resolved: Vec<_> = comments.iter().filter(|c| c.resolved).map(row).collect();
        if !resolved.is_empty() {
            let heading: menu::Act<Self> = Box::new(|_, _, _, _| {});
            resolved.insert(0, (Item::Heading("Resolved".into()), heading));
        }
        menu::sections([vec![switch], open, resolved])
    }
}

impl Render for CommentsButton {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.button(window, cx)
            .unwrap_or_else(|| div().into_any_element())
    }
}

impl CommentsButton {
    /// The button, with its menu while that is open. Nothing for an article
    /// with no comments.
    fn button(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let view = self.article.upgrade()?;
        let view = view.read(cx);
        let editor = view.editor()?.clone();
        let (project, at) = view.located(cx)?;
        let workspace = self.workspace.read(cx);
        let article = workspace.article_in(project, at)?;
        let comments = &article.comments.comments;
        if comments.is_empty() {
            return None;
        }
        let path = article.path.clone();
        let id = SharedString::from(format!("comments-card-{}", path.display()));
        let card = self
            .menu
            .showing(&())
            .then(|| {
                self.menu
                    .card(id.clone(), |this| &mut this.menu, window, cx)
            })
            .flatten()
            .map(|card| {
                let card = card.max_w(px(MENU_MAX_WIDTH)).max_h(px(MENU_MAX_HEIGHT));
                self.menu.place(id.clone(), card)
            });
        let trigger = menu::button(
            format!("comments-{}", path.display()),
            icons::social::MessageCircle,
            menu::Reveal::Group(title::TITLE_GROUP),
            self.menu.showing(&()),
            cx,
        )
        .tooltip(|window, cx| Tooltip::text("Comments", window, cx))
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            let (path, editor) = (path.clone(), editor.clone());
            ContextMenu::toggle(
                this,
                |this| &mut this.menu,
                (),
                Place::BelowEnd,
                move |this, _, cx| {
                    menu::chooser(this.comments_rows(project, at, &editor, &path, cx))
                },
                window,
                cx,
            );
        }))
        .children(card);
        Some(
            ContextMenu::trigger(trigger, |this: &mut Self| &mut this.menu, (), cx)
                .into_any_element(),
        )
    }
}
