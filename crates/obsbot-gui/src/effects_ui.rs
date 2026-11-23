//! The Effects panel: virtual camera switch plus one tab per effect family.

use crate::effects::{
    Active, BeautyPreset, Choice, EffectSettings, EffectsController, Group, ANONYMISE,
    ANONYMISE_STYLES, BACKGROUNDS, DISTORTIONS, EYE_COLORS, LIGHT_PRESETS, LIGHT_X, LIGHT_Y,
    LIP_COLORS, LOOKS, PAINTS, RESOLUTIONS, RIM_COLORS, STICKERS, TRIGGERS,
};
use crate::luts::LutLibrary;
use crate::preview::Preview;
use crate::theme::*;
use crate::widgets::{
    amount, caption, card, chip, color_dot, grid, hint, icon_tile, light_pad, link, segmented,
    status_pill, swatch_tile, tab_bar, toggle,
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
    let mut power = effects.enabled;
    let mut restart = false;

    camera_card(ui, effects, &mut power, &mut restart);
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
            Group::Backdrop => backdrop_tab(ui, settings),
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
    } else if effects.settings != before {
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
            hint(
                ui,
                if effects.enabled && effects.publishing {
                    "Choose “OBSBOT Virtual Camera” in your video app."
                } else if effects.enabled {
                    "Showing in this preview only. To use it in other apps, run \
                     scripts/setup-virtual-camera.sh and switch this on again."
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
    if let Some(picked) = luts.take_picked() {
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
        if LutLibrary::can_browse() {
            let label = if luts.browsing() {
                "Choosing…"
            } else {
                "Add LUT…"
            };
            if ui
                .add_enabled(!luts.browsing(), egui::Button::new(label))
                .on_hover_text("Import a .cube file")
                .clicked()
            {
                luts.browse();
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
    hint(ui, "Light yourself like a studio — no lamps required.");
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
    hint(ui, "Pull a face to set these off.");
    for (index, trigger) in TRIGGERS.iter().enumerate() {
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
    }
}

fn backdrop_tab(ui: &mut Ui, settings: &mut EffectSettings) {
    let labels: Vec<String> = BACKGROUNDS.iter().map(|b| b.label.to_owned()).collect();
    segmented(ui, &labels, &mut settings.background);
    hint(ui, BACKGROUNDS[settings.background].hint);
    match settings.background {
        1 => {
            amount(ui, "Blur", &mut settings.blur, 0.0..=1.0, 0.55);
        }
        2 => {
            caption(ui, "Picture file");
            ui.add(
                egui::TextEdit::singleline(&mut settings.background_path)
                    .hint_text("/home/me/Pictures/background.jpg")
                    .desired_width(f32::INFINITY),
            );
            let path = settings.background_path.trim();
            if !path.is_empty() && !Path::new(path).is_file() {
                message(ui, "That file does not exist.", WARNING);
            }
        }
        _ => {}
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
        busy.lut = format!(
            "{}/../../effects/luts/Moonlight.cube",
            env!("CARGO_MANIFEST_DIR")
        );
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
                assert_eq!(
                    effects.settings, settings,
                    "rendering must not edit settings"
                );
                assert!(!effects.enabled);
            }
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
        icons.extend(["🎥", "✔", "✖", "↺"]);

        let missing = crate::widgets::missing_glyphs(&icons);
        assert!(missing.is_empty(), "icons without a glyph: {missing:?}");
    }
}
