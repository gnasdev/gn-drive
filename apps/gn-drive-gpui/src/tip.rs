//! Every clickable control in this window goes through here, so every one has
//! a tooltip that says what it does and, when there is one, its shortcut.
//!
//! The `every_control_has_a_tooltip` test scans the sources and fails when a
//! file outside this one builds a button, switch, select, or segmented control
//! directly, or attaches `.on_click(` to an element that has no tooltip.

use ely_gpui_component::buttons::{Button, ButtonVariant, IconButton};
use ely_gpui_component::forms::Switch;
use ely_gpui_component::primitives::Tooltip;
use ely_gpui_component::theme::ControlSize;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use ns_ui::{IconTone, SelectOption};

/// How a labelled button reads. One primary action per screen; the rest are
/// quiet, and anything that deletes is `Danger`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Look {
    Primary,
    Quiet,
    Outline,
    /// Filled red. Only a confirmation uses this.
    Danger,
    /// Bordered, with danger-colored text. The press still asks before it deletes.
    OutlineDanger,
}

/// `Run now` plus `⌘R` reads "Run now  ⌘R" in the tooltip.
fn meta(shortcut: Option<&str>) -> SharedString {
    shortcut.unwrap_or("").to_string().into()
}

/// A tooltip on any element that already has an id.
pub(crate) fn tip_on<E: StatefulInteractiveElement>(
    element: E,
    text: impl Into<SharedString>,
    shortcut: Option<&str>,
) -> E {
    let text = text.into();
    match shortcut {
        Some(_) => element.tooltip(Tooltip::with_meta(text, meta(shortcut))),
        None => element.tooltip(Tooltip::text(text)),
    }
}

/// Wraps any element in a hover target with a tooltip.
pub(crate) fn with_tip(
    id: impl Into<ElementId>,
    child: impl IntoElement,
    text: impl Into<SharedString>,
    shortcut: Option<&str>,
) -> AnyElement {
    tip_on(div().id((id.into(), "tip")), text, shortcut)
        .child(child)
        .into_any_element()
}

/// A labelled button with a tooltip.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tip_button(
    id: impl Into<ElementId>,
    glyph: &'static str,
    label: impl Into<SharedString>,
    tooltip: impl Into<SharedString>,
    shortcut: Option<&str>,
    look: Look,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let id = id.into();
    let label = label.into();
    let button = match look {
        Look::Primary => ns_ui::button(id.clone(), glyph, label, on_click).into_any_element(),
        Look::Quiet => ns_ui::quiet_button(id.clone(), glyph, label, on_click).into_any_element(),
        Look::Outline => {
            ns_ui::outline_button(id.clone(), glyph, label, on_click).into_any_element()
        }
        Look::Danger => ns_ui::danger_button(id.clone(), glyph, label, on_click).into_any_element(),
        Look::OutlineDanger => {
            return outline_danger(id, glyph, label, tooltip, shortcut, on_click);
        }
    };
    with_tip(id, button, tooltip, shortcut)
}

/// A quiet outline button whose label and icon use the danger color.
fn outline_danger(
    id: impl Into<ElementId>,
    glyph: &'static str,
    label: impl Into<SharedString>,
    tooltip: impl Into<SharedString>,
    shortcut: Option<&str>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let theme = ns_ui::theme();
    let id = id.into();
    tip_on(
        div()
            .id(id)
            .role(Role::Button)
            .tab_index(0)
            .h(px(28.))
            .px_2()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_1p5()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .text_sm()
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(theme.err)
            .cursor_pointer()
            .hover(|style| style.bg(theme.hover)),
        tooltip,
        shortcut,
    )
    .on_click(on_click)
    .child(ns_ui::icon_tinted(glyph, theme.err))
    .child(label.into())
    .into_any_element()
}

fn button_variant(look: Look) -> ButtonVariant {
    match look {
        Look::Primary => ButtonVariant::Primary,
        Look::Quiet => ButtonVariant::Secondary,
        Look::Outline | Look::OutlineDanger => ButtonVariant::Outline,
        Look::Danger => ButtonVariant::Danger,
    }
}

/// A labelled button that is off, with the tooltip that says why.
///
/// Ely ignores presses on a disabled button. The tooltip stays on the wrapper
/// so a hover still explains the lock.
pub(crate) fn tip_button_off(
    id: impl Into<ElementId>,
    glyph: &'static str,
    label: impl Into<SharedString>,
    tooltip: impl Into<SharedString>,
    shortcut: Option<&str>,
    look: Look,
) -> AnyElement {
    labeled(id, glyph, label, tooltip, shortcut, look, false, true, None)
}

/// The same labelled button, stretched to the row. Used when the card is narrow
/// so the primary action is the whole width, not a chip beside helper text.
#[allow(dead_code)]
pub(crate) fn tip_button_wide(
    id: impl Into<ElementId>,
    glyph: &'static str,
    label: impl Into<SharedString>,
    tooltip: impl Into<SharedString>,
    shortcut: Option<&str>,
    look: Look,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    labeled(
        id,
        glyph,
        label,
        tooltip,
        shortcut,
        look,
        true,
        false,
        Some(Box::new(on_click)),
    )
}

/// [`tip_button_off`] at full width.
#[allow(dead_code)]
pub(crate) fn tip_button_off_wide(
    id: impl Into<ElementId>,
    glyph: &'static str,
    label: impl Into<SharedString>,
    tooltip: impl Into<SharedString>,
    shortcut: Option<&str>,
    look: Look,
) -> AnyElement {
    labeled(id, glyph, label, tooltip, shortcut, look, true, true, None)
}

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[allow(clippy::too_many_arguments)]
fn labeled(
    id: impl Into<ElementId>,
    glyph: &'static str,
    label: impl Into<SharedString>,
    tooltip: impl Into<SharedString>,
    shortcut: Option<&str>,
    look: Look,
    wide: bool,
    disabled: bool,
    on_click: Option<ClickHandler>,
) -> AnyElement {
    let id = id.into();
    let mut button = Button::new(id.clone(), label.into())
        .variant(button_variant(look))
        .size(ControlSize::Sm)
        .icon(ns_ui::icon_name(glyph));
    if wide {
        button = button.full_width();
    }
    let button = if disabled {
        button.disabled(true)
    } else if let Some(on_click) = on_click {
        button.on_click(on_click)
    } else {
        button
    };
    let mut host = div().id((id, "tip"));
    if wide {
        host = host.w_full();
    }
    tip_on(host, tooltip, shortcut)
        .child(button)
        .into_any_element()
}

/// An icon button that does not take a press: dimmed, no focus ring, not-allowed cursor.
///
/// Ely still shows the tooltip, so a hover says why it is off.
#[allow(dead_code)]
pub(crate) fn tip_icon_off(
    id: impl Into<ElementId>,
    glyph: &'static str,
    tooltip: impl Into<SharedString>,
) -> AnyElement {
    IconButton::new(id, ns_ui::icon_name(glyph))
        .size(ControlSize::Sm)
        .tooltip(tooltip.into())
        .disabled(true)
        .into_any_element()
}

/// An icon-only button. Ely draws the tooltip; the shortcut rides in the text.
pub(crate) fn tip_icon(
    id: impl Into<ElementId>,
    glyph: &'static str,
    tooltip: impl Into<SharedString>,
    shortcut: Option<&str>,
    tone: IconTone,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let tooltip = tooltip.into();
    let title: SharedString = match shortcut {
        Some(keys) => format!("{tooltip}  {keys}").into(),
        None => tooltip,
    };
    ns_ui::icon_button(id, glyph, title, tone, on_click).into_any_element()
}

/// The medium icon button used for primary sidebar actions.
#[allow(dead_code)]
pub(crate) fn tip_icon_md(
    id: impl Into<ElementId>,
    glyph: &'static str,
    tooltip: impl Into<SharedString>,
    shortcut: Option<&str>,
    tone: IconTone,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let tooltip = tooltip.into();
    let title: SharedString = match shortcut {
        Some(keys) => format!("{tooltip}  {keys}").into(),
        None => tooltip,
    };
    ns_ui::icon_button_md(id, glyph, title, tone, on_click).into_any_element()
}

/// A switch whose tooltip says what turning it on does.
pub(crate) fn tip_switch(
    id: impl Into<ElementId>,
    on: bool,
    disabled: bool,
    tooltip: impl Into<SharedString>,
    on_change: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let id = id.into();
    let switch = Switch::new(id.clone(), on)
        .disabled(disabled)
        .on_change(on_change);
    with_tip(id, switch, tooltip, None)
}

/// A drop-down with a tooltip on the whole control.
pub(crate) fn tip_select(
    id: impl Into<ElementId>,
    options: Vec<SelectOption>,
    selected: Option<&str>,
    placeholder: impl Into<SharedString>,
    tooltip: impl Into<SharedString>,
    disabled: bool,
    on_change: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let id = id.into();
    let control = ns_ui::select_disabled(
        id.clone(),
        options,
        selected,
        placeholder,
        disabled,
        on_change,
    );
    with_tip(id, control, tooltip, None)
}

/// A segmented control with a tooltip on the whole strip.
pub(crate) fn tip_segmented(
    id: impl Into<ElementId>,
    segments: impl IntoIterator<Item = (&'static str, &'static str, &'static str)>,
    selected: &str,
    tooltip: impl Into<SharedString>,
    disabled: bool,
    on_change: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let id = id.into();
    let control = ns_ui::segmented(id.clone(), segments, selected, on_change);
    let tipped = with_tip(id, control, tooltip, None);
    if !disabled {
        return tipped;
    }
    // Ely's segmented control has no disabled flag. A cover swallows the click
    // so a running flow cannot change direction from a still-looking strip.
    div()
        .relative()
        .opacity(0.45)
        .child(tipped)
        .child(div().absolute().inset_0().occlude())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    /// Constructors that draw a clickable control. Outside this file they must
    /// not be called by name; the `tip_*` wrappers are the only way in.
    const DIRECT: &[&str] = &[
        "Button::new(",
        "IconButton::new(",
        "Switch::new(",
        "button(",
        "quiet_button(",
        "danger_button(",
        "outline_button(",
        "warn_button(",
        "icon_button(",
        "icon_button_md(",
        "rail_button(",
        "select(",
        "segmented(",
    ];

    fn is_ident(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    /// Whether `line` calls `name` as a free function or associated function,
    /// not as part of a longer identifier such as `tip_button(`.
    fn calls(line: &str, name: &str) -> bool {
        let mut from = 0;
        while let Some(at) = line[from..].find(name) {
            let start = from + at;
            let before = line[..start].chars().next_back();
            let declared = line[..start].trim_end().ends_with("fn");
            let qualified = line[..start].ends_with("ns_ui::");
            let free = !before.is_some_and(|c| is_ident(c) || c == '.' || c == ':');
            if (free || qualified) && !declared {
                return true;
            }
            from = start + name.len();
        }
        false
    }

    fn sources() -> Vec<(String, Vec<String>)> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "rs") {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                let text = fs::read_to_string(&path).unwrap();
                // Unit tests may name these in strings; production code may not.
                let body = text.split("#[cfg(test)]").next().unwrap_or("");
                let lines = body
                    .lines()
                    .map(|line| line.split("//").next().unwrap_or("").to_string())
                    .collect();
                out.push((name, lines));
            }
        }
        out
    }

    #[test]
    fn every_control_has_a_tooltip() {
        let mut problems = Vec::new();
        for (file, lines) in sources() {
            if file == "tip.rs" {
                continue;
            }
            for (index, line) in lines.iter().enumerate() {
                for name in DIRECT {
                    if calls(line, name) {
                        problems.push(format!(
                            "{file}:{}: `{}` bypasses the tip_* helpers (no tooltip)",
                            index + 1,
                            name.trim_end_matches('(')
                        ));
                    }
                }
                if line.contains(".on_click(") {
                    let from = index.saturating_sub(14);
                    let to = (index + 6).min(lines.len());
                    let near = lines[from..to].join("\n");
                    let tipped = near.contains(".tooltip(")
                        || near.contains("tip_on(")
                        || near.contains("with_tip(")
                        || near.contains(".tip(");
                    if !tipped {
                        problems.push(format!(
                            "{file}:{}: `.on_click(` on an element with no tooltip nearby",
                            index + 1
                        ));
                    }
                }
            }
        }
        assert!(
            problems.is_empty(),
            "controls without a tooltip:\n{}",
            problems.join("\n")
        );
    }

    #[test]
    fn the_scanner_flags_a_bare_button_and_accepts_the_wrappers() {
        assert!(calls(
            "    .child(button(\"id\", \"x\", \"Go\", f))",
            "button("
        ));
        assert!(!calls("    .child(tip_button(\"id\"))", "button("));
        assert!(!calls("fn button(", "button("));
        assert!(calls("ns_ui::icon_button(", "icon_button("));
    }
}
