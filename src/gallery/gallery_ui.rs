use crate::autocomplete::AutocompleteOption;
use crate::data::EntryId;
use crate::data::EntryInfo;
use crate::tags::tags;
use crate::tags::tags::TagData;
use crate::tags::tags_ui::TagsUI;
use arboard::Clipboard;
use chrono::TimeZone;
use chrono::Utc;
use downcast_rs::Downcast;
use eframe::egui::Key;
use eframe::egui::Layout;
use eframe::egui::RichText;
use eframe::egui::Window;
use egui::pos2;
use egui::vec2;
use egui::Align2;
use egui::Area;
use egui::Color32;
use egui::Event;
use egui::FontId;
use egui::Frame;
use egui::Grid;
use egui::Mesh;
use egui::Order;
use egui::Painter;
use egui::PointerButton;
use egui::Pos2;
use egui::Rect;
use egui::Response;
use egui::Rounding;
use egui::Sense;
use egui::Stroke;
use egui_extras::Size;
use egui_extras::StripBuilder;
use image::FlatSamples;
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
use crate::tags::tags::TagDataRef;
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
use std::any::Any;
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
    pub tag_data: TagDataRef,
    pub gallery_entries: Option<Vec<Rc<RefCell<GalleryEntry>>>>,
    pub filtered_gallery_entries: Option<Vec<Rc<RefCell<GalleryEntry>>>>,
    pub preview_windows: Vec<ui::WindowContainer>,
    pub search_options: Option<Vec<autocomplete::AutocompleteOption>>,
    pub search_string: String,
}

pub enum PreviewStatus {
    None,
    Closed,
    Deleted(EntryId),
    Updated,
}

pub struct PreviewUI {
    pub tag_data: TagDataRef,
    pub toasts: egui_notify::Toasts,
    pub preview: Option<Preview>,
    pub entry_info: Option<Promise<Result<EntryInfo>>>,
    pub tag_edit_buffer: String,
    pub id: String,
    pub is_fullscreen: bool,
    pub ignore_fullscreen_edit: i32,

    pub view_offset: [f32; 2],
    pub view_zoom: f32,

    current_dragged_index: Option<usize>,
    current_drop_index: Option<usize>,

    pub status: PreviewStatus,
    pub is_editing_tags: bool,
    pub short_id: String,
    pub clipboard_image: Option<Promise<Result<FlatSamples<Vec<u8>>>>>,
    is_reordering: bool,
    original_order: Option<Vec<String>>,
}

pub enum Preview {
    MediaEntry(Promise<Result<RetainedImage>>),
    PoolEntry((Vec<(String, Promise<Result<RetainedImage>>)>, usize)),
}

impl ui::UserInterface for PreviewUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.process_preview();
        egui::Grid::new(format!("preview_ui_{}", self.id))
            .num_columns(3)
            .min_col_width(120.)
            .show(ui, |ui| {
                self.render_options(ui, ctx);
                self.render_info(ui, ctx);
                self.render_image(ui, ctx);
            });
        self.toasts.show(ctx);
    }
}

impl PreviewUI {
    pub fn new(id: String, all_tag_data: TagDataRef) -> Box<Self> {
        let mut short_id = id.clone();
        short_id.truncate(6);
        Box::new(PreviewUI {
            toasts: egui_notify::Toasts::default().with_anchor(egui_notify::Anchor::BottomLeft),
            preview: None,
            id,
            current_dragged_index: None,
            current_drop_index: None,
            is_reordering: false,
            tag_data: all_tag_data,
            is_fullscreen: false,
            view_offset: [0., 0.],
            view_zoom: 0.5,
            ignore_fullscreen_edit: 0,
            short_id,
            entry_info: None,
            is_editing_tags: false,
            status: PreviewStatus::None,
            tag_edit_buffer: "".to_string(),
            original_order: None,
            clipboard_image: None,
        })
    }

    fn process_preview(&mut self) {
        if self.preview.is_none() {
            self.load_preview()
        }
    }

    pub fn render_options(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let delete_modal = Modal::new(ctx, format!("delete_{}_modal", self.id));
        delete_modal.show(|ui| {
            delete_modal.frame(ui, |ui| {
                delete_modal.body(ui, format!("are you sure you want to delete {}?\n\n this cannot be undone.", self.id));
            });
            delete_modal.buttons(ui, |ui| {
                delete_modal.button(ui, "cancel");
                if delete_modal.caution_button(ui, format!("delete")).clicked() {
                    // delete logic below
                    if let Some(entry_info_promise) = &self.entry_info {
                        if let Some(Ok(entry_info)) = entry_info_promise.ready() {
                            let entry_id = entry_info.entry_id();
                            if let Err(e) = data::delete_entry(&entry_info.entry_id()) {
                                ui::toast_error(&mut self.toasts, format!("failed to delete {}: {}", self.id, e));
                            } else {
                                ui::toast_success(&mut self.toasts, format!("successfully deleted {}", self.id));
                                self.status = PreviewStatus::Deleted(entry_id.clone());
                            }
                        }
                    }
                }
            })
        });

        let mut reset_clipboard_image = false;
        if let Some(clipboard_image_promise) = self.clipboard_image.as_ref() {
            match clipboard_image_promise.ready() {
                Some(Ok(flat_samples)) => {
                    reset_clipboard_image = true;
                    match Clipboard::new() {
                        Ok(mut clipboard) => {
                            let image_data = arboard::ImageData {
                                bytes: flat_samples.as_slice().into(),
                                width: flat_samples.extents().1,
                                height: flat_samples.extents().2,
                            };
                            if let Err(e) = clipboard.set_image(image_data) {
                                ui::toast_error(&mut self.toasts, format!("failed to set clipboard contents: {e}"));
                            } else {
                                ui::toast_info(&mut self.toasts, "copied image to clipboard");
                            }
                        }
                        Err(e) => {
                            ui::toast_error(&mut self.toasts, format!("failed to access system clipboard: {e}"));
                        }
                    }
                }
                Some(Err(e)) => {
                    reset_clipboard_image = true;
                    ui::toast_error(&mut self.toasts, format!("failed to load image to clipboard: {e}"));
                }
                _ => (),
            }
        }

        if reset_clipboard_image {
            self.clipboard_image = None;
        }

        ui.add_space(ui::constants::SPACER_SIZE);
        ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
            ui.label("options");
            let mut self_updated = false;
            if let Some(entry_info_promise) = self.entry_info.as_mut() {
                if let Some(entry_info_res) = entry_info_promise.ready_mut() {
                    if let Ok(entry_info) = entry_info_res {
                        ui.add_enabled_ui(!(self.is_editing_tags || self.is_reordering), |ui| {
                            if ui
                                .button(if !entry_info.details().is_bookmarked {
                                    ui::icon_text("bookmark", ui::constants::BOOKMARK_ICON)
                                } else {
                                    ui::icon_text("unbookmark", ui::constants::REMOVE_ICON)
                                })
                                .clicked()
                            {
                                let new_state = !entry_info.details().is_bookmarked;
                                if let Err(e) = data::set_bookmark(&entry_info.entry_id(), new_state) {
                                    ui::toast_error(&mut self.toasts, format!("failed to set bookmarked {new_state}: {e}"));
                                } else {
                                    entry_info.details_mut().is_bookmarked = new_state;
                                    self_updated = true;
                                }
                            }
                            let star_response = ui::star_rating(ui, &mut entry_info.details_mut().score, Config::global().media.max_score);
                            if star_response.changed() {
                                let new_value = entry_info.details().score;
                                if let Err(e) = data::set_score(&entry_info.entry_id(), new_value) {
                                    ui::toast_error(&mut self.toasts, format!("failed to set score={new_value}: {e}"));
                                } else {
                                    entry_info.details_mut().score = new_value;
                                    self_updated = true;
                                }
                            }
                        });
                    }
                } else {
                    ui.spinner();
                }
            }
            ui.add_space(ui::constants::SPACER_SIZE);
            if self.is_editing_tags {
                if ui
                    .add(ui::suggested_button(ui::icon_text("save changes", ui::constants::SAVE_ICON)))
                    .clicked()
                {
                    self.is_editing_tags = false;
                    if let Some(entry_info_promise) = self.entry_info.as_ref() {
                        if let Some(Ok(entry_info)) = entry_info_promise.ready() {
                            let mut tagstrings = self.tag_edit_buffer.split_whitespace().map(|x| x.to_string()).collect::<Vec<_>>(); //.collect::<Vec<_>>();
                            tagstrings.sort();
                            tagstrings.dedup();
                            let tags = tagstrings.iter().map(|tagstring| Tag::from_tagstring(tagstring)).collect::<Vec<Tag>>();
                            if let Ok(tags) = data::set_tags(&entry_info.details().id, &tags) {
                                ui::toast_success(
                                    &mut self.toasts,
                                    format!("successfully set {} tag{}", tags.len(), if tags.len() == 1 { "" } else { "s" }),
                                );
                                self_updated = true;
                            };
                            self.load_entry_info(&entry_info.entry_id().clone());
                        }
                    }
                }
                if ui.button(ui::icon_text("discard changes", ui::constants::REMOVE_ICON)).clicked() {
                    self.is_editing_tags = false;
                }
            } else {
                ui.add_enabled_ui(!(self.is_editing_tags || self.is_reordering), |ui| {
                    if let Some(entry_info_promise) = self.entry_info.as_ref() {
                        if let Some(Ok(entry_info)) = entry_info_promise.ready() {
                            if ui.button(format!("{} edit tags", ui::constants::EDIT_ICON)).clicked() {
                                self.tag_edit_buffer = entry_info
                                    .details()
                                    .tags
                                    .iter()
                                    .map(|tag| tag.to_tagstring())
                                    .collect::<Vec<String>>()
                                    .join("\n");
                                self.is_editing_tags = true;
                            };
                        }
                    }
                });
            }
            if let Some(entry_info_promise) = self.entry_info.as_mut() {
                if let Some(Ok(EntryInfo::PoolEntry(pool_info))) = entry_info_promise.ready_mut() {
                    if self.is_reordering {
                        if ui
                            .add(ui::suggested_button(ui::icon_text("save changes", ui::constants::SAVE_ICON)))
                            .clicked()
                        {
                            self.is_reordering = false;
                            if let Some(current_order) = Self::get_current_order(&self.preview) {
                                if let EntryId::PoolEntry(link_id) = &pool_info.details.id {
                                    let _ = data::delete_cached_thumbnail(&pool_info.details.id);
                                    if let Err(e) = data::set_media_link_values_in_order(link_id, current_order) {
                                        ui::toast_error(&mut self.toasts, format!("failed to reorder link: {e}"));
                                    } else {
                                        ui::toast_success(&mut self.toasts, format!("successfully reordered link {link_id}"));
                                        self_updated = true;
                                    };
                                }
                            }
                        }
                        if ui.button(ui::icon_text("discard changes", ui::constants::REMOVE_ICON)).clicked() {
                            self.is_reordering = false;
                            if let Some(Preview::PoolEntry((mut images, current_index))) = self.preview.take() {
                                if let Some(original_order) = self.original_order.take() {
                                    let old_images = original_order
                                        .iter()
                                        .filter_map(|hash| {
                                            images
                                                .iter()
                                                .position(|(other_hash, image_promise)| other_hash == hash)
                                                .map(|index| images.remove(index))
                                        })
                                        .collect::<Vec<_>>();
                                    self.preview = Some(Preview::PoolEntry((old_images, 0)));
                                }
                            }
                        }
                    } else {
                        ui.add_enabled_ui(!(self.is_editing_tags || self.is_reordering), |ui| {
                            if ui.button(ui::icon_text("reorder items", ui::constants::REORDER_ICON)).clicked() {
                                self.is_reordering = true;
                                if let Some(Preview::PoolEntry(images)) = self.preview.as_ref() {}

                                self.original_order = Self::get_current_order(&self.preview)
                            }
                        });
                    }
                }
            }
            ui.add_space(ui::constants::SPACER_SIZE);
            ui.add_enabled_ui(!(self.is_editing_tags || self.is_reordering), |ui| {
                if let Some(entry_info_promise) = self.entry_info.as_ref() {
                    if let Some(Ok(media_info)) = entry_info_promise.ready() {
                        ui.menu_button(format!("{} export", ui::constants::EXPORT_ICON), |ui| {
                            if let EntryId::MediaEntry(hash) = media_info.entry_id().clone() {
                                if ui.button(ui::icon_text("to clipboard (image)", ui::constants::COPY_ICON)).clicked() {
                                    self.clipboard_image = Some(Promise::spawn_thread("load_image_clipboard", move || {
                                        let load = || -> Result<FlatSamples<Vec<u8>>> {
                                            let bytes = data::load_bytes(&hash)?;
                                            let image = image::load_from_memory(&bytes)?;
                                            let rgba_image = image.to_rgba8();
                                            let flat_samples = rgba_image.into_flat_samples();
                                            Ok(flat_samples)
                                        };
                                        load()
                                    }))
                                }
                            }
                            // }
                            if ui.button(ui::icon_text("to file", ui::constants::EXPORT_ICON)).clicked() {}
                        });
                    }
                }
                if ui.button(format!("{} find source", ui::constants::SEARCH_ICON)).clicked() {}
                ui.add_space(ui::constants::SPACER_SIZE);
                if ui.add(ui::caution_button(format!("{} delete", ui::constants::DELETE_ICON))).clicked() {
                    delete_modal.open();
                }
            });

            if self_updated {
                self.status = PreviewStatus::Updated;
            }
        });
        ui.add_space(ui::constants::SPACER_SIZE);
    }

    #[inline]
    pub fn get_current_order(preview: &Option<Preview>) -> Option<Vec<String>> {
        preview.as_ref().and_then(|images| {
            if let Preview::PoolEntry((images, current_index)) = &images {
                Some(images.iter().map(|(hash, image_promise)| hash.clone()).collect())
            } else {
                None
            }
        })
    }

    pub fn render_info(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.label("info");
            if let Some(entry_info_promise) = &self.entry_info {
                if let Some(Ok(entry_info)) = entry_info_promise.ready() {
                    egui::Grid::new(format!("info_{}", self.id)).num_columns(2).show(ui, |ui| {
                        let datetime = Utc.timestamp(entry_info.details().date_registered, 0);
                        ui.label("registered");
                        ui.label(datetime.format("%B %e, %Y @%l:%M%P").to_string());
                        ui.end_row();

                        ui.label("size");
                        ui.label(ui::readable_byte_size(entry_info.details().size, 2, ui::NumericBase::Two).to_string());
                        ui.end_row();

                        if let EntryInfo::MediaEntry(media_info) = &entry_info {
                            ui.label("type");
                            ui.label(&media_info.mime);
                            ui.end_row();
                        }
                    });
                    ui.separator();
                    ui.label("tags");
                    let tags = &entry_info.details().tags;
                    if self.is_editing_tags {
                        ui.text_edit_multiline(&mut self.tag_edit_buffer);
                    } else {
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
                                    let mut current_tag_data = None;
                                    let exists_in_tag_data = if let Some(Ok(tag_data)) = tags::unpack_tag_data(&self.tag_data) {
                                        tag_data.iter().any(|tag_data| {
                                            if tag_data.tag == *tag {
                                                current_tag_data = Some(tag_data.clone());
                                                true
                                            } else {
                                                false
                                            }
                                        })
                                    } else {
                                        true
                                    };
                                    ui.with_layout(egui::Layout::left_to_right(Align::Center).with_main_justify(true), |ui| {
                                        let mut tag_text = tag.to_rich_text();
                                        if !exists_in_tag_data {
                                            tag_text = tag_text.strikethrough();
                                        }
                                        let response = ui.label(tag_text);
                                        if !exists_in_tag_data {
                                            response.on_hover_text_at_pointer("(unknown tag)");
                                        } else if current_tag_data.is_none() {
                                            response.on_hover_text_at_pointer("(loading...)");
                                        } else {
                                            let hover_text = RichText::new(format!(
                                                "{} ({})",
                                                tag.to_tagstring(),
                                                current_tag_data
                                                    .map(|tag_data| tag_data.occurances.to_string())
                                                    .unwrap_or(String::from("?"))
                                            ))
                                            .color(tag.namespace_color().unwrap_or(ui::constants::DEFAULT_TEXT_COLOR));
                                            response.on_hover_text_at_pointer(hover_text);
                                        }
                                    });
                                    ui.end_row();
                                }
                            });
                        }
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
                fn paint_text(text: impl Into<String>, pos: Pos2, painter: &Painter) -> Rect {
                    let galley = painter.layout_no_wrap(text.into(), FontId::default(), ui::constants::DEFAULT_TEXT_COLOR);
                    let offset = vec2(-galley.rect.width() / 2., -galley.rect.height() / 2.);
                    let text_pos = pos + offset;
                    let mut painted_rect = galley.rect.clone();
                    painter.galley(text_pos, galley.clone());
                    painted_rect.set_center(text_pos + vec2(painted_rect.width() / 2., painted_rect.height() / 2.));
                    painted_rect
                }
                let mut options = RenderLoadingImageOptions::default();
                options.desired_image_size = (screen_rect.size() * self.view_zoom).into();
                let area_response = ui.allocate_response(screen_rect.size(), Sense::click());
                let mut image_size = None;
                let mut image_center = None;
                if let Some(preview) = self.preview.as_ref() {
                    let image = match preview {
                        Preview::MediaEntry(image_promise) => {
                            if let Some(Ok(image)) = image_promise.ready() {
                                Some(image)
                            } else {
                                None
                            }
                        }
                        Preview::PoolEntry((images, current_index)) => {
                            if let Some((hash, image_promise)) = images.get(*current_index) {
                                if let Some(Ok(image)) = image_promise.ready() {
                                    Some(image)
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                    };

                    if let Some(image) = image {
                        let mut mesh = Mesh::with_texture(image.texture_id(ctx));
                        let mesh_size = options.scaled_image_size(image.size_vec2().into()).into();
                        let mut mesh_pos = screen_rect.center() - (mesh_size / 2.);
                        mesh_pos += self.view_offset.into();
                        mesh.add_rect_with_uv(
                            Rect::from_min_size(mesh_pos, mesh_size),
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                        image_size = Some(mesh_size);
                        image_center = Some(mesh_pos + (mesh_size / 2.));
                        ui.painter().add(mesh);
                    } else {
                        paint_text("loading...", screen_rect.center(), ui.painter());
                    }
                } else {
                    paint_text("waiting to load", screen_rect.center(), ui.painter());
                }
                let MAX_IGNORE_FULLSCREEN_EDIT = 20; // number of frames to ignore double click
                let mut was_rect_clicked = |rect: &Rect| -> bool {
                    if ui.rect_contains_pointer(*rect) && ctx.input().pointer.primary_released() {
                        self.ignore_fullscreen_edit = MAX_IGNORE_FULLSCREEN_EDIT;
                        true
                    } else {
                        false
                    }
                };
                let control_rect_scale_y = 0.1;

                if let Some(Preview::PoolEntry((images, current_index))) = self.preview.as_mut() {
                    let rect_scale_x = 0.4;
                    let rect_scale_y = 1. - (control_rect_scale_y * 2.);
                    let rect_size = vec2(rect_scale_x * screen_rect.width(), rect_scale_y * screen_rect.height());
                    let rect_y_offset = (screen_rect.height() - rect_size.y) * 0.5;
                    let left_rect_pos = pos2(0., rect_y_offset);
                    let right_rect_pos = pos2(screen_rect.width() - rect_size.x, rect_y_offset);
                    let left_rect = Rect::from_min_max(left_rect_pos, left_rect_pos + rect_size);
                    let right_rect = Rect::from_min_max(right_rect_pos, right_rect_pos + rect_size);

                    if was_rect_clicked(&left_rect) {
                        *current_index = (*current_index as i32 - 1).max(0).min(images.len() as i32 - 1) as usize
                    } else if was_rect_clicked(&right_rect) {
                        *current_index = (*current_index as i32 + 1).max(0).min(images.len() as i32 - 1) as usize
                    }

                    let bottom_rect_size = vec2(screen_rect.width(), control_rect_scale_y * screen_rect.height());
                    let bottom_rect_pos = pos2(0., (1. - control_rect_scale_y) * screen_rect.height());
                    let bottom_rect = Rect::from_min_max(bottom_rect_pos, bottom_rect_pos + bottom_rect_size);

                    if ui.rect_contains_pointer(bottom_rect) {
                        let inner_bottom_rect = Rect::from_center_size(bottom_rect.center(), vec2(20., 20.));
                        let text_rect = paint_text(format!("{} / {}", *current_index + 1, images.len()), bottom_rect.center(), ui.painter());
                        ui.painter().rect_filled(text_rect.expand(10.), Rounding::none(), Color32::BLACK);
                        paint_text(format!("{} / {}", *current_index + 1, images.len()), bottom_rect.center(), ui.painter());
                    }
                }

                if ctx.input().pointer.primary_down() {
                    if let Some(image_size) = image_size {
                        let delta = ctx.input().pointer.delta();
                        self.view_offset = (Vec2::from(self.view_offset) + delta).into();
                        // {view_offset_bound_factor} amount of the image allowed to clip offscreeen
                        let view_offset_bound_factor = 0.9;
                        let view_offset_x_bound = (screen_rect.width() - (1. - 2. * view_offset_bound_factor) * (image_size.x)) / 2.;
                        let view_offset_y_bound = (screen_rect.height() - (1. - 2. * view_offset_bound_factor) * (image_size.y)) / 2.;
                        self.view_offset[0] = self.view_offset[0].max(-view_offset_x_bound).min(view_offset_x_bound);
                        self.view_offset[1] = self.view_offset[1].max(-view_offset_y_bound).min(view_offset_y_bound);
                    }
                }

                for event in &ctx.input().events {
                    match event {
                        Event::Scroll(scroll_delta) => {
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
                        // despite this being called "zoom" im using the above event for zoom, and this (ctrl+scroll)
                        // for page flipping'
                        Event::Zoom(factor) => {
                            if let Some(Preview::PoolEntry((images, current_index))) = self.preview.as_mut() {
                                *current_index = (*current_index as i32 + if *factor < 1. { -1 } else { 1 })
                                    .max(0)
                                    .min(images.len() as i32 - 1) as usize
                            }
                        }
                        _ => (),
                    }
                }
                self.ignore_fullscreen_edit = (self.ignore_fullscreen_edit - 1).max(0);
                if area_response.double_clicked() && self.ignore_fullscreen_edit == 0 {
                    self.is_fullscreen = false;
                }
            });
        } else {
            let mut options = RenderLoadingImageOptions::default();
            options.shrink_to_image = true;
            options.desired_image_size = [500.; 2];
            options.hover_text_on_error_image = Some(Box::new(|error| format!("failed to load image: {error}").into()));

            if !ctx.memory().is_anything_being_dragged() {
                if let Some(current_dragged_index) = self.current_dragged_index {
                    if let Some(current_drop_index) = self.current_drop_index {
                        match self.preview.as_mut() {
                            Some(Preview::PoolEntry((images, _))) => {
                                let image_promise = images.remove(current_dragged_index);
                                images.insert(current_drop_index, image_promise)
                            }
                            _ => (),
                        }
                    }
                }

                self.current_dragged_index = None;
                self.current_drop_index = None;
            }

            let image_response = match self.preview.as_mut() {
                Some(Preview::MediaEntry(image_promise)) => ui::render_loading_image(ui, ctx, Some(image_promise), &options),
                Some(Preview::PoolEntry((images_promise, current_view_index))) => {
                    options.desired_image_size = [200.; 2];
                    options.sense.push(Sense::drag());
                    ScrollArea::vertical()
                        .min_scrolled_height(500.)
                        .id_source(format!("{}_pool_scroll", self.id))
                        .show(ui, |ui| {
                            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                                let grid = Grid::new(format!("{}_pool_grid", self.id)).show(ui, |ui| {
                                    for (image_index, (hash, image_promise)) in images_promise.iter().enumerate() {
                                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                            if self.current_dragged_index.is_some() && image_index == 0 {
                                                if let Some(current_drop_index) = self.current_drop_index.as_ref() {
                                                    if *current_drop_index == 0 && self.current_dragged_index.unwrap() != 0 {
                                                        ui.separator();
                                                    }
                                                }
                                            }

                                            if let Some(current_drag_index) = self.current_dragged_index.as_ref() {
                                                if *current_drag_index == image_index {
                                                    options.image_tint = Some(ui::constants::IMPORT_IMAGE_UNLOADED_TINT);
                                                }
                                            }

                                            let image_response = ui::render_loading_image(ui, ctx, Some(image_promise), &options);
                                            options.image_tint = None;
                                            if let Some(image_response) = image_response {
                                                if image_response.dragged() && self.is_reordering {
                                                    if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
                                                        let mut options = RenderLoadingImageOptions::default();
                                                        options.desired_image_size = [600.; 2];
                                                        options.shrink_to_image = true;
                                                        egui::Area::new("dragged_item")
                                                            .interactable(false)
                                                            .fixed_pos(pointer_pos)
                                                            .order(Order::Foreground)
                                                            .show(ctx, |ui| ui::render_loading_image(ui, ctx, Some(image_promise), &options));
                                                    }
                                                    if self.current_dragged_index.is_none() {
                                                        self.current_dragged_index = Some(image_index)
                                                    }
                                                }
                                                if image_response.double_clicked() {
                                                    *current_view_index = image_index;
                                                    self.view_zoom = 1.;
                                                    self.view_offset = [0., 0.];
                                                    self.is_fullscreen = true;
                                                }
                                                if let Some(current_dragged_index) = self.current_dragged_index.as_ref() {
                                                    if ui.rect_contains_pointer(image_response.rect) {
                                                        if (*current_dragged_index == image_index)
                                                            || (((*current_dragged_index as i32) - 1).max(0) as usize == image_index)
                                                        {
                                                            self.current_drop_index = None;
                                                        } else {
                                                            self.current_drop_index = Some(image_index.max(1));
                                                            ui.separator();
                                                        }
                                                    }
                                                };
                                            };
                                        });

                                        if (image_index + 1) % 4 == 0 {
                                            ui.end_row()
                                        }
                                    }
                                });
                                if self.current_dragged_index.is_some() && !ui.rect_contains_pointer(grid.response.rect) {
                                    self.current_drop_index = Some(0);
                                }
                            });
                        });
                    None
                }
                _ => None,
            };
            if let Some(image_response) = image_response {
                if image_response.double_clicked() {
                    self.view_zoom = 1.;
                    self.view_offset = [0., 0.];
                    self.is_fullscreen = true;
                }
            };
        }
    }

    pub fn load_preview(&mut self) {
        let load = |hash: &String| -> Result<RetainedImage> {
            let bytes = data::load_bytes(hash);
            let bytes = bytes?;
            let dynamic_image = image::load_from_memory(&bytes)?;
            let retained_image = ui::generate_retained_image(&dynamic_image.to_rgba8())?;
            Ok(retained_image)
        };

        if let Some(entry_info_promise) = self.entry_info.as_ref() {
            if let Some(Ok(entry_info)) = entry_info_promise.ready() {
                match entry_info.entry_id() {
                    EntryId::MediaEntry(hash) => {
                        let hash = hash.clone();
                        self.preview = Some(Preview::MediaEntry(Promise::spawn_thread("load_media_image_preview", move || {
                            load(&hash)
                        })));
                    }
                    EntryId::PoolEntry(link_id) => {
                        if let Ok(hashes) = data::get_hashes_of_media_link(*link_id) {
                            self.preview = Some(Preview::PoolEntry((
                                hashes
                                    .into_iter()
                                    .map(|hash| (hash.clone(), Promise::spawn_thread("load_pool_image_previews", move || load(&hash))))
                                    .collect::<Vec<_>>(),
                                0,
                            )));
                        }
                    }
                }
            }
        }
    }

    // pub fn get_image(&mut self) -> Option<&Promise<Result<RetainedImage>>> {
    //     match &self.image {
    //         None => match self.entry_info.as_ref().unwrap().ready() {
    //             None => None,
    //             Some(result) => {
    //                 let (sender, promise) = Promise::new();
    //                 match result {
    //                     Err(error) => sender.send(Err(anyhow::Error::msg(format!("failed to load mediainfo: {error}")))),
    //                     Ok(entry_info) => {
    //                         let hash = entry_info.details().id.as_media_entry_id().unwrap().clone();
    //                         thread::spawn(move || {
    //                             let bytes = data::load_bytes(&hash);
    //                             let load = || -> Result<RetainedImage> {
    //                                 let bytes = bytes?;
    //                                 let dynamic_image = image::load_from_memory(&bytes)?;
    //                                 let retained_image = ui::generate_retained_image(&dynamic_image.to_rgba8())?;
    //                                 Ok(retained_image)
    //                             };
    //                             sender.send(load())
    //                         });
    //                     }
    //                 }
    //                 self.image = Some(promise);
    //                 self.image.as_ref()
    //             }
    //         },
    //         Some(_promise) => self.image.as_ref(),
    //     }
    // }
    pub fn load_entry_info(&mut self, entry_id: &EntryId) {
        let entry_id = entry_id.clone();
        self.entry_info = Some(Promise::spawn_thread("load_entry_info_preview", move || data::load_entry_info(&entry_id)))
    }
    // pub load_image
}

impl GalleryUI {
    fn process_previews(&mut self) {
        if self.tag_data.is_none() {
            tags::load_tag_data(&mut self.tag_data)
        }
        if self.search_options.is_none() {
            if let Some(Ok(tag_data)) = tags::unpack_tag_data(&self.tag_data) {
                self.search_options = Some(
                    tag_data
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
        let mut do_refresh = false;
        let mut do_refiter = false;
        self.preview_windows.retain_mut(|window_container| {
            if let Some(preview_ui) = window_container.window.downcast_mut::<PreviewUI>() {
                if preview_ui.tag_data.is_none() && self.tag_data.is_some() {
                    preview_ui.tag_data = tags::clone_tag_data_ref(&self.tag_data);
                }
                if matches!(preview_ui.status, PreviewStatus::Updated) {
                    preview_ui.status = PreviewStatus::None;
                    do_refresh = true;
                }
                if let PreviewStatus::Deleted(entry_id) = &preview_ui.status {
                    if let Some(gallery_entries) = self.gallery_entries.as_mut() {
                        gallery_entries.retain(|gallery_entry| gallery_entry.borrow().entry_info.entry_id() != entry_id);
                        do_refiter = true;
                    }
                }
                !(matches!(preview_ui.status, PreviewStatus::Closed) || matches!(preview_ui.status, PreviewStatus::Deleted(_)))
            } else {
                true
            }
        });

        // self.preview_windows.iter_mut().for_each(|window_container| {
        //     let preview_ui = window_container.window.downcast_mut::<PreviewUI>();
        //     if let Some(preview_ui) = preview_ui {
        //         if preview_ui.tag_data.is_none() && self.tag_data.is_some() {
        //             preview_ui.tag_data = tags::clone_tag_data_ref(&self.tag_data)
        //         }
        //         if matches!(preview_ui.status, PreviewStatus::Updated) {
        //             do_refresh = true;
        //         }
        //     }
        // });
        if do_refiter {
            self.filter_entries();
        }
        if do_refresh {
            self.refresh()
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
        }
        //     if self.loading_gallery_entries.is_none() {
        // self.load_gallery_entries();
        //     }
        // }

        if self.gallery_entries.is_some() && self.filtered_gallery_entries.is_none() {
            self.filter_entries();
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
                            self.filter_entries()
                        }
                        Err(error) => {
                            ui::toast_error(&mut self.toasts, format!("failed to load items: {}", error))
                                .set_duration(None)
                                .set_closable(false);
                        }
                    }
                }
            }
        }
    }

    pub fn load_gallery_entries(&mut self) {
        let search_string = self.search_string.clone();
        self.loading_gallery_entries = Some(Promise::spawn_thread("loading_gallery_entries", move || {
            let gallery_entries = load_gallery_entries(&search_string);
            match gallery_entries {
                Ok(gallery_entries) => Ok(gallery_entries),
                Err(error) => Err(error),
            }
        }));
    }

    fn is_loading_gallery_entries(&self) -> bool {
        if let Some(entries_promise) = self.loading_gallery_entries.as_ref() {
            entries_promise.ready().is_none()
        } else {
            false
        }
    }

    fn launch_preview(entry_id: &EntryId, preview_windows: &mut Vec<WindowContainer>, tag_data: TagDataRef) {
        let mut preview = PreviewUI::new(entry_id.to_string(), tag_data);
        let mut title = preview.short_id.clone();
        title.insert_str(0, &format!("{} ", ui::constants::GALLERY_ICON));
        if !ui::does_window_exist(&title, preview_windows) {
            preview.load_entry_info(entry_id);
            preview_windows.push(WindowContainer {
                title,
                is_open: Some(true),
                window: preview,
            })
        }
    }

    fn refresh(&mut self) {
        self.search_options = None;
        self.tag_data = None;
        self.preview_windows.iter_mut().for_each(|window_container| {
            let preview_ui = window_container.window.downcast_mut::<PreviewUI>();
            if let Some(preview_ui) = preview_ui {
                preview_ui.tag_data = None
            }
        });
        self.load_gallery_entries();
    }

    fn render_options(&mut self, ui: &mut Ui) {
        ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
            ui.label("gallery");
            ui.add_enabled_ui(!self.is_loading_gallery_entries(), |ui| {
                if ui
                    .button(if self.is_loading_gallery_entries() {
                        String::from("loading...")
                    } else {
                        format!("{} refresh", ui::constants::REFRESH_ICON)
                    })
                    .clicked()
                {
                    self.refresh();
                }
            });
        });
    }

    fn render_thumbnails(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if let Some(gallery_entries) = self.filtered_gallery_entries.as_mut() {
            ScrollArea::vertical()
                .id_source("previews_col")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let layout = egui::Layout::from_main_dir_and_cross_align(Direction::LeftToRight, Align::Center).with_main_wrap(true);
                    ui.allocate_ui(Vec2::new(ui.available_size_before_wrap().x, 0.0), |ui| {
                        ui.with_layout(layout, |ui| {
                            ui.style_mut().spacing.item_spacing = Vec2::new(0., 0.);
                            for gallery_entry in gallery_entries.iter_mut() {
                                // let passes_filter =
                                let status_label = gallery_entry.borrow().get_status_label().map(|label| label.into());
                                let mut options = RenderLoadingImageOptions::default();
                                let thumbnail_size = Config::global().ui.gallery.thumbnail_size as f32;
                                options.hover_text_on_none_image = Some("(loading bytes for thumbnail...)".into());
                                options.hover_text_on_loading_image = Some("(loading thumbnail...)".into());
                                options.hover_text = status_label;
                                options.desired_image_size = [thumbnail_size, thumbnail_size];
                                let response = ui::render_loading_image(ui, ctx, gallery_entry.borrow().thumbnail.as_ref(), &options);
                                if let Some(response) = response {
                                    if response.clicked() {
                                        GalleryUI::launch_preview(
                                            &gallery_entry.borrow().entry_info.entry_id(),
                                            &mut self.preview_windows,
                                            tags::clone_tag_data_ref(&self.tag_data),
                                        )
                                    }
                                }
                            }
                        });
                    });
                });
        } else {
            ui.with_layout(Layout::left_to_right(Align::Center).with_main_justify(true), |ui| {
                ui.spinner();
            });
        }
    }

    fn render_search(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(format!("{} search", ui::constants::SEARCH_ICON));

            ui.with_layout(Layout::top_down(Align::Center).with_cross_justify(true), |ui| {
                if let Some(search_options) = self.search_options.as_ref() {
                    let autocomplete = autocomplete::create(&mut self.search_string, search_options, true);
                    let response = ui.add(autocomplete);
                    if response.has_focus() {
                        if ui.ctx().input().key_pressed(Key::Tab)
                            || ui.ctx().input().key_pressed(Key::Space)
                            || ui.ctx().input().key_pressed(Key::Backspace)
                        {
                            self.filter_entries();
                        }
                    }
                }
            });
        });
    }

    fn render_preview_windows(&mut self, ctx: &egui::Context) {
        self.preview_windows.retain(|window| window.is_open.unwrap());
        for preview_window in self.preview_windows.iter_mut() {
            let window = egui::Window::new(&preview_window.title)
                .open(preview_window.is_open.as_mut().unwrap())
                .default_size([800.0, 400.0])
                .collapsible(false)
                .vscroll(false)
                .hscroll(false)
                .resizable(false);
            let _response = window.show(ctx, |ui| {
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

    fn filter_entries(&mut self) {
        let search_tags = self
            .search_string
            .split_whitespace()
            .map(|tagstring| Tag::from_tagstring(&tagstring.to_string()))
            .collect::<Vec<_>>();
        if !search_tags.is_empty() {
            self.filtered_gallery_entries = self.gallery_entries.as_ref().map(|gallery_entries| {
                gallery_entries
                    .iter()
                    .filter_map(|gallery_entry| {
                        if gallery_entry.borrow().entry_info.details().includes_tags_and(&search_tags) {
                            Some(Rc::clone(&gallery_entry))
                        } else {
                            None
                        }
                    })
                    .collect()
            });
        } else {
            self.filtered_gallery_entries = self
                .gallery_entries
                .as_ref()
                .map(|gallery_entries| gallery_entries.iter().map(|gallery_entry| Rc::clone(&gallery_entry)).collect())
        }
    }
}

impl Default for GalleryUI {
    fn default() -> Self {
        let load_buffer = PollBuffer::<GalleryEntry>::new(None, Some(10), Some(GalleryUI::buffer_add), Some(GalleryUI::load_buffer_poll), None);
        Self {
            tag_data: None,
            preview_windows: vec![],
            search_string: String::new(),
            loading_gallery_entries: None,
            toasts: egui_notify::Toasts::default().with_anchor(egui_notify::Anchor::BottomLeft),
            load_buffer,
            gallery_entries: None,
            filtered_gallery_entries: None,
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
            ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
                StripBuilder::new(ui)
                    .size(Size::exact(0.)) // FIXME: not sure why this is adding more space.
                    .size(Size::exact(100.))
                    .size(Size::exact(ui::constants::SPACER_SIZE))
                    .size(Size::remainder())
                    .horizontal(|mut strip| {
                        strip.empty();
                        strip.cell(|ui| {
                            self.render_options(ui);
                        });
                        strip.cell(|ui| {
                            ui.with_layout(Layout::left_to_right(Align::Center).with_cross_justify(true), |ui| {
                                ui.separator();
                            });
                        });
                        strip.cell(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    self.render_search(ui);
                                });

                                ui.add_space(ui::constants::SPACER_SIZE);
                                self.render_thumbnails(ui, ctx);
                            });
                        });
                    });
            });
        });
        self.toasts.show(ctx);
    }
}
