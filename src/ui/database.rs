//! The Database pane: SQLite by default, or a MariaDB or SQL Server instead.

use egui::{RichText, Ui};

use crate::app::App;
use crate::config::{self, Backend, Config};
use crate::db::Store;
use crate::db::attempt::{Attempt, Purpose, Target};
use crate::db::mariadb::MariaDbSettings;
use crate::db::mssql::MsSqlSettings;
use crate::{t, ui};

pub struct State {
    pub backend: Backend,
    /// Edited as text so the user can paste a path in.
    pub sqlite_path: String,
    pub mariadb: MariaDbSettings,
    pub mariadb_port: String,
    pub mssql: MsSqlSettings,
    pub mssql_port: String,
    pub remember_password: bool,
    /// What went wrong with the last attempt, kept beside the fields it is
    /// about rather than only in the status bar.
    pub error: Option<String>,
    /// A SQLite file that is not there yet, waiting to be confirmed before it
    /// is brought into being. See [`begin`].
    confirm_create: Option<(std::path::PathBuf, Purpose)>,
}

impl State {
    pub fn from_config(config: &Config) -> Self {
        Self {
            backend: config.backend,
            sqlite_path: config.sqlite_path().display().to_string(),
            mariadb: config.mariadb.clone(),
            mariadb_port: config.mariadb.port.to_string(),
            mssql: config.mssql.clone(),
            mssql_port: config.mssql.port.to_string(),
            remember_password: config.remember_password,
            error: None,
            confirm_create: None,
        }
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    ui::pane_header(ui, &t!("database.title"), &t!("database.subtitle"));

    let mut test_clicked = false;
    let mut apply_clicked = false;
    let connecting = app.is_connecting();

    egui::ScrollArea::vertical().show(ui, |ui| {
        let state = &mut app.database;

        choice(
            ui,
            &mut state.backend,
            Backend::Sqlite,
            &t!("database.sqlite.title"),
            &t!("database.sqlite.detail"),
        );
        choice(
            ui,
            &mut state.backend,
            Backend::MariaDb,
            &t!("database.mariadb.title"),
            &t!("database.mariadb.detail"),
        );
        choice(
            ui,
            &mut state.backend,
            Backend::MsSql,
            &t!("database.mssql.title"),
            &t!("database.mssql.detail"),
        );
        ui.add_space(14.0);

        match state.backend {
            Backend::Sqlite => {
                ui::labelled_field(
                    ui,
                    &t!("database.file"),
                    &mut state.sqlite_path,
                    &t!("database.file_hint"),
                );
                if ui.link(t!("database.use_default")).clicked() {
                    state.sqlite_path = config::default_sqlite_path().display().to_string();
                }
                ui.add_space(4.0);
                ui.label(RichText::new(t!("database.file_created")).size(12.0).weak());
            }
            Backend::MariaDb => {
                ui::labelled_field(
                    ui,
                    &t!("database.host"),
                    &mut state.mariadb.host,
                    "localhost",
                );
                ui::labelled_field(ui, &t!("database.port"), &mut state.mariadb_port, "3306");
                ui::labelled_field(
                    ui,
                    &t!("database.name"),
                    &mut state.mariadb.database,
                    "accounts",
                );
                ui::labelled_field(ui, &t!("database.user"), &mut state.mariadb.username, "");
                ui::labelled_password(ui, &t!("database.password"), &mut state.mariadb.password);

                ui.checkbox(&mut state.mariadb.use_tls, t!("database.use_tls"));
                if state.mariadb.use_tls {
                    ui.checkbox(
                        &mut state.mariadb.tls_skip_verify,
                        t!("database.tls_skip_verify"),
                    );
                    ui::labelled_field(
                        ui,
                        &t!("database.ca_cert"),
                        &mut state.mariadb.ca_cert_path,
                        &t!("database.ca_cert_hint"),
                    );
                }
                ui.checkbox(
                    &mut state.remember_password,
                    t!("database.remember_password"),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(t!("database.mariadb.privileges"))
                        .size(12.0)
                        .weak(),
                );
                ui.add_space(10.0);
                test_clicked = ui
                    .add_enabled_ui(!connecting, |ui| ui::wide_button(ui, &t!("database.test")))
                    .inner
                    .clicked();
            }
            Backend::MsSql => {
                ui::labelled_field(ui, &t!("database.host"), &mut state.mssql.host, "localhost");
                ui::labelled_field(ui, &t!("database.port"), &mut state.mssql_port, "1433");
                ui::labelled_field(
                    ui,
                    &t!("database.name"),
                    &mut state.mssql.database,
                    "accounts",
                );
                ui::labelled_field(ui, &t!("database.user"), &mut state.mssql.username, "");
                ui::labelled_password(ui, &t!("database.password"), &mut state.mssql.password);

                ui.checkbox(&mut state.mssql.use_tls, t!("database.encrypt"));
                // Not tucked away under "Encrypt the connection" the way the
                // MariaDB one is: SQL Server encrypts the login whatever this
                // box says, so its certificate is checked either way, and a
                // stock install presents one this machine has no reason to
                // trust. Hiding the way past that behind a checkbox the user
                // deliberately left clear makes the server unreachable.
                ui.checkbox(
                    &mut state.mssql.tls_skip_verify,
                    t!("database.accept_any_certificate"),
                );
                ui.checkbox(
                    &mut state.remember_password,
                    t!("database.remember_password"),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(t!("database.mssql.privileges"))
                        .size(12.0)
                        .weak(),
                );
                ui.add_space(10.0);
                test_clicked = ui
                    .add_enabled_ui(!connecting, |ui| ui::wide_button(ui, &t!("database.test")))
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
                    &if connecting {
                        t!("database.connecting")
                    } else {
                        t!("database.use_this")
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
            RichText::new(t!(
                "database.settings_kept_in",
                path = config::tilde(&config::config_path())
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

    let ctx = ui.ctx().clone();
    create_confirm(app, &ctx);
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

/// The path in the SQLite box, as somewhere on disk. Typing `~/…` into a text
/// box is the natural thing to do on macOS and Linux, and is what this app
/// puts on screen itself, so it is understood here rather than taken to mean a
/// directory called `~`.
fn sqlite_path_of(app: &App) -> std::path::PathBuf {
    config::untilde(std::path::Path::new(app.database.sqlite_path.trim()))
}

/// Read a port out of a text field, since it is the one number here.
fn parse_port(text: &str) -> Result<u16, String> {
    text.trim()
        .parse::<u16>()
        .map_err(|_| t!("database.bad_port"))
}

/// What the user has asked for, as something that can be opened.
fn target_of(app: &App) -> Result<Target, String> {
    match app.database.backend {
        Backend::Sqlite => Ok(Target::Sqlite(sqlite_path_of(app))),
        Backend::MariaDb => {
            let mut settings = app.database.mariadb.clone();
            settings.port = parse_port(&app.database.mariadb_port)?;
            Ok(Target::MariaDb(settings))
        }
        Backend::MsSql => {
            let mut settings = app.database.mssql.clone();
            settings.port = parse_port(&app.database.mssql_port)?;
            Ok(Target::MsSql(settings))
        }
    }
}

/// Start opening a database, for testing or for keeps.
///
/// A SQLite file that is not there yet is created, which is the point when the
/// backend is new and a disaster when the path simply has a typo in it: the
/// app cheerfully reports success, and the year's spending appears to have
/// vanished. Nothing distinguishes the two cases but the user, so they are
/// asked — once, and only when the file really is absent.
fn begin(app: &mut App, purpose: Purpose) {
    if app.is_connecting() {
        return;
    }
    let target = match target_of(app) {
        Ok(target) => target,
        Err(message) => {
            app.database.error = Some(message.clone());
            app.report_error(message);
            return;
        }
    };

    if let Target::Sqlite(path) = &target
        && !path.exists()
    {
        app.database.confirm_create = Some((path.clone(), purpose));
        return;
    }

    start(app, target, purpose);
}

/// Actually dial out.
///
/// Nothing waits here: the attempt goes to its own thread and the answer is
/// picked up by [`connection_finished`] a frame or two later, which is what
/// keeps a wrong hostname from freezing the window for five seconds.
fn start(app: &mut App, target: Target, purpose: Purpose) {
    app.status = Some(crate::app::Status {
        message: t!("status.connecting", target = target.label()),
        good: true,
    });
    app.database.error = None;
    app.connection = Some(app.start_connection(target, purpose));
}

/// The confirmation an absent SQLite file brings up.
fn create_confirm(app: &mut App, ctx: &egui::Context) {
    let Some((path, purpose)) = app.database.confirm_create.clone() else {
        return;
    };

    let mut open = true;
    let confirmed = ui::confirm_modal(
        ctx,
        egui::Id::new("create-database"),
        &mut open,
        &t!("database.create.title"),
        &t!("database.create.body", path = config::tilde(&path)),
        &t!("database.create.confirm"),
    );
    if !open {
        app.database.confirm_create = None;
    }
    if !confirmed {
        return;
    }

    start(app, Target::Sqlite(path), purpose);
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
            let message = t!(
                "status.connection_failed",
                target = attempt.target.label(),
                reason = reason
            );
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
        return app.report_ok(t!("status.connection_tested", target = store.describe()));
    }

    // Only now is the old connection let go: a failed attempt leaves the app
    // working on whatever it was using before.
    let where_it_is = store.describe();
    app.store = Some(store);
    app.config.backend = app.database.backend;
    let chosen_path = sqlite_path_of(app);
    app.config.sqlite_path = Some(chosen_path);
    app.config.mariadb = app.database.mariadb.clone();
    app.config.mariadb.port =
        parse_port(&app.database.mariadb_port).unwrap_or(app.config.mariadb.port);
    app.config.mssql = app.database.mssql.clone();
    app.config.mssql.port = parse_port(&app.database.mssql_port).unwrap_or(app.config.mssql.port);
    app.config.remember_password = app.database.remember_password;

    if let Err(err) = app.config.save() {
        app.report_error(t!("status.connected_but_unsaved", error = err));
    } else {
        app.report_ok(t!("status.now_using", target = where_it_is));
    }
    app.reload_totals();
    app.spending.category = None;
}
