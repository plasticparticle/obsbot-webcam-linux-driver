//! Native control center for the OBSBOT Tiny 2 Lite.
//!
//! Pure-native egui window: live preview + full camera control. No webview,
//! no local server — everything talks straight to /dev/video*.

mod effects;
mod effects_ui;
mod luts;
mod preview;
mod theme;
mod widgets;

use effects::{EffectsController, Group};
use eframe::egui::{self, Color32, RichText, Stroke};
use obsbot_core::{ctrl, AiMode, Device, Fov, TrackingSpeed};
use preview::Preview;
use theme::*;

fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 9.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.interact_size.y = 30.0;

    let visuals = &mut style.visuals;
    visuals.dark_mode = true;
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = Color32::from_rgb(13, 16, 25);
    visuals.faint_bg_color = Color32::from_rgb(28, 32, 47);
    visuals.override_text_color = Some(TEXT);
    visuals.weak_text_color = Some(TEXT_MUTED);
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.slider_trailing_fill = true;

    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(34, 39, 56);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(30, 35, 51);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, CARD_BORDER);
    visuals.widgets.inactive.corner_radius = 8.into();
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(50, 55, 77);
    visuals.widgets.hovered.bg_fill = ACCENT_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT_HOVER);
    visuals.widgets.hovered.corner_radius = 8.into();
    visuals.widgets.active.weak_bg_fill = ACCENT;
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT_HOVER);
    visuals.widgets.active.corner_radius = 8.into();
    visuals.widgets.open.weak_bg_fill = Color32::from_rgb(42, 46, 67);
    visuals.widgets.open.corner_radius = 8.into();
    visuals.window_corner_radius = 12.into();
    visuals.menu_corner_radius = 10.into();
    ctx.set_style(style);
}

fn section(ui: &mut egui::Ui, icon: &str, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    widgets::card(ui, icon, title, |_| {}, content);
}

fn metric(ui: &mut egui::Ui, label: &str, value: String) {
    ui.vertical(|ui| {
        ui.label(
            RichText::new(label.to_uppercase())
                .size(9.0)
                .color(TEXT_MUTED),
        );
        ui.label(RichText::new(value).size(16.0).strong());
    });
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 720.0])
            .with_min_inner_size([900.0, 560.0])
            .with_title("OBSBOT Control Center"),
        ..Default::default()
    };
    eframe::run_native(
        "OBSBOT Control Center",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

/// UI-side mirror of the camera state; refreshed periodically.
#[derive(Default)]
struct CamState {
    pan: f64,
    tilt: f64,
    zoom: i32,
    sleeping: bool,
    hdr: bool,
    ai_mode: Option<AiMode>,
    sport_speed: bool,
    brightness: i32,
    contrast: i32,
    saturation: i32,
    sharpness: i32,
    wb_auto: bool,
    wb_temp: i32,
    exp_auto: bool,
    exp_time: i32,
    focus_auto: bool,
    anti_flicker: i32,
    face_ae: bool,
    fov: usize,
    presets: [bool; 3],
}

struct App {
    dev: Option<Device>,
    state: CamState,
    preview: Preview,
    effects: EffectsController,
    /// 0 = camera controls, 1 = effects.
    panel: usize,
    effects_tab: Group,
    last_poll: f64,
    error: Option<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_zoom_factor(1.1);
        configure_style(&cc.egui_ctx);
        let mut app = App {
            dev: Device::discover().ok(),
            state: CamState::default(),
            preview: Preview::start(),
            effects: EffectsController::load(),
            panel: 0,
            effects_tab: Group::Looks,
            last_poll: -10.0,
            error: None,
        };
        // Defaults for XU-side toggles that have no readback.
        app.state.face_ae = true;
        app.state.fov = 0;
        app.refresh(true);
        app
    }

    /// Pull current values from the camera into the UI state.
    fn refresh(&mut self, full: bool) {
        let Some(dev) = &self.dev else { return };
        let ok = (|| -> std::io::Result<()> {
            let s = &mut self.state;
            if full {
                s.pan = dev.pan_deg()?;
                s.tilt = dev.tilt_deg()?;
                s.zoom = dev.zoom()?;
                s.brightness = dev.get(ctrl::BRIGHTNESS)?;
                s.contrast = dev.get(ctrl::CONTRAST)?;
                s.saturation = dev.get(ctrl::SATURATION)?;
                s.sharpness = dev.get(ctrl::SHARPNESS)?;
                s.wb_auto = dev.get(ctrl::WHITE_BALANCE_AUTO)? != 0;
                s.wb_temp = dev.get(ctrl::WHITE_BALANCE_TEMPERATURE)?;
                s.exp_auto = dev.get(ctrl::AUTO_EXPOSURE)? != 1;
                s.exp_time = dev.get(ctrl::EXPOSURE_TIME_ABSOLUTE)?;
                s.focus_auto = dev.get(ctrl::FOCUS_AUTO)? != 0;
                s.anti_flicker = dev.get(ctrl::POWER_LINE_FREQUENCY)?;
                if let Ok(list) = dev.preset_list() {
                    s.presets = [false; 3];
                    for e in list {
                        if let Some(p) = s.presets.get_mut(e.slot as usize) {
                            *p = true;
                        }
                    }
                }
            }
            let xs = dev.status()?;
            self.state.sleeping = xs.sleeping;
            self.state.hdr = xs.hdr;
            self.state.ai_mode = xs.ai_mode;
            self.state.sport_speed = xs.tracking_speed == TrackingSpeed::Sport;
            Ok(())
        })();
        if let Err(e) = ok {
            self.error = Some(format!("camera read failed: {e}"));
        }
    }

    fn with_dev(&mut self, f: impl FnOnce(&Device) -> std::io::Result<()>) {
        let Some(dev) = &self.dev else { return };
        match f(dev) {
            Ok(()) => self.error = None,
            Err(e) => self.error = Some(e.to_string()),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.effects.poll(&mut self.preview);
        // Reconnect / periodic status poll.
        let now = ctx.input(|i| i.time);
        if self.dev.is_none() && now - self.last_poll > 2.0 {
            self.dev = Device::discover().ok();
            self.last_poll = now;
            if self.dev.is_some() {
                self.refresh(true);
            }
        } else if now - self.last_poll > 1.5 {
            self.refresh(false);
            self.last_poll = now;
        }

        egui::SidePanel::right("controls")
            .exact_width(380.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(16, 14)),
            )
            .show(ctx, |ui| {
                self.header_ui(ui);
                if self.dev.is_none() {
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt(self.panel)
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        if self.panel == 0 {
                            self.controls_ui(ui);
                        } else {
                            let input = self.dev.as_ref().map(|device| device.path().to_path_buf());
                            effects_ui::show(
                                ui,
                                &mut self.effects,
                                &mut self.preview,
                                input.as_deref(),
                                &mut self.effects_tab,
                            );
                        }
                        ui.add_space(8.0);
                    });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(20, 18)),
            )
            .show(ctx, |ui| {
                let resolution = self.effects.selected_resolution();
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Live preview").size(22.0).strong());
                        ui.label(
                            RichText::new(if self.effects.enabled {
                                format!(
                                    "{}  •  {} @ {} fps",
                                    if self.effects.publishing {
                                        "OBSBOT Virtual Camera"
                                    } else {
                                        "Effects preview"
                                    },
                                    resolution.label,
                                    self.effects.fps
                                )
                            } else {
                                "OBSBOT Tiny 2 Lite  •  1280 × 720".to_owned()
                            })
                            .size(11.0)
                            .color(TEXT_MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.dev.is_some() {
                            widgets::status_pill(ui, "LIVE", SUCCESS);
                        } else {
                            widgets::status_pill(ui, "NO CAMERA", DANGER);
                        }
                    });
                });
                ui.add_space(10.0);
                let input = self.dev.as_ref().map(|device| device.path().to_path_buf());
                effects_ui::active_chips(
                    ui,
                    &mut self.effects,
                    &mut self.preview,
                    input.as_deref(),
                );
                ui.add_space(4.0);
                egui::Frame::new()
                    .fill(Color32::from_rgb(5, 7, 12))
                    .stroke(Stroke::new(1.0_f32, CARD_BORDER))
                    .corner_radius(14)
                    .inner_margin(egui::Margin::same(5))
                    .show(ui, |ui| self.preview.show(ui));
            });

        // Live preview needs continuous repaints.
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
}

impl App {
    /// Branding, connection status and the Camera / Effects switch.
    fn header_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            egui::Frame::new()
                .fill(ACCENT)
                .corner_radius(10)
                .inner_margin(egui::Margin::symmetric(10, 7))
                .show(ui, |ui| {
                    ui.label(RichText::new("O").size(18.0).strong().color(Color32::WHITE));
                });
            ui.vertical(|ui| {
                ui.label(RichText::new("Control Center").size(19.0).strong());
                ui.label(
                    RichText::new("Camera workspace")
                        .size(10.0)
                        .color(TEXT_MUTED),
                );
            });
        });
        ui.add_space(12.0);

        let connected = self.dev.is_some();
        egui::Frame::new()
            .fill(if connected {
                Color32::from_rgb(20, 48, 40)
            } else {
                Color32::from_rgb(57, 30, 34)
            })
            .stroke(Stroke::new(
                1.0_f32,
                if connected {
                    Color32::from_rgb(39, 100, 76)
                } else {
                    Color32::from_rgb(112, 53, 60)
                },
            ))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(12, 9))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (dot, _) =
                        ui.allocate_exact_size(egui::vec2(12.0, 30.0), egui::Sense::hover());
                    ui.painter().circle_filled(
                        dot.center(),
                        4.5,
                        if connected { SUCCESS } else { DANGER },
                    );
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(if connected {
                                "Camera connected"
                            } else {
                                "Camera unavailable"
                            })
                            .strong(),
                        );
                        ui.label(
                            RichText::new(if connected {
                                if self.state.sleeping {
                                    "Privacy mode is active"
                                } else {
                                    "Ready for control"
                                }
                            } else {
                                "Plug in an OBSBOT camera — retrying"
                            })
                            .size(10.0)
                            .color(TEXT_MUTED),
                        );
                    });
                });
            });

        if !connected {
            return;
        }
        ui.add_space(10.0);
        let count = self.effects.settings.active().len();
        let labels = [
            "📷  Camera".to_owned(),
            if count == 0 {
                "✨  Effects".to_owned()
            } else {
                format!("✨  Effects  ·  {count}")
            },
        ];
        widgets::segmented(ui, &labels, &mut self.panel);
        ui.add_space(10.0);
    }

    fn controls_ui(&mut self, ui: &mut egui::Ui) {
        if let Some(err) = &self.error {
            egui::Frame::new()
                .fill(Color32::from_rgb(55, 39, 24))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.label(RichText::new(format!("⚠  {err}")).size(10.0).color(WARNING));
                });
            ui.add_space(10.0);
        }

        let sleeping = self.state.sleeping;
        if ui
            .add_sized(
                [ui.available_width(), 38.0],
                egui::Button::new(
                    RichText::new(if sleeping {
                        "▶  Wake camera"
                    } else {
                        "🔒  Enter privacy mode"
                    })
                    .strong(),
                )
                .selected(sleeping),
            )
            .clicked()
        {
            self.with_dev(|d| d.set_sleep(!sleeping));
            self.state.sleeping = !sleeping;
        }
        ui.add_space(10.0);

        section(ui, "🎬", "Gimbal", |ui| {
            ui.columns(3, |cols| {
                metric(&mut cols[0], "Pan", format!("{:.0}°", self.state.pan));
                metric(&mut cols[1], "Tilt", format!("{:.0}°", self.state.tilt));
                metric(&mut cols[2], "Zoom", format!("{}%", self.state.zoom));
            });
            ui.add_space(4.0);

            let mut pan = self.state.pan;
            if ui
                .add(
                    egui::Slider::new(&mut pan, -130.0..=130.0)
                        .text("Pan")
                        .fixed_decimals(0),
                )
                .changed()
            {
                self.state.pan = pan;
                self.with_dev(|d| d.set_pan_deg(pan));
            }
            let mut tilt = self.state.tilt;
            if ui
                .add(
                    egui::Slider::new(&mut tilt, -90.0..=90.0)
                        .text("Tilt")
                        .fixed_decimals(0),
                )
                .changed()
            {
                self.state.tilt = tilt;
                self.with_dev(|d| d.set_tilt_deg(tilt));
            }
            let mut zoom = self.state.zoom;
            if ui
                .add(egui::Slider::new(&mut zoom, 0..=100).text("Zoom"))
                .changed()
            {
                self.state.zoom = zoom;
                self.with_dev(|d| d.set_zoom(zoom));
            }

            ui.horizontal(|ui| {
                if ui.button("⟲  Recenter").clicked() {
                    self.with_dev(|d| d.recenter());
                    self.state.pan = 0.0;
                    self.state.tilt = 0.0;
                }
                ui.separator();
                for (label, dp, dt) in [
                    ("⬅", -3.0, 0.0),
                    ("➡", 3.0, 0.0),
                    ("⬆", 0.0, 3.0),
                    ("⬇", 0.0, -3.0),
                ] {
                    if ui.button(label).clicked() {
                        let pan = (self.state.pan + dp).clamp(-130.0, 130.0);
                        let tilt = (self.state.tilt + dt).clamp(-90.0, 90.0);
                        self.state.pan = pan;
                        self.state.tilt = tilt;
                        self.with_dev(|d| {
                            d.set_pan_deg(pan)?;
                            d.set_tilt_deg(tilt)
                        });
                    }
                }
            });
        });
        ui.add_space(10.0);

        section(ui, "⭐", "Presets", |ui| {
            ui.label(
                RichText::new("Recall a saved camera position")
                    .size(10.0)
                    .color(TEXT_MUTED),
            );
            ui.horizontal(|ui| {
                for slot in 0..3u32 {
                    let stored = self.state.presets[slot as usize];
                    let response = ui
                        .add_enabled(
                            stored,
                            egui::Button::new(format!("Position {}", slot + 1))
                                .min_size([91.0, 32.0].into()),
                        )
                        .on_hover_text(if stored {
                            "Recall preset"
                        } else {
                            "This slot is empty"
                        });
                    if response.clicked() {
                        self.with_dev(|d| d.goto_preset(slot));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("SAVE TO").size(9.0).color(TEXT_MUTED));
                for slot in 0..3u32 {
                    if ui
                        .button(format!("{}", slot + 1))
                        .on_hover_text("Save current view")
                        .clicked()
                    {
                        let (pan, tilt) = (self.state.pan as f32, self.state.tilt as f32);
                        let zoom = 1.0 + self.state.zoom as f32 / 100.0 * 3.0;
                        self.with_dev(|d| d.save_preset(slot, pan, tilt, zoom));
                        self.state.presets[slot as usize] = true;
                    }
                }
            });
        });
        ui.add_space(10.0);

        section(ui, "◎", "AI Tracking", |ui| {
            let current = self.state.ai_mode;
            ui.horizontal(|ui| {
                ui.label(RichText::new("MODE").size(9.0).color(TEXT_MUTED));
                egui::ComboBox::from_id_salt("tracking-mode")
                    .width(210.0)
                    .selected_text(current.map_or("Unknown", |m| m.label()))
                    .show_ui(ui, |ui| {
                        for mode in AiMode::ALL {
                            if ui
                                .selectable_label(current == Some(mode), mode.label())
                                .clicked()
                            {
                                self.with_dev(|d| d.set_ai_mode(mode));
                                self.state.ai_mode = Some(mode);
                            }
                        }
                    });
            });
            let mut sport = self.state.sport_speed;
            if ui.checkbox(&mut sport, "Sport tracking speed").changed() {
                self.state.sport_speed = sport;
                self.with_dev(|d| {
                    d.set_tracking_speed(if sport {
                        TrackingSpeed::Sport
                    } else {
                        TrackingSpeed::Standard
                    })
                });
            }
        });
        ui.add_space(10.0);

        section(ui, "☀", "Image", |ui| {
            ui.horizontal(|ui| {
                let mut hdr = self.state.hdr;
                if ui.checkbox(&mut hdr, "HDR").changed() {
                    self.state.hdr = hdr;
                    self.with_dev(|d| d.set_hdr(hdr));
                }
                let mut face_ae = self.state.face_ae;
                if ui.checkbox(&mut face_ae, "Face-priority AE").changed() {
                    self.state.face_ae = face_ae;
                    self.with_dev(|d| d.set_face_ae(face_ae));
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("FIELD OF VIEW").size(9.0).color(TEXT_MUTED));
                for (i, (label, fov)) in [
                    ("86° Wide", Fov::Wide),
                    ("78°", Fov::Normal),
                    ("65°", Fov::Narrow),
                ]
                .iter()
                .enumerate()
                {
                    if ui
                        .add(egui::Button::new(*label).selected(self.state.fov == i))
                        .clicked()
                    {
                        self.state.fov = i;
                        self.with_dev(|d| d.set_fov(*fov));
                    }
                }
            });

            for (label, id, field) in [
                ("Brightness", ctrl::BRIGHTNESS, 0usize),
                ("Contrast", ctrl::CONTRAST, 1),
                ("Saturation", ctrl::SATURATION, 2),
                ("Sharpness", ctrl::SHARPNESS, 3),
            ] {
                let value = match field {
                    0 => &mut self.state.brightness,
                    1 => &mut self.state.contrast,
                    2 => &mut self.state.saturation,
                    _ => &mut self.state.sharpness,
                };
                let mut next = *value;
                if ui
                    .add(egui::Slider::new(&mut next, 0..=100).text(label))
                    .changed()
                {
                    *value = next;
                    self.with_dev(|d| d.set(id, next));
                }
            }

            ui.collapsing(
                RichText::new("Advanced image controls").color(TEXT_MUTED),
                |ui| {
                    let mut wb_auto = self.state.wb_auto;
                    if ui.checkbox(&mut wb_auto, "Auto white balance").changed() {
                        self.state.wb_auto = wb_auto;
                        self.with_dev(|d| d.set(ctrl::WHITE_BALANCE_AUTO, wb_auto as i32));
                    }
                    if !self.state.wb_auto {
                        let mut temperature = self.state.wb_temp;
                        if ui
                            .add(
                                egui::Slider::new(&mut temperature, 2000..=10000)
                                    .text("Temperature K")
                                    .step_by(100.0),
                            )
                            .changed()
                        {
                            self.state.wb_temp = temperature;
                            self.with_dev(|d| d.set(ctrl::WHITE_BALANCE_TEMPERATURE, temperature));
                        }
                    }

                    let mut exp_auto = self.state.exp_auto;
                    if ui.checkbox(&mut exp_auto, "Auto exposure").changed() {
                        self.state.exp_auto = exp_auto;
                        self.with_dev(|d| d.set(ctrl::AUTO_EXPOSURE, if exp_auto { 0 } else { 1 }));
                    }
                    if !self.state.exp_auto {
                        let mut time = self.state.exp_time;
                        if ui
                            .add(
                                egui::Slider::new(&mut time, 1..=2500)
                                    .text("Exposure (100µs)")
                                    .logarithmic(true),
                            )
                            .changed()
                        {
                            self.state.exp_time = time;
                            self.with_dev(|d| d.set(ctrl::EXPOSURE_TIME_ABSOLUTE, time));
                        }
                    }

                    let mut focus_auto = self.state.focus_auto;
                    if ui.checkbox(&mut focus_auto, "Auto focus").changed() {
                        self.state.focus_auto = focus_auto;
                        self.with_dev(|d| d.set(ctrl::FOCUS_AUTO, focus_auto as i32));
                    }

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("ANTI-FLICKER").size(9.0).color(TEXT_MUTED));
                        for (label, value) in [("Off", 0), ("50 Hz", 1), ("60 Hz", 2)] {
                            if ui
                                .add(
                                    egui::Button::new(label)
                                        .selected(self.state.anti_flicker == value),
                                )
                                .clicked()
                            {
                                self.state.anti_flicker = value;
                                self.with_dev(|d| d.set(ctrl::POWER_LINE_FREQUENCY, value));
                            }
                        }
                    });
                },
            );
        });
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.effects.stop(&mut self.preview);
    }
}

#[cfg(test)]
mod tests {
    /// Icons used in this file must exist in the bundled fonts.
    #[test]
    fn camera_panel_icons_have_glyphs() {
        let icons = [
            "📷", "✨", "▶", "🔒", "🎬", "⭐", "◎", "☀", "⟲", "⚠", "⬅", "➡", "⬆", "⬇",
        ];
        assert_eq!(crate::widgets::missing_glyphs(&icons), Vec::<&str>::new());
    }
}
