//! The Positions card: named places for the camera to look at.

use crate::effects::Session;
use crate::positions::{Position, Positions, DEFAULT_CORNERS, MAX_NAME_CHARS};
use crate::theme::*;
use crate::widgets::{card, hint, link, pill};
use eframe::egui::{self, RichText, Ui, Vec2};

/// Where the camera is pointing right now.
pub struct View {
    pub pan: f64,
    pub tilt: f64,
    pub zoom: i32,
}

/// Draw the card. Returns a position the user asked to go to. Whiteboard
/// straightening is switched directly on `session`.
pub fn show(
    ui: &mut Ui,
    positions: &mut Positions,
    session: &mut Session,
    view: View,
) -> Option<Position> {
    let mut goto: Option<Position> = None;
    let mut open_naming = false;
    let naming = positions.draft.is_some();
    card(
        ui,
        "📍",
        "Positions",
        |ui| {
            if !naming {
                open_naming = link(ui, "Save view as…")
                    .on_hover_text("Remember where the camera points and how far it is zoomed")
                    .clicked();
            }
        },
        |ui| {
            if positions.list.is_empty() && !naming {
                hint(
                    ui,
                    "Point the camera somewhere useful — your desk, a whiteboard, the room — \
                     and save it to come back with one click.",
                );
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                for position in &positions.list {
                    let active = positions.current.as_deref() == Some(position.name.as_str());
                    let label = if position.corners.is_some() {
                        format!("{}  🖼", position.name)
                    } else {
                        position.name.clone()
                    };
                    if pill(ui, &label, active).clicked() {
                        goto = Some(position.clone());
                    }
                }
            });

            if let Some(draft) = &mut positions.draft {
                let (mut save, mut cancel) = (false, false);
                ui.horizontal(|ui| {
                    cancel = link(ui, "Cancel").clicked();
                    save = ui.button("Save").clicked();
                    let edit = ui.add(
                        egui::TextEdit::singleline(draft)
                            .hint_text("Position name")
                            .char_limit(MAX_NAME_CHARS)
                            .desired_width(f32::INFINITY),
                    );
                    if edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                        save = true;
                    }
                    if std::mem::take(&mut positions.focus_draft) {
                        edit.request_focus();
                    }
                });
                if save {
                    let position = Position {
                        name: draft.clone(),
                        pan: view.pan,
                        tilt: view.tilt,
                        zoom: view.zoom,
                        corners: None,
                    };
                    match positions.save(position) {
                        Ok(saved) => {
                            positions.draft = None;
                            positions.message = Some((format!("Saved “{saved}”."), false));
                        }
                        Err(error) => positions.message = Some((error, true)),
                    }
                } else if cancel {
                    positions.draft = None;
                }
            } else if let Some(current) = positions
                .current
                .clone()
                .and_then(|name| positions.find(&name).cloned())
            {
                current_position(ui, positions, session, &current, &view);
            }
            if let Some((text, is_error)) = &positions.message {
                ui.label(RichText::new(text).size(10.5).color(if *is_error {
                    WARNING
                } else {
                    SUCCESS
                }));
            }
        },
    );
    if open_naming {
        positions.draft = Some(String::new());
        positions.focus_draft = true;
        positions.message = None;
    }
    goto
}

/// Actions for the position the camera was last sent to.
fn current_position(
    ui: &mut Ui,
    positions: &mut Positions,
    session: &mut Session,
    current: &Position,
    view: &View,
) {
    let name = current.name.as_str();
    if let Some(corners) = positions.adjusting {
        hint(
            ui,
            "Drag the four dots on the preview onto the corners of the whiteboard.",
        );
        ui.horizontal(|ui| {
            if ui.button("Done").clicked() {
                positions.message = Some(match positions.set_corners(name, Some(corners)) {
                    Ok(()) => {
                        session.keystone = Some(corners);
                        ("Whiteboard straightened.".into(), false)
                    }
                    Err(error) => (error, true),
                });
                positions.adjusting = None;
                session.adjusting = false;
            }
            if link(ui, "Cancel").clicked() {
                positions.adjusting = None;
                session.adjusting = false;
            }
        });
        return;
    }
    ui.horizontal_wrapped(|ui| {
        if link(ui, "Update to this view")
            .on_hover_text("Replace the saved position with where the camera points now")
            .clicked()
        {
            let updated = Position {
                pan: view.pan,
                tilt: view.tilt,
                zoom: view.zoom,
                ..current.clone()
            };
            positions.message = Some(match positions.save(updated) {
                Ok(saved) => (format!("Updated “{saved}”."), false),
                Err(error) => (error, true),
            });
        }
        let straighten = if current.corners.is_some() {
            "Adjust whiteboard corners"
        } else {
            "Straighten a whiteboard…"
        };
        if link(ui, straighten)
            .on_hover_text("Mark a whiteboard's corners so it fills the picture, squared up")
            .clicked()
        {
            positions.adjusting = Some(current.corners.unwrap_or(DEFAULT_CORNERS));
            session.adjusting = true;
            positions.message = None;
        }
        if current.corners.is_some() && link(ui, "Remove straightening").clicked() {
            if let Err(error) = positions.set_corners(name, None) {
                positions.message = Some((error, true));
            }
            session.keystone = None;
        }
        let confirming = positions.confirm_delete.as_deref() == Some(name);
        let label = if confirming {
            "Click again to delete"
        } else {
            "Delete"
        };
        if link(ui, label).clicked() {
            if confirming {
                positions.confirm_delete = None;
                positions.message = Some(match positions.delete(name) {
                    Ok(()) => {
                        session.keystone = None;
                        (format!("Deleted “{name}”."), false)
                    }
                    Err(error) => (error, true),
                });
            } else {
                positions.confirm_delete = Some(name.to_owned());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(positions: &mut Positions, session: &mut Session) -> Option<Position> {
        let ctx = egui::Context::default();
        let mut goto = None;
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let view = View {
                        pan: 10.0,
                        tilt: -5.0,
                        zoom: 40,
                    };
                    goto = show(ui, positions, session, view);
                });
            });
        }
        goto
    }

    #[test]
    fn card_renders_in_every_state_without_side_effects() {
        let path = std::env::temp_dir().join(format!("obsbot-posui-{}.conf", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut positions = Positions::load(path.clone());
        let mut session = crate::effects::EffectsController::for_tests().session;
        let before = session.clone();
        assert!(draw(&mut positions, &mut session).is_none());

        positions.draft = Some("Desk".into());
        draw(&mut positions, &mut session);
        positions.draft = None;
        positions
            .save(Position {
                name: "Board".into(),
                pan: 30.0,
                tilt: 5.0,
                zoom: 60,
                corners: Some(DEFAULT_CORNERS),
            })
            .unwrap();
        draw(&mut positions, &mut session);
        positions.adjusting = Some(DEFAULT_CORNERS);
        draw(&mut positions, &mut session);

        assert_eq!(session, before, "drawing must not change the session");
        assert_eq!(positions.list.len(), 1);
        assert_eq!(
            crate::widgets::missing_glyphs(&["📍", "🖼"]),
            Vec::<&str>::new()
        );
        let _ = std::fs::remove_file(path);
    }
}
