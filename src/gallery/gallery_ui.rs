use crate::ui::RenderLoadingImageOptions;
use crate::ui::UserInterface;

use super::super::data;
use super::super::ui;
use super::super::ui::DockedWindow;
use super::super::Config;
use super::super::Data;
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
use egui_extras::RetainedImage;
use image::DynamicImage;
use poll_promise::Promise;
use rand::Rng;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::thread;

pub struct GalleryUI {
    pub root_interface_floating_windows: Option<Rc<RefCell<Vec<ui::FloatingWindowState>>>>,
    // pub root_interface: Option<Rc<UserInterface>>,
    // pub root_interface_floating_windows: Option<&'a mut Vec<Box<dyn ui::FloatingWindow>>>,
    pub config: Option<Arc<Config>>,
    pub gallery_items: Vec<Box<dyn GalleryItem>>,
}

pub struct PreviewUI {
    pub image: Option<Promise<Result<RetainedImage>>>,
    pub media_info_plural: Option<data::MediaInfoPlural>,
    pub media_info: Option<Promise<Result<data::MediaInfo>>>,
    pub config: Arc<Config>,
}

impl ui::FloatingWindow for PreviewUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.render_image(ui, ctx);
    }
}

impl PreviewUI {
    pub fn new(config: Arc<Config>) -> Box<Self> {
        Box::new(PreviewUI {
            image: None,
            config,
            media_info: None,
            media_info_plural: None,
        })
    }

    pub fn render_image(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let mut options = RenderLoadingImageOptions::default();
        options.widget_size = [500., 500.];
        let response = ui::render_loading_image(ui, ctx, self.get_image(), options);
    }

    pub fn get_image(&mut self) -> Option<&Promise<Result<RetainedImage>>> {
        match &self.image {
            None => match self.media_info.as_ref().unwrap().ready() {
                None => None,
                Some(result) => {
                    let (sender, promise) = Promise::new();
                    match result {
                        Err(_error) => sender.send(Err(anyhow::Error::msg("failed to load mediainfo"))),
                        Ok(media_info) => {
                            let hash = media_info.hash.clone();
                            let config = Arc::clone(&self.config);
                            thread::spawn(move || {
                                let bytes = Data::load_bytes(config, &hash);
                                let load = || -> Result<RetainedImage> {
                                    let bytes = bytes?;
                                    let dynamic_image = image::load_from_memory(&bytes)?;
                                    let retained_image = ui::generate_retained_image(&dynamic_image.to_rgba8())?;
                                    Ok(retained_image)
                                };
                                sender.send(load())
                            });
                        }
                    }
                    self.image = Some(promise);
                    self.image.as_ref()
                }
            },
            Some(_promise) => self.image.as_ref(),
        }

    }
    pub fn set_media_info_by_hash(&mut self, hash: String) {
        let config = Arc::clone(&self.config);
        self.media_info = Some(Promise::spawn_thread("", move || Data::load_media_info(config, &hash)))
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
                        let config = self.get_config();
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
                                            if let Some(gallery_entry) = gallery_item.as_gallery_entry() {
                                                let mut floating_windows = self.root_interface_floating_windows.as_ref().unwrap().borrow_mut();
                                                ui::UserInterface::launch_preview_by_hash(
                                                    Arc::clone(&config),
                                                    floating_windows,
                                                    gallery_entry.hash.clone(),
                                                );
                                            }
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
            root_interface_floating_windows: None,
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
