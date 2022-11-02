

use egui::{pos2, vec2, Rect, Stroke};




use eframe::{
    egui::{self, Response, Sense, Ui},
};




use crate::config::Config;

use super::{constants, darker, generate_star_shape};

pub fn star_rating(ui: &mut Ui, current_value: &mut i64, max_value: usize) -> Response {
    let outer_canvas_height = 25.;
    let step_spacing = 0.3; // fraction
    let canvas_padding = 5.;
    let stroke_width = 1.;

    let outer_canvas_width = ui.available_width();
    let stepper_size = vec2(outer_canvas_width, outer_canvas_height);
    let (mut response, painter) = ui.allocate_painter(stepper_size, Sense::click());
    let outer_step_width = outer_canvas_width / max_value as f32;
    let inner_step_width = outer_step_width * (1. - step_spacing);
    let inner_canvas_height = outer_canvas_height - canvas_padding;
    let mut was_clicked = false;
    if response.double_clicked() {
        response.mark_changed();
        *current_value = 0;
    } else if response.clicked() || response.dragged() {
        response.mark_changed();
        was_clicked = true;
    }

    let mut paint_again = None;

    for step_index in 0..max_value {
        let is_selected = step_index < *current_value as usize;
        let left = response.rect.left() + step_index as f32 * outer_step_width;
        let top = response.rect.top() + canvas_padding;
        let right = left + outer_step_width;
        let bottom = top + inner_canvas_height;
        let step_center = [(left + right) / 2., (top + bottom) / 2.];
        let step_rect = Rect::from_min_max(pos2(left, top), pos2(right, bottom));
        let shape_radius = inner_step_width / 1.5;

        let true_base_color = Config::global().themes.override_widget_primary().unwrap_or(constants::FAVORITE_ICON_SELECTED_FILL);
        let base_color = if ui.is_enabled() {
            true_base_color
        } else {
            darker(true_base_color)
        };
        let (fill_color, stroke_color) = if is_selected {
            (base_color, darker(base_color))
        } else {
            (darker(base_color), darker(darker(base_color)))
        };

        let stroke = Stroke::new(stroke_width, stroke_color);
        let shape = generate_star_shape(shape_radius, step_center, fill_color, stroke);

        painter.add(shape);

        if ui.rect_contains_pointer(step_rect) {
            if was_clicked {
                *current_value = (step_index as i64) + 1;
            } else {
                paint_again = Some((shape_radius * 1.2, step_center, fill_color, stroke))
            }
        }

        if let Some(paint_args) = paint_again {
            ui.painter()
                .add(generate_star_shape(paint_args.0, paint_args.1, paint_args.2, paint_args.3));
        }
    }
    response
}
