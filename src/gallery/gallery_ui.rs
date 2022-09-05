use crate::tags::tags::Tag;
use crate::ui::FloatingWindowState;
use crate::ui::RenderLoadingImageOptions;
use crate::ui::UserInterface;

use super::super::autocomplete;
use super::super::data;
use super::super::ui;
use super::super::ui::DockedWindow;
use super::super::Config;
use super::gallery::load_gallery_items;
use super::gallery::GalleryEntry;
use super::gallery::GalleryItem;
use super::gallery_ui;
use anyhow::Result;
use eframe::egui::Id;
use eframe::{
    egui::{self, Ui},
    emath::{Align, Vec2},
};
use egui::Direction;
use egui::ScrollArea;
use egui_extras::RetainedImage;
use image::DynamicImage;
use poll_promise::Promise;
use rand::Rng;
use std::cell::RefCell;
use std::cell::RefMut;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

pub struct GalleryUI {
    pub root_interface_floating_windows: Option<Rc<RefCell<Vec<ui::FloatingWindowState>>>>,
    pub preview_window_state_ids: Vec<Id>,
    pub config: Option<Arc<Config>>,
    pub toasts: egui_notify::Toasts,
    pub gallery_items: Vec<Box<dyn GalleryItem>>,
    pub search_string: String,
}

pub enum PreviewStatus {
    None,
    Closed,
    Deleted,
    FailedDelete(anyhow::Error),
}

pub struct PreviewUI {
    pub toasts: egui_notify::Toasts,
    pub image: Option<Promise<Result<RetainedImage>>>,
    pub media_info_plural: Option<data::MediaInfoPlural>,
    pub media_info: Option<Promise<Result<data::MediaInfo>>>,
    pub config: Arc<Config>,
    pub tag_edit_buffer: String,
    pub id: String,

    pub status: PreviewStatus,
    pub is_editing_tags: bool,
    pub is_confirming_delete: Arc<Mutex<bool>>,
}

//

impl ui::FloatingWindow for PreviewUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT).with_cross_align(egui::Align::TOP), |ui| {
            self.render_options(ui, ctx);
            self.render_tags(ui, ctx);
            self.render_image(ui, ctx);
        });
        self.toasts.show(ctx);
    }
}

impl PreviewUI {
    pub fn new(config: Arc<Config>, id: String) -> Box<Self> {
        Box::new(PreviewUI {
            toasts: egui_notify::Toasts::default().with_anchor(egui_notify::Anchor::BottomLeft),
            image: None,
            id,
            config,
            media_info: None,
            media_info_plural: None,
            is_editing_tags: false,
            is_confirming_delete: Arc::new(Mutex::new(false)),
            status: PreviewStatus::None,
            tag_edit_buffer: "".to_string(),
        })
    }

    pub fn get_media_info(&self) -> Option<&data::MediaInfo> {
        if let Some(info_promise) = self.media_info.as_ref() {
            if let Some(info_res) = info_promise.ready() {
                if let Ok(media_info) = info_res {
                    return Some(media_info);
                }
            }
        }
        None
    }

    pub fn get_tags(&self) -> Option<&Vec<Tag>> {
        if let Some(media_info) = self.get_media_info() {
            return Some(&media_info.tags);
        }
        None
    }

    pub fn render_options(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        let padding = 15.;
        ui.add_space(padding);
        ui.vertical(|ui| {
            ui.label("options");
            if self.is_editing_tags {
                if ui.button("save changes").clicked() {
                    self.is_editing_tags = false;
                    if let Some(media_info_promise) = self.media_info.as_ref() {
                        if let Some(media_info_res) = media_info_promise.ready() {
                            if let Ok(media_info) = media_info_res {
                                let mut tagstrings = self.tag_edit_buffer.split_whitespace().map(|x| x.to_string()).collect::<Vec<_>>(); //.collect::<Vec<_>>();
                                tagstrings.sort();
                                tagstrings.dedup();
                                let tags = tagstrings.iter().map(|tagstring| Tag::from_tagstring(tagstring)).collect::<Vec<Tag>>();
                                if let Ok(tags) = data::set_tags(Arc::clone(&self.config), &media_info.hash, &tags) {
                                    ui::toast_success(
                                        &mut self.toasts,
                                        format!("successfully set {} tag{}", tags.len(), if tags.len() == 1 { "" } else { "s" }),
                                    )
                                };
                                self.load_media_info_by_hash(media_info.hash.clone());
                            }
                        }
                    }
                }
                if ui.button("discard changes").clicked() {
                    self.is_editing_tags = false;
                }
            } else {
                if ui.button("edit tags").clicked() {
                    self.tag_edit_buffer = self
                        .get_tags()
                        .unwrap_or(&vec![])
                        .iter()
                        .map(|tag| tag.to_tagstring())
                        .collect::<Vec<String>>()
                        .join("\n");
                    self.is_editing_tags = true;
                };
            }
            ui.add_enabled_ui(!self.is_editing_tags, |ui| {
                if let Ok(mut is_confirming_delete) = self.is_confirming_delete.try_lock() {
                    if *is_confirming_delete {
                        if ui.button("are you sure?").clicked() {
                            if let Some(media_info) = self.get_media_info() {
                                if let Err(e) = data::delete_media(Arc::clone(&self.config), &media_info.hash) {
                                    ui::toast_error(&mut self.toasts, format!("failed to delete {}: {}", self.id, e));
                                } else {
                                    ui::toast_success(&mut self.toasts, format!("successfully deleted {}", self.id));
                                    self.status = PreviewStatus::Deleted;
                                }
                            }
                        }
                    } else {
                        if ui.button("delete").clicked() {
                            *is_confirming_delete = true;
                            let is_confirming_delete = Arc::clone(&self.is_confirming_delete);
                            thread::spawn(move || {
                                thread::sleep(Duration::from_secs(3));
                                if let Ok(mut is_confirming_delete) = is_confirming_delete.lock() {
                                    *is_confirming_delete = false;
                                }
                            });
                        }
                    }
                } else {
                    ui.label("...");
                }
            });
        });
        ui.add_space(padding);
    }

    pub fn render_tags(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.label("tags");
            if self.is_editing_tags {
                ui.text_edit_multiline(&mut self.tag_edit_buffer);
            } else {
                egui::Grid::new(format!("tags_{}", self.id)).striped(true).show(ui, |ui| {
                    if let Some(tags) = self.get_tags() {
                        for tag in tags {
                            ui.horizontal(|ui| {
                                let info_label = egui::Label::new("?").sense(egui::Sense::click());
                                let add_label = egui::Label::new("+").sense(egui::Sense::click());
                                if ui.add(info_label).clicked() {
                                    println!("?")
                                }
                                if ui.add(add_label).clicked() {
                                    println!("+")
                                }
                                ui.label(&tag.name);
                            });
                            ui.end_row();
                        }
                    }
                });
            }
        });
    }

    pub fn render_image(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let mut options = RenderLoadingImageOptions::default();

        options.thumbnail_size = [500., 500.];
        let _response = ui::render_loading_image(ui, ctx, self.get_image(), options);
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
                                let bytes = data::load_bytes(config, &hash);
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
    pub fn load_media_info_by_hash(&mut self, hash: String) {
        let config = Arc::clone(&self.config);
        self.media_info = Some(Promise::spawn_thread("", move || data::load_media_info(config, &hash)))
    }
    // pub load_image
}

impl GalleryUI {
    fn process_previews(&mut self) {
        if let Some(floating_windows) = self.root_interface_floating_windows.as_ref() {
            floating_windows.borrow_mut().retain(|window_state| {
                if let Some(preview) = window_state.window.downcast_ref::<PreviewUI>() {
                    match &preview.status {
                        PreviewStatus::Deleted => {
                            if let Some(media_info) = preview.get_media_info() {
                                self.gallery_items.retain(|gallery_item| {
                                    if let Some(gallery_entry) = gallery_item.downcast_ref::<GalleryEntry>() {
                                        if gallery_entry.hash == media_info.hash {
                                            return false;
                                        }
                                    }
                                    true
                                });
                            }
                            ui::set_default_toast_options(self.toasts.success(format!("successfully deleted {}", preview.id)));
                            return false;
                        }
                        PreviewStatus::FailedDelete(e) => {
                            ui::set_default_toast_options(self.toasts.error(format!("failed to delete {}: {}", preview.id, e)));
                        }
                        _ => {}
                    }
                }
                true
            })
        }
    }

    fn load_gallery_entries(&mut self) {
        let gallery_items = load_gallery_items(self.get_config(), &self.search_string);
        match gallery_items {
            Ok(gallery_items) => self.gallery_items = gallery_items,
            Err(error) => {
                ui::toast_error(&mut self.toasts, format!("failed to load items due to {}", error));
            }
        }
    }

    fn launch_preview(hash: String, config: Arc<Config>, floating_windows: Option<&Rc<RefCell<Vec<FloatingWindowState>>>>) {
        if let Some(floating_windows) = floating_windows {
            let mut floating_windows = floating_windows.borrow_mut();
            for window_state in floating_windows.iter_mut() {
                if let Some(preview_ui) = window_state.window.downcast_ref::<PreviewUI>() {
                    if let Some(info_promise) = preview_ui.media_info.as_ref() {
                        if let Some(info_res) = info_promise.ready() {
                            if let Ok(media_info) = info_res {
                                if media_info.hash == hash {
                                    window_state.is_open = true;
                                    return;
                                }
                            }
                        }
                    }
                }
            }

            let mut preview = gallery_ui::PreviewUI::new(Arc::clone(&config), hash.clone());
            preview.load_media_info_by_hash(hash.clone());
            let mut title = hash.clone();
            let widget_id = Id::new(&title);
            title.truncate(6);
            title.insert_str(0, "preview-");

            floating_windows.push(FloatingWindowState {
                title,
                widget_id,
                is_open: true,
                window: preview,
            });
        }
    }

    fn render_options(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.label("info");
            if ui.button("load").clicked() {
                self.load_gallery_entries();
                // let gallery_items = load_gallery_items(self.get_config(), &self.search_string);
                // match gallery_items {
                //     Ok(gallery_items) => self.gallery_items = gallery_items,
                //     Err(error) => {
                //         // println!("failed to load items due to {}", error);
                //         ui::toast_error(&mut self.toasts, format!("failed to load items due to {}", error));
                //     }
                // }
            }
        });
    }

    fn render_thumbnails(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.label("media");
            let thumbnail_size = self.get_config().ui.import.thumbnail_size as f32;
            ScrollArea::vertical().id_source("previews_col").show(ui, |ui| {
                let layout = egui::Layout::from_main_dir_and_cross_align(Direction::LeftToRight, Align::Center).with_main_wrap(true);
                ui.allocate_ui(Vec2::new(ui.available_size_before_wrap().x, 0.0), |ui| {
                    ui.with_layout(layout, |ui| {
                        ui.style_mut().spacing.item_spacing = Vec2::new(0., 0.);
                        let config = self.get_config();
                        let gallery_items = self.gallery_items.iter_mut();
                        for gallery_item in gallery_items {
                            let status_label = gallery_item.get_status_label().map(|label| label.into());
                            let thumbnail = gallery_item.get_thumbnail(Arc::clone(&config));
                            let mut options = RenderLoadingImageOptions::default();
                            options.hover_text_on_none_image = Some("(loading bytes for thumbnail...)".into());
                            options.hover_text_on_loading_image = Some("(loading thumbnail...)".into());
                            options.hover_text = status_label;
                            options.thumbnail_size = [thumbnail_size, thumbnail_size];
                            let response = ui::render_loading_image(ui, ctx, thumbnail, options);
                            if let Some(response) = response {
                                if response.clicked() {
                                    if let Some(gallery_entry) = gallery_item.downcast_ref::<GalleryEntry>() {
                                        GalleryUI::launch_preview(
                                            gallery_entry.hash.clone().clone(),
                                            Arc::clone(&config),
                                            self.root_interface_floating_windows.as_ref(),
                                        )
                                    }
                                }
                            }
                            // match gallery_item.get_thumbnail(Arc::clone(&config)) {
                            //     None => {
                            //         // nothing, bytes havent been loaded yet
                            //         let spinner = egui::Spinner::new();
                            //         ui.add_sized(widget_size, spinner)
                            //             .on_hover_text(format!("(loading bytes for thumbnail...)"));
                            //     }
                            //     Some(promise) => match promise.ready() {
                            //         // thumbail has started loading
                            //         None => {
                            //             // thumbnail still loading
                            //             let spinner = egui::Spinner::new();
                            //             ui.add_sized(widget_size, spinner).on_hover_text(format!("(loading thumbnail...)"));
                            //         }
                            //         Some(result) => {
                            //             let mut response = match result {
                            //                 Ok(image) => {
                            //                     let image =
                            //                         egui::widgets::Image::new(image.texture_id(ctx), image.size_vec2()).sense(egui::Sense::click());

                            //                     ui.add_sized(widget_size, image)
                            //                 }
                            //                 Err(_error) => {
                            //                     // couldn't make thumbnail
                            //                     let text = egui::RichText::new("?").size(48.0);
                            //                     let label = egui::Label::new(text).sense(egui::Sense::click());
                            //                     ui.add_sized(widget_size, label)
                            //                 }
                            //             };

                            //             if let Some(status_label) = gallery_item.get_status_label() {
                            //                 response = response.on_hover_text(format!("({status_label})"));
                            //             } else {
                            //                 // response.on_hover_text(format!("{file_label} [{mime_type}]"));
                            //             }

                            //             if response.clicked() {
                            //                 if let Some(gallery_entry) = gallery_item.downcast_ref::<GalleryEntry>() {
                            //                     // self.launch_preview(ctx, gallery_entry.hash.clone());
                            //                     GalleryUI::launch_preview(
                            //                         gallery_entry.hash.clone().clone(),
                            //                         Arc::clone(&config),
                            //                         self.root_interface_floating_windows.as_ref(),
                            //                     )
                            //                 }
                            //             }
                            //         }
                            //     },
                            // }
                        }
                    });
                });
            });
        });
    }

    fn render_search(&mut self, ui: &mut Ui) {
        let options = vec!["bruh", "moment", "red", "green", "to", "two", "yellow", "deoxyribonucleic_acid"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        ui.horizontal(|ui| {
            ui.label("search");
        });
        let autocomplete = autocomplete::create(&mut self.search_string, &options, None);
        let response = ui.add(autocomplete);

        if response.changed() {
            // dbg!(&self.search_string);
            self.load_gallery_entries();
        }
    }
}

impl Default for GalleryUI {
    fn default() -> Self {
        Self {
            search_string: String::new(),
            toasts: egui_notify::Toasts::default().with_anchor(egui_notify::Anchor::BottomLeft),
            preview_window_state_ids: vec![],
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
        self.process_previews();
        // let mut s = "".to_string();
        ui.vertical(|ui| {
            self.render_search(ui);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
                self.render_options(ui);
                self.render_thumbnails(ui, ctx);
            });
        });
        self.toasts.show(ctx);
    }
}
