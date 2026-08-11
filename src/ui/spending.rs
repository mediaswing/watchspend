//! The Spending pane: the form for recording what was spent.

use chrono::{Datelike as _, Local};
use egui::{RichText, Ui};

use crate::app::App;
use crate::db::NewSpend;
use crate::locale::Locale;
use crate::ui;

#[derive(Default)]
pub struct State {
    pub date: String,
    pub category: Option<String>,
    pub amount: String,
    pub description: String,
    pub error: Option<String>,
}

impl State {
    /// Start on today's date, written the way this locale writes dates.
    pub fn reset_date(&mut self, locale: &Locale) {
        self.date = locale.format_date(Local::now().date_naive());
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let currency = app.locale.currency;
    ui::pane_header(
        ui,
        "Spending",
        &format!(
            "Amounts in {} ({}); dates as {}",
            currency.code,
            currency.symbol,
            app.locale.date_hint()
        ),
    );

    let categories = app.category_names();
    let date_hint = app.locale.date_hint();
    let amount_hint = app.locale.amount_hint();

    let mut submit = false;
    egui::ScrollArea::vertical().show(ui, |ui| {
        let state = &mut app.spending;

        ui::labelled_field(ui, "Date", &mut state.date, &date_hint);

        ui.label(RichText::new("Category").size(13.0));
        let selected = state
            .category
            .clone()
            .unwrap_or_else(|| "Choose a category…".to_owned());
        egui::ComboBox::from_id_salt("spending-category")
            .selected_text(selected)
            // The arrow and the frame's padding sit outside the width egui is
            // given here, so leave room for them and the field still ends
            // flush with the others.
            .width(ui.available_width() - 10.0)
            .show_ui(ui, |ui| {
                if categories.is_empty() {
                    ui.label(RichText::new("No categories yet").weak());
                }
                for name in &categories {
                    ui.selectable_value(&mut state.category, Some(name.clone()), name);
                }
            });
        ui.add_space(6.0);

        ui::labelled_field(
            ui,
            &format!("Amount ({})", currency.symbol),
            &mut state.amount,
            &amount_hint,
        );

        ui::labelled_field(
            ui,
            "Description (optional)",
            &mut state.description,
            "What was it for?",
        );

        if let Some(error) = &state.error {
            ui::error_text(ui, error);
            ui.add_space(4.0);
        }

        ui.add_space(6.0);
        submit = ui::wide_button(ui, "Record Spending").clicked();

        if categories.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new("Add a category first — spending has to go somewhere.")
                    .size(13.0)
                    .weak(),
            );
        }
    });

    if submit {
        record(app);
    }
}

fn record(app: &mut App) {
    let spend = match validate(app) {
        Ok(spend) => spend,
        Err(message) => {
            app.spending.error = Some(message.clone());
            app.report_error(message);
            return;
        }
    };

    let result = match app.store.as_mut() {
        Some(store) => store.add_spend(&spend),
        None => Err(crate::db::Error::Rejected(
            "No database is connected.".to_owned(),
        )),
    };

    match result {
        Ok(()) => {
            let amount = app.locale.format_money(spend.amount_minor);
            let category = spend.category.clone();
            let year = spend.spent_on.year();
            // Keep the date and the category: entering several things bought
            // on the same day is the common case.
            app.spending.amount.clear();
            app.spending.description.clear();
            app.spending.error = None;
            app.reload_totals();
            // Saying it landed in another year matters: the Categories table
            // only shows this one, so an entry dated 2025 by a slip of the
            // keyboard would otherwise look as though it had not saved at all.
            app.report_ok(if year == app.year {
                format!("Recorded {amount} in {category}.")
            } else {
                format!("Recorded {amount} in {category}, dated {year}.")
            });
        }
        Err(err) => {
            let message = err.to_string();
            app.spending.error = Some(message.clone());
            app.report_error(message);
        }
    }
}

/// Turn what is in the form into something worth storing, or say what is wrong
/// with it. Checked in the order the fields are read, so the message points at
/// the first thing to fix.
fn validate(app: &App) -> Result<NewSpend, String> {
    let state = &app.spending;

    let spent_on = app.locale.parse_date(&state.date)?;
    let category = state
        .category
        .clone()
        .ok_or_else(|| "Choose a category.".to_owned())?;
    let amount_minor = app.locale.parse_money(&state.amount)?;

    let description = state.description.trim();
    if description.chars().count() > 255 {
        return Err("That description is too long — 255 characters at most.".to_owned());
    }

    Ok(NewSpend {
        category,
        spent_on,
        amount_minor,
        currency: app.locale.currency.code.to_owned(),
        description: description.to_owned(),
    })
}
