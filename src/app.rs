//! Application state and the two-pane frame: tabs down the left, the selected
//! pane on the right.

use chrono::{Datelike as _, Local};
use eframe::CreationContext;
use egui::{Align, FontFamily, FontId, Layout, RichText, TextStyle};

use crate::audio::Sounds;
use crate::config::{Backend, Config};
use crate::db::attempt::{Attempt, Purpose, Target};
use crate::db::{CategoryTotal, SpendEntry, Store};
use crate::locale::Locale;
use crate::ui;
use crate::update::{self, Update};

/// The bold face the whole interface is set in.
const UBUNTU_BOLD: &[u8] = include_bytes!("../assets/fonts/Ubuntu-Bold.ttf");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Categories,
    Spending,
    Entries,
    Reports,
    Database,
}

impl Tab {
    const ALL: [Self; 5] = [
        Self::Categories,
        Self::Spending,
        Self::Entries,
        Self::Reports,
        Self::Database,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Categories => "Categories",
            Self::Spending => "Spending",
            Self::Entries => "Entries",
            Self::Reports => "Reports",
            Self::Database => "Database",
        }
    }
}

/// The last thing that happened, shown along the bottom of the window. Paired
/// with a sound at the moment it is set, never replayed on later frames.
pub struct Status {
    pub message: String,
    pub good: bool,
}

pub struct App {
    pub locale: Locale,
    pub sounds: Sounds,
    pub config: Config,
    /// `None` when no database could be opened; the Database tab says why.
    pub store: Option<Box<dyn Store>>,
    pub tab: Tab,
    pub year: i32,
    /// Cached so the table is not re-queried every frame; refreshed whenever
    /// something is written.
    pub totals: Vec<CategoryTotal>,
    pub foreign_entries: i64,
    /// Every entry in `year`, for the Entries tab — refreshed alongside
    /// `totals` for the same reason.
    pub entries: Vec<SpendEntry>,
    pub status: Option<Status>,
    pub categories: ui::categories::State,
    pub spending: ui::spending::State,
    pub entries_tab: ui::entries::State,
    pub reports: ui::reports::State,
    pub database: ui::database::State,
    /// Bumped whenever anything is written, so panes that cache what they
    /// have read know when to read it again.
    pub data_version: u64,
    /// A newer release, once the startup check has found one and the user has
    /// not already waved this version away.
    pub update: Option<Update>,
    update_check: update::Check,
    /// A database being opened on another thread.
    pub connection: Option<Attempt>,
}

impl App {
    pub fn new(cc: &CreationContext<'_>) -> Self {
        install_fonts_and_style(&cc.egui_ctx);

        let config = Config::load();
        let locale = Locale::detect();
        let database = ui::database::State::from_config(&config);

        let year = Local::now().year();
        let mut app = Self {
            locale,
            sounds: Sounds::new(),
            store: None,
            tab: Tab::Categories,
            year,
            totals: Vec::new(),
            foreign_entries: 0,
            entries: Vec::new(),
            status: None,
            categories: ui::categories::State::default(),
            spending: ui::spending::State::default(),
            entries_tab: ui::entries::State::default(),
            reports: ui::reports::State::new(year),
            database,
            update: None,
            connection: None,
            update_check: if config.check_for_updates {
                update::Check::start()
            } else {
                update::Check::disabled()
            },
            config,
            data_version: 0,
        };

        app.open_configured_store();
        app.spending.reset_date(&app.locale);
        app
    }

    /// Open whichever backend the saved configuration names.
    ///
    /// A local file is opened here and now. A server is opened on another
    /// thread, because a machine that is asleep, renamed or simply not there
    /// would otherwise hold up the window for the length of the timeout — and
    /// the first thing anyone would want to do about that is reach for the
    /// Database tab, which they cannot do while it is frozen.
    fn open_configured_store(&mut self) {
        let target = match self.config.backend {
            Backend::Sqlite => Target::Sqlite(self.config.sqlite_path()),
            Backend::MariaDb => Target::MariaDb(self.config.mariadb.clone()),
            Backend::MsSql => Target::MsSql(self.config.mssql.clone()),
        };

        if target.is_slow() {
            self.status = Some(Status {
                message: format!("Connecting to {}…", target.label()),
                good: true,
            });
            self.connection = Some(self.start_connection(target, Purpose::Adopt));
            return;
        }

        match target.open_and_check(self.year, self.locale.currency.code) {
            Ok(store) => {
                self.store = Some(store);
                self.reload_totals();
            }
            Err(message) => {
                // Starting with no database is survivable: the Database tab is
                // exactly where you would go to fix it.
                self.tab = Tab::Database;
                self.status = Some(Status {
                    message,
                    good: false,
                });
            }
        }
    }

    /// Begin opening a database in the background.
    pub fn start_connection(&self, target: Target, purpose: Purpose) -> Attempt {
        Attempt::start(target, purpose, self.year, self.locale.currency.code)
    }

    /// Is a connection being opened right now? The Database tab uses this to
    /// keep anyone from starting a second one on top of the first.
    pub fn is_connecting(&self) -> bool {
        self.connection.is_some()
    }

    /// Re-read the category totals from the database.
    ///
    /// Called after every write, so it is also where the rest of the app is
    /// told that what it has read is now out of date.
    pub fn reload_totals(&mut self) {
        self.data_version = self.data_version.wrapping_add(1);
        let year = self.year;
        let currency = self.locale.currency.code;
        let Some(store) = self.store.as_mut() else {
            self.totals.clear();
            self.foreign_entries = 0;
            self.entries.clear();
            return;
        };
        match store.categories_with_totals(year, currency) {
            Ok(totals) => {
                self.totals = totals;
                self.foreign_entries = store
                    .entries_in_other_currencies(year, currency)
                    .unwrap_or(0);
                self.entries = store.spending_in_year(year, currency).unwrap_or_default();
            }
            Err(err) => {
                self.totals.clear();
                self.foreign_entries = 0;
                self.entries.clear();
                self.status = Some(Status {
                    message: format!("Could not read the categories: {err}"),
                    good: false,
                });
            }
        }
    }

    /// Report success: a message along the bottom, and the success sound.
    pub fn report_ok(&mut self, message: impl Into<String>) {
        self.status = Some(Status {
            message: message.into(),
            good: true,
        });
        self.sounds.success();
    }

    /// Report a failure the same way, with the other sound.
    pub fn report_error(&mut self, message: impl Into<String>) {
        self.status = Some(Status {
            message: message.into(),
            good: false,
        });
        self.sounds.failure();
    }

    /// The names for the category picker on the Spending tab.
    pub fn category_names(&self) -> Vec<String> {
        self.totals.iter().map(|c| c.name.clone()).collect()
    }

    fn tab_strip(&mut self, ui: &mut egui::Ui) {
        ui.add_space(18.0);

        for tab in Tab::ALL {
            let selected = self.tab == tab;
            let button = egui::Button::selectable(selected, ui::centred(tab.title()))
                .corner_radius(6.0)
                .min_size(egui::vec2(ui.available_width(), 44.0));
            if ui.add(button).clicked() {
                self.tab = tab;
            }
            ui.add_space(6.0);
        }

        // The locale sits under the tabs because every figure in the app is
        // formatted by it, and it is worth being able to see which one won.
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(12.0);
            ui.label(
                RichText::new(format!(
                    "{} · {}",
                    self.locale.tag, self.locale.currency.code
                ))
                .size(12.0)
                .weak(),
            );
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        // Where the data is on the left, what just happened on the right. The
        // path is the part that gives way when the window is narrow: a message
        // about what the app just did is worth more than the middle of a path.
        egui::Sides::new().shrink_left().show(
            ui,
            |ui| {
                let where_it_is = self
                    .store
                    .as_ref()
                    .map_or_else(|| "No database".to_owned(), |s| s.describe());
                ui.add(egui::Label::new(RichText::new(where_it_is).size(12.0).weak()).truncate());
            },
            |ui| {
                if let Some(status) = &self.status {
                    let colour = if status.good {
                        ui::good_colour(ui)
                    } else {
                        ui::bad_colour(ui)
                    };
                    ui.label(RichText::new(&status.message).size(12.0).color(colour));
                }
            },
        );
        ui.add_space(4.0);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // "This year" has to mean the year it is now, including for a window
        // that was left open over New Year's Eve.
        let year = Local::now().year();
        if year != self.year {
            self.year = year;
            self.reload_totals();
        }

        // The check runs on its own thread; this is where its answer, if it
        // ever comes, is picked up. A version already dismissed is dropped
        // here rather than shown and closed again.
        if let Some(found) = self.update_check.poll()
            && self.config.dismissed_update.as_deref() != Some(found.version.as_str())
        {
            self.update = Some(found);
        }

        if let Some(attempt) = self.connection.as_mut()
            && let Some(result) = attempt.poll()
        {
            let attempt = self.connection.take().expect("just polled it");
            ui::database::connection_finished(self, &attempt, result);
        }

        // egui only draws when something happens, and a background thread
        // finishing is not something it counts. Without this, an answer can
        // sit in its channel until the user happens to move the mouse.
        if self.connection.is_some() || self.update_check.is_running() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }

        egui::Panel::left("tabs")
            .resizable(false)
            .exact_size(190.0)
            .show(ui, |ui| self.tab_strip(ui));

        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));

        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Categories => ui::categories::show(self, ui),
            Tab::Spending => ui::spending::show(self, ui),
            Tab::Entries => ui::entries::show(self, ui),
            Tab::Reports => ui::reports::show(self, ui),
            Tab::Database => ui::database::show(self, ui),
        });

        ui::update_box::show(self, &ui.ctx().clone());
    }
}

/// Use the bold Ubuntu face from `assets/` throughout, and give everything a
/// little more room than egui's defaults, which are tuned for dense tools
/// rather than for forms someone types money into.
fn install_fonts_and_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "ubuntu-bold".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(UBUNTU_BOLD)),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "ubuntu-bold".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("ubuntu-bold".to_owned());
    ctx.set_fonts(fonts);

    // Applied to both themes, since the window follows whichever the system
    // is set to.
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(24.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(15.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(12.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(14.0, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.interact_size.y = 28.0;
    });
}
