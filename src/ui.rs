// todo put common ui stuff in here, like generating thumbnails of specific sizew

use std::sync::Arc;

use super::config::Config;
use super::gallery::gallery_ui;
use super::import::import_ui;
use anyhow::Result;
use eframe::{
    egui::{self, ScrollArea},
    emath::Vec2,
    epaint::Color32,
};
use egui_extras::RetainedImage;
use image::{FlatSamples, ImageBuffer, Rgba};

pub fn generate_retained_image(image_buffer: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<RetainedImage> {
    let pixels = image_buffer.as_flat_samples();
    let color_image = egui::ColorImage::from_rgba_unmultiplied([pixels.extents().1, pixels.extents().2], pixels.as_slice());
    Ok(RetainedImage::from_color_image("", color_image))
}

pub trait DockedWindow {
    fn get_config(&self) -> Arc<Config>;
    fn set_config(&mut self, config: Arc<Config>);
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context);
}
pub trait FloatingWindow {
    fn ui(&mut self, ui: &mut egui::Ui);
}

struct FloatingWindowState {
    title: String,
    is_open: bool,
    window: Box<dyn FloatingWindow>,
}

struct DockedWindowState {
    id: String,
    window: Box<dyn DockedWindow>,
}

pub struct UserInterface {
    config: Arc<Config>,
    current_window: String,
    floating_windows: Vec<FloatingWindowState>,
    docked_windows: Vec<DockedWindowState>,
}

impl eframe::App for UserInterface {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_floating_windows(ctx);
        self.render_top_bar(ctx);
        self.render_side_panel(ctx);
        self.render_current_window(ctx);
    }
}

impl UserInterface {
    pub fn new(config: Arc<Config>) -> Self {
        UserInterface {
            floating_windows: vec![],
            docked_windows: vec![],
            current_window: "".into(),
            config,
        }
    }

    pub fn clone_config(&self) -> Arc<Config> {
        Arc::clone(&self.config)
    }

    pub fn start(app: UserInterface) {
        let mut options = eframe::NativeOptions::default();
        options.initial_window_size = Some(Vec2::new(1390.0, 600.0));
        eframe::run_native("htool2", options, Box::new(|_cc| Box::new(app)));
    }

    pub fn load_docked_windows(&mut self) {
        let mut window_states = vec![
            DockedWindowState {
                window: Box::new(gallery_ui::GalleryUI::default()),
                id: "gallery".to_string(),
            },
            DockedWindowState {
                window: Box::new(import_ui::ImporterUI::default()),
                id: "importer".to_string(),
            },
        ];

        for window in window_states.iter_mut() {
            window.window.set_config(self.clone_config());
        }
        self.docked_windows = window_states;
    }

    pub fn launch_preview(&mut self) {
        let preview = gallery_ui::PreviewUI::new(Arc::clone(&self.config));
        self.floating_windows.push(FloatingWindowState {
            title: "preview".into(),
            is_open: false,
            window: preview,
        });
    }

    pub fn render_floating_windows(&mut self, ctx: &egui::Context) {
        for window_state in self.floating_windows.iter_mut() {
            // window_state.window.show(ctx, &mut window_state.is_open);
            egui::Window::new(&window_state.title)
                .open(&mut window_state.is_open)
                .default_size([800.0, 400.0])
                .vscroll(false)
                .hscroll(true)
                .show(ctx, |ui| {
                    window_state.window.ui(ui);
                });
        }
    }

    fn render_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("wrap_app_top_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.visuals_mut().button_frame = false;
                egui::widgets::global_dark_light_mode_switch(ui);
                // ui.separator();
                // if ui.button("Organize windows").clicked() {
                //     ui.ctx().memory().reset_areas();
                // }
                ui.separator();
                for window_state in self.docked_windows.iter_mut() {
                    let response = ui.selectable_label(self.current_window == window_state.id, window_state.id.clone());
                    if response.clicked() {
                        self.current_window = window_state.id.clone();
                    }
                }
            });
        });
    }

    fn render_side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("egui_demo_panel")
            .resizable(false)
            .default_width(145.0)
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                        ui.add_space(5.0);
                        for window_state in self.floating_windows.iter_mut() {
                            ui.toggle_value(&mut window_state.is_open, window_state.title.clone());
                        }
                    });
                });
            });
    }

    fn render_current_window(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.current_window == "".to_string() {
                ui.label("Nothing here but us chickens!");
            } else {
                for window_state in self.docked_windows.iter_mut() {
                    if window_state.id == self.current_window {
                        window_state.window.ui(ui, ctx)
                    }
                }
            }
        });
    }
}
