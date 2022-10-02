use crate::autocomplete::AutocompleteOption;
use chrono::TimeZone;
use chrono::Utc;
use eframe::egui::Key;
use eframe::egui::Layout;
use eframe::egui::RichText;
use eframe::egui::Window;
// use crate::modal;
use crate::tags::tags::Tag;
use crate::ui::AppUI;
use crate::ui::RenderLoadingImageOptions;
use crate::ui::WindowContainer;
use egui_modal::Modal;

use super::super::autocomplete;
use super::super::data;
use super::super::ui;
use super::super::ui::UserInterface;
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
use std::time::SystemTime;

pub struct GalleryUI {
    pub preview_windows: Vec<ui::WindowContainer>,
    pub toasts: egui_notify::Toasts,
    pub gallery_items: Option<Vec<Box<dyn GalleryItem>>>,
    pub search_options: Option<Vec<autocomplete::AutocompleteOption>>,
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
    pub tag_edit_buffer: String,
    pub id: String,

    pub status: PreviewStatus,
    pub is_editing_tags: bool,
    pub short_id: String,
}

impl ui::UserInterface for PreviewUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Grid::new(format!("preview_ui_{}", self.id))
            .num_columns(3)
            .min_col_width(100.)
            .show(ui, |ui| {
                self.render_options(ui, ctx);
                self.render_info(ui, ctx);
                self.render_image(ui, ctx);
            });
        self.toasts.show(ctx);
    }
}

impl PreviewUI {
    pub fn new(id: String) -> Box<Self> {
        let mut short_id = id.clone();
        short_id.truncate(6);
        Box::new(PreviewUI {
            toasts: egui_notify::Toasts::default().with_anchor(egui_notify::Anchor::BottomLeft),
            image: None,
            id,
            short_id,
            media_info: None,
            media_info_plural: None,
            is_editing_tags: false,
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

    pub fn render_options(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let delete_modal = Modal::new(ctx, "delete_modal");
        delete_modal.show(|ui| {
            delete_modal.frame(ui, |ui| {
                delete_modal.body(ui, format!("are you sure you want to delete {}?", self.id));
            });
            delete_modal.buttons(ui, |ui| {
                delete_modal.button(ui, "cancel");
                if delete_modal.caution_button(ui, "delete").clicked() {
                    // delete logic below
                    if let Some(media_info) = self.get_media_info() {
                        if let Err(e) = data::delete_media(&media_info.hash) {
                            ui::toast_error(&mut self.toasts, format!("failed to delete {}: {}", self.id, e));
                        } else {
                            ui::toast_success(&mut self.toasts, format!("successfully deleted {}", self.id));
                            self.status = PreviewStatus::Deleted;
                        }
                    }
                }
            })
        });

        ui.add_space(ui::constants::SPACER_SIZE);
        ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
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
                                if let Ok(tags) = data::set_tags(&media_info.hash, &tags) {
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
            ui.add_space(ui::constants::SPACER_SIZE);
            ui.add_enabled_ui(!self.is_editing_tags, |ui| {
                if ui.add(ui::caution_button("delete")).clicked() {
                    delete_modal.open();
                }
            });
        });
        ui.add_space(ui::constants::SPACER_SIZE);
    }

    pub fn render_info(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.label("info");
            if let Some(media_info) = self.get_media_info() {
                egui::Grid::new(format!("info_{}", self.id)).num_columns(2).show(ui, |ui| {
                    let datetime = Utc.timestamp(media_info.date_registered, 0);
                    ui.label("registered");
                    ui.label(datetime.format("%B %e, %Y @%l:%M%P").to_string());
                    ui.end_row();

                    ui.label("size");
                    ui.label(ui::readable_byte_size(media_info.size, 2, ui::NumericBase::Two).to_string());
                    ui.end_row();
                });
            }
            ui.separator();
            ui.label("tags");
            if self.is_editing_tags {
                ui.text_edit_multiline(&mut self.tag_edit_buffer);
            } else {
                if let Some(tags) = self.get_tags() {
                    if tags.is_empty() {
                        ui.vertical_centered_justified(|ui| {
                            ui.label("({ no tags ))");
                        });
                    } else {
                        egui::Grid::new(format!("tags_{}", self.id)).num_columns(2).striped(true).show(ui, |ui| {
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
                                });
                                ui.with_layout(egui::Layout::left_to_right(Align::Center).with_main_justify(true), |ui| {
                                    ui.label(tag.to_rich_text());
                                });
                                ui.end_row();
                            }
                        });
                    }
                }
            }
        });
    }

    pub fn render_image(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let mut options = RenderLoadingImageOptions::default();
        options.shrink_to_image = true;
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
                            thread::spawn(move || {
                                let bytes = data::load_bytes(&hash);
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
        self.media_info = Some(Promise::spawn_thread("", move || data::load_media_info(&hash)))
    }
    // pub load_image
}

impl GalleryUI {
    fn process_previews(&mut self) {
        if self.gallery_items.is_none() {
            self.load_gallery_entries();
        }
        if self.search_options.is_none() {
            if let Ok(all_tag_data) = data::get_all_tag_data() {
                self.search_options = Some(
                    all_tag_data
                        .iter()
                        .map(|tag_data| AutocompleteOption {
                            label: tag_data.tag.name.clone(),
                            value: tag_data.tag.to_tagstring(),
                            color: tag_data.tag.namespace_color(),
                            description: tag_data.occurances.to_string(),
                        })
                        .collect::<Vec<_>>(),
                )
            }
        }
        // if let Some(floating_windows) = self.root_interface_floating_windows.as_ref() {
        //     floating_windows.borrow_mut().retain(|window_state| {
        //         if let Some(preview) = window_state.window.downcast_ref::<PreviewUI>() {
        //             match &preview.status {
        //                 PreviewStatus::Deleted => {
        //                     if let Some(media_info) = preview.get_media_info() {
        //                         if let Some(gallery_items) = self.gallery_items.as_mut() {
        //                             gallery_items.retain(|gallery_item| {
        //                                 if let Some(gallery_entry) = gallery_item.downcast_ref::<GalleryEntry>() {
        //                                     if gallery_entry.hash == media_info.hash {
        //                                         return false;
        //                                     }
        //                                 }
        //                                 true
        //                             });
        //                         }
        //                     }
        //                     ui::set_default_toast_options(self.toasts.success(format!("successfully deleted {}", preview.id)));
        //                     return false;
        //                 }
        //                 PreviewStatus::FailedDelete(e) => {
        //                     ui::set_default_toast_options(self.toasts.error(format!("failed to delete {}: {}", preview.id, e)));
        //                 }
        //                 _ => {}
        //             }
        //         }
        //         true
        //     })
        // }
    }

    fn load_gallery_entries(&mut self) {
        let gallery_items = load_gallery_items(&self.search_string);
        match gallery_items {
            Ok(gallery_items) => self.gallery_items = Some(gallery_items),
            Err(error) => {
                ui::toast_error(&mut self.toasts, format!("failed to load items due to {}", error));
            }
        }
    }

    fn launch_preview(hash: String, preview_windows: &mut Vec<WindowContainer>) {
        let mut preview = PreviewUI::new(hash.clone());
        let mut title = preview.short_id.clone();
        title.insert_str(0, "preview-");
        if !ui::does_window_exist(&title, preview_windows) {
            preview.load_media_info_by_hash(hash.clone());
            preview_windows.push(WindowContainer {
                title,
                is_open: Some(true),
                window: preview,
            })
        }
    }

    fn render_options(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.label("info");
            if ui.button("load").clicked() {
                self.load_gallery_entries();
            }
        });
    }

    fn render_thumbnails(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.label("media");
            let thumbnail_size = ui::constants::IMPORT_THUMBNAIL_SIZE;
            ScrollArea::vertical().id_source("previews_col").show(ui, |ui| {
                let layout = egui::Layout::from_main_dir_and_cross_align(Direction::LeftToRight, Align::Center).with_main_wrap(true);
                ui.allocate_ui(Vec2::new(ui.available_size_before_wrap().x, 0.0), |ui| {
                    ui.with_layout(layout, |ui| {
                        ui.style_mut().spacing.item_spacing = Vec2::new(0., 0.);
                        if let Some(gallery_items) = self.gallery_items.as_mut() {
                            for gallery_item in gallery_items.iter_mut() {
                                let status_label = gallery_item.get_status_label().map(|label| label.into());
                                let thumbnail = gallery_item.get_thumbnail();
                                let mut options = RenderLoadingImageOptions::default();
                                options.hover_text_on_none_image = Some("(loading bytes for thumbnail...)".into());
                                options.hover_text_on_loading_image = Some("(loading thumbnail...)".into());
                                options.hover_text = status_label;
                                options.thumbnail_size = [thumbnail_size, thumbnail_size];
                                let response = ui::render_loading_image(ui, ctx, thumbnail, options);
                                if let Some(response) = response {
                                    if response.clicked() {
                                        if let Some(gallery_entry) = gallery_item.downcast_ref::<GalleryEntry>() {
                                            GalleryUI::launch_preview(gallery_entry.hash.clone(), &mut self.preview_windows)
                                        }
                                    }
                                }
                            }
                        }
                    });
                });
            });
        });
    }

    fn render_search(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("search");

            ui.with_layout(Layout::top_down(Align::Center).with_cross_justify(true), |ui| {
                if let Some(search_options) = self.search_options.as_ref() {
                    let autocomplete = autocomplete::create(&mut self.search_string, search_options, true);
                    ui.add(autocomplete);
                    if ui.ctx().input().key_pressed(Key::Tab)
                        || ui.ctx().input().key_pressed(Key::Space)
                        || ui.ctx().input().key_pressed(Key::Backspace)
                    {
                        self.load_gallery_entries();
                    }
                }
            });
        });
    }

    fn render_preview_windows(&mut self, ctx: &egui::Context) {
        self.preview_windows.retain(|window| window.is_open.unwrap());
        for preview_window in self.preview_windows.iter_mut() {
            egui::Window::new(&preview_window.title)
                .open(preview_window.is_open.as_mut().unwrap())
                .default_size([800.0, 400.0])
                .collapsible(false)
                .vscroll(false)
                .hscroll(false)
                .show(ctx, |ui| {
                    preview_window.window.ui(ui, ctx);
                });
        }
    }
}

impl Default for GalleryUI {
    fn default() -> Self {
        Self {
            preview_windows: vec![],
            search_string: String::new(),
            toasts: egui_notify::Toasts::default().with_anchor(egui_notify::Anchor::BottomLeft),
            gallery_items: None,
            search_options: None,
        }
    }
}

impl ui::UserInterface for GalleryUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.process_previews();
        self.render_preview_windows(ctx);
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
