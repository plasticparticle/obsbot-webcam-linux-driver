//! The tray icon: quick controls while the window is closed.

use crate::commands::{Command, Switch};
use crate::effects::Hold;
use ksni::blocking::TrayMethods;
use std::sync::mpsc::Sender;

/// What the tray shows; refreshed from the app a few times a second.
#[derive(Clone, PartialEq, Default, Debug)]
pub struct Snapshot {
    pub connected: bool,
    pub profiles: Vec<String>,
    pub current: Option<String>,
    pub effects_on: bool,
    pub hold: Hold,
    pub sleeping: bool,
}

struct Tray {
    commands: Sender<Command>,
    state: Snapshot,
}

impl Tray {
    fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }
}

/// A 32×32 ARGB icon: a rounded tile with a lens, tinted by state.
fn icon(state: &Snapshot) -> ksni::Icon {
    let tile = if !state.connected {
        [255, 96, 100, 120]
    } else if state.sleeping {
        [255, 70, 76, 100]
    } else if state.hold != Hold::Off {
        [255, 255, 170, 60]
    } else {
        [255, 116, 105, 255]
    };
    let size = 32i32;
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5 - 16.0, y as f32 + 0.5 - 16.0);
            // Rounded square of half-size 14 with corner radius 7.
            let (dx, dy) = ((fx.abs() - 7.0).max(0.0), (fy.abs() - 7.0).max(0.0));
            let inside_tile = (dx * dx + dy * dy).sqrt() <= 7.0;
            let radius = (fx * fx + fy * fy).sqrt();
            let pixel = if !inside_tile {
                [0, 0, 0, 0]
            } else if radius <= 4.0 && state.effects_on {
                tile
            } else if radius <= 8.0 {
                [255, 255, 255, 255]
            } else {
                tile
            };
            data.extend_from_slice(&pixel);
        }
    }
    ksni::Icon {
        width: size,
        height: size,
        data,
    }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "obsbot-control-center".into()
    }

    fn title(&self) -> String {
        "OBSBOT Control Center".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![icon(&self.state)]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let state = &self.state;
        let description = if !state.connected {
            "No camera connected".to_owned()
        } else if state.sleeping {
            "Camera asleep (privacy mode)".to_owned()
        } else {
            match (state.hold, state.effects_on, &state.current) {
                (Hold::Card, ..) => "Showing “be right back”".to_owned(),
                (Hold::Freeze, ..) => "Picture frozen".to_owned(),
                (_, true, Some(profile)) => format!("Effects on — {profile}"),
                (_, true, None) => "Effects on".to_owned(),
                (_, false, _) => "Effects off".to_owned(),
            }
        };
        ksni::ToolTip {
            title: "OBSBOT Control Center".into(),
            description,
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Command::Show);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{CheckmarkItem, StandardItem};
        let state = &self.state;
        let mut items: Vec<ksni::MenuItem<Self>> = vec![
            StandardItem {
                label: "Open Control Center".into(),
                activate: Box::new(|tray: &mut Self| tray.send(Command::Show)),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
        ];
        for name in &state.profiles {
            let profile = name.clone();
            items.push(
                CheckmarkItem {
                    label: name.clone(),
                    checked: state.current.as_deref() == Some(name.as_str()),
                    enabled: state.connected,
                    activate: Box::new(move |tray: &mut Self| {
                        tray.send(Command::Profile(profile.clone()));
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }
        if !state.profiles.is_empty() {
            items.push(ksni::MenuItem::Separator);
        }
        let toggles: [(&str, bool, Command); 4] = [
            (
                "Effects",
                state.effects_on,
                Command::Effects(Switch::Toggle),
            ),
            (
                "Be right back",
                state.hold == Hold::Card,
                Command::Hold(Hold::Card, Switch::Toggle, None),
            ),
            (
                "Freeze picture",
                state.hold == Hold::Freeze,
                Command::Hold(Hold::Freeze, Switch::Toggle, None),
            ),
            (
                "Privacy mode (camera asleep)",
                state.sleeping,
                Command::Privacy(Switch::Toggle),
            ),
        ];
        for (label, checked, command) in toggles {
            items.push(
                CheckmarkItem {
                    label: label.into(),
                    checked,
                    enabled: state.connected,
                    activate: Box::new(move |tray: &mut Self| tray.send(command.clone())),
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(ksni::MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|tray: &mut Self| tray.send(Command::Quit)),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

/// A running tray icon. Dropping it removes the icon.
pub struct TrayIcon {
    handle: ksni::blocking::Handle<Tray>,
    shown: Snapshot,
}

impl TrayIcon {
    /// Put the icon in the tray. `None` when the desktop has no tray.
    pub fn start(commands: Sender<Command>) -> Option<Self> {
        let tray = Tray {
            commands,
            state: Snapshot::default(),
        };
        let handle = tray.spawn().ok()?;
        Some(Self {
            handle,
            shown: Snapshot::default(),
        })
    }

    /// Redraw the icon and menu if anything they show has changed.
    pub fn show(&mut self, snapshot: Snapshot) {
        if snapshot != self.shown {
            self.shown = snapshot.clone();
            self.handle
                .update(move |tray: &mut Tray| tray.state = snapshot);
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        let _ = self.handle.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_a_full_argb_image_that_reflects_state() {
        let idle = icon(&Snapshot {
            connected: true,
            ..Snapshot::default()
        });
        assert_eq!(idle.data.len(), 32 * 32 * 4);
        // Transparent corner, opaque centre.
        assert_eq!(&idle.data[..4], &[0, 0, 0, 0]);
        let centre = ((16 * 32 + 16) * 4) as usize;
        assert_eq!(idle.data[centre], 255);
        let away = icon(&Snapshot {
            connected: true,
            hold: Hold::Card,
            ..Snapshot::default()
        });
        assert_ne!(idle.data, away.data);
    }
}
