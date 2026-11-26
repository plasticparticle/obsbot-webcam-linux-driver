//! The Effects panel: virtual camera switch plus one tab per effect family.

use crate::backgrounds::BackdropLibrary;
use crate::effects::{
    Active, BeautyPreset, Choice, EffectSettings, EffectsController, Group, ANONYMISE,
    ANONYMISE_STYLES, BACKGROUNDS, DISTORTIONS, EYE_COLORS, FIRST_GESTURE, IDEA, LIGHT_PRESETS,
    LIGHT_X, LIGHT_Y, LIP_COLORS, LOOKS, MAGIC_COLORS, PAINTS, QUESTIONS, RESOLUTIONS, RIM_COLORS,
    STICKERS, STORM, TRIGGERS,
};
use crate::effects::{
    BadgeDesign, Broadcast, Session, BADGE_COLORS, BADGE_LAYOUTS, BADGE_STYLES, MAX_FACES,
};
use crate::fonts::FontList;
use crate::luts::LutLibrary;
use crate::picker::FilePicker;
use crate::preview::Preview;
use crate::profiles::{ProfileStore, MAX_NAME_CHARS, NEUTRAL, SHORTCUTS};
use crate::theme::*;
use crate::widgets::{
    amount, caption, card, chip, color_dot, grid, hint, icon_tile, image_tile, light_pad, link,
    pill, position_picker, segmented, status_pill, swatch_tile, switch_row, tab_bar, toggle,
};
use eframe::egui::{self, RichText, Sense, Ui, Vec2};
use std::path::Path;

/// Draw the panel and apply whatever the user changed.
pub fn show(
    ui: &mut Ui,
    effects: &mut EffectsController,
    preview: &mut Preview,
    input: Option<&Path>,
    tab: &mut Group,
) {
    let before = effects.settings.clone();
    let session_before = effects.session.clone();
    let mut power = effects.enabled;
    let mut restart = false;

    camera_card(ui, effects, &mut power, &mut restart);
    ui.add_space(10.0);
    profiles_card(ui, &mut effects.settings, &mut effects.profiles);
    ui.add_space(10.0);

    let tabs: Vec<(&str, &str, usize)> = Group::ALL
        .iter()
        .map(|group| (group.icon(), group.label(), effects.settings.count(*group)))
        .collect();
    let mut selected = Group::ALL
        .iter()
        .position(|group| group == tab)
        .unwrap_or(0);
    tab_bar(ui, &tabs, 4, &mut selected);
    *tab = Group::ALL[selected];
    ui.add_space(8.0);

    let settings = &mut effects.settings;
    let luts = &mut effects.luts;
    let session = &mut effects.session;
    let backdrops = &mut effects.backdrops;
    let fonts = &mut effects.fonts;
    let group = *tab;
    let mut reset = false;
    let resettable = settings.count(group) > 0;
    card(
        ui,
        group.icon(),
        group.title(),
        |ui| {
            if resettable {
                reset = link(ui, "Reset").clicked();
            }
        },
        |ui| match group {
            Group::Looks => looks_tab(ui, settings, luts),
            Group::Lights => lights_tab(ui, settings),
            Group::Beauty => beauty_tab(ui, settings),
            Group::Stickers => stickers_tab(ui, settings),
            Group::Funny => funny_tab(ui, settings),
            Group::Magic => magic_tab(ui, settings),
            Group::Paint => paint_tab(ui, settings),
            Group::Backdrop => scene_tab(ui, settings, session, backdrops, fonts),
        },
    );
    if reset {
        settings.reset(group);
    }

    if power != effects.enabled {
        if power {
            if let Some(input) = input {
                let _ = effects.start(input, preview);
            }
        } else {
            effects.stop_by_user(preview);
        }
    } else if restart {
        if let Some(input) = input {
            effects.restart(input, preview);
        }
    } else if effects.settings != before || effects.session != session_before {
        effects.settings_changed(input, preview);
    }
}

/// Removable summary of everything that is switched on, shown above the preview.
pub fn active_chips(
    ui: &mut Ui,
    effects: &mut EffectsController,
    preview: &mut Preview,
    input: Option<&Path>,
) {
    let active = effects.settings.active();
    if active.is_empty() {
        return;
    }
    let mut remove: Option<Active> = None;
    let mut clear_all = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
        for (entry, label) in &active {
            if chip(ui, label) {
                remove = Some(*entry);
            }
        }
        if active.len() > 1 {
            clear_all = link(ui, "Clear all").clicked();
        }
    });
    if clear_all {
        effects.settings.reset_all();
    } else if let Some(entry) = remove {
        effects.settings.clear(entry);
    } else {
        return;
    }
    effects.settings_changed(input, preview);
}

/// Saved sets of every effect setting, one click (or Ctrl+number) away.
fn profiles_card(ui: &mut Ui, settings: &mut EffectSettings, store: &mut ProfileStore) {
    let selected = store.matching(settings).map(|profile| profile.name.clone());
    let mut open_naming = false;
    let can_save_as = store.draft.is_none() && selected.is_none();
    card(
        ui,
        "💾",
        "Profiles",
        |ui| {
            if can_save_as {
                open_naming = link(ui, "Save as…")
                    .on_hover_text("Save every current effect setting as a profile")
                    .clicked();
            }
        },
        |ui| {
            if store.profiles.is_empty() && store.draft.is_none() {
                hint(
                    ui,
                    "Set up your effects, then choose “Save as…” to get back to them with one click.",
                );
            }
            let mut apply: Option<usize> = None;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                for (index, profile) in store.profiles.iter().enumerate() {
                    let active = selected.as_deref() == Some(profile.name.as_str());
                    let response = pill(ui, &profile.name, active);
                    let response = if index < SHORTCUTS {
                        response.on_hover_text(format!("Ctrl+{}", index + 1))
                    } else {
                        response
                    };
                    if response.clicked() {
                        apply = Some(index);
                    }
                }
            });
            if let Some(index) = apply {
                let profile = &store.profiles[index];
                *settings = profile.settings.clone();
                store.current = Some(profile.name.clone());
                store.confirm_delete = None;
                store.message = None;
            }

            if let Some(draft) = &mut store.draft {
                let mut save = false;
                let mut cancel = false;
                ui.horizontal(|ui| {
                    cancel = link(ui, "Cancel").clicked();
                    save = ui.button("Save").clicked();
                    let edit = ui.add(
                        egui::TextEdit::singleline(draft)
                            .hint_text("Profile name")
                            .char_limit(MAX_NAME_CHARS)
                            .desired_width(f32::INFINITY),
                    );
                    if edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                        save = true;
                    }
                    if std::mem::take(&mut store.focus_draft) {
                        edit.request_focus();
                    }
                });
                if save {
                    let name = draft.clone();
                    match store.save(&name, settings) {
                        Ok(saved) => {
                            store.draft = None;
                            store.message = Some((format!("Saved “{saved}”."), false));
                        }
                        Err(error) => store.message = Some((error, true)),
                    }
                } else if cancel {
                    store.draft = None;
                    store.message = None;
                }
            } else if selected.as_deref() == Some(NEUTRAL) {
                hint(ui, "Every effect is off. This profile is built in.");
            } else if let Some(name) = &selected {
                // Showing a saved profile untouched: the only thing left to do is delete it.
                let confirming = store.confirm_delete.as_deref() == Some(name.as_str());
                let label = if confirming {
                    format!("Click again to delete “{name}”")
                } else {
                    format!("Delete “{name}”")
                };
                if link(ui, &label).clicked() {
                    if confirming {
                        store.confirm_delete = None;
                        store.message = Some(match store.delete(name) {
                            Ok(()) => (format!("Deleted “{name}”."), false),
                            Err(error) => (error, true),
                        });
                    } else {
                        store.confirm_delete = Some(name.clone());
                    }
                }
            } else if let Some(name) = store
                .current
                .clone()
                .filter(|name| store.find(name).is_some())
            {
                // A profile was applied and then tweaked.
                store.confirm_delete = None;
                ui.horizontal(|ui| {
                    hint(ui, &format!("“{name}” has unsaved changes."));
                    if link(ui, "Update").clicked() {
                        store.message = Some(match store.save(&name, settings) {
                            Ok(saved) => (format!("Updated “{saved}”."), false),
                            Err(error) => (error, true),
                        });
                    }
                    if link(ui, "Revert").clicked() {
                        if let Some(profile) = store.find(&name) {
                            *settings = profile.settings.clone();
                        }
                    }
                });
            }
            if let Some((text, is_error)) = &store.message {
                message(ui, text, if *is_error { WARNING } else { SUCCESS });
            }
        },
    );
    if open_naming {
        store.draft = Some(String::new());
        store.focus_draft = true;
        store.message = None;
    }
}

fn camera_card(ui: &mut Ui, effects: &mut EffectsController, power: &mut bool, restart: &mut bool) {
    let (enabled, status) = (effects.enabled, effects.status);
    let publishing = effects.publishing;
    card(
        ui,
        "🎥",
        "Effects camera",
        |ui| {
            toggle(ui, power, "Effects camera").on_hover_text(if enabled {
                "Switch effects off"
            } else {
                "Switch effects on"
            });
            match (enabled, status) {
                (true, Some(status)) => status_pill(
                    ui,
                    &format!(
                        "{}  {:.0} fps",
                        if publishing { "LIVE" } else { "PREVIEW" },
                        status.fps
                    ),
                    SUCCESS,
                ),
                (true, None) => status_pill(ui, "STARTING", WARNING),
                (false, _) => status_pill(ui, "OFF", TEXT_MUTED),
            }
        },
        |ui| {
            let broadcast = effects.broadcast();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Virtual camera").size(12.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    status_pill(
                        ui,
                        &broadcast.label(),
                        match broadcast {
                            Broadcast::NotSetUp | Broadcast::Connecting => WARNING,
                            Broadcast::Idle => TEXT_MUTED,
                            Broadcast::Live { .. } => SUCCESS,
                        },
                    );
                });
            });
            hint(ui, broadcast.explanation());
            ui.add_space(4.0);
            hint(
                ui,
                if effects.enabled {
                    "Your effects are showing in the preview."
                } else if effects.settings.any_active() {
                    "Switch on to see your effects here and in other apps."
                } else if effects.starts_automatically() {
                    "Pick any effect below and the virtual camera starts by itself."
                } else {
                    "Switch on to see your effects."
                },
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("OUTPUT").size(9.0).color(TEXT_MUTED));
                egui::ComboBox::from_id_salt("effects-resolution")
                    .selected_text(effects.selected_resolution().label)
                    .show_ui(ui, |ui| {
                        for (index, resolution) in RESOLUTIONS.iter().enumerate() {
                            *restart |= ui
                                .selectable_value(&mut effects.resolution, index, resolution.label)
                                .changed();
                        }
                    });
                egui::ComboBox::from_id_salt("effects-fps")
                    .selected_text(format!("{} fps", effects.fps))
                    .show_ui(ui, |ui| {
                        for fps in [15, 30] {
                            *restart |= ui
                                .selectable_value(&mut effects.fps, fps, format!("{fps} fps"))
                                .changed();
                        }
                    });
            });

            if let Some(notice) = &effects.notice {
                message(
                    ui,
                    &notice.text,
                    if notice.is_error { WARNING } else { SUCCESS },
                );
            } else if let (true, Some(status)) = (effects.enabled, effects.status) {
                if status.face == Some(false) && effects.settings.needs_face() {
                    message(
                        ui,
                        "No face in view — face effects appear once you are in frame.",
                        WARNING,
                    );
                } else if status.fps < effects.fps as f32 * 0.7 {
                    message(
                        ui,
                        "Running slowly. Try a smaller output size or fewer effects.",
                        WARNING,
                    );
                }
            }
        },
    );
}

fn message(ui: &mut Ui, text: &str, color: egui::Color32) {
    ui.label(RichText::new(text).size(10.5).color(color));
}

fn looks_tab(ui: &mut Ui, settings: &mut EffectSettings, luts: &mut LutLibrary) {
    grid(ui, LOOKS.len(), 4, 70.0, |ui, index, size| {
        let look = &LOOKS[index];
        let response = swatch_tile(ui, size, look.swatch, look.label, settings.look == index)
            .on_hover_text(look.hint);
        if response.clicked() {
            settings.look = index;
        }
    });
    if settings.look != 0 {
        amount(ui, "Intensity", &mut settings.look_strength, 0.0..=1.0, 1.0);
    }

    lut_section(ui, settings, luts);

    caption(ui, "Film finish");
    amount(ui, "Grain", &mut settings.grain, 0.0..=1.0, 0.0);
    amount(ui, "Vignette", &mut settings.vignette, 0.0..=1.0, 0.0);
    amount(ui, "Glow", &mut settings.glow, 0.0..=1.0, 0.0);
}

/// LUT tiles plus importing your own `.cube` files.
fn lut_section(ui: &mut Ui, settings: &mut EffectSettings, luts: &mut LutLibrary) {
    luts.rescan_if_due();
    if let Some(picked) = luts.picker.take() {
        import_lut(settings, luts, &picked);
    }
    if !settings.lut.is_empty() && luts.find(&settings.lut).is_none() {
        // The selected file was deleted or renamed outside the app.
        settings.lut.clear();
    }

    caption(ui, "LUTs");
    hint(
        ui,
        "Film-style colour tables. Combine one with a look above, or add your own.",
    );
    let mut select: Option<String> = None;
    grid(ui, luts.luts.len(), 4, 70.0, |ui, index, size| {
        let lut = &luts.luts[index];
        let path = lut.path.to_string_lossy();
        let selected = settings.lut == path;
        let response =
            swatch_tile(ui, size, lut.swatch, &lut.name, selected).on_hover_text(if lut.user {
                format!("{} — imported", lut.name)
            } else {
                lut.name.clone()
            });
        if response.clicked() {
            // Clicking the active LUT again switches it off.
            select = Some(if selected {
                String::new()
            } else {
                path.into_owned()
            });
        }
    });
    if let Some(path) = select {
        settings.lut = path;
    }

    let selected_user_lut = luts
        .find(&settings.lut)
        .filter(|lut| lut.user)
        .map(|lut| lut.path.clone());
    if !settings.lut.is_empty() {
        amount(
            ui,
            "LUT intensity",
            &mut settings.lut_strength,
            0.0..=1.0,
            1.0,
        );
    }

    ui.horizontal(|ui| {
        if FilePicker::available() {
            let busy = luts.picker.busy();
            let label = if busy { "Choosing…" } else { "Add LUT…" };
            if ui
                .add_enabled(!busy, egui::Button::new(label))
                .on_hover_text("Import a .cube file")
                .clicked()
            {
                luts.picker
                    .open("Choose a .cube LUT", "LUT files", &["*.cube", "*.CUBE"]);
            }
        }
        if ui
            .button("Open folder")
            .on_hover_text("Drop .cube files here and they appear above")
            .clicked()
        {
            let _ = std::fs::create_dir_all(luts.user_dir());
            let _ = std::process::Command::new("xdg-open")
                .arg(luts.user_dir())
                .spawn();
        }
        if let Some(path) = &selected_user_lut {
            if link(ui, "Remove this LUT").clicked() {
                match luts.remove(path) {
                    Ok(()) => {
                        settings.lut.clear();
                        luts.message = Some(("LUT removed.".into(), false));
                    }
                    Err(error) => luts.message = Some((error, true)),
                }
            }
        }
    });
    ui.horizontal(|ui| {
        let import = ui
            .add_enabled(
                !luts.import_path.trim().is_empty(),
                egui::Button::new("Import"),
            )
            .clicked();
        let edit = ui.add(
            egui::TextEdit::singleline(&mut luts.import_path)
                .hint_text("…or paste the path of a .cube file")
                .desired_width(f32::INFINITY),
        );
        let entered = edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if (import || entered) && !luts.import_path.trim().is_empty() {
            let path = std::path::PathBuf::from(luts.import_path.trim());
            if import_lut(settings, luts, &path) {
                luts.import_path.clear();
            }
        }
    });
    if let Some((text, is_error)) = &luts.message {
        message(ui, text, if *is_error { WARNING } else { SUCCESS });
    }
}

/// Import `path` into the library and select it. Returns true on success.
fn import_lut(settings: &mut EffectSettings, luts: &mut LutLibrary, path: &Path) -> bool {
    match luts.import(path) {
        Ok(imported) => {
            settings.lut = imported.to_string_lossy().into_owned();
            settings.lut_strength = 1.0;
            luts.message = Some(("LUT added and applied.".into(), false));
            true
        }
        Err(error) => {
            luts.message = Some((error, true));
            false
        }
    }
}

fn lights_tab(ui: &mut Ui, settings: &mut EffectSettings) {
    amount(
        ui,
        "Auto-correct",
        &mut settings.auto_enhance,
        0.0..=1.0,
        0.0,
    );
    hint(
        ui,
        "Fixes dim rooms and colour casts by itself. Start here, then add lights if you like.",
    );
    caption(ui, "Studio lights");
    let current = settings.light_preset();
    grid(ui, LIGHT_PRESETS.len(), 4, 68.0, |ui, index, size| {
        let preset = &LIGHT_PRESETS[index];
        let response = icon_tile(ui, size, preset.icon, preset.label, current == Some(index))
            .on_hover_text(preset.hint);
        if response.clicked() {
            settings.apply_light_preset(index);
        }
    });

    caption(ui, "Key light — drag the lamp");
    // The lamp glows in the colour of the light it throws.
    let warmth = settings.light_warmth;
    let lamp = [
        (235.0 + 20.0 * warmth.max(0.0) - 60.0 * (-warmth).max(0.0)) as u8,
        (225.0 - 25.0 * warmth.abs()) as u8,
        (215.0 - 95.0 * warmth.max(0.0) + 40.0 * (-warmth).max(0.0)) as u8,
    ];
    let lit = settings.light_key > 0.01;
    if light_pad(ui, &mut settings.light_x, &mut settings.light_y, lamp, lit) && !lit {
        // Moving the lamp while it is off switches it on.
        settings.light_key = 0.5;
    }
    amount(ui, "Brightness", &mut settings.light_key, 0.0..=1.0, 0.0);
    amount(ui, "Warmth", &mut settings.light_warmth, -1.0..=1.0, 0.0);
    let moved =
        (settings.light_x - LIGHT_X).abs() > 0.005 || (settings.light_y - LIGHT_Y).abs() > 0.005;
    if moved && link(ui, "Recentre lamp").clicked() {
        settings.light_x = LIGHT_X;
        settings.light_y = LIGHT_Y;
    }

    caption(ui, "Fill light");
    amount(ui, "Lift shadows", &mut settings.light_fill, 0.0..=1.0, 0.0);

    caption(ui, "Rim light");
    amount(ui, "Edge glow", &mut settings.light_rim, 0.0..=1.0, 0.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (index, gel) in RIM_COLORS.iter().enumerate() {
            let selected = settings.light_rim_color == index;
            if color_dot(ui, Some(gel.rgb), gel.label, selected).clicked() {
                settings.light_rim_color = index;
                if settings.light_rim <= 0.01 {
                    settings.light_rim = 0.5;
                }
            }
        }
    });

    caption(ui, "Room");
    amount(
        ui,
        "Dim background",
        &mut settings.light_dim,
        0.0..=1.0,
        0.0,
    );
}

fn beauty_tab(ui: &mut Ui, settings: &mut EffectSettings) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("QUICK START").size(9.0).color(TEXT_MUTED));
        if ui
            .button("Natural")
            .on_hover_text("A light, barely-there touch-up")
            .clicked()
        {
            settings.apply_beauty_preset(BeautyPreset::Natural);
        }
        if ui
            .button("Glam")
            .on_hover_text("Smooth skin, bright eyes, rosy lips and cheeks")
            .clicked()
        {
            settings.apply_beauty_preset(BeautyPreset::Glam);
        }
    });

    caption(ui, "Skin");
    amount(ui, "Smooth skin", &mut settings.smoothing, 0.0..=1.0, 0.0);
    amount(ui, "Warmth", &mut settings.skin_tone, -1.0..=1.0, 0.0);
    amount(ui, "Blush", &mut settings.blush, 0.0..=1.0, 0.0);

    caption(ui, "Face shape");
    amount(ui, "Slim face", &mut settings.slimming, 0.0..=1.0, 0.0);
    amount(ui, "Slim nose", &mut settings.nose_slim, 0.0..=1.0, 0.0);
    amount(ui, "Bigger eyes", &mut settings.eye_size, 0.0..=1.0, 0.0);

    caption(ui, "Eyes");
    amount(ui, "Brighten", &mut settings.eye_brighten, 0.0..=1.0, 0.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (index, tint) in EYE_COLORS.iter().enumerate() {
            let color = (index != 0).then_some(tint.rgb);
            if color_dot(ui, color, tint.label, settings.eye_color == index).clicked() {
                settings.eye_color = index;
            }
        }
    });

    caption(ui, "Lips & teeth");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (index, tint) in LIP_COLORS.iter().enumerate() {
            let color = (index != 0).then_some(tint.rgb);
            if color_dot(ui, color, tint.label, settings.lip_color == index).clicked() {
                settings.lip_color = index;
            }
        }
    });
    if settings.lip_color != 0 {
        amount(ui, "Lip colour", &mut settings.lip_strength, 0.0..=1.0, 0.6);
    }
    amount(ui, "Whiten teeth", &mut settings.teeth, 0.0..=1.0, 0.0);
}

fn stickers_tab(ui: &mut Ui, settings: &mut EffectSettings) {
    hint(ui, "Mix and match — stickers follow your head.");
    grid(ui, STICKERS.len(), 4, 68.0, |ui, index, size| {
        let sticker = &STICKERS[index];
        let response = icon_tile(
            ui,
            size,
            sticker.icon,
            sticker.label,
            settings.stickers[index],
        )
        .on_hover_text(sticker.hint);
        if response.clicked() {
            settings.stickers[index] = !settings.stickers[index];
        }
    });
}

/// A grid where exactly one tile is selected; index 0 is "None".
fn pick_one(ui: &mut Ui, choices: &[Choice], columns: usize, selected: &mut usize) {
    grid(ui, choices.len(), columns, 68.0, |ui, index, size| {
        let choice = &choices[index];
        let response = icon_tile(ui, size, choice.icon, choice.label, *selected == index)
            .on_hover_text(choice.hint);
        if response.clicked() {
            // Clicking the active tile again switches it off.
            *selected = if *selected == index { 0 } else { index };
        }
    });
}

fn funny_tab(ui: &mut Ui, settings: &mut EffectSettings) {
    hint(ui, "Bend your face out of shape.");
    pick_one(ui, &DISTORTIONS, 3, &mut settings.distortion);
    if settings.distortion != 0 {
        amount(
            ui,
            "Strength",
            &mut settings.distortion_strength,
            0.0..=1.0,
            0.6,
        );
    }
}

fn paint_tab(ui: &mut Ui, settings: &mut EffectSettings) {
    hint(ui, "Painted straight onto your face.");
    pick_one(ui, &PAINTS, 4, &mut settings.paint);
    if settings.paint != 0 {
        amount(ui, "Opacity", &mut settings.paint_strength, 0.0..=1.0, 1.0);
    }

    caption(ui, "Anonymiser");
    let targets: Vec<String> = ANONYMISE.iter().map(|c| c.label.to_owned()).collect();
    segmented(ui, &targets, &mut settings.anonymise);
    if settings.anonymise == 0 {
        hint(ui, "Hide your eyes or your whole face.");
        return;
    }
    let styles: Vec<String> = ANONYMISE_STYLES
        .iter()
        .map(|c| c.label.to_owned())
        .collect();
    segmented(ui, &styles, &mut settings.anonymise_style);
    if settings.anonymise_style != 2 {
        amount(
            ui,
            "Coarseness",
            &mut settings.anonymise_amount,
            0.0..=1.0,
            0.5,
        );
    }
    hint(
        ui,
        "Follows your face and stays on briefly if tracking drops. A disguise for fun — \
         don't rely on it to protect your identity.",
    );
}

fn magic_tab(ui: &mut Ui, settings: &mut EffectSettings) {
    hint(ui, "Pull a face or use your hands to set these off.");
    let hands_ready = crate::paths::has_hand_model();
    for (index, trigger) in TRIGGERS.iter().enumerate() {
        if index == FIRST_GESTURE && !hands_ready {
            hint(
                ui,
                "The hand gestures need one more download: run scripts/install.sh again.",
            );
        }
        let row = egui::Frame::new()
            .fill(TILE)
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(trigger.icon).size(22.0));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(RichText::new(trigger.label).size(12.5).strong());
                        ui.label(RichText::new(trigger.hint).size(10.5).color(TEXT_MUTED));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        toggle(ui, &mut settings.triggers[index], trigger.label);
                    });
                });
            })
            .response;
        // The whole row is a click target, not just the switch.
        if row
            .interact(Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            settings.triggers[index] = !settings.triggers[index];
        }
        let color = match index {
            QUESTIONS => Some(("Question mark colour", &mut settings.question_color)),
            IDEA => Some(("Star colour", &mut settings.star_color)),
            _ => None,
        };
        if let (true, Some((label, choice))) = (settings.triggers[index], color) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new(label).size(11.0).color(TEXT_MUTED));
                for (position, tint) in MAGIC_COLORS.iter().enumerate() {
                    if color_dot(ui, Some(tint.rgb), tint.label, *choice == position).clicked() {
                        *choice = position;
                    }
                }
            });
        }
        if index == STORM && settings.triggers[STORM] {
            amount(
                ui,
                "Storm sensitivity",
                &mut settings.storm_sensitivity,
                0.0..=1.0,
                0.5,
            );
            hint(
                ui,
                "Lower if it rains while you look neutral, higher if a frown is not enough.",
            );
        }
    }
}

/// Background, framing, and the things laid over the picture.
fn scene_tab(
    ui: &mut Ui,
    settings: &mut EffectSettings,
    session: &mut Session,
    library: &mut BackdropLibrary,
    fonts: &mut FontList,
) {
    caption(ui, "Background");
    let labels: Vec<String> = BACKGROUNDS.iter().map(|b| b.label.to_owned()).collect();
    segmented(ui, &labels, &mut settings.background);
    match settings.background {
        1 => {
            amount(ui, "Blur", &mut settings.blur, 0.0..=1.0, 0.55);
        }
        2 => background_library(ui, settings, library),
        _ => hint(ui, BACKGROUNDS[0].hint),
    }

    caption(ui, "Framing");
    switch_row(
        ui,
        "Auto-frame — keep me centred",
        &mut settings.autoframe_on,
    );
    if settings.autoframe_on {
        amount(ui, "Tightness", &mut settings.autoframe, 0.0..=1.0, 0.5);
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("People to follow").size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for count in (1..=MAX_FACES).rev() {
                let chosen = settings.faces == count;
                if ui
                    .add(egui::Button::new(count.to_string()).selected(chosen))
                    .on_hover_text(
                        "Effects, auto-framing and the anonymiser follow this many faces",
                    )
                    .clicked()
                {
                    settings.faces = count;
                }
            }
        });
    });
    switch_row(ui, "Mirror picture", &mut session.mirror);
    switch_row(ui, "Flip upside down", &mut session.flip);

    caption(ui, "Name badge");
    switch_row(ui, "Show my name", &mut settings.badge);
    ui.add(
        egui::TextEdit::singleline(&mut session.badge_name)
            .hint_text("Your name")
            .char_limit(60)
            .desired_width(f32::INFINITY),
    );
    ui.add(
        egui::TextEdit::singleline(&mut session.badge_title)
            .hint_text("Title or team (optional)")
            .char_limit(80)
            .desired_width(f32::INFINITY),
    );
    ui.collapsing(RichText::new("Badge design").color(TEXT_MUTED), |ui| {
        badge_design(ui, &mut session.badge_design, fonts);
    });

    caption(ui, "Logo");
    switch_row(ui, "Show logo in the corner", &mut settings.logo);
    if let Some(picked) = library.logo_picker.take() {
        session.logo_path = picked.to_string_lossy().into_owned();
        settings.logo = true;
    }
    ui.horizontal(|ui| {
        if FilePicker::available()
            && ui
                .add_enabled(!library.logo_picker.busy(), egui::Button::new("Choose…"))
                .clicked()
        {
            library.logo_picker.open(
                "Choose a logo",
                "Pictures",
                &["*.png", "*.PNG", "*.jpg", "*.jpeg", "*.JPG"],
            );
        }
        ui.add(
            egui::TextEdit::singleline(&mut session.logo_path)
                .hint_text("Path of a PNG or JPEG")
                .desired_width(f32::INFINITY),
        );
    });
    let logo = session.logo_path.trim();
    if settings.logo && logo.is_empty() {
        message(ui, "Choose a picture to use as the logo.", WARNING);
    } else if !logo.is_empty() && !Path::new(logo).is_file() {
        message(ui, "That file does not exist.", WARNING);
    }
}

/// Layout, colour and font of the name badge.
fn badge_design(ui: &mut Ui, design: &mut BadgeDesign, fonts: &mut FontList) {
    caption(ui, "Style");
    let styles: Vec<String> = BADGE_STYLES.iter().map(|c| c.label.to_owned()).collect();
    segmented(ui, &styles, &mut design.style);
    hint(ui, BADGE_STYLES[design.style].hint);
    let layouts: Vec<String> = BADGE_LAYOUTS.iter().map(|c| c.label.to_owned()).collect();
    segmented(ui, &layouts, &mut design.layout);

    caption(ui, "Position");
    ui.horizontal(|ui| {
        position_picker(ui, &mut design.position);
        hint(ui, "Click where the badge\nshould sit.");
    });
    amount(ui, "Size", &mut design.size, 0.0..=1.0, 0.35);
    amount(ui, "Edge distance", &mut design.margin, 0.0..=1.0, 0.35);

    caption(ui, "Colour");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (index, tint) in BADGE_COLORS.iter().enumerate() {
            if color_dot(ui, Some(tint.rgb), tint.label, design.color == index).clicked() {
                design.color = index;
            }
        }
    });
    if design.style != 4 {
        amount(ui, "Panel opacity", &mut design.opacity, 0.0..=1.0, 0.8);
    }

    caption(ui, "Font");
    fonts.refresh();
    let shown = if design.font.is_empty() {
        "System default".to_owned()
    } else {
        design.font.clone()
    };
    egui::ComboBox::from_id_salt("badge-font")
        .width(ui.available_width() - 8.0)
        .height(280.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut fonts.filter)
                    .hint_text("Search fonts…")
                    .desired_width(f32::INFINITY),
            );
            if fonts.loading() {
                hint(ui, "Listing fonts…");
            }
            if fonts.filter.trim().is_empty() {
                ui.selectable_value(&mut design.font, String::new(), "System default");
            }
            let matches = fonts.matches();
            if matches.is_empty() && !fonts.loading() {
                hint(ui, "No font matches.");
            }
            // Long lists are trimmed; searching narrows them down.
            for family in matches.into_iter().take(150) {
                ui.selectable_value(&mut design.font, family.to_owned(), family);
            }
        });
    switch_row(ui, "Bold name", &mut design.bold);
    switch_row(ui, "Name in capitals", &mut design.caps);
    if *design != BadgeDesign::default() && link(ui, "Reset design").clicked() {
        *design = BadgeDesign::default();
    }
}

/// Thumbnails of the bundled and added backgrounds.
fn background_library(ui: &mut Ui, settings: &mut EffectSettings, library: &mut BackdropLibrary) {
    library.rescan_if_due();
    if let Some(picked) = library.picker.take() {
        match library.import(&picked) {
            Ok(added) => {
                settings.background_path = added.to_string_lossy().into_owned();
                library.message = Some(("Background added.".into(), false));
            }
            Err(error) => library.message = Some((error, true)),
        }
    }
    let ctx = ui.ctx().clone();
    // Decode at most one new thumbnail per frame.
    let mut budget = true;
    let mut choose: Option<String> = None;
    grid(ui, library.items.len(), 3, 82.0, |ui, index, size| {
        let (name, path) = {
            let item = &library.items[index];
            (item.name.clone(), item.path.to_string_lossy().into_owned())
        };
        let selected = settings.background_path == path;
        let texture = library.texture(&ctx, index, &mut budget);
        if image_tile(ui, size, texture, &name, selected).clicked() {
            choose = Some(path);
        }
    });
    if let Some(path) = choose {
        settings.background_path = path;
    }
    if !budget {
        ctx.request_repaint();
    }
    if library.find(&settings.background_path).is_none() {
        if settings.background_path.trim().is_empty() {
            hint(ui, "Pick a background above.");
        } else if !Path::new(settings.background_path.trim()).is_file() {
            message(ui, "The chosen picture no longer exists.", WARNING);
        }
    }

    let removable = library
        .find(&settings.background_path)
        .filter(|item| item.user)
        .map(|item| item.path.clone());
    ui.horizontal(|ui| {
        if FilePicker::available() {
            let busy = library.picker.busy();
            let label = if busy {
                "Choosing…"
            } else {
                "Add picture…"
            };
            if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                library.picker.open(
                    "Choose a background picture",
                    "Pictures",
                    &["*.jpg", "*.jpeg", "*.png", "*.JPG", "*.PNG"],
                );
            }
        }
        if ui
            .button("Open folder")
            .on_hover_text("Drop JPEG or PNG pictures here and they appear above")
            .clicked()
        {
            let _ = std::fs::create_dir_all(library.user_dir());
            let _ = std::process::Command::new("xdg-open")
                .arg(library.user_dir())
                .spawn();
        }
        if let Some(path) = &removable {
            if link(ui, "Remove this picture").clicked() {
                match library.remove(path) {
                    Ok(()) => {
                        settings.background_path.clear();
                        library.message = Some(("Picture removed.".into(), false));
                    }
                    Err(error) => library.message = Some((error, true)),
                }
            }
        }
    });
    if let Some((text, is_error)) = &library.message {
        message(ui, text, if *is_error { WARNING } else { SUCCESS });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lay out and paint every tab, with effects both off and on, without a
    /// window or a camera.
    #[test]
    fn every_tab_renders_headlessly() {
        let ctx = egui::Context::default();
        let mut preview = Preview::detached();
        let mut busy = EffectSettings {
            look: 3,
            grain: 0.4,
            stickers: [true; STICKERS.len()],
            distortion: 2,
            paint: 4,
            triggers: [true; TRIGGERS.len()],
            background: 2,
            anonymise: 2,
            ..EffectSettings::default()
        };
        busy.apply_beauty_preset(BeautyPreset::Glam);
        busy.apply_light_preset(6);
        busy.auto_enhance = 0.5;
        busy.autoframe_on = true;
        busy.faces = 2;
        busy.badge = true;
        busy.logo = true;
        busy.lut = crate::paths::bundled("luts")
            .join("Moonlight.cube")
            .to_string_lossy()
            .into_owned();
        for settings in [EffectSettings::default(), busy] {
            for group in Group::ALL {
                let mut effects = EffectsController::for_tests();
                effects.settings = settings.clone();
                let mut tab = group;
                // Two passes: egui sizes some widgets on the first and paints on the second.
                for _ in 0..2 {
                    let output = ctx.run(egui::RawInput::default(), |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            active_chips(ui, &mut effects, &mut preview, None);
                            show(ui, &mut effects, &mut preview, None, &mut tab);
                        });
                    });
                    assert!(!output.shapes.is_empty());
                }
                assert_eq!(tab, group);
                assert!(effects.profiles.current.is_none());
                assert_eq!(
                    effects.settings, settings,
                    "rendering must not edit settings"
                );
                assert!(!effects.enabled);
            }
        }
    }

    /// The Profiles card in each of its states: empty, naming, showing a
    /// saved profile, and showing one with unsaved changes.
    #[test]
    fn profiles_card_renders_in_every_state() {
        let ctx = egui::Context::default();
        let dir = std::env::temp_dir().join(format!("obsbot-ui-profiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = ProfileStore::load(dir.clone(), true);
        let mut settings = EffectSettings::default();
        let draw = |settings: &mut EffectSettings, store: &mut ProfileStore| {
            for _ in 0..2 {
                let _ = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| profiles_card(ui, settings, store));
                });
            }
        };
        draw(&mut settings, &mut store);
        store.draft = Some("Evening".into());
        draw(&mut settings, &mut store);
        store.draft = None;
        settings = store.find("Party").unwrap().settings.clone();
        store.current = Some("Party".into());
        draw(&mut settings, &mut store);
        assert_eq!(store.matching(&settings).unwrap().name, "Party");
        settings.grain = 0.5;
        draw(&mut settings, &mut store);
        assert!(store.matching(&settings).is_none());
        assert_eq!(store.profiles.len(), 5, "drawing must not save or delete");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The badge design controls, which live in a section that starts closed.
    #[test]
    fn badge_design_controls_render_without_changing_the_design() {
        let ctx = egui::Context::default();
        let mut fonts = FontList::with_families(&["Liberation Sans", "Noto Serif"]);
        for style in 0..BADGE_STYLES.len() {
            let mut design = BadgeDesign {
                style,
                font: "Noto Serif".into(),
                ..BadgeDesign::default()
            };
            let before = design.clone();
            for _ in 0..2 {
                let output = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| badge_design(ui, &mut design, &mut fonts));
                });
                assert!(!output.shapes.is_empty());
            }
            assert_eq!(design, before);
        }
    }

    /// Every icon must exist in the bundled fonts, or tiles would show blanks.
    #[test]
    fn every_catalogue_icon_has_a_glyph() {
        let mut icons: Vec<&str> = Vec::new();
        icons.extend(STICKERS.iter().map(|c| c.icon));
        icons.extend(DISTORTIONS.iter().map(|c| c.icon));
        icons.extend(PAINTS.iter().map(|c| c.icon));
        icons.extend(TRIGGERS.iter().map(|c| c.icon));
        icons.extend(Group::ALL.iter().map(|g| g.icon()));
        icons.extend(LIGHT_PRESETS.iter().map(|p| p.icon));
        icons.extend(["🎥", "✔", "✖", "↺", "💾"]);

        let missing = crate::widgets::missing_glyphs(&icons);
        assert!(missing.is_empty(), "icons without a glyph: {missing:?}");
    }
}
