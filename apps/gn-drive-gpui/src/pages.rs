//! Cloud accounts and Settings, drawn as pages in the editor.
//!
//! The chrome title is the page name. This module adds the 44px band under it
//! (title and one sentence) and a centered column. The column scrolls; the
//! band and the pinned controls stay put. A narrow window already puts a back
//! arrow on the chrome title, which returns to the flow list.

use ely_gpui_component::layout::ScrollArea;
use gpui::{AnyElement, Context, IntoElement, Window, div, prelude::*, px};
use ns_ui::theme;

use crate::{GnDriveWindow, Panel};

const PAGE_W: f32 = 760.;

pub(crate) fn open_page(
    this: &mut GnDriveWindow,
    _window: &mut Window,
    cx: &mut Context<GnDriveWindow>,
) -> Option<AnyElement> {
    let (title, description, pinned, body) = match this.panel {
        Panel::Closed => return None,
        Panel::Remotes => (
            "Cloud accounts",
            "Accounts your flows can sync with, such as Google Drive or a NAS.",
            crate::remotes::pin_add(this).then(|| crate::remotes::pinned(cx)),
            crate::remotes::body(this, cx),
        ),
        Panel::Settings => (
            "Settings",
            "Preferences, the background service, past runs, and logs.",
            Some(crate::settings::tabs(this, cx)),
            crate::settings::body(this, cx),
        ),
    };
    Some(shell(
        title,
        description,
        pinned,
        body,
        #[cfg(feature = "devtools")]
        &this.perf_list_scroll,
    ))
}

fn shell(
    title: &'static str,
    description: &'static str,
    pinned: Option<AnyElement>,
    body: AnyElement,
    #[cfg(feature = "devtools")] perf_scroll: &gpui::ScrollHandle,
) -> AnyElement {
    let theme = theme();
    let mut column = div()
        .w_full()
        .max_w(px(PAGE_W))
        .h_full()
        .min_w(px(0.))
        .min_h(px(0.))
        .flex()
        .flex_col();
    if let Some(pinned) = pinned {
        column = column.child(
            div()
                .w_full()
                .min_w(px(0.))
                .flex_shrink_0()
                .pt_3()
                .child(pinned),
        );
    }
    let scroll = ScrollArea::new("page-scroll").vertical();
    #[cfg(feature = "devtools")]
    let scroll = if crate::perf::running() {
        scroll.track_scroll(perf_scroll)
    } else {
        scroll
    };
    column = column.child(
        scroll
            .w_full()
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .child(div().w_full().min_w(px(0.)).py_3().child(body)),
    );

    div()
        .size_full()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(theme.editor)
        .child(
            div()
                .h(px(44.))
                .flex_shrink_0()
                .px_4()
                .flex()
                .items_center()
                .gap_3()
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex_shrink_0()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div().flex_1().min_w(px(0.)).child(
                        crate::text::clip(format!("page-desc-{title}"), description)
                            .text_xs()
                            .text_color(theme.muted),
                    ),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h(px(0.))
                .min_w(px(0.))
                .w_full()
                .flex()
                .justify_center()
                .px_4()
                .child(column),
        )
        .into_any_element()
}
