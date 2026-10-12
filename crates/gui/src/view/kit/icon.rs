//! The marks entries wear.

use artifact::kind::Kind;
use bezel::ui::icons::{self, Icon};

/// The mark an entry of `kind` wears wherever it is listed or linked.
pub(crate) fn kind_icon(kind: Kind) -> Icon {
    match kind {
        Kind::Session => icons::social::MessageCircle.into(),
        Kind::Board => icons::development::SquareKanban.into(),
        Kind::Article => icons::files::FileText.into(),
        Kind::Table => icons::files::Table2.into(),
    }
}
