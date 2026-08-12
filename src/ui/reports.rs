//! The Reports pane: a year's figures, written out as a file.

use egui::{RichText, Ui};

use crate::app::App;
use crate::report::{Format, Report, Sections};
use crate::ui;

pub struct State {
    pub year: i32,
    pub format: Format,
    pub sections: Sections,
    /// The folder to write into, edited as text so a path can be pasted in.
    pub folder: String,
    /// The report currently being shown, and the year and data version it was
    /// built from — so it is rebuilt when either changes, and not per frame.
    report: Option<Report>,
    built_for: Option<(i32, u64)>,
    error: Option<String>,
}

impl State {
    pub fn new(year: i32) -> Self {
        Self {
            year,
            format: Format::Csv,
            sections: Sections::default(),
            folder: default_folder(),
            report: None,
            built_for: None,
            error: None,
        }
    }
}

/// Somewhere a person would look for a file they just saved.
fn default_folder() -> String {
    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .display()
        .to_string()
}

pub fn show(app: &mut App, ui: &mut Ui) {
    ui::pane_header(
        ui,
        "Reports",
        "Write a year's spending out as a file you can keep, send or print",
    );

    refresh(app);

    let mut save = false;
    egui::ScrollArea::vertical().show(ui, |ui| {
        year_row(app, ui);
        ui.add_space(10.0);
        summary(app, ui);
        ui.add_space(14.0);

        ui.label(RichText::new("Format").size(13.0));
        format_row(&mut app.reports.format, ui);
        ui.label(
            RichText::new(app.reports.format.detail())
                .size(12.0)
                .weak(),
        );
        ui.add_space(12.0);

        ui.label(RichText::new("Include").size(13.0));
        ui.checkbox(&mut app.reports.sections.monthly, "A month-by-month table");
        ui.checkbox(
            &mut app.reports.sections.entries,
            "Every entry, itemised",
        );
        ui.add_space(12.0);

        let folder_label = format!(
            "Save in this folder, as {}",
            file_name(app.reports.year, app.reports.format)
        );
        let mut folder = std::mem::take(&mut app.reports.folder);
        ui::labelled_field(ui, &folder_label, &mut folder, "path to a folder");
        app.reports.folder = folder;

        if let Some(error) = &app.reports.error {
            ui::error_text(ui, error);
            ui.add_space(4.0);
        }

        let has_figures = app
            .reports
            .report
            .as_ref()
            .is_some_and(|report| !report.entries.is_empty());
        ui.add_space(4.0);
        save = ui::wide_button(ui, "Save Report").clicked();
        if !has_figures {
            ui.add_space(8.0);
            ui.label(
                RichText::new("There is nothing recorded for this year yet — a report of it would be an empty one.")
                    .size(13.0)
                    .weak(),
            );
        }
    });

    if save {
        write_report(app);
    }
}

/// The year being reported on, with a step either side. Years are picked
/// rather than typed: there is exactly one right way to write one here, and a
/// stepper cannot get it wrong.
fn year_row(app: &mut App, ui: &mut Ui) {
    ui.label(RichText::new("Year").size(13.0));
    ui.horizontal(|ui| {
        let step = 34.0;
        if ui
            .add_sized([step, step], egui::Button::new("◀"))
            .on_hover_text("The year before")
            .clicked()
        {
            app.reports.year -= 1;
        }
        ui.add_sized(
            [90.0, step],
            egui::Label::new(RichText::new(app.reports.year.to_string()).size(17.0)),
        );
        if ui
            .add_sized([step, step], egui::Button::new("▶"))
            .on_hover_text("The year after")
            .clicked()
        {
            app.reports.year += 1;
        }
        if app.reports.year != app.year && ui.button("This year").clicked() {
            app.reports.year = app.year;
        }
    });
}

fn summary(app: &App, ui: &mut Ui) {
    let Some(report) = &app.reports.report else {
        return;
    };
    let locale = &app.locale;
    ui.label(
        RichText::new(format!(
            "{} across {} {} in {} {}",
            locale.format_money(report.total_minor),
            report.entries.len(),
            if report.entries.len() == 1 {
                "entry"
            } else {
                "entries"
            },
            report.categories.len(),
            if report.categories.len() == 1 {
                "category"
            } else {
                "categories"
            },
        ))
        .size(16.0),
    );
}

fn format_row(current: &mut Format, ui: &mut Ui) {
    ui.horizontal(|ui| {
        let spacing = ui.spacing().item_spacing.x;
        let width = (ui.available_width() - spacing * 3.0) / 4.0;
        for format in Format::ALL {
            let button = egui::Button::selectable(*current == format, ui::centred(format.label()))
                .corner_radius(6.0)
                .frame_when_inactive(true)
                .min_size(egui::vec2(width, 36.0));
            if ui.add(button).clicked() {
                *current = format;
            }
        }
    });
}

fn file_name(year: i32, format: Format) -> String {
    format!("spending-report-{year}.{}", format.extension())
}

/// Read the year's entries, if what is on screen is out of date.
fn refresh(app: &mut App) {
    let wanted = (app.reports.year, app.data_version);
    if app.reports.built_for == Some(wanted) {
        return;
    }

    let currency = app.locale.currency.code;
    let year = app.reports.year;
    let Some(store) = app.store.as_mut() else {
        app.reports.report = None;
        app.reports.built_for = Some(wanted);
        return;
    };

    match store.spending_in_year(year, currency) {
        Ok(entries) => {
            app.reports.report = Some(Report::build(year, entries));
            app.reports.error = None;
        }
        Err(err) => {
            app.reports.report = None;
            app.reports.error = Some(format!("Could not read the figures: {err}"));
        }
    }
    app.reports.built_for = Some(wanted);
}

fn write_report(app: &mut App) {
    let Some(report) = &app.reports.report else {
        return app.report_error("There is no database to report on.");
    };

    let format = app.reports.format;
    let bytes = match report.render(format, &app.locale, app.reports.sections) {
        Ok(bytes) => bytes,
        Err(message) => {
            app.reports.error = Some(message.clone());
            return app.report_error(message);
        }
    };

    let folder = std::path::PathBuf::from(app.reports.folder.trim());
    if folder.as_os_str().is_empty() {
        let message = "Say which folder to save into.".to_owned();
        app.reports.error = Some(message.clone());
        return app.report_error(message);
    }

    // The name is built from the year and the format, never from anything
    // typed, so it cannot climb out of the folder that was chosen.
    let path = folder.join(report.file_name(format));

    if let Err(err) = std::fs::create_dir_all(&folder).and_then(|()| std::fs::write(&path, &bytes))
    {
        let message = format!("Could not write {}: {err}", path.display());
        app.reports.error = Some(message.clone());
        return app.report_error(message);
    }

    app.reports.error = None;
    app.report_ok(format!(
        "Saved {} to {}",
        format.label(),
        crate::config::tilde(&path)
    ));
}
