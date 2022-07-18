use super::super::ui;
use super::super::ui::DockedWindow;
use super::super::Config;
use super::super::Data;
use super::super::data;
use super::gallery::load_gallery_items;
use super::gallery::GalleryEntry;
use super::gallery::GalleryItem;
use super::gallery_ui;
use anyhow::Result;
use eframe::egui::Direction;
use eframe::egui::ScrollArea;
use eframe::{
    egui::{self, Ui},
    emath::{Align, Vec2},
};
use poll_promise::Promise;
use rand::Rng;
use std::sync::Arc;

pub struct GalleryUI {
    config: Option<Arc<Config>>,
    gallery_items: Vec<Box<dyn GalleryItem>>,
}

pub struct PreviewUI {
    media_info_plural: Option<data::MediaInfoPlural>,
    media_info: Option<Promise<Result<data::MediaInfo>>>,
    config: Arc<Config>,
}

impl ui::FloatingWindow for PreviewUI {
    fn ui(&mut self, ui: &mut egui::Ui) {
        self.render_image(ui)
    }
}

impl PreviewUI {
    pub fn new(config: Arc<Config>) -> Box<Self> {
        Box::new(PreviewUI {
            config,
            media_info: None,
            media_info_plural: None,
        })
    }
    pub fn render_image(&mut self, ui: &mut Ui) {
        ui.label("text");
    }
    pub fn set_media_info_by_hash(&mut self, hash: String) {
        let config = Arc::clone(&self.config);
        self.media_info = Some(Promise::spawn_thread("", move || {
            Data::load_media_info(config, &hash)
        }))
    }
    // pub load_image
}

impl GalleryUI {
    fn render_options(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.label("info");
            if ui.button("load").clicked() {
                let gallery_items = load_gallery_items(self.get_config());
                match gallery_items {
                    Ok(gallery_items) => self.gallery_items = gallery_items,
                    Err(error) => {
                        println!("failed to load items due to {}", error)
                    }
                }
            }
        });
    }

    fn render_thumbnails(&mut self, ui: &mut Ui, ctx: &egui::Context, config: Arc<Config>) {
        ui.vertical(|ui| {
            ui.label("media");
            let thumbnail_size = self.get_config().ui.import.thumbnail_size;
            ScrollArea::vertical().id_source("previews_col").show(ui, |ui| {
                let layout = egui::Layout::from_main_dir_and_cross_align(Direction::LeftToRight, Align::Center).with_main_wrap(true);
                ui.allocate_ui(Vec2::new(ui.available_size_before_wrap().x, 0.0), |ui| {
                    ui.with_layout(layout, |ui| {
                        ui.style_mut().spacing.item_spacing = Vec2::new(0., 0.);
                        for gallery_item in self.gallery_items.iter_mut() {
                            let widget_size = (thumbnail_size + 3) as f32;
                            let widget_size = [widget_size, widget_size];
                            match gallery_item.get_thumbnail(Arc::clone(&config)) {
                                None => {
                                    // nothing, bytes havent been loaded yet
                                    let spinner = egui::Spinner::new();
                                    ui.add_sized(widget_size, spinner)
                                        .on_hover_text(format!("(loading bytes for thumbnail...)"));
                                }
                                Some(promise) => match promise.ready() {
                                    // thumbail has started loading
                                    None => {
                                        // thumbnail still loading
                                        let spinner = egui::Spinner::new();
                                        ui.add_sized(widget_size, spinner).on_hover_text(format!("(loading thumbnail...)"));
                                    }
                                    Some(result) => {
                                        let mut response = match result {
                                            Ok(image) => {
                                                let image =
                                                    egui::widgets::Image::new(image.texture_id(ctx), image.size_vec2()).sense(egui::Sense::click());

                                                ui.add_sized(widget_size, image)
                                            }
                                            Err(error) => {
                                                // couldn't make thumbnail
                                                let text = egui::RichText::new("?").size(48.0);
                                                let label = egui::Label::new(text).sense(egui::Sense::click());
                                                ui.add_sized(widget_size, label)
                                            }
                                        };

                                        if let Some(status_label) = gallery_item.get_status_label() {
                                            response = response.on_hover_text(format!("({status_label})"));
                                        } else {
                                            // response.on_hover_text(format!("{file_label} [{mime_type}]"));
                                        }

                                        if response.clicked() {
                                            println!("hello");
                                            egui::Window::new("My Window").show(ctx, |ui| {
                                                ui.label("Hello World!");
                                            });
                                        }
                                    }
                                },
                            }
                        }
                    });
                });
            });
        });
    }
}

impl Default for GalleryUI {
    fn default() -> Self {
        Self {
            config: None,
            gallery_items: vec![],
        }
    }
}

impl ui::DockedWindow for GalleryUI {
    fn set_config(&mut self, config: Arc<Config>) {
        self.config = Some(config);
    }
    fn get_config(&self) -> Arc<Config> {
        Arc::clone(self.config.as_ref().unwrap())
    }
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // egui::CentralPanel::default().show(ctx, |ui| {
        ui.with_layout(egui::Layout::left_to_right(), |ui| {
            self.render_options(ui);
            self.render_thumbnails(ui, ctx, Arc::clone(&self.get_config()));
        });
        // });
    }
}
