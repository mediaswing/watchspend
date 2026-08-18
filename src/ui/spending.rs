//! The Spending pane: the form for recording what was spent.

use chrono::{Datelike as _, Local};
use egui::{RichText, Ui};

use crate::app::App;
use crate::db::NewSpend;
use crate::locale::Locale;
use crate::{t, ui};

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
        &t!("spending.title"),
        &t!(
            "spending.subtitle",
            code = currency.code,
            symbol = currency.symbol,
            format = app.locale.date_hint(),
        ),
    );

    let categories = app.category_names();
    let date_hint = app.locale.date_hint();
    let amount_hint = app.locale.amount_hint();

    let mut submit = false;
    egui::ScrollArea::vertical().show(ui, |ui| {
        let state = &mut app.spending;

        ui::labelled_field(ui, &t!("spending.date"), &mut state.date, &date_hint);

        ui.label(RichText::new(t!("spending.category")).size(13.0));
        let selected = state
            .category
            .clone()
            .unwrap_or_else(|| t!("spending.choose_category"));
        egui::ComboBox::from_id_salt("spending-category")
            .selected_text(selected)
            // The arrow and the frame's padding sit outside the width egui is
            // given here, so leave room for them and the field still ends
            // flush with the others.
            .width(ui.available_width() - 10.0)
            .show_ui(ui, |ui| {
                if categories.is_empty() {
                    ui.label(RichText::new(t!("spending.no_categories_yet")).weak());
                }
                for name in &categories {
                    ui.selectable_value(&mut state.category, Some(name.clone()), name);
                }
            });
        ui.add_space(6.0);

        ui::labelled_field(
            ui,
            &t!("spending.amount", symbol = currency.symbol),
            &mut state.amount,
            &amount_hint,
        );

        ui::labelled_field(
            ui,
            &t!("spending.description"),
            &mut state.description,
            &t!("spending.description_hint"),
        );

        if let Some(error) = &state.error {
            ui::error_text(ui, error);
            ui.add_space(4.0);
        }

        ui.add_space(6.0);
        submit = ui::wide_button(ui, &t!("spending.record")).clicked();

        if categories.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(t!("spending.add_a_category_first"))
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
        None => Err(crate::db::Error::Rejected(t!("common.no_database"))),
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
                t!("status.recorded", amount = amount, category = category)
            } else {
                t!(
                    "status.recorded_other_year",
                    amount = amount,
                    category = category,
                    year = year
                )
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
    spend_from_fields(
        &app.locale,
        &state.date,
        state.category.clone(),
        &state.amount,
        &state.description,
    )
}

/// The same checks `validate` runs, pulled out so the Entries tab's edit
/// form can share them rather than risk drifting from what "Record
/// Spending" accepts.
pub(super) fn spend_from_fields(
    locale: &Locale,
    date: &str,
    category: Option<String>,
    amount: &str,
    description: &str,
) -> Result<NewSpend, String> {
    let spent_on = locale.parse_date(date)?;
    let category = category.ok_or_else(|| t!("spending.choose_a_category"))?;
    let amount_minor = locale.parse_money(amount)?;

    let description = description.trim();
    if description.chars().count() > 255 {
        return Err(t!("spending.description_too_long"));
    }

    Ok(NewSpend {
        category,
        spent_on,
        amount_minor,
        currency: locale.currency.code.to_owned(),
        description: description.to_owned(),
    })
}
