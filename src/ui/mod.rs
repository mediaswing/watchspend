//! The three panes, and the few widget helpers they share.

pub mod categories;
pub mod database;
pub mod spending;

use egui::{Atom, Color32, Response, RichText, Ui, Widget as _};

/// Height of the buttons that span a whole pane.
const WIDE_BUTTON_HEIGHT: f32 = 40.0;

/// Label for a button, padded either side so the text sits in the middle
/// rather than against the left edge. A button that spans the pane is much
/// wider than its words, and left-aligned words in a wide button read as the
/// start of a list rather than as the button's name.
pub fn centred<'a>(text: impl Into<egui::WidgetText>) -> (Atom<'a>, Atom<'a>, Atom<'a>) {
    (Atom::grow(), text.into().into(), Atom::grow())
}

/// A button that fills the width of whatever it is placed in — which is what
/// the design asks for in several places, and what makes the primary action of
/// a pane unmissable.
pub fn wide_button(ui: &mut Ui, text: &str) -> Response {
    egui::Button::new(centred(RichText::new(text).size(16.0)))
        .corner_radius(6.0)
        .min_size(egui::vec2(ui.available_width(), WIDE_BUTTON_HEIGHT))
        .ui(ui)
}

/// A single-line text field that fills the width of the pane, under its label.
pub fn labelled_field(ui: &mut Ui, label: &str, value: &mut String, hint: &str) -> Response {
    ui.label(RichText::new(label).size(13.0));
    let response = egui::TextEdit::singleline(value)
        .hint_text(hint)
        .desired_width(f32::INFINITY)
        .margin(egui::vec2(8.0, 6.0))
        .ui(ui);
    ui.add_space(6.0);
    response
}

/// The same, for text that should not be shown as it is typed.
pub fn labelled_password(ui: &mut Ui, label: &str, value: &mut String) -> Response {
    ui.label(RichText::new(label).size(13.0));
    let response = egui::TextEdit::singleline(value)
        .password(true)
        .desired_width(f32::INFINITY)
        .margin(egui::vec2(8.0, 6.0))
        .ui(ui);
    ui.add_space(6.0);
    response
}

pub fn error_text(ui: &mut Ui, message: &str) {
    ui.label(RichText::new(message).color(Color32::from_rgb(0xc6, 0x28, 0x28)));
}

/// A pane heading with a quieter line of context under it.
pub fn pane_header(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.add_space(10.0);
    ui.heading(title);
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).size(13.0).weak());
    }
    ui.add_space(10.0);
}
