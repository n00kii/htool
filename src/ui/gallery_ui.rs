use crate::data::EntryId;
use crate::data::EntryInfo;

use crate::gallery::load_gallery_entries;
use crate::gallery::GalleryEntry;
use crate::tags;
use crate::tags::Tag;

use crate::ui::SharedState;
use crate::util;

use crate::util::BatchPollBuffer;
// use anyhow::Context;

use anyhow::Context;
use eframe::egui::Key;
use eframe::egui::Layout;

use eframe::epaint::Shadow;
use egui::Align2;
use egui::Color32;
use egui::FontId;
use egui::Id;
use egui::Mesh;
use egui::Pos2;
use egui::Rect;
use egui::RichText;
use egui::Rounding;
use egui::Stroke;
use egui::TextureId;
use egui_extras::Size;
use egui_extras::StripBuilder;
use egui_modal::Modal;
use egui_notify::Toasts;
use regex::Regex;
use ui::{constants, icon_text};

use crate::ui::RenderLoadingImageOptions;
use crate::ui::WindowContainer;
use crate::util::PollBuffer;

use super::super::data;
use super::super::ui;
use super::super::Config;
use super::autocomplete;
use super::icon;
use super::preview_ui::MediaPreview;
use super::preview_ui::PreviewStatus;
use super::preview_ui::PreviewUI;

use anyhow::Result;

use eframe::{
    egui::{self, Ui},
    emath::{Align, Vec2},
};
use egui::Direction;
use egui::ScrollArea;

use poll_promise::Promise;

use std::cell::RefCell;

use std::rc::Rc;
use std::sync::Arc;
// use std::sync::Mutex;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::thread;

pub struct GalleryUI {
    pub thumbnail_buffer: BatchPollBuffer<GalleryEntry>,
    pub refresh_buffer: BatchPollBuffer<GalleryEntry>,
    pub toasts: Toasts,
    pub loading_gallery_entries: Option<Promise<Result<Vec<GalleryEntry>>>>,
    pub shared_state: Rc<SharedState>,
    pub gallery_entries: Option<Vec<Rc<RefCell<GalleryEntry>>>>,
    pub filtered_gallery_entries: Option<Vec<Rc<RefCell<GalleryEntry>>>>,
    pub preview_windows: Vec<ui::WindowContainer>,
    pub search_string: String,
    last_gallery_entry_index: usize,
    max_processed_gallery_entries_per_frame: usize,
    is_selection_mode: bool,
    include_dependants: bool,
    include_pools: bool,
    last_hovered: Vec<(TextureId, Rect, f32)>,
    selected_rects: Vec<Rect>,
}

impl GalleryUI {
    pub fn new(shared_state: &Rc<SharedState>) -> Self {
        let thumbnail_poll_buffer = PollBuffer::<GalleryEntry>::new(None, Some(100), None, Some(GalleryUI::thumbnail_buffer_poll), None);
        let thumbnail_buffer = BatchPollBuffer::new(thumbnail_poll_buffer);
        let refresh_poll_buffer = PollBuffer::<GalleryEntry>::new(
            None,
            Some(1),
            Some(GalleryUI::refresh_buffer_add),
            Some(GalleryUI::refresh_buffer_poll),
            None,
        );
        let refresh_buffer = BatchPollBuffer::new(refresh_poll_buffer);
        Self {
            preview_windows: vec![],
            selected_rects: vec![],
            is_selection_mode: false,
            search_string: String::new(),
            loading_gallery_entries: None,
            last_hovered: vec![],
            shared_state: Rc::clone(&shared_state),
            toasts: Toasts::default().with_anchor(egui_notify::Anchor::BottomLeft),
            thumbnail_buffer,
            refresh_buffer,
            gallery_entries: None,
            filtered_gallery_entries: None,
            last_gallery_entry_index: 0,
            max_processed_gallery_entries_per_frame: 100,
            include_dependants: false,
            include_pools: true,
        }
    }
}

#[derive(Debug)]
pub struct EntrySearch {
    pub and_relations: Vec<Vec<Tag>>,
    pub not_relations: Vec<Vec<Tag>>,
    pub or_relations: Vec<Vec<Tag>>,
    pub score_min: Option<(i64, bool)>, //value, inclusive
    pub score_max: Option<(i64, bool)>, //value, inclusive
    pub score_exact: Option<i64>,
    pub is_bookmarked: Option<bool>,
    pub is_independant: Option<bool>,
    pub is_pool: Option<bool>,
    pub is_media: Option<bool>,
    pub is_valid: bool,
    pub limit: Option<i64>,
}

impl Default for EntrySearch {
    fn default() -> Self {
        Self {
            and_relations: vec![],
            not_relations: vec![],
            or_relations: vec![],
            score_max: None,
            score_min: None,
            score_exact: None,
            is_bookmarked: None,
            is_independant: None,
            is_media: None,
            is_pool: None,
            is_valid: true,
            limit: None,
        }
    }
}

const OR_QUANTIFIER: &str = "or";
const NOT_QUANTIFIER: &str = "not";
const TYPE_QUANTIFIER: &str = "type";
const INDEPENDANT_QUANTIFIER: &str = "independant";
const LIMIT_QUANTIFIER: &str = "limit";
const BOOKMARKED_QUANTIFIER: &str = "bookmarked";
const SCORE_QUANTIFIER: &str = "score";
const SCORE_Q_QUANTIFIER: &str = "score_q";

fn grouping_re(q: impl Into<String>) -> String {
    format!(r"{0}\((?P<{0}>.+?)\)", q.into())
}
fn value_re(q: impl Into<String>) -> String {
    format!(r"{0}=(?P<{0}>.+?)\b", q.into())
}

fn regex(re_string: &String) -> Regex {
    Regex::new(re_string).context("couldn't form regex").unwrap()
}
///or(hey babe how going) not(bruh mo_::ment) type=poo_ff bookmarked=false score>3 score<=23 or(nope)
impl From<String> for EntrySearch {
    fn from(search: String) -> Self {
        lazy_static! {
            static ref OR_RE: Regex = regex(&grouping_re(OR_QUANTIFIER));
            static ref NOT_RE: Regex = regex(&grouping_re(NOT_QUANTIFIER));
            static ref TYPE_RE: Regex = regex(&value_re(TYPE_QUANTIFIER));
            static ref BOOKMARKED_RE: Regex = regex(&value_re(BOOKMARKED_QUANTIFIER));
            static ref INDEPENDANT_RE: Regex = regex(&value_re(INDEPENDANT_QUANTIFIER));
            static ref LIMIT_RE: Regex = regex(&value_re(LIMIT_QUANTIFIER));
            static ref SCORE_RE: Regex = regex(&format!(r"{SCORE_QUANTIFIER}(?P<{SCORE_Q_QUANTIFIER}>.+?)(?P<{SCORE_QUANTIFIER}>\d+)"));
        }
        fn str_to_bool(st: &str) -> Option<bool> {
            match st {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            }
        }
        let mut entry_search = EntrySearch::default();
        for cap in OR_RE.captures_iter(&search) {
            let tags = Tag::from_tagstrings(&cap[OR_QUANTIFIER]);
            entry_search.or_relations.push(tags)
        }
        for cap in NOT_RE.captures_iter(&search) {
            let tags = Tag::from_tagstrings(&cap[NOT_QUANTIFIER]);
            entry_search.not_relations.push(tags)
        }
        for cap in TYPE_RE.captures_iter(&search) {
            match &cap[TYPE_QUANTIFIER] {
                "pool" => entry_search.is_pool = Some(true),
                "media" => entry_search.is_media = Some(true),
                _ => entry_search.is_valid = false,
            }
        }
        for cap in BOOKMARKED_RE.captures_iter(&search) {
            entry_search.is_bookmarked = str_to_bool(&cap[BOOKMARKED_QUANTIFIER])
        }
        for cap in INDEPENDANT_RE.captures_iter(&search) {
            entry_search.is_independant = str_to_bool(&cap[INDEPENDANT_QUANTIFIER])
        }
        for cap in SCORE_RE.captures_iter(&search) {
            let score = (&cap[SCORE_QUANTIFIER]).parse().ok();
            if let Some(score) = score {
                match &cap[SCORE_Q_QUANTIFIER] {
                    "<" => entry_search.score_max = Some((score, false)),
                    "<=" => entry_search.score_max = Some((score, true)),
                    ">" => entry_search.score_min = Some((score, false)),
                    ">=" => entry_search.score_min = Some((score, true)),
                    "=" => entry_search.score_exact = Some(score),
                    _ => entry_search.is_valid = false,
                }
            } else {
                entry_search.is_valid = false;
            }
        }
        for cap in LIMIT_RE.captures_iter(&search) {
            let limit: Option<i64> = (&cap[LIMIT_QUANTIFIER]).parse().ok();
            if let Some(limit) = limit {
                entry_search.limit = Some(limit);
            } else {
                entry_search.is_valid = false;
            }
        }
        let mut stripped = search.clone();
        stripped = OR_RE.replace_all(&stripped, "").to_string();
        stripped = NOT_RE.replace_all(&stripped, "").to_string();
        stripped = TYPE_RE.replace_all(&stripped, "").to_string();
        stripped = BOOKMARKED_RE.replace_all(&stripped, "").to_string();
        stripped = INDEPENDANT_RE.replace_all(&stripped, "").to_string();
        stripped = SCORE_RE.replace_all(&stripped, "").to_string();
        stripped = LIMIT_RE.replace_all(&stripped, "").to_string();

        entry_search.and_relations.push(Tag::from_tagstrings(&stripped));
        entry_search
    }
}

impl GalleryUI {
    fn get_selected_gallery_entries(&self) -> Vec<Rc<RefCell<GalleryEntry>>> {
        util::filter_opt_vec(&self.gallery_entries, |gallery_entry| gallery_entry.borrow().is_selected)
    }
    fn process_previews(&mut self, ctx: &egui::Context) {
        let mut do_refiter = false;
        // let mut preview_index = 0;
        let mut new_previews = vec![];
        self.preview_windows.retain_mut(|window_container| {
            if let Some(preview_ui) = window_container.window.downcast_mut::<PreviewUI>() {
                let preview_status = PreviewUI::try_get_status(&preview_ui.status);
                let close_preview =
                    !(matches!(preview_status, Some(PreviewStatus::Closed)) || matches!(preview_status, Some(PreviewStatus::Deleted(_))));
                if matches!(preview_status, Some(PreviewStatus::Updated)) || matches!(preview_status, Some(PreviewStatus::HardUpdated)) {
                    if matches!(preview_status, Some(PreviewStatus::HardUpdated)) {
                        preview_ui.load_preview(ctx);
                    }
                    do_refiter = true;
                    // PreviewUI::set_status(&preview_ui.status, PreviewStatus::None);
                } else if matches!(preview_status, Some(PreviewStatus::Previous(_))) || matches!(preview_status, Some(PreviewStatus::Next(_))) {
                    let mut next_entry_info = None;
                    let mut gallery_entries_len = 0;
                    let mut increment = 0;
                    let preview_entry_id = match preview_status.as_ref().unwrap() {
                        PreviewStatus::Previous(entry_id) => {
                            increment = -1;
                            entry_id.clone()
                        }
                        PreviewStatus::Next(entry_id) => {
                            increment = 1;
                            entry_id.clone()
                        }
                        _ => unreachable!(),
                    };
                    let gallery_entry_index = self.filtered_gallery_entries.as_ref().and_then(|gallery_entries| {
                        gallery_entries_len = gallery_entries.len();
                        gallery_entries.iter().enumerate().find_map(|(entry_index, gallery_entry)| {
                            gallery_entry.borrow().entry_info.try_lock().and_then(|entry_info| {
                                if entry_info.entry_id() == &preview_entry_id {
                                    Some(entry_index)
                                } else {
                                    None
                                }
                            })
                        })
                    });

                    if let Some(current_entry_index) = gallery_entry_index {
                        let mut next_gallery_index = current_entry_index as i64;

                        next_gallery_index = next_gallery_index + increment;

                        if next_gallery_index >= 0 && next_gallery_index <= gallery_entries_len as i64 - 1 {
                            next_entry_info = self.filtered_gallery_entries.as_ref().and_then(|gallery_entries| {
                                gallery_entries
                                    .get(next_gallery_index as usize)
                                    .and_then(|gallery_entry| Some(Arc::clone(&gallery_entry.borrow().entry_info)))
                            });
                        }
                    }
                    // }
                    if let Some(next_entry_info) = next_entry_info {
                        preview_ui.entry_info = next_entry_info;
                        preview_ui.preview = None;
                    }
                    // PreviewUI::set_status(&preview_ui.status, PreviewStatus::None);
                } else if let Some(PreviewStatus::RequestingNew(entry_id)) = &preview_status {
                    if let Some(gallery_entries) = self.gallery_entries.as_ref() {
                        let mut found = false;
                        for gallery_entry in gallery_entries.iter() {
                            if gallery_entry.borrow().entry_info.lock().entry_id() == entry_id {
                                found = true;
                                new_previews.push(Arc::clone(&gallery_entry.borrow().entry_info))
                            }
                        }
                        if !found {
                            
                        }
                    }
                    // PreviewUI::set_status(&preview_ui.status, PreviewStatus::None);
                }
                //todo handle deletedandupdated
                // chunkinglist up every frame?
                if let Some(PreviewStatus::Deleted(entry_id)) = &preview_status {
                    if let Some(gallery_entries) = self.gallery_entries.as_mut() {
                        gallery_entries.retain(|gallery_entry| {
                            if let Some(entry_info) = gallery_entry.borrow().entry_info.try_lock() {
                                entry_info.entry_id() != entry_id
                            } else {
                                true
                            }
                        });
                        do_refiter = true;
                    }
                }

                PreviewUI::set_status(&preview_ui.status, PreviewStatus::None);
                close_preview
                // !(matches!(preview_status, Some(PreviewStatus::Closed)) || matches!(preview_status, Some(PreviewStatus::Deleted(_))))
            } else {
                true
            }
        });

        for new_preview in new_previews {
            Self::launch_preview(&new_preview, &mut self.preview_windows, &self.shared_state);
        }

        if do_refiter {
            tags::reload_tag_data(&self.shared_state.tag_data_ref);
            self.filter_entries();
        }
    }

    fn load_entries(&mut self) {
        if let Some(gallery_entries) = self.filtered_gallery_entries.as_ref() {
            let requests = gallery_entries
                .iter()
                .filter_map(|gallery_entry| {
                    if !gallery_entry.borrow().did_complete_request {
                        Some(gallery_entry.borrow_mut().generate_load_request())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            // data::load_thumbnail_with_requests(requests);
            thread::spawn(|| data::load_gallery_entries_with_requests(requests));
        }
    }

    fn process_gallery_entries(&mut self) {
        if let Some(gallery_entries) = self.gallery_entries.as_ref() {
            for gallery_entry in gallery_entries {
                let mut gallery_entry = gallery_entry.borrow_mut();
                if util::is_opt_promise_ready(&gallery_entry.updated_entry_info) {
                    if let Some(updated_info_promise) = gallery_entry.updated_entry_info.take() {
                        match updated_info_promise.try_take() {
                            Ok(Ok(updated_info)) => {
                                let mut gallery_entry_info = gallery_entry.entry_info.lock();
                                *gallery_entry_info = updated_info
                            }
                            Ok(Err(e)) => {
                                // print!("failed {e}")
                            }
                            Err(_p) => {
                                // print!("wasnt ready!")
                            }
                        }
                    }
                }
            }
        }

        if self.gallery_entries.is_some() && self.filtered_gallery_entries.is_none() {
            self.filter_entries();
        }

        if util::is_opt_promise_ready(&self.loading_gallery_entries) {
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
                            self.filter_entries();
                            // self.load_entries();
                            // self.load_all_thumbnails();
                            // self.load_all_entry_info();
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

    pub fn generate_entries(&mut self) {
        self.loading_gallery_entries = Some(Promise::spawn_thread("loading_gallery_entries", move || load_gallery_entries()));
    }

    fn is_loading_gallery_entries(&self) -> bool {
        if let Some(entries_promise) = self.loading_gallery_entries.as_ref() {
            entries_promise.ready().is_none()
        } else {
            false
        }
    }

    fn launch_preview(entry_info: &Arc<Mutex<EntryInfo>>, preview_windows: &mut Vec<WindowContainer>, shared_state: &Rc<SharedState>) {
        let preview = PreviewUI::new(
            &entry_info,
            &shared_state, // &shared_state.tag_data_ref,
                           // &shared_state.toasts,
                           // &shared_state.autocomplete_options,
        );
        let entry_info = entry_info.lock();
        // let icon = match entry_info.entry_id() {
        //     EntryId::MediaEntry(_) => ui::constants::GALLERY_ICON,
        //     EntryId::PoolEntry(_) => ui::constants::LINK_ICON,
        // };
        let title = ui::pretty_entry_id(entry_info.entry_id()); //ui::icon_text(&preview.short_id, icon);
        if !ui::does_window_exist(&title, preview_windows) {
            preview_windows.push(WindowContainer {
                title,
                is_open: Some(true),
                window: preview,
            })
        } else {
        }
    }

    pub fn update_entries(&self, updated_entries: &Vec<EntryId>) {
        if let Some(gallery_entries) = self.gallery_entries.as_ref() {
            let requests = gallery_entries
                .iter()
                .filter_map(|gallery_entry| {
                    if updated_entries.contains(gallery_entry.borrow().entry_info.lock().entry_id()) {
                        Some(gallery_entry.borrow_mut().generate_entry_info_request())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            thread::spawn(|| data::load_entry_info_with_requests(requests));
        }
    }

    pub fn refresh(&mut self) {
        if let Some(gallery_entries) = self.gallery_entries.as_mut() {
            for gallery_entry in gallery_entries.iter_mut() {
                gallery_entry.borrow_mut().is_info_dirty = true;
            }
        }
    }

    fn render_options(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let link_modal = self.render_make_link_modal(ctx);
        ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
            ui.label("gallery");
            ui.add_enabled_ui(!self.is_loading_gallery_entries(), |ui| {
                if ui
                    .button(if self.is_loading_gallery_entries() {
                        String::from("loading...")
                    } else {
                        icon!("refresh", REFRESH_ICON)
                    })
                    .clicked()
                {
                    tags::reload_tag_data(&self.shared_state.tag_data_ref);
                    self.generate_entries();
                }
            });
            ui.add_space(ui::constants::SPACER_SIZE);
            let currently_selected_len = self.get_selected_gallery_entries().len();
            ui.group(|ui| {
                ui.checkbox(&mut self.is_selection_mode, "selection mode");
                if currently_selected_len > 0 {
                    let text = RichText::new(format!("{} selected", currently_selected_len)).weak().italics();
                    ui.label(text);
                }
            });

            ui.add_space(ui::constants::SPACER_SIZE);
            if ui.button("select all").clicked() {
                util::opt_vec_applyeach_refcell(&mut self.filtered_gallery_entries, |gallery_entry| gallery_entry.is_selected = true)
            }

            if ui.button("deselect all").clicked() {
                util::opt_vec_applyeach_refcell(&mut self.filtered_gallery_entries, |gallery_entry| gallery_entry.is_selected = false)
            }

            if ui.button("invert").clicked() {
                util::opt_vec_applyeach_refcell(&mut self.filtered_gallery_entries, |gallery_entry| gallery_entry.is_selected ^= true)
            }
            ui.add_enabled_ui(currently_selected_len > 0, |ui| {
                ui.add_space(ui::constants::SPACER_SIZE);
                ui.add_enabled_ui(currently_selected_len > 1, |ui| {
                    if ui.button(ui::icon_text("link", ui::constants::LINK_ICON)).clicked() {
                        link_modal.open();
                    }
                });
                ui.add_space(ui::constants::SPACER_SIZE);

                if ui.add(ui::caution_button(ui::icon_text("delete", ui::constants::DELETE_ICON))).clicked() {}
            });
        });
    }
    fn render_make_link_modal(&self, ctx: &egui::Context) -> Modal {
        let modal = ui::modal(ctx, "link_modal");
        let currently_selected = self.get_selected_gallery_entries();
        modal.show(|ui| {
            modal.title(ui, "new link");
            modal.body(ui, format!("make a new link between {} media?", currently_selected.len()));
            modal.buttons(ui, |ui| {
                modal.button(ui, "cancel");
                if modal.suggested_button(ui, ui::icon_text("make link", ui::constants::LINK_ICON)).clicked() {
                    let entry_ids = currently_selected
                        .iter()
                        .map(|gallery_entry| gallery_entry.borrow().entry_info.lock().entry_id().clone())
                        .collect::<Vec<_>>();
                    let toasts = Arc::clone(&self.shared_state.toasts);
                    let updated_list = Arc::clone(&self.shared_state.updated_entries);
                    thread::spawn(move || {
                        let hashes = entry_ids
                            .iter()
                            .filter_map(|entry_id| entry_id.as_media_entry_id().map(|s| s.clone()))
                            .collect::<Vec<_>>();
                        if hashes.len() != entry_ids.len() {
                            ui::toast_error_lock(&toasts, format!("links can only be made from media"))
                        } else {
                            match data::create_pool_link(&hashes) {
                                Ok(link_id) => {
                                    let mut updated_list = updated_list.lock().unwrap();
                                    updated_list.extend(hashes.iter().map(|h| EntryId::MediaEntry(h.clone())));
                                    ui::toast_success_lock(&toasts, format!("successfully created {}", ui::pretty_link_id(&link_id)))
                                }
                                Err(e) => ui::toast_error_lock(&toasts, format!("failed to create link: {e}")),
                            };
                        }
                    });
                }
            });
        });
        modal
    }
    fn render_gallery_entries(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if let Some(gallery_entries) = self.filtered_gallery_entries.as_mut() {
            ScrollArea::vertical()
                .id_source("previews_col")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let layout = egui::Layout::from_main_dir_and_cross_align(Direction::LeftToRight, Align::Center).with_main_wrap(true);
                    ui.allocate_ui(Vec2::new(ui.available_size_before_wrap().x, 0.0), |ui| {
                        ui.with_layout(layout, |ui| {
                            let mut current_hovered = None;
                            ui.style_mut().spacing.item_spacing = Vec2::new(0., 0.);
                            for gallery_entry in gallery_entries.iter() {
                                let status_label = gallery_entry.borrow().get_status_label().map(|label| label.into());
                                let mut options = RenderLoadingImageOptions::default();
                                let thumbnail_size = Config::global().gallery.thumbnail_size as f32;
                                options.hover_text_on_none_image = Some("(loading bytes for thumbnail...)".into());
                                options.hover_text_on_loading_image = Some("(loading thumbnail...)".into());
                                options.hover_text = status_label;
                                options.desired_image_size = [thumbnail_size, thumbnail_size];
                                let response = ui::render_loading_preview(ui, ctx, gallery_entry.borrow_mut().thumbnail.as_mut(), &options);
                                if let Some(response) = response {
                                    if self.is_selection_mode {
                                        if response.clicked() {
                                            gallery_entry.borrow_mut().is_selected ^= true;
                                        }

                                        if gallery_entry.borrow().is_selected {
                                            let base_color = Config::global().themes.accent_fill_color().unwrap_or(Color32::WHITE);
                                            let secondary_color = Config::global().themes.accent_stroke_color().unwrap_or(Color32::BLACK);
                                            let stroke = Stroke::new(3., base_color);
                                            let mut text_fid = FontId::default();
                                            text_fid.size = 32.;
                                            ui.painter()
                                                .rect(response.rect, Rounding::from(3.), secondary_color.linear_multiply(0.3), stroke);
                                            ui.painter().circle(response.rect.center(), 20., base_color, Stroke::none());
                                            ui.painter().text(
                                                response.rect.center(),
                                                Align2::CENTER_CENTER,
                                                ui::constants::SUCCESS_ICON,
                                                text_fid,
                                                secondary_color,
                                            );
                                        } else if response.hovered() {
                                            ui.painter()
                                                .rect_stroke(response.rect, Rounding::from(3.), Stroke::new(2., Color32::GRAY));
                                        }
                                    } else {
                                        if response.clicked() {
                                            GalleryUI::launch_preview(
                                                &gallery_entry.borrow().entry_info,
                                                &mut self.preview_windows,
                                                &self.shared_state,
                                            )
                                        }
                                        let gallery_entry = gallery_entry.borrow();
                                        if let Some(thumbnail) = gallery_entry.thumbnail.as_ref() {
                                            if let Some(thumbnail) = thumbnail.ready() {
                                                if let Ok(MediaPreview::Picture(thumbnail)) = thumbnail {
                                                    // get current hovered
                                                    if response.hovered() {
                                                        current_hovered = Some((thumbnail.texture_id(ctx), response.rect));
                                                    }
                                                    for (t, r, _) in self.last_hovered.iter_mut() {
                                                        if *t == thumbnail.texture_id(ctx) {
                                                            *r = response.rect;
                                                        }
                                                    }
                                                }
                                            }
                                        };
                                    }
                                }
                            }

                            if !self.is_selection_mode {
                                // --hover stuff--
                                let max_hovered_dur = 0.7;
                                let delay = 0.15;

                                if let Some((tex_id, rect)) = current_hovered {
                                    // increment current hovered, or add it if dont exists
                                    let mut exists = false;
                                    for (other_tex_id, _, other_hover_dur) in self.last_hovered.iter_mut() {
                                        if tex_id == *other_tex_id {
                                            exists = true;
                                            // dbg!(ctx.input().pointer.delta().length());
                                            if ctx.input().pointer.delta().length() < 5. {
                                                *other_hover_dur = (*other_hover_dur + ctx.input().stable_dt).min(max_hovered_dur + delay);
                                            }
                                        }
                                    }
                                    if !exists {
                                        self.last_hovered.push((tex_id, rect, 0.))
                                    }
                                }

                                // remove non hovered expired
                                self.last_hovered.retain(|(_, _, d)| !(*d < 0.));

                                // paint, decrement
                                for (tex_id, rect, hover_duration) in self.last_hovered.iter_mut() {
                                    let mut expansion = 10.;
                                    let hovered_frac = ((*hover_duration - delay).max(0.) / max_hovered_dur).min(1.);
                                    if hovered_frac > 0. {
                                        expansion *= ui::ease_in_cubic(hovered_frac);

                                        let mut options = RenderLoadingImageOptions::default();
                                        options.desired_image_size = [Config::global().gallery.thumbnail_size as f32; 2];
                                        let image_mesh_size = options.scaled_image_size(rect.size().into()).into();
                                        let image_mesh_pos = rect.center() - (image_mesh_size / 2.);
                                        let image_rect = Rect::from_min_size(image_mesh_pos, image_mesh_size).expand(expansion);
                                        let tweened_image_rect = Rect::from_min_size(image_mesh_pos, image_mesh_size).expand(expansion);
                                        let mut shadow = Shadow::big_dark();
                                        shadow.color = shadow.color.linear_multiply(ui::ease_in_cubic(hovered_frac));

                                        let shadow_mesh = shadow.tessellate(image_rect, Rounding::none());

                                        ui.painter().add(shadow_mesh);
                                        ui::paint_image(ui.painter(), &tex_id, tweened_image_rect);

                                        let is_currently_hovered = if let Some((current_hovered_tex_id, _)) = current_hovered {
                                            current_hovered_tex_id == *tex_id
                                        } else {
                                            false
                                        };

                                        if !is_currently_hovered {
                                            *hover_duration -= ctx.input().stable_dt * 3.;
                                        }
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
                let search_options = Rc::clone(&self.shared_state.autocomplete_options);
                let search_options = search_options.borrow();
                if let Some(search_options) = search_options.as_ref() {
                    let autocomplete = autocomplete::create(&mut self.search_string, search_options, false, true);
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

    fn thumbnail_buffer_poll(gallery_entry: &Rc<RefCell<GalleryEntry>>) -> bool {
        gallery_entry.borrow().thumbnail.is_none() || gallery_entry.borrow().is_thumbnail_loading()
    }

    fn refresh_buffer_add(gallery_entry: &Rc<RefCell<GalleryEntry>>) {
        gallery_entry.borrow_mut().is_info_dirty = false;
    }

    fn refresh_buffer_poll(gallery_entry: &Rc<RefCell<GalleryEntry>>) -> bool {
        gallery_entry.borrow().updated_entry_info.is_none() || gallery_entry.borrow().is_refreshing()
    }

    fn filter_entries(&mut self) {
        // let BASE_SEARCH = ;
        let base_search = Config::global().gallery.base_search.clone().unwrap_or(String::new());
        let mut search = self.search_string.clone();
        search.insert_str(0, &format!("{base_search} "));
        let entry_search = EntrySearch::from(search);

        let mut current_index = 0;
        let limit = entry_search.limit.unwrap_or(i64::MAX);
        self.filtered_gallery_entries = self.gallery_entries.as_ref().map(|gallery_entries| {
            gallery_entries
                .iter()
                .filter_map(|gallery_entry| {
                    if let Some(entry_info) = gallery_entry.borrow().entry_info.try_lock() {
                        //TODO: HERE
                        if current_index < limit && entry_info.passes_entry_search(&entry_search) {
                            current_index = current_index + 1;
                            Some(Rc::clone(&gallery_entry))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect()
        });
        self.load_entries();
    }
}

impl ui::UserInterface for GalleryUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        
        self.process_previews(ctx);
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
                            self.render_options(ui, ctx);
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
                                self.render_gallery_entries(ui, ctx);
                            });
                        });
                    });
            });
        });
        self.toasts.show(ctx);
    }
}
