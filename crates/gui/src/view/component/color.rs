//! The card a colour well opens: presets, the picker for anything outside
//! them, and a row that picks none. The host owns the well, whether the card is
//! open, and the picker entity; this draws what is in the card.

use crate::model::settings::Paint;
use bezel::{
    gpui::{Context, Div, Entity, Window, div, prelude::*, px},
    theme::Theme,
    ui::{
        color::{ColorPicker, Swatch},
        popover,
        widgets::Controls as _,
    },
};
use std::rc::Rc;

/// Swatches a row of the card holds.
const SWATCH_COLUMNS: usize = 6;

/// The card for a well showing `current`. `default` names the row that picks
/// `None`, where there is one; `custom` goes under the presets. A pick reports
/// to `on_pick`, which closes the card if it is to close.
#[allow(clippy::too_many_arguments)]
pub(crate) fn card<V: 'static>(
    theme: &Theme,
    id: &'static str,
    paints: Vec<Paint>,
    current: Option<Paint>,
    custom: Option<Entity<ColorPicker>>,
    default: Option<&'static str>,
    on_pick: impl Fn(&mut V, Option<Paint>, &mut Window, &mut Context<V>) + 'static,
    cx: &Context<V>,
) -> Div {
    let on_pick = Rc::new(on_pick);
    let swatches: Vec<Swatch> = paints
        .iter()
        .map(|paint| Swatch::fixed(paint.key(), paint.solid(theme)))
        .collect();
    let selected = current.and_then(|current| paints.iter().position(|paint| *paint == current));
    let presets = theme.swatch_picker(
        (id, 0usize),
        &swatches,
        selected,
        Some(SWATCH_COLUMNS),
        cx.listener({
            let on_pick = on_pick.clone();
            move |this, ix: &usize, window, cx| {
                if let Some(paint) = paints.get(*ix) {
                    on_pick(this, Some(*paint), window, cx);
                }
            }
        }),
    );
    let reset = default.map(|label| {
        popover::menu_row(theme, current.is_none(), None)
            .id((id, 1usize))
            .cursor_pointer()
            .hover(|row| row.bg(theme.element_hover))
            .child(label)
            .on_click(cx.listener(move |this, _, window, cx| on_pick(this, None, window, cx)))
    });
    popover::popover_card(theme)
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
        .children(reset)
}
