//! The Database pane: SQLite by default, or a MariaDB server instead.

use egui::{RichText, Ui};

use crate::app::App;
use crate::config::{self, Backend, Config};
use crate::db::Store;
use crate::db::mariadb::{MariaDbSettings, MariaDbStore};
use crate::db::sqlite::SqliteStore;
use crate::ui;

pub struct State {
    pub backend: Backend,
    /// Edited as text so the user can paste a path in.
    pub sqlite_path: String,
    pub mariadb: MariaDbSettings,
    pub port: String,
    pub remember_password: bool,
}

impl State {
    pub fn from_config(config: &Config) -> Self {
        Self {
            backend: config.backend,
            sqlite_path: config.sqlite_path().display().to_string(),
            mariadb: config.mariadb.clone(),
            port: config.mariadb.port.to_string(),
            remember_password: config.remember_password,
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
                test_clicked = ui::wide_button(ui, "Test Connection").clicked();
            }
        }

        ui.add_space(10.0);
        apply_clicked = ui::wide_button(ui, "Use This Database").clicked();
        ui.add_space(10.0);
        ui.label(
            RichText::new(format!("Settings are kept in {}", config::tilde(&config::config_path())))
                .size(12.0)
                .weak(),
        );
    });

    if test_clicked {
        test_connection(app);
    }
    if apply_clicked {
        apply(app);
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

/// Open the chosen backend and prove it is usable before anyone relies on it.
///
/// Connecting only shows that the login worked. It says nothing about whether
/// this account can read the tables it just created, and finding that out
/// after the switch means finding it out from an app that has already put its
/// old, working database down. So the connection is made, read from, and only
/// then handed back to be adopted.
fn open_and_check(app: &App) -> Result<Box<dyn Store>, String> {
    let mut store: Box<dyn Store> = match app.database.backend {
        Backend::Sqlite => {
            let path = std::path::PathBuf::from(app.database.sqlite_path.trim());
            if path.as_os_str().is_empty() {
                return Err("Give the database file a path.".to_owned());
            }
            Box::new(SqliteStore::open(&path).map_err(|e| e.to_string())?)
        }
        Backend::MariaDb => {
            let mut settings = app.database.mariadb.clone();
            settings.port = port_of(app)?;
            Box::new(MariaDbStore::connect(&settings).map_err(|e| e.to_string())?)
        }
    };

    store
        .categories_with_totals(app.year, app.locale.currency.code)
        .map_err(|err| format!("Connected, but could not read from it: {err}"))?;

    Ok(store)
}

/// The Test Connection button: everything `open_and_check` does, but the
/// result is only reported, and the app carries on where it was.
fn test_connection(app: &mut App) {
    match open_and_check(app) {
        Ok(store) => {
            let where_it_is = store.describe();
            app.report_ok(format!("{where_it_is} — connected, and readable."));
        }
        Err(message) => app.report_error(message),
    }
}

/// Switch the app over to the chosen backend, and remember the choice.
fn apply(app: &mut App) {
    let backend = app.database.backend;
    match open_and_check(app) {
        Ok(store) => {
            // Only now is the old connection let go: a failed switch leaves
            // the app working on whatever it was using before.
            app.store = Some(store);
            app.config.backend = backend;
            app.config.sqlite_path = Some(std::path::PathBuf::from(
                app.database.sqlite_path.trim(),
            ));
            app.config.mariadb = app.database.mariadb.clone();
            app.config.mariadb.port = port_of(app).unwrap_or(app.config.mariadb.port);
            app.config.remember_password = app.database.remember_password;

            let where_it_is = app
                .store
                .as_ref()
                .map_or_else(String::new, |s| s.describe());
            if let Err(err) = app.config.save() {
                app.report_error(format!("Connected, but could not save the setting: {err}"));
            } else {
                app.report_ok(format!("Now using {where_it_is}"));
            }
            app.reload_totals();
            app.spending.category = None;
        }
        Err(message) => app.report_error(message),
    }
}
