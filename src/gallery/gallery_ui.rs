use crate::autocomplete::AutocompleteOption;
use crate::data::EntryId;
use chrono::TimeZone;
use chrono::Utc;
use eframe::egui::Key;
use eframe::egui::Layout;
use eframe::egui::RichText;
use eframe::egui::Window;
use egui::vec2;
use egui::Align2;
use egui::Area;
use egui::Color32;
use egui::Event;
use egui::Frame;
use egui::Mesh;
use egui::PointerButton;
use egui::Pos2;
use egui::Rect;
use egui::Rounding;
use egui::Sense;
// use crate::modal;
use crate::tags::tags::Tag;
use crate::ui::AppUI;
use crate::ui::RenderLoadingImageOptions;
use crate::ui::UserInterface;
use crate::ui::WindowContainer;
use crate::util::PollBuffer;
use egui_modal::Modal;

use super::super::autocomplete;
use super::super::data;
use super::super::ui;
use super::super::Config;
use super::gallery::load_gallery_entries;
// use super::gallery::GalleryEntry;
use super::gallery::GalleryEntry;
// use super::gallery::GalleryItem;
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
    pub load_buffer: PollBuffer<GalleryEntry>,
    pub toasts: egui_notify::Toasts,
    pub loading_gallery_entries: Option<Promise<Result<Vec<GalleryEntry>>>>,
    pub gallery_entries: Option<Vec<Rc<RefCell<GalleryEntry>>>>,
    pub preview_windows: Vec<ui::WindowContainer>,
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
    pub media_info_plural: Option<data::PoolInfo>,
    pub media_info: Option<Promise<Result<data::MediaInfo>>>,
    pub tag_edit_buffer: String,
    pub id: String,
    pub is_fullscreen: bool,

    pub view_offset: [f32; 2],
    pub view_zoom: f32,

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
            is_fullscreen: false,
            view_offset: [0., 0.],
            view_zoom: 0.5,
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
        if self.is_fullscreen {
            let area = Area::new("media_fullview").interactable(true).fixed_pos(Pos2::ZERO);
            area.show(ctx, |ui: &mut Ui| {
                let screen_rect = ui.ctx().input().screen_rect;
                ui.painter().rect_filled(screen_rect, Rounding::none(), Color32::BLACK);
                let mut options = RenderLoadingImageOptions::default();
                options.desired_image_size = (screen_rect.size() * self.view_zoom).into();
                let area_response = ui.allocate_response(screen_rect.size(), Sense::click());

                let mut image_center = None;
                if let Some(image_promise) = self.image.as_ref() {
                    if let Some(image_res) = image_promise.ready() {
                        if let Ok(image) = image_res {
                            let mut mesh = Mesh::with_texture(image.texture_id(ctx));
                            let mesh_size = options.scaled_image_size(image.size_vec2().into()).into();
                            let mut mesh_pos = screen_rect.center() - (mesh_size / 2.);
                            mesh_pos += self.view_offset.into();
                            mesh.add_rect_with_uv(
                                Rect::from_min_size(mesh_pos, mesh_size),
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                Color32::WHITE,
                            );

                            image_center = Some(mesh_pos + (mesh_size / 2.));
                            ui.painter().add(mesh);
                        }
                    }
                }

                if ctx.input().pointer.primary_down() {
                    let delta = ctx.input().pointer.delta();
                    self.view_offset[0] += delta.x;
                    self.view_offset[1] += delta.y;
                }

                for event in &ctx.input().events {
                    if let Event::Scroll(scroll_delta) = event {
                        if let Some(image_center) = image_center {
                            if let Some(hover_pos) = ui.ctx().input().pointer.hover_pos() {
                                let zoom_factor = 1. / 300.;
                                let new_view_zoom = (self.view_zoom + scroll_delta.y * zoom_factor).max(0.1);
                                let delta_zoom = new_view_zoom - self.view_zoom;
                                let zoom_center = if scroll_delta.y > 0. { hover_pos } else { screen_rect.center() };
                                let zoom_offset = ((zoom_center - image_center) / new_view_zoom) * (delta_zoom);
                                self.view_offset = (Vec2::from(self.view_offset) - zoom_offset).into();
                                self.view_zoom = new_view_zoom;
                            }
                        }
                    }
                }

                if area_response.double_clicked() {
                    self.is_fullscreen = false;
                }
            });
        } else {
            let mut options = RenderLoadingImageOptions::default();
            options.shrink_to_image = true;
            options.desired_image_size = [500., 500.];
            let image_response = ui::render_loading_image(ui, ctx, self.get_image(), options);
            if let Some(image_response) = image_response {
                if image_response.double_clicked() {
                    self.view_zoom = 1.;
                    self.view_offset = [0., 0.];
                    self.is_fullscreen = true;
                }
            };
        }
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

    }

    fn process_gallery_entries(&mut self) {
        // FIXME: Make the buffering here better. right now it can still fail on some entries
        // if an item takes >5 sec to load. you should make a data::load_thumbnails that results in one
        // db access at a time, instead of how it is rn where #data accesses is limited by count
        // of buffer
        if let Some(gallery_entries) = self.gallery_entries.as_ref() {
            for gallery_entry in gallery_entries.iter() {
                if !gallery_entry.borrow().is_loaded() {
                    let _ = self.load_buffer.try_add_entry(Rc::clone(gallery_entry));
                }
            }
            self.load_buffer.poll();
        } else {
            if self.loading_gallery_entries.is_none() {
                println!("initial load");
                self.load_gallery_entries();
            }
        }

        let mut do_take = false;
        if let Some(loaded_gallery_entries_promise) = self.loading_gallery_entries.as_ref() {
            if let Some(_loaded_gallery_entries_res) = loaded_gallery_entries_promise.ready() {
                do_take = true;
            }
        }
        if do_take {
            if let Some(loaded_gallery_entries_promise) = self.loading_gallery_entries.take() {
                if let Ok(loaded_gallery_entries_res) = loaded_gallery_entries_promise.try_take() {
                    match loaded_gallery_entries_res {
                        Ok(loaded_gallery_entries) => {
                            self.gallery_entries = Some(
                                loaded_gallery_entries
                                    .into_iter()
                                    .map(|gallery_entry| Rc::new(RefCell::new(gallery_entry)))
                                    .collect::<Vec<_>>(),
                            );
                        }
                        Err(error) => {
                            ui::toast_error(&mut self.toasts, format!("failed to load items due to {}", error));
                        }
                    }
                }
            }
        }
    }

    fn load_gallery_entries(&mut self) {
        println!("loading entries");
        let search_string = self.search_string.clone();
        self.loading_gallery_entries = Some(Promise::spawn_thread("", move || {
            let gallery_entries = load_gallery_entries(&search_string);
            match gallery_entries {
                Ok(gallery_entries) => Ok(gallery_entries),
                Err(error) => Err(error),
            }
        }));

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
                        if let Some(gallery_entries) = self.gallery_entries.as_mut() {
                            for gallery_entry in gallery_entries.iter_mut() {
                                let status_label = gallery_entry.borrow().get_status_label().map(|label| label.into());
                                let mut options = RenderLoadingImageOptions::default();
                                options.hover_text_on_none_image = Some("(loading bytes for thumbnail...)".into());
                                options.hover_text_on_loading_image = Some("(loading thumbnail...)".into());
                                options.hover_text = status_label;
                                options.desired_image_size = [thumbnail_size, thumbnail_size];
                                let response = ui::render_loading_image(ui, ctx, gallery_entry.borrow().thumbnail.as_ref(), options);
                                if let Some(response) = response {
                                    if response.clicked() {
                                        if let EntryId::MediaEntry(hash) = &gallery_entry.borrow().entry_id {
                                            GalleryUI::launch_preview(hash.clone(), &mut self.preview_windows)
                                        }
                                    }
                                }
                            }
                        } else {
                            ui.label("loading...");
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
                    let response = ui.add(autocomplete);
                    if response.has_focus() {
                        if ui.ctx().input().key_pressed(Key::Tab)
                            || ui.ctx().input().key_pressed(Key::Space)
                            || ui.ctx().input().key_pressed(Key::Backspace)
                        {
                            self.load_gallery_entries();
                        }
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

    fn buffer_add(gallery_entry: &Rc<RefCell<GalleryEntry>>) {
        if gallery_entry.borrow().thumbnail.is_none() {
            gallery_entry.borrow_mut().load_thumbnail();
        }
    }

    fn load_buffer_poll(gallery_entry: &Rc<RefCell<GalleryEntry>>) -> bool {
        gallery_entry.borrow().is_loading()
    }
}

impl Default for GalleryUI {
    fn default() -> Self {
        let load_buffer = PollBuffer::<GalleryEntry>::new(None, Some(10), Some(GalleryUI::buffer_add), Some(GalleryUI::load_buffer_poll), None);
        Self {
            preview_windows: vec![],
            search_string: String::new(),
            loading_gallery_entries: None,
            toasts: egui_notify::Toasts::default().with_anchor(egui_notify::Anchor::BottomLeft),
            load_buffer,
            gallery_entries: None,
            search_options: None,
        }
    }
}

impl ui::UserInterface for GalleryUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.process_previews();
        self.process_gallery_entries();
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

