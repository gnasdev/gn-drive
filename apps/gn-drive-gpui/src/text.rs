//! Text that stays inside its column.
//!
//! GPUI draws a string as one line unless the element asks for normal
//! whitespace, so a long sentence paints past the card. A label that should
//! stay one line truncates, and the tooltip keeps the full string.

use ely_gpui_component::primitives::Tooltip;
use gpui::{ElementId, SharedString, Stateful, div, prelude::*};
use ns_ui::theme;

/// Break opportunities in a path or URL so a wrapped line can split.
fn soft_breaks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        out.push(ch);
        if matches!(ch, '/' | '\\' | '?' | '&' | '=' | '-' | '_' | '.') {
            out.push('\u{200b}');
        }
    }
    out
}

/// A sentence that wraps inside the parent's width.
pub(crate) fn wrap(text: impl Into<SharedString>) -> gpui::Div {
    let text = text.into();
    div()
        .w_full()
        .min_w_0()
        .whitespace_normal()
        .child(SharedString::from(soft_breaks(&text)))
}

pub(crate) fn wrap_muted(text: impl Into<SharedString>) -> gpui::Div {
    wrap(text).text_xs().text_color(theme().muted)
}

pub(crate) fn wrap_err(text: impl Into<SharedString>) -> gpui::Div {
    wrap(text).text_xs().text_color(theme().err)
}

/// One line. The end becomes an ellipsis, and a hover shows the whole string.
pub(crate) fn clip(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<gpui::Div> {
    let text = text.into();
    clip_tip(id, text.clone(), text)
}

/// One line of `shown`, with `full` in the tooltip.
pub(crate) fn clip_tip(
    id: impl Into<ElementId>,
    shown: impl Into<SharedString>,
    full: impl Into<SharedString>,
) -> Stateful<gpui::Div> {
    div()
        .id(id.into())
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .truncate()
        .tooltip(Tooltip::text(full.into()))
        .child(shown.into())
}

/// A wrapped error, with the longer raw text on hover.
pub(crate) fn wrap_err_tip(
    id: impl Into<ElementId>,
    shown: impl Into<SharedString>,
    full: impl Into<SharedString>,
) -> Stateful<gpui::Div> {
    wrap_err(shown)
        .id(id.into())
        .tooltip(Tooltip::text(full.into()))
}
