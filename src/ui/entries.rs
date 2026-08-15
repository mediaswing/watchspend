//! The Entries pane: every spend entry recorded this year, editable or
//! removable — the fix the add-only Spending tab has no way to offer.

use egui::{Align, Id, Layout, RichText, Ui};
use egui_extras::{Column, TableBuilder};

use crate::app::App;
use crate::db::SpendEntry;
use crate::locale::Locale;
use crate::ui;
use crate::ui::spending::spend_from_fields;

#[derive(Default)]
pub struct State {
    /// The id of the entry the edit box is open for, if any.
    editing: Option<i64>,
    date: String,
    category: Option<String>,
    amount: String,
    description: String,
    error: Option<String>,
    /// The edit box's first field asks for focus once, when it opens, and
    /// not on every frame after — otherwise the caret could never be moved.
    focus_taken: bool,
    /// The id and a short label of the entry the delete confirmation is
    /// open for, if any.
    confirm_delete: Option<(i64, String)>,
}

impl State {
    fn open_edit(&mut self, entry: &SpendEntry, locale: &Locale) {
        self.editing = Some(entry.id);
        self.date = locale.format_date(entry.spent_on);
        self.category = Some(entry.category.clone());
        // The same digits `format_money` would show, minus the currency
        // symbol — what `parse_money` expects back, same as the Spending
        // form's own amount field.
        let sign = if entry.amount_minor < 0 { "-" } else { "" };
        self.amount = format!(
            "{sign}{}",
            locale.format_digits(entry.amount_minor.unsigned_abs())
        );
        self.description = entry.description.clone();
        self.error = None;
        self.focus_taken = false;
    }

    fn close_edit(&mut self) {
        self.editing = None;
        self.error = None;
    }
}

enum RowAction {
    Edit(SpendEntry),
    Delete { id: i64, label: String },
}

pub fn show(app: &mut App, ui: &mut Ui) {
    ui::pane_header(
        ui,
        "Entries",
        &format!("Everything recorded in {}, oldest first", app.year),
    );

    let action = table(app, ui);
    match action {
        Some(RowAction::Edit(entry)) => {
            let locale = app.locale.clone();
            app.entries_tab.open_edit(&entry, &locale);
        }
        Some(RowAction::Delete { id, label }) => {
            app.entries_tab.confirm_delete = Some((id, label));
        }
        None => {}
    }

    let ctx = ui.ctx().clone();
    edit_box(app, &ctx);
    delete_confirm(app, &ctx);
}

fn table(app: &App, ui: &mut Ui) -> Option<RowAction> {
    if app.entries.is_empty() {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(if app.store.is_some() {
                    "No entries yet."
                } else {
                    "No database is connected."
                })
                .size(16.0)
                .weak(),
            );
            ui.label(
                RichText::new(if app.store.is_some() {
                    "Record some on the Spending tab and they will show up here."
                } else {
                    "Choose one on the Database tab."
                })
                .size(13.0)
                .weak(),
            );
        });
        return None;
    }

    let mut action = None;

    let amount_width = 110.0_f32.min(ui.available_width() * 0.15);
    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::exact(100.0))
        .column(Column::exact(160.0).clip(true))
        .column(Column::exact(amount_width))
        .column(Column::remainder().at_least(120.0).clip(true))
        .column(Column::exact(130.0))
        .header(30.0, |mut header| {
            header.col(|ui| {
                ui.label(RichText::new("Date").size(13.0).weak());
            });
            header.col(|ui| {
                ui.label(RichText::new("Category").size(13.0).weak());
            });
            header.col(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("Amount").size(13.0).weak());
                });
            });
            header.col(|ui| {
                ui.label(RichText::new("Description").size(13.0).weak());
            });
            header.col(|_ui| {});
        })
        .body(|mut body| {
            for entry in &app.entries {
                body.row(32.0, |mut row| {
                    row.col(|ui| {
                        ui.label(app.locale.format_date(entry.spent_on));
                    });
                    row.col(|ui| {
                        ui.label(&entry.category);
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(app.locale.format_money(entry.amount_minor));
                        });
                    });
                    row.col(|ui| {
                        ui.label(&entry.description);
                    });
                    row.col(|ui| {
                        ui.horizontal(|ui| {
                            if ui.button("Edit").clicked() {
                                action = Some(RowAction::Edit(entry.clone()));
                            }
                            if ui.button("Delete").clicked() {
                                let label = format!(
                                    "{} in {} on {}",
                                    app.locale.format_money(entry.amount_minor),
                                    entry.category,
                                    app.locale.format_date(entry.spent_on)
                                );
                                action = Some(RowAction::Delete {
                                    id: entry.id,
                                    label,
                                });
                            }
                        });
                    });
                });
            }
        });

    action
}

/// The box the "Edit" button on a row brings up.
fn edit_box(app: &mut App, ctx: &egui::Context) {
    let Some(id) = app.entries_tab.editing else {
        return;
    };

    let categories = app.category_names();
    let date_hint = app.locale.date_hint();
    let amount_hint = app.locale.amount_hint();
    let mut submit = false;
    let mut cancel = false;
    let state = &mut app.entries_tab;

    let modal = egui::Modal::new(Id::new("edit-entry")).show(ctx, |ui| {
        ui.set_width(360.0);
        ui.heading("Edit Entry");
        ui.add_space(12.0);

        ui::labelled_field(ui, "Date", &mut state.date, &date_hint);

        ui.label(RichText::new("Category").size(13.0));
        let selected = state
            .category
            .clone()
            .unwrap_or_else(|| "Choose a category…".to_owned());
        egui::ComboBox::from_id_salt("edit-entry-category")
            .selected_text(selected)
            .width(ui.available_width() - 10.0)
            .show_ui(ui, |ui| {
                for name in &categories {
                    ui.selectable_value(&mut state.category, Some(name.clone()), name);
                }
            });
        ui.add_space(6.0);

        let field = ui::labelled_field(
            ui,
            &format!("Amount ({})", app.locale.currency.symbol),
            &mut state.amount,
            &amount_hint,
        );
        if !state.focus_taken {
            field.request_focus();
            state.focus_taken = true;
        }

        ui::labelled_field(
            ui,
            "Description (optional)",
            &mut state.description,
            "What was it for?",
        );

        if let Some(error) = &state.error {
            ui.add_space(4.0);
            ui::error_text(ui, error);
        }

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
            if ui
                .add_sized([width, 36.0], egui::Button::new(ui::centred("Cancel")))
                .clicked()
            {
                cancel = true;
            }
            if ui
                .add_sized([width, 36.0], egui::Button::new(ui::centred("Save")))
                .clicked()
            {
                submit = true;
            }
        });
    });

    if modal.should_close() {
        cancel = true;
    }

    if cancel {
        app.entries_tab.close_edit();
        return;
    }
    if !submit {
        return;
    }

    let spend = match spend_from_fields(
        &app.locale,
        &app.entries_tab.date,
        app.entries_tab.category.clone(),
        &app.entries_tab.amount,
        &app.entries_tab.description,
    ) {
        Ok(spend) => spend,
        Err(message) => {
            app.entries_tab.error = Some(message);
            return;
        }
    };

    let result = match app.store.as_mut() {
        Some(store) => store.update_spend(id, &spend),
        None => Err(crate::db::Error::Rejected(
            "No database is connected.".to_owned(),
        )),
    };
    match result {
        Ok(()) => {
            app.entries_tab.close_edit();
            app.reload_totals();
            app.report_ok("Saved the change.");
        }
        Err(err) => {
            app.entries_tab.error = Some(err.to_string());
            app.sounds.failure();
        }
    }
}

/// The confirmation the "Delete" button on a row brings up.
fn delete_confirm(app: &mut App, ctx: &egui::Context) {
    let Some((id, label)) = app.entries_tab.confirm_delete.clone() else {
        return;
    };

    let mut open = true;
    let confirmed = ui::confirm_modal(
        ctx,
        Id::new("delete-entry"),
        &mut open,
        "Delete Entry?",
        &format!("Delete {label}? This cannot be undone."),
        "Delete",
    );
    if !open {
        app.entries_tab.confirm_delete = None;
    }
    if !confirmed {
        return;
    }

    let result = match app.store.as_mut() {
        Some(store) => store.delete_spend(id),
        None => Err(crate::db::Error::Rejected(
            "No database is connected.".to_owned(),
        )),
    };
    match result {
        Ok(()) => {
            app.reload_totals();
            app.report_ok("Deleted the entry.");
        }
        Err(err) => {
            app.report_error(err.to_string());
        }
    }
}
