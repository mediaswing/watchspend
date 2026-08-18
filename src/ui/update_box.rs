//! The box that appears when there is a newer release.
//!
//! Two ways out, as few as the choice allows: go and look at it, or dismiss
//! it. Dismissing remembers the version, so the same release is never
//! mentioned twice — a prompt that returns every morning is one people learn
//! to click through without reading, which is a bad habit to teach anyone
//! about software updates.

use egui::{Id, RichText};

use crate::app::App;
use crate::update::{CURRENT, Update};
use crate::{t, ui};

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(update) = app.update.clone() else {
        return;
    };

    let mut go = false;
    let mut dismiss = false;
    let mut keep_checking = app.config.check_for_updates;

    let modal = egui::Modal::new(Id::new("update-available")).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.heading(t!("update.title"));
        ui.add_space(6.0);
        ui.label(t!(
            "update.body",
            current = CURRENT,
            version = update.version
        ));
        ui.add_space(4.0);
        ui.label(
            RichText::new(t!("update.nothing_downloaded"))
                .size(12.0)
                .weak(),
        );

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
            if ui
                .add_sized(
                    [width, 36.0],
                    egui::Button::new(ui::centred(t!("update.dismiss"))),
                )
                .on_hover_text(t!("update.dismiss_hint"))
                .clicked()
            {
                dismiss = true;
            }
            if ui
                .add_sized(
                    [width, 36.0],
                    egui::Button::new(ui::centred(t!("update.open_page"))),
                )
                .clicked()
            {
                go = true;
            }
        });

        ui.add_space(8.0);
        ui.checkbox(&mut keep_checking, t!("settings.check_for_updates"));
    });

    // Clicking away or pressing Escape leaves the question for next time,
    // rather than deciding it.
    let closed_without_answering = modal.should_close();

    if keep_checking != app.config.check_for_updates {
        app.config.check_for_updates = keep_checking;
        save(app);
    }

    if go {
        // egui hands this to the platform, which opens the default browser.
        ctx.open_url(egui::OpenUrl::new_tab(&update.page));
        remember(app, &update);
    } else if dismiss {
        remember(app, &update);
    } else if closed_without_answering {
        app.update = None;
    }
}

/// Note the version as answered, so it is not raised again.
fn remember(app: &mut App, update: &Update) {
    app.config.dismissed_update = Some(update.version.clone());
    app.update = None;
    save(app);
}

fn save(app: &mut App) {
    if let Err(err) = app.config.save() {
        log::warn!("could not save the update setting: {err}");
    }
}
