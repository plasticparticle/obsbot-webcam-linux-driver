//! Custom-painted controls shared by the control center panels.

use crate::theme::*;
use eframe::egui::{
    self, epaint::Mesh, Align2, Color32, CursorIcon, FontId, Pos2, Rect, Response, RichText, Sense,
    Shape, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType,
};
use std::ops::RangeInclusive;

/// Width of each cell when `columns` equal cells share the available width.
pub fn cell_width(ui: &Ui, columns: usize, gap: f32) -> f32 {
    ((ui.available_width() - gap * (columns as f32 - 1.0)) / columns as f32).floor()
}

/// Lay `count` items out as a grid of equal cells; `add` draws item `index`.
pub fn grid(
    ui: &mut Ui,
    count: usize,
    columns: usize,
    height: f32,
    mut add: impl FnMut(&mut Ui, usize, Vec2),
) {
    let gap = 8.0;
    let size = Vec2::new(cell_width(ui, columns, gap), height);
    for row in 0..count.div_ceil(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for index in (row * columns)..((row + 1) * columns).min(count) {
                add(ui, index, size);
            }
        });
    }
}

/// Small muted heading used to group controls inside a card.
pub fn caption(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(text.to_uppercase())
            .size(9.0)
            .color(TEXT_MUTED),
    );
}

pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(10.5).color(TEXT_MUTED));
}

/// A titled card. `trailing` adds controls to the right end of the header.
pub fn card(
    ui: &mut Ui,
    icon: &str,
    title: &str,
    trailing: impl FnOnce(&mut Ui),
    content: impl FnOnce(&mut Ui),
) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, CARD_BORDER))
        .corner_radius(12)
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).size(16.0).color(ACCENT_HOVER));
                ui.label(RichText::new(title).size(15.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), trailing);
            });
            ui.add_space(3.0);
            content(ui);
        });
}

/// Borderless text button for secondary actions such as "Reset".
pub fn link(ui: &mut Ui, text: &str) -> Response {
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), FontId::proportional(11.0), TEXT_MUTED);
    let (rect, response) =
        ui.allocate_exact_size(galley.size() + Vec2::new(12.0, 8.0), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, text));
    if ui.is_rect_visible(rect) {
        let color = if response.hovered() {
            ACCENT_HOVER
        } else {
            TEXT_MUTED
        };
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(11.0),
            color,
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn tile_frame(ui: &Ui, rect: Rect, response: &Response, selected: bool) {
    let glow = ui.ctx().animate_bool_responsive(response.id, selected);
    let base = if response.hovered() { TILE_HOVER } else { TILE };
    let painter = ui.painter();
    painter.rect_filled(rect, 10.0, mix(base, TILE_ACTIVE, glow));
    painter.rect_stroke(
        rect,
        10.0,
        Stroke::new(1.0 + 0.6 * glow, mix(CARD_BORDER, ACCENT, glow)),
        StrokeKind::Inside,
    );
}

/// A square-ish toggle tile with a large icon and a label beneath it.
pub fn icon_tile(ui: &mut Ui, size: Vec2, icon: &str, label: &str, selected: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, selected, label));
    if ui.is_rect_visible(rect) {
        tile_frame(ui, rect, &response, selected);
        let painter = ui.painter();
        painter.text(
            Pos2::new(rect.center().x, rect.top() + (size.y - 16.0) * 0.5),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(23.0),
            if selected { Color32::WHITE } else { TEXT },
        );
        painter.text(
            Pos2::new(rect.center().x, rect.bottom() - 12.0),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(10.5),
            if selected { TEXT } else { TEXT_MUTED },
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A tile showing a two-colour gradient swatch, used for colour looks.
pub fn swatch_tile(
    ui: &mut Ui,
    size: Vec2,
    colors: [[u8; 3]; 2],
    label: &str,
    selected: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, selected, label));
    if ui.is_rect_visible(rect) {
        tile_frame(ui, rect, &response, selected);
        let swatch = Rect::from_min_max(
            rect.min + Vec2::splat(5.0),
            Pos2::new(rect.right() - 5.0, rect.bottom() - 22.0),
        );
        gradient_rect(ui, swatch, 6.0, rgb(colors[0]), rgb(colors[1]));
        if selected {
            ui.painter().text(
                swatch.center(),
                Align2::CENTER_CENTER,
                "✔",
                FontId::proportional(15.0),
                Color32::WHITE,
            );
        }
        ui.painter().text(
            Pos2::new(rect.center().x, rect.bottom() - 11.5),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(10.5),
            if selected { TEXT } else { TEXT_MUTED },
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A round colour chip; `None` draws the "no colour" option.
pub fn color_dot(ui: &mut Ui, color: Option<[u8; 3]>, label: &str, selected: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    response.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, selected, label));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let center = rect.center();
        let radius = if response.hovered() { 11.0 } else { 10.0 };
        match color {
            Some(color) => {
                painter.circle_filled(center, radius, rgb(color));
            }
            None => {
                painter.circle_stroke(center, radius - 0.5, Stroke::new(1.5_f32, TEXT_MUTED));
                let reach = Vec2::splat(radius * 0.68);
                painter.line_segment(
                    [center - reach, center + reach],
                    Stroke::new(1.5_f32, TEXT_MUTED),
                );
            }
        }
        if selected {
            painter.circle_stroke(center, 13.5, Stroke::new(1.8_f32, ACCENT_HOVER));
        }
    }
    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(label)
}

/// An on/off switch.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(38.0, 22.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, *on, label));
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(response.id, *on);
        let painter = ui.painter();
        let track = mix(Color32::from_rgb(52, 58, 80), ACCENT, t);
        painter.rect_filled(rect, 11.0, track);
        let x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), t);
        painter.circle_filled(Pos2::new(x, rect.center().y), 8.0, Color32::WHITE);
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A labelled 0–100 % slider row. Clicking the value restores `default`.
pub fn amount(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: RangeInclusive<f32>,
    default: f32,
) -> bool {
    let mut changed = false;
    let signed = *range.start() < 0.0;
    ui.horizontal(|ui| {
        // A slimmer track and handle than the default controls.
        ui.spacing_mut().interact_size.y = 22.0;
        let (label_rect, _) = ui.allocate_exact_size(Vec2::new(98.0, 22.0), Sense::hover());
        ui.painter().text(
            label_rect.left_center(),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(12.0),
            TEXT,
        );
        let value_width = 40.0;
        ui.spacing_mut().slider_width =
            (ui.available_width() - value_width - ui.spacing().item_spacing.x).max(60.0);
        let slider = ui.add(
            egui::Slider::new(value, range.clone())
                .show_value(false)
                .trailing_fill(!signed)
                .clamping(egui::SliderClamping::Always),
        );
        slider.widget_info(|| WidgetInfo::slider(true, f64::from(*value), label));
        changed |= slider.changed();

        let is_default = (*value - default).abs() < 0.005;
        let (value_rect, reset) =
            ui.allocate_exact_size(Vec2::new(value_width, 22.0), Sense::click());
        let percent = (*value * 100.0).round() as i32;
        let offer_reset = reset.hovered() && !is_default;
        let text = if offer_reset {
            "↺".to_owned()
        } else if signed && percent > 0 {
            format!("+{percent}")
        } else {
            format!("{percent}")
        };
        ui.painter().text(
            value_rect.right_center(),
            Align2::RIGHT_CENTER,
            text,
            FontId::proportional(if offer_reset { 14.0 } else { 11.0 }),
            if offer_reset {
                ACCENT_HOVER
            } else if is_default {
                TEXT_MUTED
            } else {
                TEXT
            },
        );
        if !is_default {
            let reset = reset
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text("Reset");
            if reset.clicked() {
                *value = default;
                changed = true;
            }
        }
    });
    changed
}

/// Equal-width segmented control. Returns true when the selection changed.
pub fn segmented(ui: &mut Ui, labels: &[String], selected: &mut usize) -> bool {
    let height = 34.0;
    let (outer, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    ui.painter().rect_filled(outer, 10.0, TILE);
    let inner = outer.shrink(3.0);
    let width = inner.width() / labels.len() as f32;
    let mut changed = false;
    for (index, label) in labels.iter().enumerate() {
        let rect = Rect::from_min_size(
            Pos2::new(inner.left() + width * index as f32, inner.top()),
            Vec2::new(width, inner.height()),
        );
        let response = ui
            .interact(rect, ui.id().with(("segment", label)), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        response.widget_info(|| {
            WidgetInfo::selected(WidgetType::RadioButton, true, *selected == index, label)
        });
        let active = *selected == index;
        if active {
            ui.painter().rect_filled(rect, 8.0, ACCENT);
        } else if response.hovered() {
            ui.painter().rect_filled(rect, 8.0, TILE_HOVER);
        }
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(12.5),
            if active { Color32::WHITE } else { TEXT_MUTED },
        );
        if response.clicked() && !active {
            *selected = index;
            changed = true;
        }
    }
    changed
}

/// Tab buttons in rows of `columns`. `tabs` holds (icon, label, active effect count).
pub fn tab_bar(ui: &mut Ui, tabs: &[(&str, &str, usize)], columns: usize, selected: &mut usize) {
    let gap = 4.0;
    let size = Vec2::new(cell_width(ui, columns, gap), 34.0);
    for row in tabs.chunks(columns).enumerate() {
        let (row_index, row_tabs) = row;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (offset, (icon, label, count)) in row_tabs.iter().enumerate() {
                let index = row_index * columns + offset;
                let (rect, response) = ui.allocate_exact_size(size, Sense::click());
                let active = *selected == index;
                response.widget_info(|| {
                    WidgetInfo::selected(WidgetType::RadioButton, true, active, *label)
                });
                if ui.is_rect_visible(rect) {
                    let t = ui.ctx().animate_bool_responsive(response.id, active);
                    let base = if response.hovered() { TILE_HOVER } else { TILE };
                    let painter = ui.painter();
                    painter.rect_filled(rect, 9.0, mix(base, ACCENT, t));
                    painter.text(
                        Pos2::new(rect.left() + 17.0, rect.center().y),
                        Align2::CENTER_CENTER,
                        *icon,
                        FontId::proportional(15.0),
                        if active { Color32::WHITE } else { TEXT },
                    );
                    painter.text(
                        Pos2::new(rect.left() + 31.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        *label,
                        FontId::proportional(11.0),
                        if active { Color32::WHITE } else { TEXT_MUTED },
                    );
                    if *count > 0 {
                        let dot = Pos2::new(rect.right() - 7.0, rect.top() + 7.0);
                        painter.circle_filled(dot, 3.0, SUCCESS);
                    }
                }
                let response = response.on_hover_cursor(CursorIcon::PointingHand);
                let response = match count {
                    0 => response,
                    1 => response.on_hover_text("1 effect on"),
                    n => response.on_hover_text(format!("{n} effects on")),
                };
                if response.clicked() {
                    *selected = index;
                }
            }
        });
    }
}

/// A stage seen from the camera: drag the lamp around the head to aim the
/// key light. `x` and `y` run from -1 to 1, with +y above the head.
/// Returns true when the position changed.
pub fn light_pad(ui: &mut Ui, x: &mut f32, y: &mut f32, lamp: [u8; 3], lit: bool) -> bool {
    let size = Vec2::new(ui.available_width(), 132.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, "Key light position"));
    let travel = Vec2::new(rect.width() / 2.0 - 18.0, rect.height() / 2.0 - 16.0);
    let mut changed = false;
    if let Some(pointer) = response.interact_pointer_pos() {
        let offset = pointer - rect.center();
        let next_x = (offset.x / travel.x).clamp(-1.0, 1.0);
        let next_y = (-offset.y / travel.y).clamp(-1.0, 1.0);
        changed = (next_x - *x).abs() > 0.001 || (next_y - *y).abs() > 0.001;
        *x = next_x;
        *y = next_y;
    }
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 10.0, Color32::from_rgb(14, 17, 27));
        painter.rect_stroke(
            rect,
            10.0,
            Stroke::new(1.0_f32, CARD_BORDER),
            StrokeKind::Inside,
        );
        let center = rect.center();
        let lamp_pos = center + Vec2::new(*x * travel.x, -*y * travel.y);
        let color = if lit { rgb(lamp) } else { TEXT_MUTED };
        if lit {
            // The pool of light the lamp throws.
            for (radius, alpha) in [(46.0, 10), (32.0, 16), (20.0, 26)] {
                painter.circle_filled(
                    lamp_pos,
                    radius,
                    Color32::from_rgba_unmultiplied(lamp[0], lamp[1], lamp[2], alpha),
                );
            }
        }
        // Head and shoulders, shaded on the side away from the lamp.
        let head = center + Vec2::new(0.0, -8.0);
        let shadow = Color32::from_rgb(38, 43, 62);
        let skin = if lit {
            mix(Color32::from_rgb(92, 98, 126), rgb(lamp), 0.35)
        } else {
            Color32::from_rgb(70, 76, 100)
        };
        let shoulders =
            Rect::from_center_size(center + Vec2::new(0.0, 42.0), Vec2::new(84.0, 44.0));
        painter.rect_filled(shoulders, 20.0, shadow);
        painter.circle_filled(head, 23.0, shadow);
        let lean = Vec2::new(*x, -*y) * 7.0;
        painter.circle_filled(head + lean, 23.0 - lean.length() * 0.9, skin);
        painter.line_segment(
            [lamp_pos, head],
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 26)),
        );
        let grab = if response.dragged() || response.hovered() {
            9.5
        } else {
            8.0
        };
        painter.circle_filled(lamp_pos, grab, color);
        painter.circle_stroke(lamp_pos, grab + 2.5, Stroke::new(1.5_f32, Color32::WHITE));
    }
    response
        .on_hover_cursor(CursorIcon::Grab)
        .on_hover_text("Drag the lamp to move the key light");
    changed
}

/// A selectable pill, used for profiles.
pub fn pill(ui: &mut Ui, text: &str, selected: bool) -> Response {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::proportional(12.0),
        if selected { Color32::WHITE } else { TEXT },
    );
    let size = Vec2::new(galley.size().x + 24.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, selected, text));
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(response.id, selected);
        let base = if response.hovered() { TILE_HOVER } else { TILE };
        let painter = ui.painter();
        painter.rect_filled(rect, 14.0, mix(base, ACCENT, t));
        painter.rect_stroke(
            rect,
            14.0,
            Stroke::new(1.0_f32, mix(CARD_BORDER, ACCENT_HOVER, t)),
            StrokeKind::Inside,
        );
        painter.galley(
            Pos2::new(rect.left() + 12.0, rect.center().y - galley.size().y / 2.0),
            galley,
            TEXT,
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A removable pill. Returns true when its remove button was clicked.
pub fn chip(ui: &mut Ui, text: &str) -> bool {
    let font = FontId::proportional(11.0);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, TEXT);
    let size = Vec2::new(galley.size().x + 38.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response
        .widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, format!("Remove {text}")));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let hovered = response.hovered();
        painter.rect_filled(rect, 12.0, if hovered { TILE_HOVER } else { TILE });
        painter.rect_stroke(
            rect,
            12.0,
            Stroke::new(1.0_f32, if hovered { DANGER } else { CARD_BORDER }),
            StrokeKind::Inside,
        );
        painter.galley(
            Pos2::new(rect.left() + 11.0, rect.center().y - galley.size().y / 2.0),
            galley,
            TEXT,
        );
        painter.text(
            Pos2::new(rect.right() - 13.0, rect.center().y),
            Align2::CENTER_CENTER,
            "✖",
            FontId::proportional(9.0),
            if hovered { DANGER } else { TEXT_MUTED },
        );
    }
    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text("Remove")
        .clicked()
}

/// A rounded status badge with a coloured dot.
pub fn status_pill(ui: &mut Ui, text: &str, color: Color32) {
    let font = FontId::proportional(10.0);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let size = Vec2::new(galley.size().x + 32.0, 22.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 11.0, mix(CARD, color, 0.16));
    painter.circle_filled(Pos2::new(rect.left() + 12.0, rect.center().y), 3.5, color);
    painter.galley(
        Pos2::new(rect.left() + 21.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
}

fn rgb(color: [u8; 3]) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}

fn mix(from: Color32, to: Color32, t: f32) -> Color32 {
    let channel = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Color32::from_rgb(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
    )
}

/// Rounded rectangle filled with a diagonal two-colour gradient.
fn gradient_rect(ui: &Ui, rect: Rect, radius: f32, from: Color32, to: Color32) {
    let color_at = |point: Pos2| {
        let t =
            ((point.x - rect.left()) / rect.width() + (point.y - rect.top()) / rect.height()) / 2.0;
        mix(from, to, t.clamp(0.0, 1.0))
    };
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.center(), color_at(rect.center()));
    let corners = [
        (rect.right() - radius, rect.top() + radius, -90.0_f32),
        (rect.right() - radius, rect.bottom() - radius, 0.0),
        (rect.left() + radius, rect.bottom() - radius, 90.0),
        (rect.left() + radius, rect.top() + radius, 180.0),
    ];
    for (x, y, start) in corners {
        for step in 0..=6 {
            let angle = (start + step as f32 * 15.0).to_radians();
            let point = Pos2::new(x + radius * angle.cos(), y + radius * angle.sin());
            mesh.colored_vertex(point, color_at(point));
        }
    }
    let rim = mesh.vertices.len() as u32 - 1;
    for index in 0..rim {
        mesh.add_triangle(0, 1 + index, 1 + (index + 1) % rim);
    }
    ui.painter().add(Shape::mesh(mesh));
}

/// The entries of `icons` that the bundled fonts cannot draw.
#[cfg(test)]
pub fn missing_glyphs<'a>(icons: &[&'a str]) -> Vec<&'a str> {
    let ctx = egui::Context::default();
    let mut missing = Vec::new();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        let font = FontId::proportional(14.0);
        ctx.fonts(|fonts| {
            for icon in icons {
                if !fonts.has_glyphs(&font, icon) {
                    missing.push(*icon);
                }
            }
        });
    });
    missing
}
