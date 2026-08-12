//! The Database pane: SQLite by default, or a MariaDB server instead.

use egui::{RichText, Ui};

use crate::app::App;
use crate::config::{self, Backend, Config};
use crate::db::Store;
use crate::db::attempt::{Attempt, Purpose, Target};
use crate::db::mariadb::MariaDbSettings;
use crate::ui;

pub struct State {
    pub backend: Backend,
    /// Edited as text so the user can paste a path in.
    pub sqlite_path: String,
    pub mariadb: MariaDbSettings,
    pub port: String,
    pub remember_password: bool,
    /// What went wrong with the last attempt, kept beside the fields it is
    /// about rather than only in the status bar.
    pub error: Option<String>,
}

impl State {
    pub fn from_config(config: &Config) -> Self {
        Self {
            backend: config.backend,
            sqlite_path: config.sqlite_path().display().to_string(),
            mariadb: config.mariadb.clone(),
            port: config.mariadb.port.to_string(),
            remember_password: config.remember_password,
            error: None,
        }
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    ui::pane_header(
        ui,
        "Database",
        "Where this app keeps your categories and spending",
    );

    let mut test_clicked = false;
    let mut apply_clicked = false;
    let connecting = app.is_connecting();

    egui::ScrollArea::vertical().show(ui, |ui| {
        let state = &mut app.database;

        choice(
            ui,
            &mut state.backend,
            Backend::Sqlite,
            "SQLite file (default)",
            "A single file on this machine. Nothing to set up.",
        );
        choice(
            ui,
            &mut state.backend,
            Backend::MariaDb,
            "MariaDB server",
            "A server you already run, so several machines can share the figures.",
        );
        ui.add_space(14.0);

        match state.backend {
            Backend::Sqlite => {
                ui::labelled_field(
                    ui,
                    "Database file",
                    &mut state.sqlite_path,
                    "path to a .sqlite file",
                );
                if ui.link("Use the default location").clicked() {
                    state.sqlite_path = config::default_sqlite_path().display().to_string();
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "The file is created if it is not there yet, along with any \
                         folders leading to it.",
                    )
                    .size(12.0)
                    .weak(),
                );
            }
            Backend::MariaDb => {
                ui::labelled_field(ui, "Host", &mut state.mariadb.host, "localhost");
                ui::labelled_field(ui, "Port", &mut state.port, "3306");
                ui::labelled_field(ui, "Database", &mut state.mariadb.database, "accounts");
                ui::labelled_field(ui, "User name", &mut state.mariadb.username, "");
                ui::labelled_password(ui, "Password", &mut state.mariadb.password);

                ui.checkbox(&mut state.mariadb.use_tls, "Connect over TLS");
                if state.mariadb.use_tls {
                    ui.checkbox(
                        &mut state.mariadb.tls_skip_verify,
                        "Accept a certificate that does not match the host name",
                    );
                }
                ui.checkbox(
                    &mut state.remember_password,
                    "Remember the password (stored as plain text in the config file)",
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "The tables are created on first connection, so the user needs \
                         CREATE as well as SELECT and INSERT.",
                    )
                    .size(12.0)
                    .weak(),
                );
                ui.add_space(10.0);
                test_clicked = ui
                    .add_enabled_ui(!connecting, |ui| ui::wide_button(ui, "Test Connection"))
                    .inner
                    .clicked();
            }
        }

        ui.add_space(10.0);
        // Both buttons go quiet while an attempt is in flight, so a slow
        // server cannot be asked the same question five times over.
        apply_clicked = ui
            .add_enabled_ui(!connecting, |ui| {
                ui::wide_button(
                    ui,
                    if connecting {
                        "Connecting…"
                    } else {
                        "Use This Database"
                    },
                )
            })
            .inner
            .clicked();

        if let Some(error) = &app.database.error {
            ui.add_space(6.0);
            ui::error_text(ui, error);
        }

        ui.add_space(10.0);
        ui.label(
            RichText::new(format!(
                "Settings are kept in {}",
                config::tilde(&config::config_path())
            ))
            .size(12.0)
            .weak(),
        );
    });

    if test_clicked {
        begin(app, Purpose::Test);
    }
    if apply_clicked {
        begin(app, Purpose::Adopt);
    }
}

/// One of the two backend choices, as a wide button that stays lit when it is
/// the one in use.
fn choice(ui: &mut Ui, current: &mut Backend, value: Backend, title: &str, detail: &str) {
    let selected = *current == value;
    let button = egui::Button::selectable(selected, ui::centred(RichText::new(title).size(15.0)))
        .corner_radius(6.0)
        // Without this the unpicked option loses its outline and reads as a
        // heading rather than as the other thing you could choose.
        .frame_when_inactive(true)
        .min_size(egui::vec2(ui.available_width(), 38.0));
    if ui.add(button).clicked() {
        *current = value;
    }
    ui.label(RichText::new(detail).size(12.0).weak());
    ui.add_space(8.0);
}

/// Read the port out of the text field, since it is the one number here.
fn port_of(app: &App) -> Result<u16, String> {
    app.database
        .port
        .trim()
        .parse::<u16>()
        .map_err(|_| "The port has to be a number between 1 and 65535.".to_owned())
}

/// What the user has asked for, as something that can be opened.
fn target_of(app: &App) -> Result<Target, String> {
    match app.database.backend {
        Backend::Sqlite => Ok(Target::Sqlite(std::path::PathBuf::from(
            app.database.sqlite_path.trim(),
        ))),
        Backend::MariaDb => {
            let mut settings = app.database.mariadb.clone();
            settings.port = port_of(app)?;
            Ok(Target::MariaDb(settings))
        }
    }
}

/// Start opening a database, for testing or for keeps.
///
/// Nothing waits here: the attempt goes to its own thread and the answer is
/// picked up by [`connection_finished`] a frame or two later, which is what
/// keeps a wrong hostname from freezing the window for five seconds.
fn begin(app: &mut App, purpose: Purpose) {
    if app.is_connecting() {
        return;
    }
    match target_of(app) {
        Ok(target) => {
            app.status = Some(crate::app::Status {
                message: format!("Connecting to {}…", target.label()),
                good: true,
            });
            app.database.error = None;
            app.connection = Some(app.start_connection(target, purpose));
        }
        Err(message) => {
            app.database.error = Some(message.clone());
            app.report_error(message);
        }
    }
}

/// Deal with the answer, whichever way it went.
pub fn connection_finished(
    app: &mut App,
    attempt: &Attempt,
    result: Result<Box<dyn Store>, String>,
) {
    let store = match result {
        Ok(store) => store,
        Err(reason) => {
            // Which database failed, as well as how: by the time the answer
            // comes back the user may have typed a different hostname into
            // the fields, and the message should still be about the attempt
            // that was actually made.
            let message = format!("{} — {reason}", attempt.target.label());
            app.database.error = Some(message.clone());
            app.report_error(message);
            // Nothing to fall back on means the app has no database at all,
            // and this pane is where that gets fixed.
            if app.store.is_none() && attempt.purpose == Purpose::Adopt {
                app.tab = crate::app::Tab::Database;
            }
            return;
        }
    };

    app.database.error = None;

    if attempt.purpose == Purpose::Test {
        // Tested and thrown away: the app carries on with whatever it had.
        return app.report_ok(format!("{} — connected, and readable.", store.describe()));
    }

    // Only now is the old connection let go: a failed attempt leaves the app
    // working on whatever it was using before.
    let where_it_is = store.describe();
    app.store = Some(store);
    app.config.backend = app.database.backend;
    app.config.sqlite_path = Some(std::path::PathBuf::from(app.database.sqlite_path.trim()));
    app.config.mariadb = app.database.mariadb.clone();
    app.config.mariadb.port = port_of(app).unwrap_or(app.config.mariadb.port);
    app.config.remember_password = app.database.remember_password;

    if let Err(err) = app.config.save() {
        app.report_error(format!("Connected, but could not save the setting: {err}"));
    } else {
        app.report_ok(format!("Now using {where_it_is}"));
    }
    app.reload_totals();
    app.spending.category = None;
}
