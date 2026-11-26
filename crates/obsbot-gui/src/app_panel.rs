//! The App panel: how the control center itself behaves.

use crate::app_settings::{set_start_at_login, starts_at_login, AppSettings};
use crate::effects::EffectsController;
use crate::theme::*;
use crate::widgets::{caption, card, hint, switch_row};
use eframe::egui::{self, RichText, Ui};

/// Draw the panel. `has_tray` says whether a tray icon could be shown.
pub fn show(ui: &mut Ui, app: &mut AppSettings, effects: &mut EffectsController, has_tray: bool) {
    let before = app.clone();
    let mut session_changed = false;
    let mut problem: Option<String> = None;

    card(
        ui,
        "🏠",
        "Start-up",
        |_| {},
        |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("START WITH").size(9.0).color(TEXT_MUTED));
                let shown = if app.startup_profile.is_empty() {
                    "My last settings".to_owned()
                } else {
                    app.startup_profile.clone()
                };
                egui::ComboBox::from_id_salt("startup-profile")
                    .width(190.0)
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut app.startup_profile,
                            String::new(),
                            "My last settings",
                        );
                        for profile in &effects.profiles.profiles {
                            ui.selectable_value(
                                &mut app.startup_profile,
                                profile.name.clone(),
                                profile.name.as_str(),
                            );
                        }
                    });
            });
            hint(
                ui,
                "Pick a work profile here and you never join a call with yesterday's effects.",
            );
            switch_row(ui, "Switch effects on at start", &mut app.start_effects);
            switch_row(
                ui,
                "Keep running when the window is closed",
                &mut app.keep_running,
            );
            if app.keep_running && !has_tray {
                hint(
                    ui,
                    "No tray was found on this desktop. Reopen the window by starting the app \
                 again or with “obsbotctl app show”.",
                );
            }
            let mut login = starts_at_login();
            if switch_row(ui, "Start in the tray when I log in", &mut login) {
                if let Err(error) = set_start_at_login(login) {
                    problem = Some(format!("Could not change the login entry: {error}"));
                }
            }
        },
    );
    ui.add_space(10.0);

    card(
        ui,
        "💬",
        "Away message",
        |_| {},
        |ui| {
            hint(
                ui,
                "Shown by “Be right back” above the preview, in the tray, or from a shortcut.",
            );
            let edit = ui.add(
                egui::TextEdit::singleline(&mut effects.session.hold_text)
                    .hint_text("Be right back")
                    .char_limit(48)
                    .desired_width(f32::INFINITY),
            );
            session_changed |= edit.changed();
        },
    );
    ui.add_space(10.0);

    card(
        ui,
        "🔒",
        "Automatic privacy",
        |_| {},
        |ui| {
            switch_row(
                ui,
                "Sleep the camera while the screen is locked",
                &mut app.sleep_on_lock,
            );
            switch_row(
                ui,
                "Sleep the camera when nothing is watching",
                &mut app.sleep_when_unused,
            );
            hint(
                ui,
                "“Watching” means this window is open or another app has the virtual camera \
             open. The camera wakes again by itself — unless you put it to sleep yourself.",
            );
        },
    );
    ui.add_space(10.0);

    card(
        ui,
        "📷",
        "Camera",
        |_| {},
        |ui| {
            switch_row(
                ui,
                "Put my image settings back when it connects",
                &mut app.restore_camera,
            );
            hint(
                ui,
                "Zoom, field of view, tracking mode, exposure, white balance and the other \
             Image settings are saved as you change them.",
            );
        },
    );
    ui.add_space(10.0);

    card(
        ui,
        "⚡",
        "Shortcuts",
        |_| {},
        |ui| {
            hint(
                ui,
                "Inside this window, Ctrl+1 … Ctrl+9 switch profiles. For shortcuts that work \
             everywhere, bind these commands in your desktop's keyboard settings:",
            );
            caption(ui, "Commands");
            for command in [
                "obsbotctl app brb",
                "obsbotctl app freeze",
                "obsbotctl app privacy toggle",
                "obsbotctl app effects toggle",
                "obsbotctl app profile next",
                "obsbotctl app profile Meeting",
                "obsbotctl app position Whiteboard",
                "obsbotctl app show",
            ] {
                ui.horizontal(|ui| {
                    if ui
                        .small_button("Copy")
                        .on_hover_text("Copy this command")
                        .clicked()
                    {
                        ui.ctx().copy_text(command.to_owned());
                    }
                    ui.label(RichText::new(command).monospace().size(11.0));
                });
            }
        },
    );

    if *app != before {
        if let Err(error) = app.save() {
            problem = Some(format!("Could not save settings: {error}"));
        }
    }
    if session_changed {
        if let Err(error) = effects.save() {
            problem = Some(format!("Could not save settings: {error}"));
        }
    }
    if let Some(problem) = problem {
        ui.add_space(8.0);
        ui.label(RichText::new(problem).size(10.5).color(WARNING));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_renders_and_leaves_settings_alone() {
        let path = std::env::temp_dir().join(format!("obsbot-panel-{}.conf", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut app = AppSettings::from_file(path.clone());
        app.keep_running = true;
        let before = app.clone();
        let mut effects = EffectsController::for_tests();
        let ctx = egui::Context::default();
        for has_tray in [true, false] {
            for _ in 0..2 {
                let output = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| show(ui, &mut app, &mut effects, has_tray));
                });
                assert!(!output.shapes.is_empty());
            }
        }
        assert_eq!(app, before);
        assert!(
            !path.exists(),
            "nothing changed, so nothing should be written"
        );
        let icons = ["🏠", "💬", "🔒", "📷", "⚡"];
        assert_eq!(crate::widgets::missing_glyphs(&icons), Vec::<&str>::new());
    }
}
