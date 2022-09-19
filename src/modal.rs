use std::num::IntErrorKind;

use eframe::{
    self,
    egui::{Area, Context, Frame, Id, InnerResponse, LayerId, Order, Sense, Ui, Window, RichText, style::Margin, Label, TextEdit, Response, Layout},
    emath::Align2,
    epaint::{Color32, Pos2, Rounding},
};

#[derive(Clone)]
struct ModalState {
    is_open: bool,
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
pub struct Modal {
    close_on_outside_click: bool,
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

    pub fn close_on_outside_click(mut self, do_close_on_click_ouside: bool) -> Self {
        self.close_on_outside_click = do_close_on_click_ouside;
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
        ui_with_margin(ui, 5., |ui| {
            ui.label(text);
        })
    }

    pub fn buttons<R>(&self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) {
        // ui.with_layout(Layout::, add_contents)
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
                ui.painter().rect_filled(screen_rect, Rounding::none(), Color32::from_rgba_unmultiplied(100, 100, 100, 100));
            });
            let window = Window::new("")
                .id(self.window_id)
                .open(&mut modal_state.is_open)
                .default_size([400., 600.])
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
