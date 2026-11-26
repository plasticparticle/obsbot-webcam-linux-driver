//! Native control center for the OBSBOT Tiny 2 Lite.
//!
//! Pure-native egui window: live preview + full camera control. No webview,
//! no local server — everything talks straight to /dev/video*.

mod app_panel;
mod app_settings;
mod automation;
mod backgrounds;
mod camera_store;
mod commands;
mod conf;
mod effects;
mod effects_ui;
mod fonts;
mod ipc;
mod luts;
mod paths;
mod picker;
mod positions;
mod positions_ui;
mod preview;
mod profiles;
mod theme;
mod tray;
mod widgets;

use app_settings::AppSettings;
use automation::{Action, AutoSleep, Monitor};
use camera_store::CameraSettings;
use commands::Command;
use effects::{EffectsController, Group, Hold};
use eframe::egui::{self, Color32, RichText, Stroke};
use obsbot_core::{ctrl, AiMode, Device, Fov, TrackingSpeed};
use positions::{Position, Positions};
use preview::Preview;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use theme::*;
use tray::TrayIcon;

const FOVS: [Fov; 3] = [Fov::Wide, Fov::Normal, Fov::Narrow];

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
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "OBSBOT Control Center\n\n  --background  start in the tray without opening the window\n  \
             --paths       show where the effects runtime was found\n\n\
             Control a running instance with: obsbotctl app help"
        );
        return Ok(());
    }
    if arguments.iter().any(|arg| arg == "--paths") {
        println!("{}", paths::report());
        return Ok(());
    }
    let background = arguments.iter().any(|arg| arg == "--background");
    let (sender, receiver) = std::sync::mpsc::channel();
    let status = Arc::new(Mutex::new(ipc::Shared::default()));
    match ipc::serve(sender.clone(), status.clone()) {
        Ok(true) => {}
        Ok(false) => {
            // Already running: bring that window up instead of starting twice.
            if !background {
                let _ = obsbot_core::app_ipc::send("show");
            }
            return Ok(());
        }
        Err(error) => eprintln!("commands from outside are unavailable: {error}"),
    }
    let tray = TrayIcon::start(sender);
    let core = Arc::new(Mutex::new(Core::new(receiver, status, tray, !background)));

    let mut show_window = !background;
    loop {
        if show_window {
            let (size, min_size) = window_sizes(lock(&core).app.compact);
            let options = eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size(size)
                    .with_min_inner_size(min_size)
                    .with_title("OBSBOT Control Center"),
                ..Default::default()
            };
            let shared = core.clone();
            eframe::run_native(
                "OBSBOT Control Center",
                options,
                Box::new(move |cc| Ok(Box::new(App::new(cc, shared)))),
            )?;
            let mut core = lock(&core);
            let hidden = std::mem::take(&mut core.hide_requested);
            if core.quit || !(hidden || core.app.keep_running) {
                break;
            }
        }
        // Window closed (or never opened): keep serving the tray and commands.
        show_window = loop {
            {
                let mut core = lock(&core);
                core.tick();
                if core.quit {
                    break false;
                }
                if std::mem::take(&mut core.show_requested) {
                    break true;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        if !show_window {
            break;
        }
    }
    lock(&core).shutdown();
    ipc::close();
    Ok(())
}

fn lock(core: &Mutex<Core>) -> std::sync::MutexGuard<'_, Core> {
    // A panic elsewhere must not take the camera controls down with it.
    core.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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

impl CamState {
    /// The part of the state that is saved and restored.
    fn saved(&self) -> CameraSettings {
        CameraSettings {
            zoom: self.zoom,
            brightness: self.brightness,
            contrast: self.contrast,
            saturation: self.saturation,
            sharpness: self.sharpness,
            wb_auto: self.wb_auto,
            wb_temp: self.wb_temp,
            exp_auto: self.exp_auto,
            exp_time: self.exp_time,
            focus_auto: self.focus_auto,
            anti_flicker: self.anti_flicker,
            hdr: self.hdr,
            face_ae: self.face_ae,
            fov: self.fov,
            ai_mode: self.ai_mode.map(|mode| mode.label().to_owned()),
            sport_speed: self.sport_speed,
        }
    }
}

/// Everything that keeps working while the window is closed.
struct Core {
    dev: Option<Device>,
    state: CamState,
    preview: Preview,
    effects: EffectsController,
    app: AppSettings,
    positions: Positions,
    /// 0 = camera controls, 1 = effects, 2 = app settings.
    panel: usize,
    effects_tab: Group,
    last_poll: Instant,
    last_tick: Instant,
    error: Option<String>,
    camera_file: PathBuf,
    /// What is in `camera_file`, once the camera state is known to be real.
    camera_saved: Option<CameraSettings>,
    camera_changed: Option<Instant>,
    monitor: Monitor,
    auto_sleep: AutoSleep,
    watching_virtual_camera: bool,
    commands: Receiver<Command>,
    tray: Option<TrayIcon>,
    status: Arc<Mutex<ipc::Shared>>,
    window_open: bool,
    quit: bool,
    show_requested: bool,
    /// Set when the layout changed and the window should take its new size.
    resize_requested: bool,
    /// Close the window but stay alive, whatever the setting says.
    hide_requested: bool,
    /// The last sleep/wake this app sent, and when.
    sleep_commanded: Option<(bool, Instant)>,
}

impl Core {
    fn new(
        commands: Receiver<Command>,
        status: Arc<Mutex<ipc::Shared>>,
        tray: Option<TrayIcon>,
        window_open: bool,
    ) -> Self {
        let mut core = Core {
            dev: Device::discover().ok(),
            state: CamState::default(),
            preview: Preview::start(),
            effects: EffectsController::load(),
            app: AppSettings::load(),
            positions: Positions::load(conf::config_dir().join("positions.conf")),
            panel: 0,
            effects_tab: Group::Looks,
            last_poll: Instant::now(),
            last_tick: Instant::now(),
            error: None,
            camera_file: conf::config_dir().join("camera.conf"),
            camera_saved: None,
            camera_changed: None,
            monitor: Monitor::start(),
            auto_sleep: AutoSleep::default(),
            watching_virtual_camera: false,
            commands,
            tray,
            status,
            window_open: true,
            quit: false,
            show_requested: false,
            resize_requested: false,
            hide_requested: false,
            sleep_commanded: None,
        };
        // Defaults for XU-side toggles that have no readback.
        core.state.face_ae = true;
        core.state.fov = 0;
        if core.dev.is_some() {
            core.connected();
        }
        core.set_window_open(window_open);

        // Start-up profile, then effects if asked for.
        let input = core.input();
        if !core.app.startup_profile.is_empty() {
            let name = core.app.startup_profile.clone();
            if let Some(profile) = core.effects.profiles.find(&name).cloned() {
                core.effects.settings = profile.settings;
                core.effects.profiles.current = Some(profile.name);
                let _ = core.effects.save();
            }
        }
        if core.app.start_effects && core.effects.wants_worker() {
            if let Some(input) = &input {
                let _ = core.effects.start(input, &mut core.preview);
            }
        }
        core
    }

    fn input(&self) -> Option<PathBuf> {
        self.dev.as_ref().map(|device| device.path().to_path_buf())
    }

    /// The camera has just been found: read it, then put saved settings back.
    fn connected(&mut self) {
        self.refresh(true);
        let saved = std::fs::read_to_string(&self.camera_file)
            .ok()
            .and_then(|text| CameraSettings::from_text(&text));
        if let (true, Some(saved)) = (self.app.restore_camera, saved) {
            self.restore_camera(&saved);
        }
        self.camera_saved = Some(self.state.saved());
        self.camera_changed = None;
    }

    /// Apply every saved setting that differs from what the camera reports.
    fn restore_camera(&mut self, saved: &CameraSettings) {
        let now = self.state.saved();
        let mode = saved
            .ai_mode
            .as_deref()
            .and_then(|label| AiMode::ALL.into_iter().find(|mode| mode.label() == label));
        self.with_dev(|d| {
            for (id, was, want) in [
                (ctrl::BRIGHTNESS, now.brightness, saved.brightness),
                (ctrl::CONTRAST, now.contrast, saved.contrast),
                (ctrl::SATURATION, now.saturation, saved.saturation),
                (ctrl::SHARPNESS, now.sharpness, saved.sharpness),
                (
                    ctrl::POWER_LINE_FREQUENCY,
                    now.anti_flicker,
                    saved.anti_flicker,
                ),
            ] {
                if was != want {
                    d.set(id, want)?;
                }
            }
            if now.wb_auto != saved.wb_auto {
                d.set(ctrl::WHITE_BALANCE_AUTO, saved.wb_auto as i32)?;
            }
            if !saved.wb_auto && (now.wb_auto || now.wb_temp != saved.wb_temp) {
                d.set(ctrl::WHITE_BALANCE_TEMPERATURE, saved.wb_temp)?;
            }
            if now.exp_auto != saved.exp_auto {
                d.set(ctrl::AUTO_EXPOSURE, if saved.exp_auto { 0 } else { 1 })?;
            }
            if !saved.exp_auto && (now.exp_auto || now.exp_time != saved.exp_time) {
                d.set(ctrl::EXPOSURE_TIME_ABSOLUTE, saved.exp_time)?;
            }
            if now.focus_auto != saved.focus_auto {
                d.set(ctrl::FOCUS_AUTO, saved.focus_auto as i32)?;
            }
            if now.zoom != saved.zoom {
                d.set_zoom(saved.zoom)?;
            }
            if now.hdr != saved.hdr {
                d.set_hdr(saved.hdr)?;
            }
            // Neither of these can be read back, so always send them.
            d.set_face_ae(saved.face_ae)?;
            d.set_fov(FOVS[saved.fov.min(FOVS.len() - 1)])?;
            if let Some(mode) = mode {
                if now.ai_mode.as_deref() != saved.ai_mode.as_deref() {
                    d.set_ai_mode(mode)?;
                }
            }
            if now.sport_speed != saved.sport_speed {
                d.set_tracking_speed(if saved.sport_speed {
                    TrackingSpeed::Sport
                } else {
                    TrackingSpeed::Standard
                })?;
            }
            Ok(())
        });
        let state = &mut self.state;
        state.zoom = saved.zoom;
        state.brightness = saved.brightness;
        state.contrast = saved.contrast;
        state.saturation = saved.saturation;
        state.sharpness = saved.sharpness;
        state.wb_auto = saved.wb_auto;
        state.wb_temp = saved.wb_temp;
        state.exp_auto = saved.exp_auto;
        state.exp_time = saved.exp_time;
        state.focus_auto = saved.focus_auto;
        state.anti_flicker = saved.anti_flicker;
        state.hdr = saved.hdr;
        state.face_ae = saved.face_ae;
        state.fov = saved.fov;
        if let Some(mode) = mode {
            state.ai_mode = Some(mode);
        }
        state.sport_speed = saved.sport_speed;
    }

    /// Write changed camera settings to disk once they have settled.
    fn autosave_camera(&mut self) {
        if self.dev.is_none() || self.camera_saved.is_none() {
            return;
        }
        let current = self.state.saved();
        if self.camera_saved.as_ref() == Some(&current) {
            self.camera_changed = None;
            return;
        }
        let since = *self.camera_changed.get_or_insert_with(Instant::now);
        if since.elapsed() >= Duration::from_millis(1200) {
            if conf::write_atomically(&self.camera_file, &current.to_text()).is_ok() {
                self.camera_saved = Some(current);
            }
            self.camera_changed = None;
        }
    }

    /// Whether the live preview is on screen: the window is open and not in
    /// its compact, controls-only layout.
    fn preview_visible(&self) -> bool {
        self.window_open && !self.app.compact
    }

    /// Tell the effects worker and the camera preview whether anyone is
    /// looking, so they stop producing pictures nobody sees.
    fn sync_preview(&mut self) {
        let input = self.input();
        let visible = self.preview_visible();
        self.effects
            .set_window_open(visible, input.as_deref(), &mut self.preview);
    }

    fn set_compact(&mut self, compact: bool) {
        if self.app.compact == compact {
            return;
        }
        self.app.compact = compact;
        if let Err(error) = self.app.save() {
            self.error = Some(format!("Could not save app settings: {error}"));
        }
        self.sync_preview();
        self.resize_requested = true;
    }

    fn set_window_open(&mut self, open: bool) {
        self.window_open = open;
        self.sync_preview();
        if !open {
            // Textures belong to the window that is going away.
            self.preview.forget_texture();
            self.effects.backdrops.forget_textures();
            self.positions.adjusting = None;
            self.effects.session.adjusting = false;
        }
    }

    /// One step of everything that runs with or without a window.
    fn tick(&mut self) {
        let dt = self.last_tick.elapsed().as_secs_f32().min(5.0);
        self.last_tick = Instant::now();
        self.effects.poll(&mut self.preview);
        if self.effects.watch_virtual_camera() {
            if let Some(input) = self.input() {
                self.effects.restart(&input, &mut self.preview);
            }
        }

        // Reconnect / periodic status poll.
        if self.dev.is_none() && self.last_poll.elapsed() > Duration::from_secs(2) {
            self.dev = Device::discover().ok();
            self.last_poll = Instant::now();
            if self.dev.is_some() {
                self.connected();
            }
        } else if self.last_poll.elapsed() > Duration::from_millis(1500) {
            self.refresh(false);
            self.last_poll = Instant::now();
        }
        self.autosave_camera();

        while let Ok(command) = self.commands.try_recv() {
            self.handle(command);
        }

        // Automatic privacy.
        if self.effects.publishing != self.watching_virtual_camera {
            self.watching_virtual_camera = self.effects.publishing;
            self.monitor.watch(if self.effects.publishing {
                effects::find_virtual_camera()
            } else {
                None
            });
        }
        let session = self.monitor.session();
        self.effects.viewers = if self.effects.publishing {
            session.viewers
        } else {
            0
        };
        let in_use = self.window_open || session.viewers > 0;
        if self.dev.is_some() {
            let action = self.auto_sleep.step(
                dt,
                self.app.sleep_on_lock,
                self.app.sleep_when_unused,
                session.locked,
                in_use,
                self.state.sleeping,
            );
            if action != Action::None {
                let sleep = action == Action::Sleep;
                self.set_sleep(sleep);
            }
        }

        self.report();
    }

    /// Refresh the tray and the `status` reply.
    fn report(&mut self) {
        let profile = self
            .effects
            .profiles
            .matching(&self.effects.settings)
            .map(|profile| profile.name.clone());
        let snapshot = tray::Snapshot {
            connected: self.dev.is_some(),
            profiles: self
                .effects
                .profiles
                .profiles
                .iter()
                .map(|profile| profile.name.clone())
                .collect(),
            current: profile.clone(),
            effects_on: self.effects.enabled,
            hold: self.effects.session.hold,
            sleeping: self.state.sleeping,
        };
        let text = format!(
            "camera={} effects={} output={} profile={} hold={} privacy={} window={}",
            if snapshot.connected {
                "connected"
            } else {
                "missing"
            },
            if snapshot.effects_on { "on" } else { "off" },
            if self.effects.publishing {
                "virtual-camera"
            } else {
                "preview"
            },
            profile.as_deref().unwrap_or("custom").replace(' ', "_"),
            match snapshot.hold {
                Hold::Off => "off",
                Hold::Freeze => "freeze",
                Hold::Card => "brb",
            },
            if snapshot.sleeping { "on" } else { "off" },
            if self.window_open { "open" } else { "closed" },
        );
        if let Ok(mut shared) = self.status.lock() {
            shared.status = text;
            shared.profiles.clone_from(&snapshot.profiles);
            shared.positions = self
                .positions
                .list
                .iter()
                .map(|position| position.name.clone())
                .collect();
        }
        if let Some(tray) = &mut self.tray {
            tray.show(snapshot);
        }
    }

    /// Carry out a command from the tray or the command line.
    fn handle(&mut self, command: Command) {
        let input = self.input();
        let input = input.as_deref();
        match command {
            Command::Show => self.show_requested = true,
            Command::Hide => self.hide_requested = self.window_open,
            Command::Quit => self.quit = true,
            Command::Profile(name) => {
                if !self
                    .effects
                    .apply_profile_named(&name, input, &mut self.preview)
                {
                    self.error = Some(format!("There is no profile called “{name}”."));
                }
            }
            Command::ProfileStep(step) => self.effects.step_profile(step, input, &mut self.preview),
            Command::Effects(switch) => {
                if switch.apply(self.effects.enabled) {
                    if let Some(input) = input {
                        let _ = self.effects.start(input, &mut self.preview);
                    }
                } else {
                    self.effects.stop_by_user(&mut self.preview);
                }
            }
            Command::Hold(kind, switch, text) => {
                if let Some(text) = text {
                    self.effects.session.hold_text = text;
                }
                let on = switch.apply(self.effects.session.hold == kind);
                let hold = if on { kind } else { Hold::Off };
                self.effects.set_hold(hold, input, &mut self.preview);
            }
            Command::Privacy(switch) => {
                let sleep = switch.apply(self.state.sleeping);
                self.set_sleep(sleep);
            }
            Command::Position(name) => {
                let wanted = name.trim().to_lowercase();
                let found = self
                    .positions
                    .list
                    .iter()
                    .find(|position| position.name.to_lowercase() == wanted)
                    .cloned();
                match found {
                    Some(position) => self.goto(&position),
                    None => self.error = Some(format!("There is no position called “{name}”.")),
                }
            }
        }
    }

    /// Put the camera to sleep ("privacy mode") or wake it.
    fn set_sleep(&mut self, sleep: bool) {
        self.with_dev(|d| d.set_sleep(sleep));
        self.state.sleeping = sleep;
        self.sleep_commanded = Some((sleep, Instant::now()));
    }

    /// Send the camera to a saved position.
    fn goto(&mut self, position: &Position) {
        let (pan, tilt, zoom) = (position.pan, position.tilt, position.zoom);
        let fixed = position.corners.is_some();
        self.with_dev(|d| {
            if fixed {
                // A straightened whiteboard only works if the camera holds still.
                d.set_ai_mode(AiMode::NoTracking)?;
            }
            d.set_pan_deg(pan)?;
            d.set_tilt_deg(tilt)?;
            d.set_zoom(zoom)
        });
        self.state.pan = pan;
        self.state.tilt = tilt;
        self.state.zoom = zoom;
        if fixed {
            self.state.ai_mode = Some(AiMode::NoTracking);
        }
        self.positions.current = Some(position.name.clone());
        self.positions.adjusting = None;
        self.positions.confirm_delete = None;
        self.effects.session.adjusting = false;
        self.effects.session.keystone = position.corners;
        let input = self.input();
        self.effects
            .settings_changed(input.as_deref(), &mut self.preview);
    }

    /// Stop the worker and release the camera before the process exits.
    fn shutdown(&mut self) {
        self.effects.stop(&mut self.preview);
        self.preview.pause();
        self.tray = None;
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
            // The camera takes a few seconds to report a sleep or wake it was
            // just told to do; trust the command until then.
            self.state.sleeping = match self.sleep_commanded {
                Some((wanted, at)) if at.elapsed() < Duration::from_secs(6) => wanted,
                _ => xs.sleeping,
            };
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

/// The window. It can be closed and reopened; `Core` lives on.
struct App {
    core: Arc<Mutex<Core>>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, core: Arc<Mutex<Core>>) -> Self {
        cc.egui_ctx.set_zoom_factor(1.1);
        configure_style(&cc.egui_ctx);
        lock(&core).set_window_open(true);
        Self { core }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        lock(&self.core).set_window_open(false);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut core = lock(&self.core);
        core.tick();
        if core.quit || core.hide_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if std::mem::take(&mut core.show_requested) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if std::mem::take(&mut core.resize_requested) {
            let (size, min_size) = window_sizes(core.app.compact);
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(min_size.into()));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size.into()));
        }
        core.ui(ctx);
    }
}

impl Core {
    fn ui(&mut self, ctx: &egui::Context) {
        // Ctrl+1 … Ctrl+9 switch effect profiles from anywhere in the window.
        const DIGITS: [egui::Key; profiles::SHORTCUTS] = [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
            egui::Key::Num6,
            egui::Key::Num7,
            egui::Key::Num8,
            egui::Key::Num9,
        ];
        let picked = ctx.input_mut(|input| {
            DIGITS
                .iter()
                .position(|key| input.consume_key(egui::Modifiers::COMMAND, *key))
        });
        if let (Some(index), Some(input)) = (picked, self.input()) {
            self.effects
                .apply_profile(index, Some(&input), &mut self.preview);
        }

        if self.app.compact {
            // Controls only: no second view of your own face during a call.
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(PANEL))
                .show(ctx, |ui| {
                    let spare = (ui.available_width() - COMPACT_WIDTH).max(0.0) / 2.0;
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric((16.0 + spare) as i8, 14))
                        .show(ui, |ui| self.side_ui(ui));
                });
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }

        egui::SidePanel::right("controls")
            .exact_width(380.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(16, 14)),
            )
            .show(ctx, |ui| self.side_ui(ui));

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
                        self.hold_ui(ui);
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
                    .show(ui, |ui| {
                        let picture = self.preview.show(ui);
                        if let (Some(rect), Some(corners)) =
                            (picture, &mut self.positions.adjusting)
                        {
                            widgets::corner_handles(ui, rect, corners);
                        }
                    });
            });

        // Live preview needs continuous repaints.
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
}

/// Width of the controls column in the compact layout.
const COMPACT_WIDTH: f32 = 380.0;

/// Window size and minimum size for the full and the compact layout.
fn window_sizes(compact: bool) -> ([f32; 2], [f32; 2]) {
    if compact {
        ([COMPACT_WIDTH + 32.0, 720.0], [COMPACT_WIDTH + 32.0, 420.0])
    } else {
        ([1180.0, 720.0], [900.0, 560.0])
    }
}

impl Core {
    /// Everything in the controls column: header, panel switch and the panel.
    fn side_ui(&mut self, ui: &mut egui::Ui) {
        self.header_ui(ui);
        if self.dev.is_none() {
            return;
        }
        if self.app.compact {
            // The preview's status and step-away buttons, which would
            // otherwise be out of reach in this layout.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.hold_ui(ui);
            });
            ui.add_space(8.0);
        }
        egui::ScrollArea::vertical()
            .id_salt(self.panel)
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                if self.panel == 0 {
                    self.controls_ui(ui);
                } else if self.panel == 1 {
                    let input = self.input();
                    effects_ui::show(
                        ui,
                        &mut self.effects,
                        &mut self.preview,
                        input.as_deref(),
                        &mut self.effects_tab,
                    );
                } else {
                    let has_tray = self.tray.is_some();
                    app_panel::show(ui, &mut self.app, &mut self.effects, has_tray);
                }
                ui.add_space(8.0);
            });
    }

    /// On-air status and the step-away buttons, laid out right to left.
    fn hold_ui(&mut self, ui: &mut egui::Ui) {
        let hold = self.effects.session.hold;
        match (self.dev.is_some(), hold) {
            (false, _) => widgets::status_pill(ui, "NO CAMERA", DANGER),
            (true, Hold::Card) => widgets::status_pill(ui, "AWAY", WARNING),
            (true, Hold::Freeze) => widgets::status_pill(ui, "FROZEN", WARNING),
            (true, Hold::Off) => widgets::status_pill(ui, "LIVE", SUCCESS),
        }
        if self.dev.is_some() {
            // One click to step away from a call.
            let mut wanted = hold;
            for (label, kind, tip) in [
                ("Freeze", Hold::Freeze, "Hold the last picture still"),
                (
                    "Be right back",
                    Hold::Card,
                    "Show your away message instead of the camera",
                ),
            ] {
                if ui
                    .add(egui::Button::new(label).selected(hold == kind))
                    .on_hover_text(tip)
                    .clicked()
                {
                    wanted = if hold == kind { Hold::Off } else { kind };
                }
            }
            if wanted != hold {
                let input = self.input();
                self.effects
                    .set_hold(wanted, input.as_deref(), &mut self.preview);
            }
        }
    }

    /// Branding, connection status and the panel switch.
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let compact = self.app.compact;
                let (label, tip) = if compact {
                    ("Show preview", "Bring back the live preview")
                } else {
                    (
                        "Hide preview",
                        "Controls only — less distracting during a call",
                    )
                };
                if ui.button(label).on_hover_text(tip).clicked() {
                    self.set_compact(!compact);
                }
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
            "⚙  App".to_owned(),
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
            self.set_sleep(!sleeping);
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

        let before = self.effects.session.clone();
        let view = positions_ui::View {
            pan: self.state.pan,
            tilt: self.state.tilt,
            zoom: self.state.zoom,
        };
        let goto = positions_ui::show(ui, &mut self.positions, &mut self.effects.session, view);
        if let Some(position) = goto {
            self.goto(&position);
        } else if self.effects.session != before {
            let input = self.input();
            self.effects
                .settings_changed(input.as_deref(), &mut self.preview);
        }
        ui.add_space(10.0);

        section(ui, "⭐", "Camera memory", |ui| {
            ui.label(
                RichText::new("Three positions stored in the camera itself")
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

#[cfg(test)]
mod tests {
    /// Icons used in this file must exist in the bundled fonts.
    #[test]
    fn camera_panel_icons_have_glyphs() {
        let icons = [
            "📷", "✨", "▶", "🔒", "🎬", "⭐", "◎", "☀", "⟲", "⚠", "⬅", "➡", "⬆", "⬇", "⚙", "📍",
            "🏠", "💬", "⚡", "🖼",
        ];
        assert_eq!(crate::widgets::missing_glyphs(&icons), Vec::<&str>::new());
    }
}
