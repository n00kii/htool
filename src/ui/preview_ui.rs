use super::{
    icon, tags_ui::TagsUI, widgets::autocomplete, widgets::star_rating::star_rating, RenderLoadingImageOptions, ToastsRef,
};
use crate::{
    config::Config,
    data::{self, EntryId, EntryInfo},
    tags::Tag,
    ui, app::{SharedState, UpdateList},
};
use anyhow::Result;
use arboard::Clipboard;
use chrono::{TimeZone, Utc};
use egui::{
    pos2, vec2, Align, Area, Color32, Context, Event, FontId, Grid, Key, Label, Layout, Mesh, Modifiers, Order, Painter, Pos2, Rect, RichText,
    Rounding, ScrollArea, Sense, Ui, Vec2,
};
use egui_extras::RetainedImage;
use egui_modal::Modal;
use egui_video::Player;
use image::FlatSamples;
use parking_lot::Mutex;
use rfd::FileDialog;
use std::{rc::Rc, sync::Arc, thread};
// use eg;
use poll_promise::Promise;
pub struct PreviewUI {
    // pub tag_data: TagDataRef,
    // arc_toast: ToastsRef,
    shared_state: Rc<SharedState>,
    pub preview: Option<Preview>,
    pub entry_info: Arc<Mutex<EntryInfo>>,
    pub updated_entry_info: Option<Promise<Result<EntryInfo>>>,
    tag_edit_buffer: String,
    pub id: String,
    is_fullscreen: bool,
    ignore_fullscreen_edit: i32,

    view_offset: [f32; 2],
    view_zoom: f32,

    current_dragged_index: Option<usize>,
    current_drop_index: Option<usize>,

    pub status: Arc<Mutex<PreviewStatus>>,

    is_editing_tags: bool,
    clipboard_image: Option<Promise<Result<FlatSamples<Vec<u8>>>>>,

    // autocomplete_options: AutocompleteOptionsRef,
    player: Option<Player>,
    disassociated_entry_ids: Arc<Mutex<Vec<EntryId>>>,
    register_unknown_tags: bool,
    is_reordering: bool,
    original_order: Option<Vec<String>>,
    movie_loaded: bool,
}

pub enum MediaPreview {
    Picture(RetainedImage),
    Movie(Player),
}

pub enum Preview {
    MediaEntry(Promise<Result<MediaPreview>>),
    PoolEntry((Vec<(String, Promise<Result<MediaPreview>>)>, usize)),
}

#[derive(Clone)]
pub enum PreviewStatus {
    None,
    Closed,
    Next(EntryId),
    Previous(EntryId),
    Deleted(EntryId),
    DeletedWithUpdate(EntryId),
    RequestingNew(EntryId),
    Updated,
    HardUpdated,
}

impl ui::UserInterface for PreviewUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.process_keybinds(ui, ctx);
        self.process_preview(ctx);
        ui.centered_and_justified(|ui| {
            egui::Grid::new(format!("preview_ui_{}", self.id))
                .num_columns(3)
                .min_col_width(120.)
                .show(ui, |ui| {
                    self.render_options(ui, ctx);
                    self.render_info(ui, ctx);
                    self.render_preview(ui, ctx);
                });
        });
    }
}

impl PreviewUI {
    pub fn new(
        entry_info: &Arc<Mutex<EntryInfo>>,
        shared_shate: &Rc<SharedState>, // all_tag_data: &TagDataRef,
                                        // toasts: &ToastsRef,
                                        // autocomplete_options: &AutocompleteOptionsRef,
    ) -> Box<Self> {
        let id = entry_info.lock().entry_id().to_string();
        Box::new(PreviewUI {
            preview: None,
            updated_entry_info: None,
            shared_state: Rc::clone(shared_shate),
            id,
            player: None,
            // arc_toast: Arc::clone(&toasts),
            register_unknown_tags: false,
            current_dragged_index: None,
            current_drop_index: None,
            is_reordering: false,
            // tag_data: Rc::clone(all_tag_data),
            movie_loaded: false,
            is_fullscreen: false,
            view_offset: [0., 0.],
            view_zoom: 0.5,
            ignore_fullscreen_edit: 0,
            entry_info: Arc::clone(&entry_info),
            is_editing_tags: false,
            status: Arc::new(Mutex::new(PreviewStatus::None)),
            disassociated_entry_ids: Arc::new(Mutex::new(vec![])),
            tag_edit_buffer: "".to_string(),
            original_order: None,
            // autocomplete_options: Rc::clone(&autocomplete_options),
            clipboard_image: None,
        })
    }

    fn process_preview(&mut self, ctx: &egui::Context) {
        if self.preview.is_none() {
            self.load_preview(ctx)
        }
        let movie_loaded = self.preview.as_ref().map(|p| match p {
            Preview::MediaEntry(promise) => {
                matches!(promise.ready(), Some(Ok(MediaPreview::Movie(_))))
            }
            _ => false
        }).unwrap_or(false);
        if !self.movie_loaded && movie_loaded {
            self.preview = match self.preview.take() {
                Some(Preview::MediaEntry(p)) => {
                    match p.block_and_take() {
                        Ok(MediaPreview::Movie(player)) => {
                            let mut player = player.with_audio(&mut self.shared_state.audio_device.borrow_mut()).ok();
                            if let Some(player) = player.as_mut() {
                                player.start();
                            }
                            player.map(|p| Preview::MediaEntry(Promise::from_ready(Ok(MediaPreview::Movie(p)))))
                        }
                        _ => unreachable!()
                    }
                }
                _ => unreachable!()
            };
        }
        self.movie_loaded = movie_loaded;
        if let Some(mut removed_list) = self.disassociated_entry_ids.try_lock() {
            match self.entry_info.try_lock().as_deref_mut() {
                Some(EntryInfo::PoolEntry(pool_info)) => {
                    if let Some(Preview::PoolEntry((image_promises, _))) = self.preview.as_mut() {
                        let removed_list = removed_list.iter().filter_map(|e| e.as_media_entry_id()).collect::<Vec<_>>();
                        image_promises.retain(|(h, _)| !removed_list.contains(&h));
                        pool_info.hashes.retain(|h| !removed_list.contains(&h))
                    }
                }
                Some(EntryInfo::MediaEntry(media_info)) => {
                    let removed_list = removed_list.iter().filter_map(|e| e.as_pool_entry_id()).collect::<Vec<_>>();
                    media_info.links.retain(|l| !removed_list.contains(&l))
                }
                _ => (),
            }
            removed_list.clear();
        }
    }

    pub fn set_status(current_status: &Arc<Mutex<PreviewStatus>>, new_status: PreviewStatus) {
        *current_status.lock() = new_status
    }
    pub fn try_get_status(current_status: &Arc<Mutex<PreviewStatus>>) -> Option<PreviewStatus> {
        current_status.try_lock().map(|status| status.clone())
    }

    pub fn render_options(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let delete_entry_modal = self.render_delete_entry_modal(ctx, &self.entry_info);
        let delete_linked_entries_modal = self.render_delete_linked_entries_modal(ctx);

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
                                ui::toast_error_lock(&self.shared_state.toasts, format!("failed to set clipboard contents: {e}"));
                            } else {
                                ui::toast_info_lock(&self.shared_state.toasts, "copied image to clipboard");
                            }
                        }
                        Err(e) => {
                            ui::toast_error_lock(&self.shared_state.toasts, format!("failed to access system clipboard: {e}"));
                        }
                    }
                }
                Some(Err(e)) => {
                    reset_clipboard_image = true;
                    ui::toast_error_lock(&self.shared_state.toasts, format!("failed to load image to clipboard: {e}"));
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
            if let Some(mut entry_info) = self.entry_info.try_lock() {
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
                        entry_info.details_mut().is_bookmarked = new_state;

                        let entry_id = entry_info.entry_id().clone();
                        let status = Arc::clone(&self.status);
                        let toasts = Arc::clone(&self.shared_state.toasts);
                        let entry_info = Arc::clone(&self.entry_info);
                        thread::spawn(move || {
                            if let Err(e) = data::set_bookmark(&entry_id, new_state) {
                                ui::toast_error_lock(&toasts, format!("failed to set bookmarked {new_state}: {e}"));
                                entry_info.lock().details_mut().is_bookmarked = !new_state;
                            } else {
                                Self::set_status(&status, PreviewStatus::Updated)
                            }
                        });
                    }
                    let previous_score = entry_info.details_mut().score;
                    let star_response = star_rating(ui, &mut entry_info.details_mut().score, Config::global().general.entry_max_score);
                    if star_response.changed() {
                        let entry_id = entry_info.entry_id().clone();
                        let status = Arc::clone(&self.status);
                        let toasts = Arc::clone(&self.shared_state.toasts);
                        let new_score = entry_info.details().score;
                        entry_info.details_mut().score = new_score;
                        let entry_info = Arc::clone(&self.entry_info);
                        thread::spawn(move || {
                            if let Err(e) = data::set_score(&entry_id, new_score) {
                                ui::toast_error_lock(&toasts, format!("failed to set score={new_score}: {e}"));
                                entry_info.lock().details_mut().score = previous_score;
                            } else {
                                Self::set_status(&status, PreviewStatus::Updated)
                            }
                        });
                    }
                });
            } else {
                ui.spinner();
            }
            ui.add_space(ui::constants::SPACER_SIZE);
            if self.is_editing_tags {
                ui.group(|ui| {
                    ui.checkbox(&mut self.register_unknown_tags, "register unknown");
                });
                ui.add_space(ui::constants::SPACER_SIZE);
                if ui
                    .add(ui::suggested_button(ui::icon_text("save tags", ui::constants::SAVE_ICON)))
                    .clicked()
                {
                    self.is_editing_tags = false;
                    let mut tagstrings = self.tag_edit_buffer.split_whitespace().map(|x| x.to_string()).collect::<Vec<_>>(); //.collect::<Vec<_>>();
                    tagstrings.sort();
                    tagstrings.dedup();
                    let tags = tagstrings.iter().map(|tagstring| Tag::from_tagstring(tagstring)).collect::<Vec<Tag>>();
                    let status = Arc::clone(&self.status);
                    let entry_info = Arc::clone(&self.entry_info);
                    let arc_toasts = Arc::clone(&self.shared_state.toasts);
                    // let shared_state.toastss = Arc::clone(&self.arc_toast);
                    let do_register_unknown_tags = self.register_unknown_tags;
                    thread::spawn(move || {
                        let mut entry_info = entry_info.lock();
                        match data::set_tags(&entry_info.details().id, &tags) {
                            Ok(tags) => {
                                ui::toast_success_lock(
                                    &arc_toasts,
                                    format!("successfully set {} tag{}", tags.len(), if tags.len() == 1 { "" } else { "s" }),
                                );
                                // }

                                if do_register_unknown_tags {
                                    if let Ok(unknown_tags) = data::filter_to_unknown_tags(&tags) {
                                        for unknown_tag in unknown_tags {
                                            if let Err(e) = data::register_tag(&unknown_tag) {
                                                TagsUI::toast_failed_new_tag(&unknown_tag.to_tagstring(), &e, &arc_toasts)
                                            } else {
                                                TagsUI::toast_success_new_tag(&unknown_tag.to_tagstring(), &arc_toasts)
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                ui::toast_error_lock(&arc_toasts, format!("failed to set tags: {e}"));
                            }
                        }

                        if let Ok(new_info) = data::get_entry_info(&entry_info.entry_id()) {
                            *entry_info = new_info
                        }
                        Self::set_status(&status, PreviewStatus::Updated);
                    });
                }
                if ui.button(ui::icon_text("discard changes", ui::constants::REMOVE_ICON)).clicked() {
                    self.is_editing_tags = false;
                }
            } else {
                ui.add_enabled_ui(!(self.is_editing_tags || self.is_reordering), |ui| {
                    if let Some(entry_info) = self.entry_info.try_lock() {
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
                });
            }
            if let Some(entry_info) = self.entry_info.try_lock() {
                // if let Some(entry_info_promise) = self.entry_info.as_mut() {
                if let EntryInfo::PoolEntry(pool_info) = &*entry_info {
                    if self.is_reordering {
                        if ui
                            .add(ui::suggested_button(ui::icon_text("save order", ui::constants::SAVE_ICON)))
                            .clicked()
                        {
                            self.is_reordering = false;
                            if let Some(current_order) = Self::get_current_order(&self.preview) {
                                if let EntryId::PoolEntry(link_id) = &pool_info.details.id {
                                    let _ = data::delete_cached_thumbnail(&pool_info.details.id);
                                    if let Err(e) = data::set_media_link_values_in_order(link_id, current_order) {
                                        ui::toast_error_lock(&self.shared_state.toasts, format!("failed to reorder link: {e}"));
                                    } else {
                                        ui::toast_success_lock(&self.shared_state.toasts, format!("successfully reordered link {link_id}"));
                                        Self::set_status(&self.status, PreviewStatus::Updated)
                                    };
                                }
                            }
                        }
                        if ui.button(ui::icon_text("discard changes", ui::constants::REMOVE_ICON)).clicked() {
                            self.is_reordering = false;
                            if let Some(Preview::PoolEntry((mut images, _current_index))) = self.preview.take() {
                                if let Some(original_order) = self.original_order.take() {
                                    let old_images = original_order
                                        .iter()
                                        .filter_map(|hash| {
                                            images
                                                .iter()
                                                .position(|(other_hash, _image_promise)| other_hash == hash)
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
                                if let Some(Preview::PoolEntry(_images)) = self.preview.as_ref() {}

                                self.original_order = Self::get_current_order(&self.preview)
                            }
                        });
                    }
                } else if let EntryInfo::MediaEntry(media_info) = &*entry_info {
                    if media_info.links.len() > 0 {
                        ui.add_space(ui::constants::SPACER_SIZE);
                        ui.group(|ui| {
                            for link_id in &media_info.links {
                                let label = Label::new(ui::icon_text(link_id, ui::constants::LINK_ICON)).sense(Sense::click());
                                let response = ui.add(label);
                                let remove_media_from_link_modal = Self::render_remove_link_modal(
                                    ctx,
                                    entry_info.entry_id().as_media_entry_id().unwrap(),
                                    &link_id,
                                    &self.id,
                                    &self.shared_state.toasts,
                                    &self.status,
                                    &self.entry_info,
                                    &self.shared_state.updated_entries, // self.
                                );
                                response.context_menu(|ui| {
                                    if ui.button(icon!("open link", OPEN_ICON)).clicked() {
                                        Self::set_status(&self.status, PreviewStatus::RequestingNew(EntryId::PoolEntry(*link_id)));
                                        ui.close_menu();
                                    }
                                    if ui.button(icon!("remove link", REMOVE_ICON)).clicked() {
                                        remove_media_from_link_modal.open();
                                        ui.close_menu();
                                    }
                                });
                                // if response.clicked() {
                                //     Self::set_status(&self.status, PreviewStatus::RequestingNew(EntryId::PoolEntry(*link_id)));
                                // } else if response.secondary_clicked() {
                                // }
                            }
                        });
                    }
                }
            }
            ui.add_space(ui::constants::SPACER_SIZE);
            ui.add_enabled_ui(!(self.is_editing_tags || self.is_reordering), |ui| {
                if let Some(entry_info) = self.entry_info.try_lock() {
                    ui.menu_button(format!("{} export", ui::constants::EXPORT_ICON), |ui| {
                        if let EntryId::MediaEntry(hash) = entry_info.entry_id().clone() {
                            if ui.button(ui::icon_text("to clipboard (image)", ui::constants::COPY_ICON)).clicked() {
                                ui.close_menu();
                                self.clipboard_image = Some(Promise::spawn_thread("load_image_clipboard", move || {
                                    let load = || -> Result<FlatSamples<Vec<u8>>> {
                                        let bytes = data::get_media_bytes(&hash)?;
                                        let image = image::load_from_memory(&bytes)?;
                                        let rgba_image = image.to_rgba8();
                                        let flat_samples = rgba_image.into_flat_samples();
                                        Ok(flat_samples)
                                    };
                                    load()
                                }));
                            }
                        }
                        // }
                        if ui.button(ui::icon_text("to file", ui::constants::EXPORT_ICON)).clicked() {
                            if let Some(export_path) = FileDialog::new().pick_folder() {
                                match data::export_entry(entry_info.entry_id(), export_path) {
                                    Err(e) => {
                                        ui::toast_error_lock(&self.shared_state.toasts, format!("failed to export: {e}"));
                                    }
                                    Ok(path) => {
                                        ui::toast_success_lock(&self.shared_state.toasts, format!("successfully exported to:\n{}", path.display()));
                                    }
                                }
                            }
                            ui.close_menu();
                        }
                    });
                    // }
                }
                if ui.button(format!("{} find source", ui::constants::SEARCH_ICON)).clicked() {}
                ui.add_space(ui::constants::SPACER_SIZE);
                if let Some(entry_info) = self.entry_info.try_lock() {
                    match &*entry_info {
                        EntryInfo::PoolEntry(_pool_info) => {
                            if ui.add(ui::caution_button(format!("{} delete link", ui::constants::LINK_ICON))).clicked() {
                                delete_entry_modal.open();
                            }
                            if ui.add(ui::caution_button(format!("{} delete all", ui::constants::DELETE_ICON))).clicked() {
                                delete_linked_entries_modal.open();
                            }
                        }
                        EntryInfo::MediaEntry(_media_info) => {
                            if ui.add(ui::caution_button(format!("{} delete", ui::constants::DELETE_ICON))).clicked() {
                                delete_entry_modal.open();
                            }
                        }
                    }
                }
            });
        });
        ui.add_space(ui::constants::SPACER_SIZE);
    }

    #[inline]
    pub fn get_current_order(preview: &Option<Preview>) -> Option<Vec<String>> {
        preview.as_ref().and_then(|images| {
            if let Preview::PoolEntry((images, _current_index)) = &images {
                Some(images.iter().map(|(hash, _image_promise)| hash.clone()).collect())
            } else {
                None
            }
        })
    }

    pub fn render_info(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.label("info");
            if let Some(entry_info) = self.entry_info.try_lock() {
                egui::Grid::new(format!("info_{}", self.id)).num_columns(2).show(ui, |ui| {
                    let datetime = Utc.timestamp_opt(entry_info.details().date_registered, 0);
                    ui.label("registered");
                    ui.label(datetime.unwrap().format("%B %e, %Y @%l:%M%P").to_string());
                    ui.end_row();

                    ui.label("size");
                    ui.label(ui::readable_byte_size(entry_info.details().size, 2, ui::NumericBase::Two).to_string());
                    ui.end_row();

                    if let EntryInfo::MediaEntry(media_info) = &*entry_info {
                        ui.label("type");
                        ui.label(&media_info.mime);
                        ui.end_row();
                    } else if let EntryInfo::PoolEntry(pool_info) = &*entry_info {
                        ui.label("count");
                        ui.label(format!("{} media", pool_info.hashes.len()));
                        ui.end_row();
                    }
                });
                ui.separator();
                ui.label("tags");
                let tags = &entry_info.details().tags;
                if self.is_editing_tags {
                    if let Some(options) = self.shared_state.autocomplete_options.borrow().as_ref() {
                        //tags::generate_autocomplete_options(&self.tag_data) {
                        ui.add(autocomplete::create(&mut self.tag_edit_buffer, &options, true, true));
                    }
                } else {
                    if tags.is_empty() {
                        ui.vertical_centered_justified(|ui| {
                            ui.label("-- no tags --");
                        });
                    } else {
                        // ui.ctx().set_debug_on_hover(true);
                        ScrollArea::vertical()
                            .auto_shrink([false, true])
                            .id_source(format!("tags_scroll_{}", self.id))
                            .show(ui, |ui| {
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
                                        let exists_in_tag_data = if let Some(Ok(tag_data)) = self.shared_state.tag_data_ref.borrow().ready() {
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
                                            let mut tag_text = tag.to_rich_text(&self.shared_state);
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
                                                .color(tag.namespace_color(&self.shared_state).unwrap_or(ui.style().visuals.text_color()));
                                                response.on_hover_text_at_pointer(hover_text);
                                            }
                                        });
                                        ui.end_row();
                                    }
                                });
                            });
                    }
                }
                // }
            }
        });
    }

    pub fn render_delete_linked_entries_modal(&self, ctx: &egui::Context) -> Modal {
        let modal = ui::modal(ctx, format!("delete_{}_modal_linked", self.id));
        if let Some(EntryInfo::PoolEntry(pool_info)) = self.entry_info.try_lock().as_deref() {
            modal.show(|ui| {
                modal.frame(ui, |ui| {
                    modal.body(
                        ui,
                        format!(
                            "are you sure you want to delete all {} media within {}{} as well as the link itself?\n\n this cannot be undone.",
                            pool_info.hashes.len(),
                            ui::constants::LINK_ICON,
                            self.id
                        ),
                    );
                });
                modal.buttons(ui, |ui| {
                    modal.button(ui, "cancel");
                    if modal.caution_button(ui, format!("delete")).clicked() {
                        let entry_info = Arc::clone(&self.entry_info);
                        let toasts = Arc::clone(&self.shared_state.toasts);
                        let status = Arc::clone(&self.status);
                        let id = self.id.clone();
                        thread::spawn(move || {
                            if let Some(entry_info) = entry_info.try_lock() {
                                let entry_id = entry_info.entry_id();
                                if let Err(e) = data::delete_link_and_linked(entry_id.as_pool_entry_id().unwrap()) {
                                    ui::toast_error_lock(&toasts, format!("failed to delete {}: {}", id, e));
                                } else {
                                    ui::toast_success_lock(&toasts, format!("successfully deleted {}", id));
                                    Self::set_status(&status, PreviewStatus::Deleted(entry_id.clone()))
                                }
                            }
                        });
                    }
                })
            });
        }
        modal
    }

    // remove link_id from that hashes's links
    pub fn render_remove_link_modal(
        ctx: &egui::Context,
        hash: &String,
        link_id: &i32,
        owner_id: &String,
        arc_toasts: &ToastsRef,
        status: &Arc<Mutex<PreviewStatus>>,
        entry_info: &Arc<Mutex<EntryInfo>>,
        updated_list: &UpdateList<EntryId>,
    ) -> Modal {
        let modal = ui::modal(ctx, format!("remove_media_from_link_{}_{}_{}", hash, link_id, owner_id));
        modal.show(|ui| {
            modal.frame(ui, |ui| {
                modal.body(
                    ui,
                    format!(
                        "are you sure you want to remove {} from {}?",
                        ui::pretty_media_id(hash),
                        ui::pretty_link_id(link_id)
                    ),
                )
            });

            modal.buttons(ui, |ui| {
                modal.button(ui, "cancel");
                if modal.button(ui, icon!("remove", REMOVE_ICON)).clicked() {
                    let hash = hash.clone();
                    let link_id = *link_id;
                    let toasts = Arc::clone(arc_toasts);
                    let status = Arc::clone(status);
                    let entry_info = Arc::clone(&entry_info);
                    let update_list = Arc::clone(updated_list);
                    thread::spawn(move || {
                        if let Err(e) = data::remove_media_from_link(&link_id, &hash) {
                            ui::toast_error_lock(&toasts, format!("failed to remove link: {e}"));
                        } else {
                            ui::toast_success_lock(
                                &toasts,
                                format!(
                                    "successfully removed {} from {}",
                                    ui::pretty_media_id(&hash),
                                    ui::pretty_link_id(&link_id)
                                ),
                            );
                            let entry_info = &mut *entry_info.lock();
                            match entry_info {
                                EntryInfo::MediaEntry(media_info) => {
                                    media_info.links.retain(|l| *l != link_id);
                                }
                                EntryInfo::PoolEntry(pool_info) => {
                                    pool_info.hashes.retain(|h| *h != hash);
                                }
                            }
                            let mut update_list = update_list.lock();
                            match entry_info.entry_id() {
                                EntryId::MediaEntry(_) => update_list.push(EntryId::PoolEntry(link_id)),
                                EntryId::PoolEntry(_) => update_list.push(EntryId::MediaEntry(hash)),
                            }
                            Self::set_status(&status, PreviewStatus::Updated)
                        }
                    });
                }
            });
        });
        modal
    }

    pub fn render_delete_entry_modal(&self, ctx: &egui::Context, entry_info_arc: &Arc<Mutex<EntryInfo>>) -> Modal {
        let modal = ui::modal(ctx, format!("delete_entry_{}", &self.id));
        if let Some(entry_info) = entry_info_arc.try_lock().as_deref() {
            modal.show(|ui| {
                modal.frame(ui, |ui| match entry_info {
                    EntryInfo::MediaEntry(_media_entry) => {
                        modal.body(
                            ui,
                            format!(
                                "are you sure you want to delete {}{}?\n\n this cannot be undone.",
                                ui::constants::GALLERY_ICON,
                                self.id
                            ),
                        );
                    }
                    EntryInfo::PoolEntry(_pool_entry) => {
                        modal.body(
                            ui,
                            format!(
                                "are you sure you want to delete {}{}?\
                                        \nthis will delete the link itself, NOT the media in the link.\
                                        \n\n this cannot be undone.",
                                ui::constants::LINK_ICON,
                                self.id
                            ),
                        );
                    }
                });
                modal.buttons(ui, |ui| {
                    modal.button(ui, "cancel");
                    if modal.caution_button(ui, format!("delete")).clicked() {
                        // delete logic below
                        let entry_info = Arc::clone(entry_info_arc);
                        let toasts = Arc::clone(&self.shared_state.toasts);
                        let deleted_list = Arc::clone(&self.shared_state.deleted_entries);
                        let updated_list = Arc::clone(&self.shared_state.updated_entries);

                        let status = Arc::clone(&self.status);
                        let id = self.id.clone();
                        thread::spawn(move || {
                            let entry_info = entry_info.lock();
                            // if let Some(entry_info) = entry_info.lock() {
                            let entry_id = entry_info.entry_id();
                            let associated_hashes = entry_id.as_pool_entry_id().map(|link_id| data::get_hashes_of_media_link(link_id));
                            if let Err(e) = data::delete_entry(&entry_info.entry_id()) {
                                ui::toast_error_lock(&toasts, format!("failed to delete {}: {}", id, e));
                            } else {
                                ui::toast_success_lock(&toasts, format!("successfully deleted {}", id));
                                Self::set_status(&status, PreviewStatus::Deleted(entry_id.clone()));
                                deleted_list.lock().push(entry_id.clone());
                                if let Some(Ok(hashes)) = associated_hashes {
                                    let mut updated_list = updated_list.lock();
                                    updated_list.extend(hashes.into_iter().map(|h| EntryId::MediaEntry(h)));
                                }
                            }
                            // }
                        });
                    }
                })
            });
        }
        modal
    }

    fn render_fullscreen_preview(&mut self, _ui: &mut Ui, ctx: &egui::Context) {
        let area = Area::new("media_fullview").interactable(true).fixed_pos(Pos2::ZERO);
        area.show(ctx, |ui: &mut Ui| {
            let screen_rect = ui.ctx().input(|i| i.screen_rect);
            ui.painter().rect_filled(screen_rect, Rounding::none(), Color32::BLACK);

            fn paint_text(text: impl Into<String>, text_color: Color32, pos: Pos2, painter: &Painter) -> Rect {
                let galley = painter.layout_no_wrap(text.into(), FontId::default(), text_color);
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
            let text_color = ui::text_color();
            if let Some(preview) = self.preview.as_mut() {
                let mut image = match preview {
                    Preview::MediaEntry(image_promise) => image_promise.ready_mut(),
                    Preview::PoolEntry((images, current_index)) => images.get_mut(*current_index).and_then(|(_, i)| i.ready_mut()),
                };

                match image.as_mut() {
                    Some(Ok(image)) => {
                        let (texture_id, size) = match image {
                            MediaPreview::Picture(image) => (image.texture_id(ctx), image.size_vec2().into()),
                            MediaPreview::Movie(streamer) => (streamer.texture_handle.id(), [streamer.width as f32, streamer.height as f32]),
                        };
                        let mesh_size = options.scaled_image_size(size).into();
                        let mut mesh_pos = screen_rect.center() - (mesh_size / 2.);
                        mesh_pos += self.view_offset.into();
                        let mesh_rect = Rect::from_min_size(mesh_pos, mesh_size);
                        image_size = Some(mesh_size);
                        image_center = Some(mesh_pos + (mesh_size / 2.));
                        match image {
                            MediaPreview::Picture(_) => {
                                let mut mesh = Mesh::with_texture(texture_id);
                                mesh.add_rect_with_uv(mesh_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                                ui.painter().add(mesh);
                            }
                            MediaPreview::Movie(player) => {
                                player.ui_at(ui, mesh_rect);
                            }
                        }
                    }
                    Some(Err(e)) => {
                        paint_text(format!("failed to load: {e}"), text_color, screen_rect.center(), ui.painter());
                    }
                    None => {
                        paint_text("loading...", text_color, screen_rect.center(), ui.painter());
                    }
                }
            } else {
                paint_text("waiting to load", text_color, screen_rect.center(), ui.painter());
            }

            let ignore_fullscreen_frames = 20; // number of frames to ignore double click
            let mut was_rect_clicked = |rect: &Rect| -> bool {
                if ui.rect_contains_pointer(*rect) && ctx.input(|i| i.pointer.primary_released()) {
                    self.ignore_fullscreen_edit = ignore_fullscreen_frames;
                    true
                } else {
                    false
                }
            };
            // ui.ctx().input().pointer.
            let control_rect_scale_y = 0.1;

            let outer_rect_scale_x = 0.4;
            let inner_rect_scale_x = 0.05;

            let rect_scale_y = 1. - (control_rect_scale_y * 2.);

            let outer_rect_size = vec2(outer_rect_scale_x * screen_rect.width(), rect_scale_y * screen_rect.height());
            let inner_rect_size = vec2(inner_rect_scale_x * screen_rect.width(), rect_scale_y * screen_rect.height());

            let rect_y_offset = (screen_rect.height() - outer_rect_size.y) * 0.5;

            let left_rect_pos = pos2(0., rect_y_offset);
            let outer_right_rect_pos = pos2(screen_rect.width() - outer_rect_size.x, rect_y_offset);
            let inner_right_rect_pos = pos2(screen_rect.width() - inner_rect_size.x, rect_y_offset);

            let outer_left_rect = Rect::from_min_max(left_rect_pos, left_rect_pos + outer_rect_size);
            let inner_left_rect = Rect::from_min_max(left_rect_pos, left_rect_pos + inner_rect_size);
            let outer_right_rect = Rect::from_min_max(outer_right_rect_pos, outer_right_rect_pos + outer_rect_size);
            let inner_right_rect = Rect::from_min_max(inner_right_rect_pos, inner_right_rect_pos + inner_rect_size);

            // ui.painter().rect_filled(outer_left_rect, Rounding::none(), Color32::GOLD);
            // ui.painter().rect_filled(outer_right_rect, Rounding::none(), Color32::GREEN);
            // ui.painter().rect_filled(inner_right_rect, Rounding::none(), Color32::BLUE);
            // ui.painter().rect_filled(inner_left_rect, Rounding::none(), Color32::RED);

            if was_rect_clicked(&inner_left_rect) {
                Self::set_status(&self.status, PreviewStatus::Previous(self.entry_info.lock().entry_id().clone()))
            } else if was_rect_clicked(&inner_right_rect) {
                Self::set_status(&self.status, PreviewStatus::Next(self.entry_info.lock().entry_id().clone()))
            } else {
                if let Some(Preview::PoolEntry((images, current_index))) = self.preview.as_mut() {
                    if was_rect_clicked(&outer_left_rect) {
                        *current_index = (*current_index as i32 - 1).max(0).min(images.len() as i32 - 1) as usize
                    } else if was_rect_clicked(&outer_right_rect) {
                        *current_index = (*current_index as i32 + 1).max(0).min(images.len() as i32 - 1) as usize
                    }

                    let bottom_rect_size = vec2(screen_rect.width(), control_rect_scale_y * screen_rect.height());
                    let bottom_rect_pos = pos2(0., (1. - control_rect_scale_y) * screen_rect.height());
                    let bottom_rect = Rect::from_min_max(bottom_rect_pos, bottom_rect_pos + bottom_rect_size);

                    if ui.rect_contains_pointer(bottom_rect) || *current_index == 0 || *current_index == images.len() - 1 {
                        // let inner_bottom_rect = Rect::from_center_size(bottom_rect.center(), vec2(20., 20.));
                        let text_rect = paint_text(
                            format!("{} / {}", *current_index + 1, images.len()),
                            text_color,
                            bottom_rect.center(),
                            ui.painter(),
                        );
                        ui.painter().rect_filled(text_rect.expand(10.), Rounding::none(), Color32::BLACK);
                        paint_text(
                            format!("{} / {}", *current_index + 1, images.len()),
                            text_color,
                            bottom_rect.center(),
                            ui.painter(),
                        );
                    }
                }
            }
            let something_dragged = ctx.memory(|m| m.is_anything_being_dragged());
            let primary_down = ctx.input(|i| i.pointer.primary_down());
            if !something_dragged && primary_down {
                if let Some(image_size) = image_size {
                    let delta = ctx.input(|i| i.pointer.delta());
                    self.view_offset = (Vec2::from(self.view_offset) + delta).into();
                    // {view_offset_bound_factor} amount of the image allowed to clip offscreeen
                    let view_offset_bound_factor = 0.9;
                    let view_offset_x_bound = (screen_rect.width() - (1. - 2. * view_offset_bound_factor) * (image_size.x)) / 2.;
                    let view_offset_y_bound = (screen_rect.height() - (1. - 2. * view_offset_bound_factor) * (image_size.y)) / 2.;
                    self.view_offset[0] = self.view_offset[0].max(-view_offset_x_bound).min(view_offset_x_bound);
                    self.view_offset[1] = self.view_offset[1].max(-view_offset_y_bound).min(view_offset_y_bound);
                }
            }

            for event in &ctx.input(|i| i.events.clone()) {
                match event {
                    Event::Scroll(scroll_delta) => {
                        if let Some(image_center) = image_center {
                            if let Some(hover_pos) = ui.ctx().input(|i| i.pointer.hover_pos()) {
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
    }

    fn render_windowed_preview(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let mut options = RenderLoadingImageOptions::default();
        options.shrink_to_image = true;
        options.desired_image_size = [Config::global().ui.preview_size as f32; 2];
        options.hover_text_on_error_image = Some(Box::new(|error| format!("failed to load image: {error}").into()));

        if !ctx.memory(|m| m.is_anything_being_dragged()) {
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
            Some(Preview::MediaEntry(image_promise)) => {
                // match image_promise.ready_mut() {
                //     Some(Ok(MediaPreview::Movie(player))) => player.process_state(),
                //     _ => ()
                // }
                ui::render_loading_preview(ui, ctx, Some(image_promise), &options)
            },
            Some(Preview::PoolEntry((images_promise, current_view_index))) => {
                options.desired_image_size = [Config::global().ui.preview_pool_size as f32; 2];
                options.sense.push(Sense::drag());
                ScrollArea::vertical()
                    .min_scrolled_height(500.)
                    .id_source(format!("{}_pool_scroll", self.id))
                    .show(ui, |ui| {
                        ui.with_layout(Layout::top_down(Align::Center), |ui| {
                            ui.group(|ui| {
                                let grid = Grid::new(format!("{}_pool_grid", self.id)).show(ui, |ui| {
                                    // let mut removed_images = vec![];
                                    if let Some(EntryInfo::PoolEntry(pool_info)) = self.entry_info.try_lock().as_deref_mut() {
                                        images_promise.retain(|(h, _)| pool_info.hashes.contains(h))
                                    }
                                    for (image_index, (hash, image_promise)) in images_promise.iter_mut().enumerate() {
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

                                            let image_response = ui::render_loading_preview(ui, ctx, Some(image_promise), &options);
                                            options.image_tint = None;
                                            if let Some(mut image_response) = image_response {
                                                if image_response.dragged() && self.is_reordering {
                                                    if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
                                                        let mut options = RenderLoadingImageOptions::default();
                                                        options.desired_image_size = [Config::global().ui.preview_reorder_size as f32; 2];
                                                        options.shrink_to_image = true;
                                                        egui::Area::new("dragged_item")
                                                            .interactable(false)
                                                            .fixed_pos(pointer_pos)
                                                            .order(Order::Foreground)
                                                            .show(ctx, |ui| ui::render_loading_preview(ui, ctx, Some(image_promise), &options));
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
                                                if let Some(EntryId::PoolEntry(link_id)) = self.entry_info.try_lock().map(|i| i.entry_id().clone()) {
                                                    let remove_media_from_link_modal = Self::render_remove_link_modal(
                                                        ctx,
                                                        hash,
                                                        &link_id,
                                                        &self.id,
                                                        &self.shared_state.toasts,
                                                        &self.status,
                                                        &self.entry_info,
                                                        &self.shared_state.updated_entries,
                                                    );
                                                    image_response = image_response.context_menu(|ui| {
                                                        if ui.button(icon!("open media", OPEN_ICON)).clicked() {
                                                            Self::set_status(
                                                                &self.status,
                                                                PreviewStatus::RequestingNew(EntryId::MediaEntry(hash.clone())),
                                                            );
                                                            ui.close_menu();
                                                        }
                                                        if ui.button(icon!("remove link", REMOVE_ICON)).clicked() {
                                                            remove_media_from_link_modal.open();
                                                            ui.close_menu();
                                                        }
                                                        if ui.add(ui::caution_button(icon!("delete media", DELETE_ICON))).clicked() {
                                                            ui.close_menu();
                                                        }
                                                    });
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

                                        if (image_index + 1) % Config::global().ui.preview_pool_columns as usize == 0 {
                                            ui.end_row()
                                        }
                                    }
                                });

                                if self.current_dragged_index.is_some() && !ui.rect_contains_pointer(grid.response.rect) {
                                    self.current_drop_index = Some(0);
                                }
                            });
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

    pub fn render_preview(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.is_fullscreen {
            self.render_fullscreen_preview(ui, ctx)
        } else {
            self.render_windowed_preview(ui, ctx)
        }
    }

    pub fn load_preview(&mut self, ctx: &egui::Context) {
        // let ctx = ctx.clone();
        let load = |hash: &String, ctx: egui::Context| -> Result<MediaPreview> {
            let bytes = data::get_media_bytes(hash)?;
            let entry_info = data::get_entry_info(&EntryId::MediaEntry(hash.clone()))?;
            if entry_info.is_movie() {
                let mut player = Player::new_from_bytes(&ctx, &bytes)?;
                player.start();
                return Ok(MediaPreview::Movie(player));
            } else {
                let dynamic_image = image::load_from_memory(&bytes)?;
                let retained_image = ui::generate_retained_image(&dynamic_image.to_rgba8())?;
                return Ok(MediaPreview::Picture(retained_image));
            }
        };

        if let Some(entry_info) = self.entry_info.try_lock() {
            match entry_info.entry_id() {
                EntryId::MediaEntry(hash) => {
                    let hash = hash.clone();
                    let ctx = ctx.clone();
                    self.preview = Some(Preview::MediaEntry(Promise::spawn_thread("load_media_image_preview", move || {
                        load(&hash, ctx)
                    })));
                }
                EntryId::PoolEntry(_link_id) => {
                    if let EntryInfo::PoolEntry(pool_info) = &*entry_info {
                        self.preview = Some(Preview::PoolEntry((
                            pool_info
                                .hashes
                                .clone()
                                .into_iter()
                                .map(|hash| {
                                    let ctx = ctx.clone();
                                    (hash.clone(), Promise::spawn_thread("load_pool_image_previews", move || load(&hash, ctx)))
                                })
                                .collect::<Vec<_>>(),
                            0,
                        )));
                    }
                }
            }
            // }
        }
    }

    fn process_keybinds(&mut self, ui: &mut Ui, ctx: &Context) {
        if ui::does_ui_have_focus(ui, ctx) {
            if let Some(entry_info) = self.entry_info.try_lock() {
                if ui::key_pressed(ctx, Key::ArrowLeft, Modifiers::NONE) {
                    Self::set_status(&self.status, PreviewStatus::Previous(entry_info.entry_id().clone()))
                } else if ui::key_pressed(ctx, Key::ArrowRight, Modifiers::NONE) {
                    Self::set_status(&self.status, PreviewStatus::Next(entry_info.entry_id().clone()))
                }
            }
        }
    }

    pub fn load_entry_info(&self) {
        let status = Arc::clone(&self.status);
        let entry_info = Arc::clone(&self.entry_info);
        thread::spawn(move || {
            let mut entry_info = entry_info.lock();
            // if let Some(mut entry_info) = entry_info.lock() {
            if let Ok(new_info) = data::get_entry_info(&entry_info.entry_id()) {
                *entry_info = new_info
            }
            Self::set_status(&status, PreviewStatus::Updated);
            // }
        });
    }
    // pub load_image
}
