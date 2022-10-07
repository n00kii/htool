#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use data::ImportationStatus;

use crate::util::PollBuffer;

// hide console window on Windows in release
use super::super::data;
use super::super::ui;
use super::super::ui::UserInterface;
use super::super::Config;
use super::import::scan_directory;
// use super::import::{import_media, MediaEntry};
use super::import::MediaEntry;
use anyhow::Result;
use eframe::egui::{self, Button, Direction, ProgressBar, ScrollArea, Ui};
use eframe::emath::{Align, Vec2};
use poll_promise::Promise;
use rfd::FileDialog;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Condvar;
use std::time::Duration;
use std::{fs, path::Path};

use std::sync::{Arc, Mutex};
const MAX_CONCURRENT_BYTE_LOADING: u32 = 25;
pub struct ImporterUI {
    toasts: egui_notify::Toasts,
    media_entries: Option<Vec<Rc<RefCell<MediaEntry>>>>,
    alternate_scan_dir: Option<PathBuf>,
    delete_files_on_import: bool,
    show_hidden_entries: bool,
    hide_errored_entries: bool,
    import_hidden_entries: bool,
    current_import_res: Option<Promise<Result<()>>>,
    page_count: usize,
    page_index: usize,
    scan_extension_filter: HashMap<String, HashMap<String, bool>>,
    batch_import_status: Option<Promise<Arc<Result<()>>>>,
    dir_link_map: Arc<Mutex<HashMap<String, i32>>>,
    load_buffer: PollBuffer<MediaEntry>,
    import_buffer: PollBuffer<MediaEntry>,
}

impl Default for ImporterUI {
    fn default() -> Self {
        let load_buffer = PollBuffer::new(
            Some(5_000_000),
            None,
            Some(ImporterUI::buffer_add),
            Some(ImporterUI::load_buffer_poll),
            Some(ImporterUI::buffer_entry_size),
        );

        let import_buffer = PollBuffer::new(
            Some(10_000_000),
            Some(10),
            Some(ImporterUI::buffer_add),
            Some(ImporterUI::import_buffer_poll),
            Some(ImporterUI::buffer_entry_size),
        );

        Self {
            toasts: egui_notify::Toasts::default().with_anchor(egui_notify::Anchor::BottomLeft),
            delete_files_on_import: false,
            show_hidden_entries: false,
            load_buffer,
            import_buffer,
            hide_errored_entries: true,
            import_hidden_entries: true,
            batch_import_status: None,
            media_entries: None,
            alternate_scan_dir: None,
            current_import_res: None,
            page_count: 1000,
            page_index: 0,
            scan_extension_filter: HashMap::from([
                (
                    "image".into(),
                    HashMap::from([("png".into(), true), ("jpg".into(), true), ("webp".into(), true)]),
                ),
                (
                    "video".into(),
                    HashMap::from([("gif".into(), true), ("mp4".into(), true), ("webm".into(), true)]),
                ),
                ("archive".into(), HashMap::from([("zip".into(), true), ("rar".into(), true)])),
            ]),

            dir_link_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl ImporterUI {
    fn buffer_add(media_entry: &Rc<RefCell<MediaEntry>>) {
        if media_entry.borrow().bytes.is_none() {
            media_entry.borrow_mut().load_bytes();
        }
    }

    fn load_buffer_poll(media_entry: &Rc<RefCell<MediaEntry>>) -> bool {
        let mut media_entry = media_entry.borrow_mut();
        if media_entry.are_bytes_loaded() {
            if media_entry.thumbnail.is_none() {
                media_entry.load_thumbnail(); //FIXME: replace w config
            }
            if media_entry.mime_type.is_none() {
                media_entry.load_mime_type();
            }
        }
        media_entry.is_loading_or_needs_to_load()
    }
    fn buffer_entry_size(media_entry: &Rc<RefCell<MediaEntry>>) -> usize {
        media_entry.borrow().file_size
    }

    fn import_buffer_poll(media_entry: &Rc<RefCell<MediaEntry>>) -> bool {
        media_entry.borrow().is_importing()
        // true
    }

    fn get_scan_dir(&self) -> PathBuf {
        let landing_result = Config::global().path.landing();
        let landing = landing_result.unwrap_or_else(|_| PathBuf::from(""));
        if self.alternate_scan_dir.is_some() {
            self.alternate_scan_dir.as_ref().unwrap().clone()
        } else {
            landing
        }
    }

    fn is_importing(&self) -> bool {
        match self.batch_import_status.as_ref() {
            None => false,
            Some(import_promise) => match import_promise.ready() {
                Some(import_res) => false,
                None => true,
            },
        }
    }

    fn is_any_entry_selected(&self) -> bool {
        self.get_selected_media_entries().len() > 0
    }

    fn is_any_entry_hidden(&self) -> bool {
        self.get_hidden_media_entries().len() > 0
    }

    fn filter_media_entries(&self, predicate: impl Fn(&Rc<RefCell<MediaEntry>>) -> bool) -> Vec<Rc<RefCell<MediaEntry>>> {
        if let Some(media_entries) = self.media_entries.as_ref() {
            media_entries
                .iter()
                .filter(|media_entry| predicate(media_entry))
                .map(|media_entry| Rc::clone(&media_entry))
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    }

    fn get_importable_media_entries(&self) -> Vec<Rc<RefCell<MediaEntry>>> {
        self.filter_media_entries(|media_entry| media_entry.borrow().is_importable())
    }

    fn get_selected_media_entries(&self) -> Vec<Rc<RefCell<MediaEntry>>> {
        self.filter_media_entries(|media_entry| media_entry.borrow().is_selected)
    }

    fn get_hidden_media_entries(&self) -> Vec<Rc<RefCell<MediaEntry>>> {
        self.filter_media_entries(|media_entry| media_entry.borrow().is_hidden)
    }

    fn get_number_of_loading_bytes(media_entries: &Vec<MediaEntry>) -> u32 {
        let mut num_loading = 0;
        for media_entry in media_entries {
            if media_entry.are_bytes_loaded() {
                num_loading += 1;
            }
        }

        num_loading
    }

    fn render_scan_directory_selection(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("scan directory");
            if ui.button("change").clicked() {
                if let Some(path) = FileDialog::new().pick_folder() {
                    self.alternate_scan_dir = Some(path);
                    self.media_entries = None;
                    self.load_buffer.entries.clear();
                    self.import_buffer.entries.clear();
                }
            }
            if self.alternate_scan_dir.as_ref().is_some() && ui.button("remove").clicked() {
                self.alternate_scan_dir = None;
                self.media_entries = None
            }
            ui.label(format!("{}", self.get_scan_dir().display()));
        });
    }

    fn render_options(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.label("options");
            ScrollArea::vertical().id_source("options").show(ui, |ui| {
                if ui.button(format!("{} {}", ui::constants::SEARCH_ICON, if self.media_entries.is_some() { "re-scan" } else { "scan" })).clicked() {
                    let mut extension_filter = vec![];
                    for (_extension_group, extensions) in &self.scan_extension_filter {
                        for (extension, do_include) in extensions {
                            if !do_include {
                                extension_filter.push(extension)
                            }
                        }
                    }
                    let media_entries = scan_directory(self.get_scan_dir(), 0, None, &extension_filter);
                    if let Ok(media_entries) = media_entries {
                        self.media_entries = Some(media_entries);
                    }
                }

                ui.collapsing("filters", |ui| {
                    if self.media_entries.is_some() {
                        let text = egui::RichText::new("rescan to apply filters").color(egui::Color32::RED);
                        ui.label(text);
                    }

                    egui::Grid::new("filters").max_col_width(100.).show(ui, |ui| {
                        for (extension_group, extensions) in self.scan_extension_filter.iter_mut() {
                            let mut any_selected = extensions.values().any(|&x| x);
                            if ui.checkbox(&mut any_selected, extension_group).changed() {
                                for (_extension, do_include) in extensions.iter_mut() {
                                    *do_include = any_selected;
                                }
                            }

                            ui.vertical(|ui| {
                                for (extension, do_include) in extensions.iter_mut() {
                                    ui.checkbox(do_include, extension);
                                }
                            });
                            ui.end_row();
                        }
                    });
                });

                ui.collapsing("page", |ui| {
                    ui.horizontal(|ui| {
                        if ui.add_enabled(self.page_index > 0, Button::new("<")).clicked() {
                            self.page_index -= 1;
                        }
                        ui.label(format!("{}", self.page_index));

                        if ui
                            .add_enabled(
                                !(((self.page_index + 1) * self.page_count)
                                    >= if let Some(media_entries) = self.media_entries.as_ref() {
                                        media_entries.len()
                                    } else {
                                        0
                                    }),
                                Button::new(">"),
                            )
                            .clicked()
                        {
                            self.page_index += 1;
                        }
                    });
                    ui.add(
                        egui::DragValue::new(&mut self.page_count)
                            .speed(100)
                            .clamp_range(10..=10000)
                            .prefix("page count: "),
                    );
                });

                ui.collapsing("selection", |ui| {
                    if ui.add_enabled(self.media_entries.is_some(), Button::new("select all")).clicked() {
                        for media_entry in self.get_importable_media_entries() {
                            media_entry.borrow_mut().is_selected = true;
                        }
                    }

                    if ui.add_enabled(self.media_entries.is_some(), Button::new("deselect all")).clicked() {
                        for media_entry in self.get_importable_media_entries() {
                            media_entry.borrow_mut().is_selected = false;
                        }
                    }

                    if ui.add_enabled(self.media_entries.is_some(), Button::new("invert")).clicked() {
                        for media_entry in self.get_importable_media_entries() {
                            let current_state = media_entry.borrow().is_selected;
                            media_entry.borrow_mut().is_selected = !current_state;
                        }
                    }
                });

                ui.collapsing("view", |ui| {
                    ui.checkbox(&mut self.hide_errored_entries, "hide errored");
                    ui.checkbox(&mut self.show_hidden_entries, "show hidden");
                });

                ui.collapsing("hide", |ui| {
                    if ui.add_enabled(self.is_any_entry_selected(), Button::new("hide selected")).clicked() {
                        for media_entry in self.get_selected_media_entries() {
                            media_entry.borrow_mut().is_hidden = true;
                        }
                    }

                    if ui.add_enabled(self.is_any_entry_selected(), Button::new("unhide selected")).clicked() {
                        for media_entry in self.get_selected_media_entries() {
                            media_entry.borrow_mut().is_hidden = false;
                        }
                    }

                    if ui.add_enabled(self.is_any_entry_hidden(), Button::new("select hidden")).clicked() {
                        for media_entry in self.get_selected_media_entries() {
                            media_entry.borrow_mut().is_selected = true;
                        }
                    }

                    if ui.add_enabled(self.is_any_entry_hidden(), Button::new("deselect hidden")).clicked() {
                        for media_entry in self.get_hidden_media_entries() {
                            media_entry.borrow_mut().is_selected = false;
                        }
                    }
                });

                ui.collapsing("import", |ui| {
                    ui.checkbox(&mut self.import_hidden_entries, "don't import hidden");
                    ui.checkbox(&mut self.delete_files_on_import, "delete files on import");

                    if ui.add_enabled(self.is_any_entry_selected(), ui::suggested_button(format!("{} import selected", ui::constants::IMPORT_ICON))).clicked() {
                        let selected_media_entries = self.get_selected_media_entries();
                        ui::toast_info(&mut self.toasts, format!("marked {} media for importing", selected_media_entries.len()));
                        for media_entry in self.get_selected_media_entries() {
                            media_entry.borrow_mut().importation_status = Some(Promise::from_ready(ImportationStatus::Pending));
                            media_entry.borrow_mut().is_hidden = false;
                            media_entry.borrow_mut().is_selected = false;
                        }
                    }
                });
            });
        });
    }

    fn render_files(&mut self, ui: &mut Ui) {
        ui.vertical(|files_col| {
            files_col.label("file");
            if let Some(scanned_dirs) = &mut self.media_entries {
                ScrollArea::vertical().id_source("files_col").show(files_col, |files_col_scroll| {
                    for media_entry in scanned_dirs.iter_mut() {
                        if media_entry.borrow().is_hidden && !self.show_hidden_entries {
                            continue;
                        }
                        // display label stuff
                        files_col_scroll.add_enabled_ui(true, |files_col_scroll| {
                            let mut label = media_entry.borrow().file_label.clone();
                            let max_len = 30;
                            if label.len() > max_len {
                                label = match label.char_indices().nth(max_len) {
                                    None => label,
                                    Some((idx, _)) => label[..idx].to_string(),
                                };
                                label.push_str("...");
                            }
                            let mut text = egui::RichText::new(format!("{}", label));
                            if media_entry.borrow().is_hidden {
                                text = text.color(egui::Color32::from_rgb(255, 150, 150));
                            }
                            let mut response = files_col_scroll.selectable_label(media_entry.borrow().is_selected, text);
                            if response.clicked() {
                                media_entry.borrow_mut().is_selected = !media_entry.borrow().is_selected;
                            };
                            let disabled_reason = media_entry.borrow().get_status_label();
                            if media_entry.borrow().is_hidden {
                                response = response.on_hover_text("(hidden)");
                            }
                            response.on_disabled_hover_text(format!("({})", disabled_reason.unwrap_or("unknown error".to_string())));
                        });
                    }
                });
            }
        });
    }

    fn process_media(&mut self) {
        if let Some(media_entries) = self.media_entries.as_ref() {
            let mut no_more_pending_imports = true;
            for media_entry in media_entries {
                if media_entry.borrow().is_loading_or_needs_to_load() {
                    let _ = self.load_buffer.try_add_entry(Rc::clone(&media_entry));
                }
                if media_entry.borrow().match_importation_status(ImportationStatus::Pending) {
                    // if something didn't get added to the buffer (buffer full), there are still pending imports
                    if let Err(e) = self.import_buffer.try_add_entry(Rc::clone(media_entry)) {
                        if self.import_buffer.is_full() {
                            no_more_pending_imports = false;
                        }
                    }
                }
            }
            //FIXME: media importing a lot???
            let should_start_batch_import = !self.import_buffer.entries.is_empty() && (self.import_buffer.is_full() || no_more_pending_imports) ;
            let mut clear_batch_import_status = false;
            if let Some(batch_import_promise) = self.batch_import_status.as_ref() {
                if let Some(batch_import_res) = batch_import_promise.ready() {
                    clear_batch_import_status = true;
                    match &**batch_import_res {
                        Ok(()) => ui::toast_success(&mut self.toasts, "susc"),
                        Err(error) => ui::toast_success(&mut self.toasts, format!("failed to import media: {error}")),
                    }
                }
            }
            if clear_batch_import_status {
                self.batch_import_status = None;
            }

            if !self.is_importing() && should_start_batch_import {
                let reg_forms = self
                    .import_buffer
                    .entries
                    .iter()
                    .filter_map(|media_entry| media_entry.borrow_mut().generate_reg_form(Arc::clone(&self.dir_link_map)).ok())
                    .collect::<Vec<_>>();

                self.batch_import_status = Some(Promise::spawn_thread("", || Arc::new(data::register_media_with_forms(reg_forms))));
            }
            self.load_buffer.poll();
            self.import_buffer.poll();
        }
    }

    fn render_previews(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.label("preview");

            if let Some(scanned_dirs) = self.media_entries.as_mut() {
                // iterate through each mediaentry to draw its name on the sidebar, and to load its image
                // wrapped in an arc mutex for multithreading purposes
                ScrollArea::vertical().id_source("previews_col").auto_shrink([false, false]).show(ui, |ui| {
                    let layout = egui::Layout::from_main_dir_and_cross_align(Direction::LeftToRight, Align::Center).with_main_wrap(true);
                    ui.allocate_ui(Vec2::new(ui.available_size_before_wrap().x, 0.0), |ui| {
                        ui.with_layout(layout, |ui| {
                            for media_entry in scanned_dirs.iter() {
                                let mut media_entry = media_entry.borrow_mut();
                                let file_label = media_entry.file_label.clone();
                                let file_label_clone = file_label.clone();
                                let mime_type_label = match media_entry.mime_type.as_ref() {
                                    Some(Ok(mime_type)) => mime_type.to_string(),
                                    Some(Err(err)) => format!("failed to read file type: {err}"),
                                    None => String::from("?"),
                                };

                                let mut options = ui::RenderLoadingImageOptions::default();
                                let thumbnail_size = Config::global().ui.import.thumbnail_size as f32;
                                options.widget_margin = [10., 10.];
                                options.desired_image_size = [thumbnail_size, thumbnail_size];
                                options.hover_text_on_loading_image =
                                    Some(format!("{file_label} [{mime_type_label}] (loading thumbnail...)",).into());
                                options.hover_text_on_error_image = Some(Box::new(move |error| format!("{file_label_clone} ({error})").into()));
                                options.hover_text_on_none_image = Some(format!("{file_label} (waiting to load image...)").into());
                                options.hover_text = Some(if let Some(status_label) = media_entry.get_status_label() {
                                    format!("{file_label} [{mime_type_label}] ({status_label})").into()
                                } else {
                                    format!("{file_label} [{mime_type_label}]").into()
                                });
                                options.image_tint = if media_entry.is_hidden {
                                    Some(ui::constants::IMPORT_IMAGE_HIDDEN_TINT)
                                } else if media_entry.match_importation_status(data::ImportationStatus::Success) {
                                    Some(ui::constants::IMPORT_IMAGE_SUCCESS_TINT)
                                } else if media_entry.match_importation_status(data::ImportationStatus::Fail(anyhow::Error::msg(""))) {
                                    Some(ui::constants::IMPORT_IMAGE_FAIL_TINT)
                                } else if media_entry.match_importation_status(data::ImportationStatus::Duplicate) {
                                    Some(ui::constants::IMPORT_IMAGE_DUPLICATE_TINT)
                                } else {
                                    None
                                };
                                options.is_button = media_entry.is_importable();
                                options.is_button_selected = Some(media_entry.is_selected);
                                let response = ui::render_loading_image(ui, ctx, media_entry.thumbnail.as_ref(), options);
                                if let Some(response) = response.as_ref() {
                                    if response.clicked() {
                                        media_entry.is_selected = !media_entry.is_selected
                                    }
                                }
                            }
                        });
                    });
                });
            }
        });
    }

    fn get_load_progress(&mut self) -> f32 {
        let mut progress = 0.0;
        if let Some(scanned_dirs) = &mut self.media_entries {
            let mut loaded_count: f32 = 0.0;
            let total_count = scanned_dirs.len() as f32;
            for media_entry in scanned_dirs.iter_mut() {
                if media_entry.borrow().mime_type.is_some() {
                    loaded_count += 1.0;
                }
            }
            progress = loaded_count / total_count;
        }

        progress
    }

    fn render_progress(&mut self, ui: &mut Ui) {
        let progress = self.get_load_progress();
        if self.media_entries.is_some() && progress < 1.0 {
            let progress_bar = ProgressBar::new(progress).text("loading media...").show_percentage();
            ui.add(progress_bar);
        }
    }
}

impl ui::UserInterface for ImporterUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.process_media();

        ui.horizontal(|ui| {
            self.render_scan_directory_selection(ui);
            self.render_progress(ui);
        });
        ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
            self.render_options(ui);
            self.render_files(ui);
            self.render_previews(ui, ctx);
        });
        self.toasts.show(ctx);
    }
}
