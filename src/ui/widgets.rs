use egui::{Align2, Color32, FontId, Painter, Response, Rounding, Stroke};

use crate::config::Config;
use crate::ui;
pub mod autocomplete;
pub mod star_rating;

pub fn selected(is_selected: bool, response: &Response, painter: &Painter) {
    if is_selected {
        let base_color = Config::global().themes.accent_fill_color().unwrap_or(Color32::WHITE);
        let secondary_color = Config::global().themes.accent_stroke_color().unwrap_or(Color32::BLACK);
        let stroke = Stroke::new(3., base_color);
        let mut text_fid = FontId::default();
        text_fid.size = 32.;
        painter.rect(response.rect, Rounding::from(3.), secondary_color.linear_multiply(0.3), stroke);
        painter.circle(response.rect.center(), 20., base_color, Stroke::NONE);
        painter.text(
            response.rect.center(),
            Align2::CENTER_CENTER,
            ui::constants::SUCCESS_ICON,
            text_fid,
            secondary_color,
        );
    }
}
