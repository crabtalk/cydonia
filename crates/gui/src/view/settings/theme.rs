//! The appearance section: which of the three modes the app paints in.

use crate::model::{
    settings::{Highlight, Paint},
    themes,
};
use crate::{
    model::workspace::{ArticleChange, Workspace},
    view::{
        kit,
        settings::{self, SettingsWindow, Switch, group, labelled},
    },
};
use artifact::board::View;
use bezel::theme::AppExt as _;
use bezel::ui::color::Swatch;
use bezel::{
    gpui::{
        AnyElement, Context, DragMoveEvent, Empty, Entity, FontWeight, Hsla, SharedString, div,
        prelude::*, px,
    },
    theme::{Appearance, TextStyle, Theme, ThemeFamily, Tint, Typeset, appearance::AppearanceMode},
    ui::combobox::{Combobox, ComboboxEvent},
    ui::widgets::{self, Controls, Scaffolding, SliderDrag},
};
use std::path::PathBuf;

/// The tint's ceiling: Slate's chroma, the most coloured of the five neutrals
/// bezel ships in `BASE_COLORS`. Past it the greys stop reading as greys.
const CHROMA_MAX: f32 = 0.046;

/// A full turn of oklch hue.
const HUE_MAX: f32 = 360.;

/// How wide a slider sits in its row.
const SLIDER_WIDTH: f32 = 160.;

/// The picker's name for bezel's own palette.
const DEFAULT_THEME: &str = "Default";

const MODES: [AppearanceMode; 3] = [
    AppearanceMode::System,
    AppearanceMode::Light,
    AppearanceMode::Dark,
];

/// Swatches a row of the colour popover holds.
const SWATCH_COLUMNS: usize = 6;

impl SettingsWindow {
    /// The whole page: the mode it paints in, then the colours it mixes and the
    /// size it reads at.
    /// Typography is a group here rather than a section of its own — a size is
    /// a question about appearance.
    pub(super) fn appearance_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_col()
            .gap(px(settings::GROUP_GAP))
            .child(
                theme
                    .group_box()
                    .child(self.theme_row(cx))
                    .child(self.theme_family_row(Appearance::Light, cx))
                    .child(self.theme_family_row(Appearance::Dark, cx)),
            )
            .child(self.window_group(cx))
            .child(self.colors_group(cx))
            .child(self.typography_group(cx))
            .child(self.families_group(cx))
            .child(self.sidebar_group(cx))
            .child(self.scrollbars_group(cx))
            .children(self.boards_group(cx))
            .into_any_element()
    }

    /// One card row: what the setting is on the left, the control on the right.
    pub(super) fn theme_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let current = cx.appearance_mode();
        theme
            .card_row(true)
            .child(labelled(
                &theme,
                "Theme",
                "Follow the system, or pick one.",
                false,
            ))
            .child(
                // A segmented control rather than a select: three options that
                // all fit are worth showing at once.
                theme
                    .segmented(
                        "appearance",
                        MODES.map(|mode| mode.label()),
                        MODES
                            .iter()
                            .position(|mode| *mode == current)
                            .unwrap_or(usize::MAX),
                        cx.listener(|this, ix: &usize, _, cx| {
                            this.workspace.update(cx, |workspace, cx| {
                                workspace.set_appearance(MODES[*ix], cx)
                            });
                            cx.notify();
                        }),
                    )
                    .flex_none(),
            )
    }

    /// The bundled theme families with a variant for `appearance`, `Default`
    /// first for bezel's own palette.
    pub(super) fn theme_family_picker(
        appearance: Appearance,
        current: Option<&'static ThemeFamily>,
        cx: &mut Context<Self>,
    ) -> Entity<Combobox> {
        let families: Vec<&'static ThemeFamily> = themes::all()
            .iter()
            .filter(|family| family.variant(appearance).is_some())
            .collect();
        let items = std::iter::once(DEFAULT_THEME.into())
            .chain(families.iter().map(|family| family.name.clone().into()))
            .collect();
        let selected = current
            .and_then(|current| {
                families
                    .iter()
                    .position(|family| family.name == current.name)
            })
            .map_or(0, |ix| ix + 1);
        let shipped = Theme::for_appearance(appearance);
        let faces: Vec<(Hsla, Hsla)> = std::iter::once((shipped.bg, shipped.text.alpha(1.)))
            .chain(families.iter().filter_map(|family| {
                let theme = family.theme(appearance)?;
                Some((theme.bg, theme.accent))
            }))
            .collect();
        let combobox = cx.new(|cx| {
            Combobox::new(items, DEFAULT_THEME, cx)
                .with_selection(selected)
                .with_leading(move |item, theme| {
                    let (bg, ink) = faces.get(item).copied().unwrap_or(faces[0]);
                    theme_swatch(theme, bg, ink)
                })
        });
        cx.subscribe(&combobox, move |this: &mut Self, _, event, cx| {
            let ComboboxEvent::Selected(item) = event;
            let name = item
                .checked_sub(1)
                .and_then(|ix| families.get(ix))
                .map(|family| family.name.clone());
            this.workspace.update(cx, |workspace, cx| {
                workspace.set_theme(appearance, name, cx)
            });
            cx.notify();
        })
        .detach();
        combobox
    }

    fn theme_family_row(
        &self,
        appearance: Appearance,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let (title, picker) = match appearance {
            Appearance::Light => ("Light theme", self.light_theme.clone()),
            Appearance::Dark => ("Dark theme", self.dark_theme.clone()),
        };
        theme
            .card_row(false)
            .child(theme.row_title(title).flex_1())
            .child(div().flex_none().w(px(220.)).child(picker))
    }

    /// What the greys are mixed from, and whether they are see-through.
    fn colors_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        // The tint reaches only an appearance painted in bezel's own palette.
        let tinted = {
            let workspace = self.workspace.read(cx);
            [Appearance::Light, Appearance::Dark]
                .into_iter()
                .any(|appearance| workspace.theme(appearance).is_none())
        };
        group(&theme, "Colors")
            .child(
                theme
                    .group_box()
                    .children(self.vibrancy_row(cx))
                    .children(self.blur_row(cx))
                    .when(tinted, |group| {
                        group.child(self.hue_row(cx)).child(self.intensity_row(cx))
                    }),
            )
            .into_any_element()
    }

    fn scrollbars_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        group(&theme, "Scrollbars")
            .child(
                theme
                    .group_box()
                    .child(self.scrollbars_row(true, cx))
                    .child(self.scrollbars_row(false, cx)),
            )
            .into_any_element()
    }

    fn scrollbars_row(&self, sidebar: bool, cx: &mut Context<Self>) -> AnyElement {
        use crate::model::settings::Scrollbars;
        let theme = Theme::of(cx).clone();
        let look = self.workspace.read(cx).settings.appearance.clone();
        let current = if sidebar {
            look.sidebar_scrollbars
        } else {
            look.scrollbars
        };
        theme
            .card_row(sidebar)
            .child(
                theme
                    .row_title(if sidebar {
                        "Sidebar scrollbars"
                    } else {
                        "Content scrollbars"
                    })
                    .flex_1(),
            )
            .child(
                theme
                    .segmented(
                        if sidebar {
                            "sidebar-scrollbars"
                        } else {
                            "content-scrollbars"
                        },
                        Scrollbars::ALL.map(|value| value.label()),
                        Scrollbars::ALL
                            .iter()
                            .position(|value| *value == current)
                            .unwrap_or(usize::MAX),
                        cx.listener(move |this, ix: &usize, _, cx| {
                            this.workspace.update(cx, |workspace, cx| {
                                workspace.set_scrollbars(Scrollbars::ALL[*ix], sidebar, cx)
                            });
                            cx.notify();
                        }),
                    )
                    .flex_none(),
            )
            .into_any_element()
    }

    fn window_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let picker = kit::layout::picker("window-layout", &theme, &self.workspace, cx);
        group(&theme, "Window")
            .child(
                theme
                    .group_box()
                    .child(
                        theme
                            .card_row(true)
                            .child(labelled(
                                &theme,
                                "Layout",
                                "Flush with the window, or on a card over it.",
                                false,
                            ))
                            .child(picker),
                    )
                    .children(self.content_opacity_row(cx)),
            )
            .into_any_element()
    }

    /// How much of the window shows through the floating card. Shown with
    /// [`Self::vibrancy_row`], and only while the content floats.
    fn content_opacity_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::model::settings::CONTENT_OPACITY;
        let look = &self.workspace.read(cx).settings.appearance;
        if !(look.floating && Theme::of(cx).vibrancy) {
            return None;
        }
        let (min, max) = CONTENT_OPACITY;
        let opacity = look.content_opacity;
        // Right is more see-through, as the window's own transparency runs.
        Some(self.tint_row(
            false,
            "content-opacity",
            "Card transparency",
            "How much of the window shows through the card.",
            (max - opacity) / (max - min),
            move |workspace, fraction, cx| {
                workspace.set_content_opacity(max - fraction * (max - min), cx);
            },
            cx,
        ))
    }

    fn sidebar_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let on = workspace.indent_project_rows;
        let fits = workspace.settings.appearance.settings_sidebar_fits;
        let extended = workspace.settings.appearance.extended_sidebar;
        let lights = workspace.settings.appearance.traffic_lights;
        group(&theme, "Sidebar")
            .child(
                theme
                    .group_box()
                    .child(
                        self.switch_row(
                            Switch::new(
                                "indent-project-rows",
                                "Indent sidebar rows",
                                "Inset items below each project and space heading by one icon width.",
                                on,
                            )
                            .first(true),
                            cx,
                            move |this, cx| {
                                this.workspace.update(cx, |workspace, cx| {
                                    workspace.set_indent_project_rows(!on, cx);
                                });
                            },
                        ),
                    )
                    .child(self.switch_row(
                        Switch::new(
                            "extended-sidebar",
                            "Extended sidebar",
                            "Show each item's preview, labels and when it was last changed.",
                            extended,
                        ),
                        cx,
                        move |this, cx| {
                            this.workspace.update(cx, |workspace, cx| {
                                workspace.set_extended_sidebar(!extended, cx);
                            });
                        },
                    ))
                    .child(self.switch_row(
                        Switch::new(
                            "settings-sidebar-fits",
                            "Fit settings sidebar",
                            "Size this window's sidebar to its widest section.",
                            fits,
                        ),
                        cx,
                        move |this, cx| {
                            this.workspace.update(cx, |workspace, cx| {
                                workspace.set_settings_sidebar_fits(!fits, cx);
                            });
                        },
                    ))
                    .when(!cfg!(target_os = "macos"), |group| {
                        group.child(self.switch_row(
                            Switch::new(
                                "traffic-lights",
                                "Traffic light window buttons",
                                "Close, minimise and maximise as coloured dots on the left.",
                                lights,
                            ),
                            cx,
                            move |this, cx| {
                                this.workspace.update(cx, |workspace, cx| {
                                    workspace.set_traffic_lights(!lights, cx);
                                });
                            },
                        ))
                    }),
            )
            .into_any_element()
    }

    /// How a new board is laid out. The pill at the foot of a board is what
    /// moves one already made — every board carries its own answer, so this
    /// only says what a board starts as.
    ///
    /// Nothing at all with boards switched off: a default for a pane that
    /// cannot be reached is a switch that does nothing.
    fn boards_group(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        if !workspace.settings.features.boards {
            return None;
        }
        let on = workspace.board_view == View::List;
        Some(
            group(&theme, "Boards")
                .child(
                    theme.group_box().child(
                        self.switch_row(
                            Switch::new(
                                "board-view",
                                "New boards in list view",
                                "Start a board as one list down rather than lanes across.",
                                on,
                            )
                            .first(true),
                            cx,
                            move |this, cx| {
                                let view = match on {
                                    true => View::Lanes,
                                    false => View::List,
                                };
                                this.workspace.update(cx, |workspace, cx| {
                                    workspace.set_default_board_view(view, cx);
                                });
                            },
                        ),
                    ),
                )
                .into_any_element(),
        )
    }

    /// The Editor section. The Cursor group's rows reach every text field, not
    /// the editor alone.
    pub(super) fn documents_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let boxed = |label: &'static str, rows: Vec<AnyElement>| {
            group(&theme, label).child(theme.group_box().children(rows))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(settings::GROUP_GAP))
            .child(boxed("Cursor", {
                let block = self.workspace.read(cx).settings.appearance.caret_shape
                    == crate::model::settings::CaretShape::Block;
                let mut rows = vec![self.cursor_row(cx), self.caret_shape_row(cx)];
                if block {
                    rows.extend([self.caret_height_row(cx), self.hollow_caret_row(cx)]);
                }
                rows.extend([self.caret_row(cx), self.terminal_caret_row(cx)]);
                rows
            }))
            .child(boxed(
                "Layout",
                vec![
                    self.pages_row(cx),
                    self.outline_row(cx),
                    self.comments_row(cx),
                    self.wrap_row(cx),
                ],
            ))
            .child(boxed(
                "Colours",
                vec![
                    self.highlight_row(cx),
                    self.selection_row(cx),
                    self.find_row(cx),
                    self.comment_row(cx),
                ],
            ))
            .child(boxed(
                "Pasting",
                vec![self.source_paste_row(cx), self.download_web_row(cx)],
            ))
            .child(boxed("Article session", self.article_session_rows(cx)))
            .into_any_element()
    }

    /// The article default: the agent an article's session opens on, and the
    /// mode and model it opens with. The same value an article's composer sets
    /// before its first send. Mode and model are read from the project this
    /// window is on, once its agent has connected there.
    fn article_session_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        use crate::model::session_preferences;
        use cacp::schema::{SessionConfigKind, SessionConfigOptionCategory};
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let Some(cwd) = workspace
            .active_project()
            .map(|project| project.path.clone())
        else {
            return vec![
                theme
                    .card_row(false)
                    .child(labelled(
                        &theme,
                        "Article agent",
                        "Open a project to choose the agent and its model.",
                        false,
                    ))
                    .into_any_element(),
            ];
        };
        let Some((agent, offers)) = workspace.article_default_offers(&cwd, cx) else {
            return vec![
                theme
                    .card_row(false)
                    .child(labelled(
                        &theme,
                        "Article agent",
                        "Add an agent to choose one.",
                        false,
                    ))
                    .into_any_element(),
            ];
        };
        let key = session_preferences::agent_key(&agent);
        let agents: Vec<(SharedString, ArticleChange)> = workspace
            .settings
            .agents
            .iter()
            .enumerate()
            .map(|(ix, entry)| (entry.name.clone().into(), ArticleChange::Agent(ix)))
            .collect();
        let current = workspace
            .settings
            .agents
            .iter()
            .position(|entry| session_preferences::agent_key(entry) == key);
        let mut rows = vec![self.article_choice_row(
            "article-agent",
            "Article agent",
            "The agent an article's session opens on.",
            agents,
            current,
            cwd.clone(),
            cx,
        )];
        if let Some(modes) = offers
            .modes
            .as_ref()
            .filter(|modes| !modes.available_modes.is_empty())
        {
            let options: Vec<(SharedString, ArticleChange)> = modes
                .available_modes
                .iter()
                .map(|mode| {
                    (
                        mode.name.clone().into(),
                        ArticleChange::Mode(mode.id.to_string()),
                    )
                })
                .collect();
            let current = modes
                .available_modes
                .iter()
                .position(|mode| mode.id == modes.current_mode_id);
            rows.push(self.article_choice_row(
                "article-mode",
                "Mode",
                "The mode an article's session opens in.",
                options,
                current,
                cwd.clone(),
                cx,
            ));
        }
        let model = offers.config.iter().find_map(|option| {
            let SessionConfigKind::Select(select) = &option.kind else {
                return None;
            };
            (option.category == Some(SessionConfigOptionCategory::Model))
                .then_some((option, select))
        });
        if let Some((option, select)) = model {
            let values = session_preferences::select_values(&select.options);
            let options: Vec<(SharedString, ArticleChange)> = values
                .iter()
                .map(|value| {
                    (
                        value.name.clone().into(),
                        ArticleChange::Config {
                            id: option.id.to_string(),
                            value: value.value.to_string(),
                        },
                    )
                })
                .collect();
            let current = values
                .iter()
                .position(|value| value.value == select.current_value);
            rows.push(self.article_choice_row(
                "article-model",
                option.name.clone(),
                "The model an article's session opens on.",
                options,
                current,
                cwd,
                cx,
            ));
        }
        rows
    }

    /// A row that picks one of `options` from a menu on its trigger, each
    /// option being the change it makes to the article default.
    #[allow(clippy::too_many_arguments)]
    fn article_choice_row(
        &self,
        id: &'static str,
        title: impl Into<SharedString>,
        blurb: &'static str,
        options: Vec<(SharedString, ArticleChange)>,
        current: Option<usize>,
        cwd: PathBuf,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use bezel::ui::popover;
        let theme = Theme::of(cx).clone();
        let label = current
            .and_then(|ix| options.get(ix))
            .map(|(name, _)| name.clone())
            .unwrap_or_default();
        let trigger = popover::menu_trigger_matching(
            theme
                .select_trigger_with(None::<Empty>, label)
                .gap(px(8.))
                .id(id)
                .relative(),
            |this: &mut Self| &mut this.picker,
            move |open| *open == id,
            move |_| id,
            cx,
        );
        let card = (self.picker.get() == Some(&id)).then(|| {
            let rows = options.into_iter().enumerate().map(|(ix, (name, change))| {
                let cwd = cwd.clone();
                popover::menu_row(&theme, current == Some(ix), None)
                    .id((id, ix))
                    .cursor_pointer()
                    .hover(|row| row.bg(theme.element_hover))
                    .child(name)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.workspace.update(cx, |workspace, cx| {
                            workspace.set_article_default(&cwd, change.clone(), cx)
                        });
                        popover::close_popup(this, cx, |this: &mut Self| &mut this.picker);
                    }))
            });
            popover::anchored_menu_below_end(
                id,
                popover::dismiss_on_out(
                    popover::popover_card(&theme)
                        .flex()
                        .flex_col()
                        .children(rows),
                    |this: &mut Self| &mut this.picker,
                    cx,
                )
                .into_any_element(),
                self.picker.closing_since(),
            )
        });
        theme
            .card_row(false)
            .child(labelled(&theme, title, blurb, false))
            .child(trigger.children(card))
            .into_any_element()
    }

    /// What a line too long for a code block does.
    ///
    /// Every document at once, an article's fences and a transcript's alike:
    /// the renderer takes one answer for the app. Off is what an editor
    /// usually does, and the cost of it here is that nothing scrolls a fence
    /// back to a caret typed off its right edge — the page follows the caret
    /// down, but a block's own sideways scroll is the reader's to drag.
    pub(super) fn outline_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).settings.appearance.outline;
        self.switch_row(
            Switch::new(
                "article-outline",
                "Article outline",
                "A floating button over an article that lists its headings.",
                on,
            ),
            cx,
            move |this, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_outline(!on, cx));
            },
        )
    }

    pub(super) fn comments_row(&self, cx: &mut Context<Self>) -> AnyElement {
        use crate::model::settings::CommentDisplay as Comments;
        let theme = Theme::of(cx).clone();
        let current = self.workspace.read(cx).settings.appearance.comments;
        theme
            .card_row(false)
            .child(
                labelled(&theme, "Comments", "Notes beside an article's lines, or a mark to open each one, for an article without its own choice. A narrow pane shows marks.", false),
            )
            .child(
                theme
                    .segmented(
                        "article-comments",
                        Comments::ALL.map(|value| value.label()),
                        Comments::ALL
                            .iter()
                            .position(|value| *value == current)
                            .unwrap_or(usize::MAX),
                        cx.listener(|this, ix: &usize, _, cx| {
                            this.workspace.update(cx, |workspace, cx| {
                                workspace.set_comments(Comments::ALL[*ix], cx)
                            });
                            cx.notify();
                        }),
                    )
                    .flex_none(),
            )
            .into_any_element()
    }

    pub(super) fn wrap_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).wrap_code;
        self.switch_row(
            Switch::new(
                "wrap-code",
                "Wrap long lines in code",
                "Off scrolls a long line sideways inside the block instead.",
                on,
            ),
            cx,
            move |this, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_wrap_code(!on, cx));
            },
        )
    }

    /// Whether plain-text mode takes a pasted picture as an image line.
    fn source_paste_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).settings.paste_images_in_source;
        self.switch_row(
            Switch::new(
                "paste-images-in-source",
                "Paste pictures in plain text",
                "A pasted picture, file or picture link goes in as an image line. Off pastes text.",
                on,
            )
            .first(true),
            cx,
            move |this, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.set_paste_images_in_source(!on, cx)
                });
            },
        )
    }

    /// Whether a picture's pasted web address is downloaded into the article.
    fn download_web_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).settings.download_web_images;
        self.switch_row(
            Switch::new(
                "download-web-images",
                "Download web pictures",
                "Save a copy of a pasted picture link into the article. Off keeps the web address.",
                on,
            ),
            cx,
            move |this, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.set_download_web_images(!on, cx)
                });
            },
        )
    }

    /// The colour behind `==text==`.
    pub(super) fn highlight_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.workspace.read(cx).settings.appearance.highlight;
        let paints = Highlight::ALL.into_iter().map(Paint::Named).collect();
        self.color_row(
            true,
            "highlight-color",
            "Highlight colour",
            Some(current),
            paints,
            None,
            cx,
            |workspace, value, cx| {
                if let Some(value) = value {
                    workspace.set_highlight(value, cx);
                }
            },
        )
    }

    /// The colour behind selected text, or the palette's own.
    fn selection_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let current = self.workspace.read(cx).settings.appearance.selection;
        let system = Theme::for_appearance(theme.appearance).selection;
        self.preset_row(
            "selection-color",
            "Selection colour",
            current,
            system,
            cx,
            |workspace, value, cx| workspace.set_selection(value, cx),
        )
    }

    fn caret_shape_row(&self, cx: &mut Context<Self>) -> AnyElement {
        use crate::model::settings::CaretShape;
        use bezel::ui::popover;
        const ID: &str = "caret-shape";
        let theme = Theme::of(cx).clone();
        let appearance = &self.workspace.read(cx).settings.appearance;
        let current = appearance.caret_shape;
        let caret = appearance
            .caret
            .map_or(theme.caret, |paint| paint.solid(&theme));
        let trigger = popover::menu_trigger_matching(
            theme
                .select_trigger_with(
                    Some(div().text_color(caret).child(current.glyph())),
                    current.label(),
                )
                .gap(px(8.))
                .id(ID)
                .relative(),
            |this: &mut Self| &mut this.picker,
            |open| *open == ID,
            |_| ID,
            cx,
        );
        let card = (self.picker.get() == Some(&ID)).then(|| {
            let rows = CaretShape::ALL.into_iter().enumerate().map(|(ix, shape)| {
                popover::menu_row(&theme, shape == current, None)
                    .id((ID, ix))
                    .cursor_pointer()
                    .hover(|row| row.bg(theme.element_hover))
                    .child(div().text_color(caret).child(shape.glyph()))
                    .child(shape.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.workspace
                            .update(cx, |workspace, cx| workspace.set_caret_shape(shape, cx));
                        popover::close_popup(this, cx, |this: &mut Self| &mut this.picker);
                    }))
            });
            popover::anchored_menu_below_end(
                "caret-shape-menu",
                popover::dismiss_on_out(
                    popover::popover_card(&theme)
                        .flex()
                        .flex_col()
                        .children(rows),
                    |this: &mut Self| &mut this.picker,
                    cx,
                )
                .into_any_element(),
                self.picker.closing_since(),
            )
        });
        theme
            .card_row(false)
            .child(theme.row_title("Cursor shape").flex_1())
            .child(trigger.children(card))
            .into_any_element()
    }

    /// Whether a block caret fills its line or stands as tall as the text.
    fn caret_height_row(&self, cx: &mut Context<Self>) -> AnyElement {
        use crate::model::settings::CaretHeight;
        let on = self.workspace.read(cx).settings.appearance.caret_height == CaretHeight::Line;
        self.switch_row(
            Switch::new(
                "caret-height",
                "Fill the line",
                "Off sizes a block cursor to the text instead of its line.",
                on,
            ),
            cx,
            move |this, cx| {
                let height = match on {
                    true => CaretHeight::Text,
                    false => CaretHeight::Line,
                };
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_caret_height(height, cx));
            },
        )
    }

    /// Whether a block caret shows outlined in an inactive window.
    fn hollow_caret_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).settings.appearance.hollow_caret;
        self.switch_row(
            Switch::new(
                "hollow-caret",
                "Outline when inactive",
                "Off hides a block cursor while the window is in the background.",
                on,
            ),
            cx,
            move |this, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_hollow_caret(!on, cx));
            },
        )
    }

    fn terminal_caret_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).settings.appearance.terminal_caret;
        self.switch_row(
            Switch::new(
                "terminal-caret",
                "Use in terminals",
                "Off lets the running program choose the terminal cursor.",
                on,
            ),
            cx,
            move |this, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_terminal_caret(!on, cx));
            },
        )
    }

    /// The caret's colour, or the palette's own.
    fn caret_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let current = self.workspace.read(cx).settings.appearance.caret;
        let system = Theme::for_appearance(theme.appearance).caret;
        self.preset_row(
            "caret-color",
            "Cursor colour",
            current,
            system,
            cx,
            |workspace, value, cx| workspace.set_caret(value, cx),
        )
    }

    /// The colour behind find matches, or the accent.
    fn find_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let current = self.workspace.read(cx).settings.appearance.search;
        let system = markdown::default_find(&theme).1;
        self.preset_row(
            "search-color",
            "Search results colour",
            current,
            system,
            cx,
            |workspace, value, cx| workspace.set_search(value, cx),
        )
    }

    /// The colour behind commented words, or the default.
    fn comment_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let current = self.workspace.read(cx).settings.appearance.comment_color;
        let system = markdown::default_comment(&theme).color;
        self.preset_row(
            "comment-color",
            "Comments colour",
            current,
            system,
            cx,
            |workspace, value, cx| workspace.set_comment_color(value, cx),
        )
    }

    /// bezel's preset colours in rows of six, the colour picker, and Default
    /// under them. A colour written by hand that is not among them rings
    /// nothing.
    fn preset_row(
        &self,
        id: &'static str,
        title: &'static str,
        current: Option<Paint>,
        system: bezel::gpui::Hsla,
        cx: &mut Context<Self>,
        set: fn(&mut Workspace, Option<Paint>, &mut Context<Workspace>),
    ) -> AnyElement {
        let paints = Highlight::ALL.into_iter().map(Paint::Named).collect();
        self.color_row(false, id, title, current, paints, Some(system), cx, set)
    }

    /// A row showing one colour, which opens `paints` and a colour picker
    /// under it to pick another. With `system`, the popover ends
    /// in a Default item that sets `None`, and `None` shows `system`.
    #[allow(clippy::too_many_arguments)]
    fn color_row(
        &self,
        first: bool,
        id: &'static str,
        title: &'static str,
        current: Option<Paint>,
        paints: Vec<Paint>,
        system: Option<bezel::gpui::Hsla>,
        cx: &mut Context<Self>,
        set: fn(&mut Workspace, Option<Paint>, &mut Context<Workspace>),
    ) -> AnyElement {
        use bezel::ui::color::{ColorPicker, ColorPickerEvent};
        use bezel::ui::popover;
        let theme = Theme::of(cx).clone();
        let shown = current
            .map(|paint| paint.solid(&theme))
            .or(system)
            .unwrap_or_default();
        let selected =
            current.and_then(|current| paints.iter().position(|paint| *paint == current));
        // Down lands before the trigger's click opens the card, so the picker
        // is in place, at the colour shown, by the card's first frame.
        let ready = cx.listener(move |this, _: &bezel::gpui::MouseDownEvent, _, cx| {
            if let Some(custom) = this.custom.as_ref().filter(|custom| custom.id == id) {
                custom
                    .picker
                    .update(cx, |picker, cx| picker.set_color(shown, cx));
                return;
            }
            let picker = cx.new(|cx| ColorPicker::new(shown, false, cx));
            let changed = cx.subscribe(&picker, move |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Changed(color) = *event;
                let paint = Paint::from_hsla(color);
                this.workspace
                    .update(cx, |workspace, cx| set(workspace, Some(paint), cx));
            });
            this.custom = Some(settings::CustomColor {
                id,
                picker,
                _changed: changed,
            });
        });
        let well = popover::menu_trigger_matching(
            theme
                .color_well(shown)
                .id(id)
                .relative()
                .on_mouse_down(bezel::gpui::MouseButton::Left, ready),
            |this: &mut Self| &mut this.picker,
            move |open| *open == id,
            move |_| id,
            cx,
        );
        let card = (self.picker.get() == Some(&id)).then(|| {
            let swatches: Vec<Swatch> = paints
                .iter()
                .map(|paint| Swatch::fixed(paint.key(), paint.solid(&theme)))
                .collect();
            let presets =
                theme.swatch_picker((id, 0usize), &swatches, selected, Some(SWATCH_COLUMNS), {
                    let paints = paints.clone();
                    cx.listener(move |this, ix: &usize, _, cx| {
                        let paint = paints[*ix];
                        this.workspace
                            .update(cx, |workspace, cx| set(workspace, Some(paint), cx));
                        if let Some(custom) = this.custom.as_ref().filter(|custom| custom.id == id)
                        {
                            let color = paint.solid(Theme::of(cx));
                            custom
                                .picker
                                .update(cx, |picker, cx| picker.set_color(color, cx));
                        }
                        popover::close_popup(this, cx, |this: &mut Self| &mut this.picker);
                    })
                });
            let custom = self
                .custom
                .as_ref()
                .filter(|custom| custom.id == id)
                .map(|custom| custom.picker.clone());
            let reset = system.map(|_| {
                popover::menu_row(&theme, current.is_none(), None)
                    .id((id, 1usize))
                    .cursor_pointer()
                    .hover(|row| row.bg(theme.element_hover))
                    .child("Default")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.workspace
                            .update(cx, |workspace, cx| set(workspace, None, cx));
                        popover::close_popup(this, cx, |this: &mut Self| &mut this.picker);
                    }))
            });
            popover::anchored_menu_below_end(
                bezel::gpui::SharedString::from(format!("{id}-swatches")),
                popover::dismiss_on_out(
                    popover::popover_card(&theme)
                        .flex()
                        .flex_col()
                        .child(
                            // As wide as the preset grid; the picker fills it.
                            div()
                                .p(px(popover::MENU_ROW_INSET))
                                .flex()
                                .flex_col()
                                .gap(px(popover::MENU_ROW_INSET))
                                .child(presets)
                                .children(custom),
                        )
                        .children(reset),
                    |this: &mut Self| &mut this.picker,
                    cx,
                )
                .into_any_element(),
                self.picker.closing_since(),
            )
        });
        theme
            .card_row(first)
            .child(theme.row_title(title).flex_1())
            .child(well.children(card))
            .into_any_element()
    }

    /// How wide a page is set when it has not been told otherwise.
    ///
    /// The default alone. A page's own `···` menu writes the measure into its
    /// `properties.toml`, and one written down there is the document's — it
    /// stays what its author made it whatever this switch says, which is what
    /// the menu's Use default width hands back.
    pub(super) fn pages_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).wide_pages;
        self.switch_row(
            Switch::new(
                "wide-pages",
                "Full width pages",
                "Set articles across the pane rather than in a reading column.",
                on,
            )
            .first(true),
            cx,
            move |this, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_wide_pages(!on, cx));
            },
        )
    }

    /// Whether the caret blinks. bezel holds the caret, so the switch sets it
    /// there rather than keeping a second copy of the answer.
    pub(super) fn cursor_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).cursor_blink;
        self.switch_row(
            Switch::new(
                "cursor-blink",
                "Blink the cursor",
                "Off holds the text caret lit while it has focus.",
                on,
            )
            .first(true),
            cx,
            move |this, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_cursor_blink(!on, cx));
            },
        )
    }

    /// The hue every grey carries, and how much of it. Two rows because they
    /// are two questions: a hue nobody can see is still the hue that returns
    /// when the intensity comes back up.
    pub(super) fn hue_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let tint = self.workspace.read(cx).tint;
        // First in its group where the frost rows are not shown.
        self.tint_row(
            !Theme::of(cx).vibrancy,
            "hue",
            "Hue",
            "Which hue the greys are mixed from.",
            tint.hue / HUE_MAX,
            move |workspace, fraction, cx| {
                let tint = Tint::new(fraction * HUE_MAX, workspace.tint.chroma);
                workspace.set_tint(tint, cx);
            },
            cx,
        )
    }

    /// How much of the desktop shows through. Nothing where the window is not
    /// translucent — opaque, or an appearance the palette keeps opaque.
    fn vibrancy_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::model::settings::VIBRANCY;
        if !Theme::of(cx).vibrancy {
            return None;
        }
        let (min, max) = VIBRANCY;
        let alpha = self.workspace.read(cx).settings.appearance.vibrancy;
        // Right is more see-through, so the slider runs against the alpha.
        Some(self.tint_row(
            true,
            "vibrancy",
            "Transparency",
            "How much of what is behind the window shows through.",
            (max - alpha) / (max - min),
            move |workspace, fraction, cx| {
                workspace.set_vibrancy(max - fraction * (max - min), cx);
            },
            cx,
        ))
    }

    /// How far what is behind the window is blurred. Shown with
    /// [`Self::vibrancy_row`].
    fn blur_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::model::settings::BLUR;
        if !Theme::of(cx).vibrancy {
            return None;
        }
        let (min, max) = BLUR;
        let blur = self.workspace.read(cx).settings.appearance.blur;
        Some(self.tint_row(
            false,
            "blur",
            "Blur",
            "How much what is behind the window is blurred.",
            (blur - min) / (max - min),
            move |workspace, fraction, cx| {
                workspace.set_blur(min + fraction * (max - min), cx);
            },
            cx,
        ))
    }

    pub(super) fn intensity_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let tint = self.workspace.read(cx).tint;
        self.tint_row(
            false,
            "intensity",
            "Intensity",
            "How much of that hue they carry. None is the shipped neutral.",
            tint.chroma / CHROMA_MAX,
            move |workspace, fraction, cx| {
                let tint = Tint::new(workspace.tint.hue, fraction * CHROMA_MAX);
                workspace.set_tint(tint, cx);
            },
            cx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn tint_row(
        &self,
        first: bool,
        id: &'static str,
        title: &'static str,
        note: &'static str,
        fraction: f32,
        set: impl Fn(&mut Workspace, f32, &mut Context<Workspace>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        theme
            .card_row(first)
            .child(labelled(&theme, title, note, false))
            .child(
                div()
                    .id(id)
                    .flex_none()
                    .w(px(SLIDER_WIDTH))
                    .child(theme.slider(fraction))
                    .on_drag(SliderDrag(id.into()), |_, _, _, cx| cx.new(|_| Empty))
                    .on_drag_move(cx.listener(
                        move |this, event: &DragMoveEvent<SliderDrag>, _, cx| {
                            let Some(fraction) = widgets::slider_fraction(event, id, cx) else {
                                return;
                            };
                            this.workspace
                                .update(cx, |workspace, cx| set(workspace, fraction, cx));
                            cx.notify();
                        },
                    )),
            )
            .into_any_element()
    }
}

/// A theme family's face in the picker: "Aa" in its accent on its background,
/// as the variant for the appearance being painted.
fn theme_swatch(theme: &Theme, bg: Hsla, ink: Hsla) -> AnyElement {
    div()
        .flex_none()
        .size(px(20.))
        .rounded_full()
        .border_1()
        .border_color(theme.border)
        .bg(bg)
        .flex()
        .items_center()
        .justify_center()
        .text_style(TextStyle::Caption2)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(ink)
        .child("Aa")
        .into_any_element()
}
