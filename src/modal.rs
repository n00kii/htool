use std::num::IntErrorKind;

use eframe::{
    self,
    egui::{
        style::Margin, Area, Button, Context, Frame, Id, InnerResponse, Label, LayerId, Layout, Order, Response, RichText, Sense, TextEdit, Ui,
        Window,
    },
    emath::{Align, Align2},
    epaint::{Color32, Pos2, Rounding},
};

pub enum ModalButtonStyle {
    None,
    Suggested,
    Caution,
}

#[derive(Clone)]
struct ModalState {
    is_open: bool,
}

pub struct ModalStyle {
    default_window_size: [f32; 2],
    body_margin: f32,
    overlay_color: Color32,

    caution_button_fill: Color32,
    suggested_button_fill: Color32,

    caution_button_text_color: Color32,
    suggested_button_text_color: Color32,
}

impl ModalState {
    fn load(ctx: &Context, id: Id) -> Self {
        ctx.data().get_persisted(id).unwrap_or_default()
    }
    fn save(self, ctx: &Context, id: Id) {
        ctx.data().insert_persisted(id, self)
    }
}

impl Default for ModalState {
    fn default() -> Self {
        Self { is_open: false }
    }
}

impl Default for ModalStyle {
    fn default() -> Self {
        Self {
            body_margin: 5.,
            default_window_size: [100., 100.],
            overlay_color: Color32::from_rgba_unmultiplied(0, 0, 0, 200),

            caution_button_fill: Color32::from_rgb(87, 38, 34),
            suggested_button_fill: Color32::from_rgb(33, 54, 84),

            caution_button_text_color: Color32::from_rgb(242, 148, 148),
            suggested_button_text_color: Color32::from_rgb(141, 182, 242),
        }
    }
}
pub struct Modal {
    close_on_outside_click: bool,
    style: ModalStyle,
    id: Id,
    window_id: Id,
}

fn ui_with_margin<R>(ui: &mut Ui, margin: f32, add_contents: impl FnOnce(&mut Ui) -> R) {
    ui.vertical(|ui| {
        ui.add_space(margin);
        ui.horizontal(|ui| {
            ui.add_space(margin);
            add_contents(ui);
            ui.add_space(margin);
        });
        ui.add_space(margin);
    });
}

impl Modal {
    pub fn new(id_source: impl std::fmt::Display) -> Self {
        Self {
            id: Id::new(id_source.to_string()),
            style: ModalStyle::default(),
            close_on_outside_click: false,
            window_id: Id::new("window_".to_string() + &id_source.to_string()),
        }
    }

    fn set_open_state(&self, ctx: &Context, is_open: bool) {
        let mut modal_state = ModalState::load(ctx, self.id);
        modal_state.is_open = is_open;
        modal_state.save(ctx, self.id)
    }

    pub fn open(&self, ctx: &Context) {
        self.set_open_state(ctx, true)
    }

    pub fn close(&self, ctx: &Context) {
        self.set_open_state(ctx, false)
    }

    pub fn with_close_on_outside_click(mut self, do_close_on_click_ouside: bool) -> Self {
        self.close_on_outside_click = do_close_on_click_ouside;
        self
    }

    pub fn with_style(mut self, style: ModalStyle) -> Self {
        self.style = style;
        self
    }

    pub fn title(&self, ui: &mut Ui, text: impl Into<RichText>) {
        let text: RichText = text.into();
        ui.vertical_centered(|ui| {
            ui.heading(text);
        });
        ui.separator();
    }

    pub fn body(&self, ui: &mut Ui, text: impl Into<RichText>) {
        let text: RichText = text.into();
        ui_with_margin(ui, self.style.body_margin, |ui| {
            ui.label(text);
        })
    }

    pub fn buttons<R>(&self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) {
        ui.separator();
        ui.with_layout(Layout::right_to_left(Align::Min), add_contents);
    }

    pub fn button(&self, ui: &mut Ui, text: impl Into<RichText>) -> Response {
        self.styled_button(ui, text, ModalButtonStyle::None)
    }
    pub fn caution_button(&self, ui: &mut Ui, text: impl Into<RichText>) -> Response {
        self.styled_button(ui, text, ModalButtonStyle::Caution)
    }
    pub fn suggested_button(&self, ui: &mut Ui, text: impl Into<RichText>) -> Response {
        self.styled_button(ui, text, ModalButtonStyle::Suggested)
    }

    pub fn styled_button(&self, ui: &mut Ui, text: impl Into<RichText>, button_style: ModalButtonStyle) -> Response {
        let button = match button_style {
            ModalButtonStyle::Suggested => {
                let text: RichText = text.into().color(self.style.suggested_button_text_color);
                Button::new(text).fill(self.style.suggested_button_fill)
            }
            ModalButtonStyle::Caution => {
                let text: RichText = text.into().color(self.style.caution_button_text_color);
                Button::new(text).fill(self.style.caution_button_fill)
            }
            ModalButtonStyle::None => Button::new(text.into()),
        };

        let response = ui.add(button);
        if response.clicked() {
            self.close(ui.ctx())
        }
        response
    }

    pub fn show<R>(&self, ctx: &Context, add_contents: impl FnOnce(&mut Ui) -> R) {
        let mut modal_state = ModalState::load(ctx, self.id);
        if modal_state.is_open {
            let ctx_clone = ctx.clone();
            Area::new(self.id).interactable(true).fixed_pos(Pos2::ZERO).show(ctx, |ui: &mut Ui| {
                let screen_rect = ui.ctx().input().screen_rect;
                let area_response = ui.allocate_response(screen_rect.size(), Sense::click());
                if area_response.clicked() && self.close_on_outside_click {
                    self.close(ctx);
                }
                ui.painter().rect_filled(screen_rect, Rounding::none(), self.style.overlay_color);
            });
            let window = Window::new("")
                .id(self.window_id)
                .open(&mut modal_state.is_open)
                .default_size(self.style.default_window_size)
                .title_bar(false)
                .anchor(Align2::CENTER_CENTER, [0., 0.])
                .resizable(false);

            let response = window.show(&ctx_clone, add_contents);
            if let Some(inner_response) = response {
                inner_response.response.request_focus();
                ctx_clone.move_to_top(inner_response.response.layer_id);
            }
        }
        // frame.
    }
}