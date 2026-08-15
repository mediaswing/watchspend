//! The Categories pane: what has been spent, per category, this year.

use egui::{Align, Id, Layout, RichText, Ui, Widget as _};
use egui_extras::{Column, TableBuilder};

use crate::app::App;
use crate::ui;

/// State belonging to this pane alone — everything else lives on [`App`].
#[derive(Default)]
pub struct State {
    /// Is the "Add New Category" box open?
    pub adding: bool,
    pub name: String,
    pub error: Option<String>,
    /// The name field asks for focus once, when the box opens, and not on
    /// every frame after — otherwise the caret could never be moved.
    focus_taken: bool,
    /// The category the "Rename" box is open for, if any — its current name.
    renaming: Option<String>,
    rename_name: String,
    rename_error: Option<String>,
    rename_focus_taken: bool,
    /// The category the delete confirmation is open for, if any.
    confirm_delete: Option<String>,
}

impl State {
    fn open(&mut self) {
        self.adding = true;
        self.name.clear();
        self.error = None;
        self.focus_taken = false;
    }

    fn close(&mut self) {
        self.adding = false;
        self.name.clear();
        self.error = None;
    }

    fn open_rename(&mut self, name: &str) {
        self.renaming = Some(name.to_owned());
        self.rename_name = name.to_owned();
        self.rename_error = None;
        self.rename_focus_taken = false;
    }

    fn close_rename(&mut self) {
        self.renaming = None;
        self.rename_error = None;
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    ui::pane_header(
        ui,
        "Categories",
        &format!("Total spent in each category so far in {}", app.year),
    );

    let total: i64 = app.totals.iter().map(|c| c.total_minor).sum();
    let grand_total = app.locale.format_money(total);
    let foreign = app.foreign_entries;
    let has_store = app.store.is_some();

    // The footer is laid out first so that it keeps its space at the bottom of
    // the pane, and the table scrolls in whatever is left.
    let mut add_clicked = false;
    egui::Panel::bottom("categories-footer")
        .show_separator_line(false)
        .show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Total").size(15.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(&grand_total).size(15.0));
                });
            });
            if foreign > 0 {
                ui.label(
                    RichText::new(format!(
                        "{foreign} {} recorded in another currency and not counted here.",
                        if foreign == 1 {
                            "entry was"
                        } else {
                            "entries were"
                        }
                    ))
                    .size(12.0)
                    .weak(),
                );
            }
            ui.add_space(8.0);
            add_clicked = ui::wide_button(ui, "Add New Category").clicked();
            ui.add_space(10.0);
        });

    let action = table(app, ui);
    match action {
        Some(RowAction::Rename(name)) => app.categories.open_rename(&name),
        Some(RowAction::Delete(name)) => app.categories.confirm_delete = Some(name),
        None => {}
    }

    if add_clicked {
        if has_store {
            app.categories.open();
        } else {
            app.report_error("Connect a database on the Database tab first.");
        }
    }

    let ctx = ui.ctx().clone();
    add_category_box(app, &ctx);
    rename_category_box(app, &ctx);
    delete_confirm(app, &ctx);
}

enum RowAction {
    Rename(String),
    Delete(String),
}

fn table(app: &App, ui: &mut Ui) -> Option<RowAction> {
    if app.totals.is_empty() {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(if app.store.is_some() {
                    "No categories yet."
                } else {
                    "No database is connected."
                })
                .size(16.0)
                .weak(),
            );
            ui.label(
                RichText::new(if app.store.is_some() {
                    "Add one below to start putting spending into it."
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

    // Three columns spanning the pane: the name takes whatever is left, the
    // amount gets a fixed column it can be right-aligned inside, and the
    // actions get a fixed column of their own, so the figures line up on
    // their last digit however long the names are.
    let amount_width = 160.0_f32.min(ui.available_width() * 0.4);
    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::remainder().at_least(120.0).clip(true))
        .column(Column::exact(amount_width))
        .column(Column::exact(150.0))
        .header(30.0, |mut header| {
            header.col(|ui| {
                ui.label(RichText::new("Category").size(13.0).weak());
            });
            header.col(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("Spent").size(13.0).weak());
                });
            });
            header.col(|_ui| {});
        })
        .body(|mut body| {
            for category in &app.totals {
                body.row(32.0, |mut row| {
                    row.col(|ui| {
                        ui.label(&category.name);
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let amount = app.locale.format_money(category.total_minor);
                            // Untouched categories are shown, but quietly.
                            let text = RichText::new(amount);
                            ui.label(if category.entries == 0 {
                                text.weak()
                            } else {
                                text
                            });
                        });
                    });
                    row.col(|ui| {
                        ui.horizontal(|ui| {
                            if ui.button("Rename").clicked() {
                                action = Some(RowAction::Rename(category.name.clone()));
                            }
                            if ui.button("Delete").clicked() {
                                action = Some(RowAction::Delete(category.name.clone()));
                            }
                        });
                    });
                });
            }
        });

    action
}

/// The box the "Add New Category" button brings up.
fn add_category_box(app: &mut App, ctx: &egui::Context) {
    if !app.categories.adding {
        return;
    }

    let mut submit = false;
    let mut cancel = false;
    let state = &mut app.categories;

    let modal = egui::Modal::new(Id::new("add-category")).show(ctx, |ui| {
        ui.set_width(340.0);
        ui.heading("Add New Category");
        ui.add_space(4.0);
        ui.label(
            RichText::new("Spending is filed under categories; this adds another one.")
                .size(13.0)
                .weak(),
        );
        ui.add_space(12.0);

        let field = egui::TextEdit::singleline(&mut state.name)
            .hint_text("e.g. Groceries")
            .desired_width(f32::INFINITY)
            .margin(egui::vec2(8.0, 6.0))
            .ui(ui);
        if !state.focus_taken {
            field.request_focus();
            state.focus_taken = true;
        }
        // Enter is the obvious way to finish a one-field box.
        if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            submit = true;
        }

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
                .add_sized(
                    [width, 36.0],
                    egui::Button::new(ui::centred("Add Category")),
                )
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
        app.categories.close();
        return;
    }
    if !submit {
        return;
    }

    let name = app.categories.name.trim().to_owned();
    let result = match app.store.as_mut() {
        Some(store) => store.add_category(&name),
        None => Err(crate::db::Error::Rejected(
            "No database is connected.".to_owned(),
        )),
    };
    match result {
        Ok(()) => {
            app.categories.close();
            app.reload_totals();
            app.report_ok(format!("Added the category “{name}”."));
        }
        Err(err) => {
            // The box stays open with the reason in it, so the typing is not
            // lost and the fix is in the same place as the mistake.
            app.categories.error = Some(err.to_string());
            app.sounds.failure();
        }
    }
}

/// The box the "Rename" button on a row brings up.
fn rename_category_box(app: &mut App, ctx: &egui::Context) {
    let Some(old_name) = app.categories.renaming.clone() else {
        return;
    };

    let mut submit = false;
    let mut cancel = false;
    let state = &mut app.categories;

    let modal = egui::Modal::new(Id::new("rename-category")).show(ctx, |ui| {
        ui.set_width(340.0);
        ui.heading("Rename Category");
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "Renaming “{old_name}”. Its spending stays with it."
            ))
            .size(13.0)
            .weak(),
        );
        ui.add_space(12.0);

        let field = egui::TextEdit::singleline(&mut state.rename_name)
            .hint_text("e.g. Groceries")
            .desired_width(f32::INFINITY)
            .margin(egui::vec2(8.0, 6.0))
            .ui(ui);
        if !state.rename_focus_taken {
            field.request_focus();
            state.rename_focus_taken = true;
        }
        if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            submit = true;
        }

        if let Some(error) = &state.rename_error {
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
        app.categories.close_rename();
        return;
    }
    if !submit {
        return;
    }

    let new_name = app.categories.rename_name.trim().to_owned();
    let result = match app.store.as_mut() {
        Some(store) => store.rename_category(&old_name, &new_name),
        None => Err(crate::db::Error::Rejected(
            "No database is connected.".to_owned(),
        )),
    };
    match result {
        Ok(()) => {
            app.categories.close_rename();
            app.reload_totals();
            app.report_ok(format!("Renamed “{old_name}” to “{new_name}”."));
        }
        Err(err) => {
            app.categories.rename_error = Some(err.to_string());
            app.sounds.failure();
        }
    }
}

/// The confirmation the "Delete" button on a row brings up.
fn delete_confirm(app: &mut App, ctx: &egui::Context) {
    let Some(name) = app.categories.confirm_delete.clone() else {
        return;
    };

    let mut open = true;
    let confirmed = ui::confirm_modal(
        ctx,
        Id::new("delete-category"),
        &mut open,
        "Delete Category?",
        &format!("Delete “{name}”? This cannot be undone."),
        "Delete",
    );
    if !open {
        app.categories.confirm_delete = None;
    }
    if !confirmed {
        return;
    }

    let result = match app.store.as_mut() {
        Some(store) => store.delete_category(&name),
        None => Err(crate::db::Error::Rejected(
            "No database is connected.".to_owned(),
        )),
    };
    match result {
        Ok(()) => {
            app.reload_totals();
            app.report_ok(format!("Deleted the category “{name}”."));
        }
        Err(err) => {
            app.report_error(err.to_string());
        }
    }
}
