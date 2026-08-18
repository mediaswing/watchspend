//! The Settings pane: what language the app speaks, and what colour it is.
//!
//! Everything here is about how the app presents itself, which is why it is not
//! on the Database tab: where the figures are kept and what language they are
//! labelled in are unrelated questions, and the second one is the only page in
//! the app somebody might have to find while unable to read a word of it.
//!
//! That is also why the language picker is first. Someone who has opened the
//! app in a language they cannot read needs one control, and the only thing
//! they can rely on to find it is where it sits — not what it says.

use egui::{RichText, Ui};

use crate::app::App;
use crate::config::Appearance;
use crate::{i18n, t, tn, ui};

pub fn show(app: &mut App, ui: &mut Ui) {
    ui::pane_header(ui, &t!("settings.title"), &t!("settings.subtitle"));

    let mut edited = false;
    egui::ScrollArea::vertical().show(ui, |ui| {
        edited |= language(app, ui);
        ui.add_space(16.0);
        edited |= appearance(app, ui);
        ui.add_space(16.0);
        edited |= updates(app, ui);

        ui.add_space(16.0);
        ui.label(
            RichText::new(t!(
                "database.settings_kept_in",
                path = crate::config::tilde(&crate::config::config_path())
            ))
            .size(12.0)
            .weak(),
        );
    });

    if edited && let Err(err) = app.config.save() {
        app.report_error(t!("settings.could_not_save", error = err));
    }
}

/// The language picker, the problems with the file in use, and the folder a
/// translator works in.
fn language(app: &mut App, ui: &mut Ui) -> bool {
    let mut edited = false;
    ui.label(RichText::new(t!("settings.language")).size(13.0));

    // Each language is named in its own language, since somebody looking for
    // theirs is looking for the word they call it by, not ours.
    let available = i18n::available();
    let selected = if app.config.language == i18n::AUTO {
        t!("settings.language.system")
    } else {
        available
            .iter()
            .find(|(code, _)| *code == app.config.language)
            .map_or_else(|| app.config.language.clone(), |(_, name)| name.clone())
    };

    let mut chosen = app.config.language.clone();
    egui::ComboBox::from_id_salt("settings-language")
        .selected_text(selected)
        .width(ui.available_width() - 10.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut chosen,
                i18n::AUTO.to_owned(),
                t!("settings.language.system"),
            );
            ui.separator();
            for (code, name) in &available {
                ui.selectable_value(&mut chosen, code.clone(), name);
            }
        });

    if chosen != app.config.language {
        app.config.language = chosen;
        i18n::apply_setting(&app.config.language);
        app.report_ok(t!("status.language_changed", name = i18n::current_name()));
        edited = true;
    }

    // Files in the folder that never became a language at all, first — a
    // translator whose file is simply not in the picker above has no other way
    // to find out why, and this is the failure that costs them the most time.
    let files = i18n::folder_problems();
    if !files.is_empty() {
        ui.add_space(4.0);
        ui::error_text(ui, &tn!("settings.language.file_count", files.len() as u64));
        for file in &files {
            let name = crate::config::tilde(&file.path);
            ui::error_text(
                ui,
                &match file.reason {
                    i18n::FileReason::Unreadable => {
                        t!("settings.language.file_unreadable", path = name)
                    }
                    i18n::FileReason::NoCode => t!("settings.language.file_no_code", path = name),
                    i18n::FileReason::WouldReplaceEnglish => {
                        t!("settings.language.file_is_english", path = name)
                    }
                },
            );
        }
    }

    // Then whatever the current file could not be read as, said plainly: a
    // translator's first draft always has a stray quote in it somewhere, and
    // hunting for it without a line number is miserable.
    let problems = i18n::current_problems();
    if !problems.is_empty() {
        ui.add_space(4.0);
        ui::error_text(
            ui,
            &tn!("settings.language.problem_count", problems.len() as u64),
        );
        for problem in problems.iter().take(10) {
            ui::error_text(
                ui,
                &t!(
                    "settings.language.problem",
                    line = problem.line,
                    what = problem.what
                ),
            );
        }
    }

    ui.add_space(6.0);
    ui.add(
        egui::Label::new(
            RichText::new(t!("settings.language.help"))
                .size(12.0)
                .weak(),
        )
        .wrap(),
    );

    let dir = i18n::languages_dir();
    ui.add_space(6.0);
    ui.add(
        egui::Label::new(
            RichText::new(t!(
                "settings.language.folder",
                path = crate::config::tilde(&dir)
            ))
            .size(12.0)
            .weak(),
        )
        .wrap(),
    );
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
        if ui
            .add_sized(
                [width, 32.0],
                egui::Button::new(ui::centred(t!("settings.language.open_folder"))),
            )
            .clicked()
        {
            // Created on the way, so the button never opens nothing — "put a
            // file in this folder" is no use if the folder is only made once a
            // file is already in it.
            if let Err(err) = std::fs::create_dir_all(&dir) {
                log::warn!("languages: could not create {} — {err}", dir.display());
            }
            ui.ctx().open_url(egui::OpenUrl::same_tab(file_url(&dir)));
        }
        if ui
            .add_sized(
                [width, 32.0],
                egui::Button::new(ui::centred(t!("settings.language.reload"))),
            )
            .clicked()
        {
            // The translator's edit-and-see-it loop: change a line, press this,
            // watch the interface change. Without it the loop runs through a
            // restart, which is slow enough to make a long file a chore.
            i18n::reload();
            app.report_ok(t!("status.language_reloaded", name = i18n::current_name()));
        }
    });

    edited
}

/// A path as a `file:` URL the platform's opener will actually accept.
///
/// `format!("file://{path}")` is wrong twice over. On Windows the path is
/// `C:\Users\…`, and `file://C:\Users\…` reads `C:` as the *host* rather than
/// as a drive, so the folder never opens; the separators have to be forward
/// slashes and the whole thing needs the third one. And the default folder on
/// macOS is inside `Library/Application Support`, whose space ends the URL as
/// far as some openers are concerned — so the handful of characters that mean
/// something in a URL are percent-encoded.
fn file_url(path: &std::path::Path) -> String {
    let mut out = String::from("file://");
    let text = path.display().to_string().replace('\\', "/");
    if !text.starts_with('/') {
        out.push('/');
    }
    for c in text.chars() {
        match c {
            '%' => out.push_str("%25"),
            ' ' => out.push_str("%20"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            _ => out.push(c),
        }
    }
    out
}

/// The light/dark picker.
///
/// The choice takes effect on the next frame — which is this one — so the
/// window changes colour under the cursor as the answer is picked, rather than
/// needing a restart to show what was chosen.
fn appearance(app: &mut App, ui: &mut Ui) -> bool {
    ui.label(RichText::new(t!("settings.appearance")).size(13.0));

    let mut chosen = app.config.appearance;
    egui::ComboBox::from_id_salt("settings-appearance")
        .selected_text(chosen.label())
        .width(ui.available_width() - 10.0)
        .show_ui(ui, |ui| {
            for option in Appearance::ALL {
                // The description is the hover text on each row rather than a
                // second line inside it: a row that repeats its own explanation
                // every time the pointer passes over it is slower to read
                // through, not clearer.
                ui.selectable_value(&mut chosen, option, option.label())
                    .on_hover_text(option.description());
            }
        });

    if chosen == app.config.appearance {
        return false;
    }
    app.config.appearance = chosen;
    crate::theme::apply_appearance(ui.ctx(), chosen);
    app.report_ok(t!("status.appearance_changed", name = chosen.label()));
    true
}

/// The startup update check, which used to be reachable only from inside the
/// box that offers an update — a place you cannot get back to once you have
/// dismissed it.
fn updates(app: &mut App, ui: &mut Ui) -> bool {
    ui.label(RichText::new(t!("settings.updates")).size(13.0));

    let mut checking = app.config.check_for_updates;
    ui.checkbox(&mut checking, t!("settings.check_for_updates"));
    ui.label(
        RichText::new(t!("settings.check_for_updates.hint"))
            .size(12.0)
            .weak(),
    );

    if checking == app.config.check_for_updates {
        return false;
    }
    app.config.check_for_updates = checking;
    true
}

#[cfg(test)]
mod tests {
    use super::file_url;
    use std::path::Path;

    /// The two shapes that were wrong before: a Windows path, whose drive
    /// letter was being read as a host name, and the default macOS folder,
    /// whose space ended the URL early.
    #[test]
    fn a_folder_becomes_a_url_its_platform_will_open() {
        assert_eq!(
            file_url(Path::new(r"C:\Users\sam\AppData\Roaming\GAS\languages")),
            "file:///C:/Users/sam/AppData/Roaming/GAS/languages"
        );
        assert_eq!(
            file_url(Path::new("/Users/sam/Library/Application Support/GAS")),
            "file:///Users/sam/Library/Application%20Support/GAS"
        );
    }
}
