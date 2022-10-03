#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use data::ImportationStatus;

use crate::util::SizedEntryBuffer;

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
    load_buffer: SizedEntryBuffer<MediaEntry>,
    import_buffer: SizedEntryBuffer<MediaEntry>,
}

impl Default for ImporterUI {
    fn default() -> Self {
        let load_buffer = SizedEntryBuffer::new(
            Some(5_000_000),
            None,
            Some(ImporterUI::buffer_add),
            Some(ImporterUI::load_buffer_poll),
            Some(ImporterUI::buffer_entry_size),
        );

        let import_buffer = SizedEntryBuffer::new(
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
                media_entry.load_thumbnail(100); //FIXME: replace w config
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
                if ui.button(if self.media_entries.is_some() { "re-scan" } else { "scan" }).clicked() {
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
                        // if let Some(scanned_dirs) = &mut self.media_entries {
                        // }
                    }

                    if ui.add_enabled(self.media_entries.is_some(), Button::new("deselect all")).clicked() {
                        for media_entry in self.get_importable_media_entries() {
                            media_entry.borrow_mut().is_selected = false;
                        }
                    }

                    if ui.add_enabled(self.media_entries.is_some(), Button::new("invert")).clicked() {
                        for media_entry in self.get_importable_media_entries() {
                            media_entry.borrow_mut().is_selected = !media_entry.borrow().is_selected;
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

                    if ui.add_enabled(self.is_any_entry_selected(), Button::new("import selected")).clicked() {
                        // if let Some(scanned_dirs) = &mut self.media_entries {
                        // IMPORT HERE
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

            let should_start_batch_import = !self.import_buffer.entries.is_empty() && (self.import_buffer.is_full() || no_more_pending_imports);
            if !self.is_importing() && should_start_batch_import {
                let reg_forms = self
                    .import_buffer
                    .entries
                    .iter()
                    .filter_map(|media_entry| {
                        media_entry
                            .borrow_mut()
                            .generate_reg_form(Arc::clone(&self.dir_link_map))
                            .ok()
                    })
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
                ScrollArea::vertical()
                    .id_source("previews_col")
                    .show(ui, |ui| {
                        let layout = egui::Layout::from_main_dir_and_cross_align(Direction::LeftToRight, Align::Center).with_main_wrap(true);
                        ui.allocate_ui(Vec2::new(ui.available_size_before_wrap().x, 0.0), |ui| {
                            ui.with_layout(layout, |ui| {
                                // scroll_wrap.ctx().request_repaint();
                                // let mut num_loading = ImporterUI::get_number_of_loading_bytes(scanned_dirs);
                                for (index, media_entry) in scanned_dirs.iter_mut().enumerate() {
                                    // media_entry.unload_bytes_if_unnecessary();
                                    // if media_entry.is_hidden && !self.show_hidden_entries {
                                    //     continue;
                                    // }

                                    // // Only render previews for this current page
                                    // if index < self.page_count * self.page_index || index >= self.page_count * (self.page_index + 1) {
                                    //     continue;
                                    // }

                                    // // If this entry still needs to load, let it if possible
                                    // if media_entry.is_loading_or_needs_to_load() && !media_entry.are_bytes_loaded() {
                                    //     if num_loading < MAX_CONCURRENT_BYTE_LOADING {
                                    //         num_loading += 1;
                                    //         media_entry.set_load_status(true);
                                    //     }
                                    // }

                                    // // TODO: need to be able to buffer all media entries that wanna load in some sorta global buffer, which has a max bytes capacity
                                    // // if needs to load thumbnail/info, it will go into the bytes loading buffer, load, then exit and finish loading
                                    // // move buffer stuff out of ui function?
                                    // // if needs to import, still use bytes loading buffer
                                    // // import ALSO uses import buffer; collect all media entries with in byte buffer that wanna import every frame and load them
                                    // // while importing, a promise is held, resolves with error or success of import

                                    // // If this entry needs to load bytes for import, get bytes
                                    // if media_entry.bytes.is_none() && media_entry.match_importation_status(ImportationStatus::PendingBytes) {
                                    //     media_entry.get_bytes();
                                    // } else if media_entry.are_bytes_loaded() && media_entry.match_importation_status(ImportationStatus::PendingBytes)
                                    // {
                                    //     // Otherwise if bytes are loaded, start the import
                                    //     let dir_link_map = Arc::clone(&self.dir_link_map);
                                    //     //HERE1 import_media(media_entry, dir_link_map, Arc::clone(self.config.as_ref().unwrap()));
                                    // }

                                    let widget_size = ui::constants::IMPORT_THUMBNAIL_SIZE + 10.;
                                    let widget_size = [widget_size, widget_size];
                                    let mut media_entry = media_entry.borrow_mut();
                                    let file_label = media_entry.file_label.clone();
                                    let is_importable = media_entry.is_importable();
                                    let mime_type = media_entry.mime_type.as_ref();
                                    if let Some(Ok(mime_type)) = mime_type {
                                        // mime type is known, file loaded
                                        let mime_type = mime_type.clone();
                                        match media_entry.thumbnail.as_ref() {
                                            None => {
                                                // nothing, bytes havent been loaded yet
                                                let spinner = egui::Spinner::new();
                                                ui
                                                    .add_sized(widget_size, spinner)
                                                    .on_hover_text(format!("{file_label} [{mime_type}] (loading bytes for thumbnail...)"));
                                            }
                                            Some(promise) => match promise.ready() {
                                                // thumbail has started loading
                                                None => {
                                                    // thumbnail still loading
                                                    let spinner = egui::Spinner::new();
                                                    ui
                                                        .add_sized(widget_size, spinner)
                                                        .on_hover_text(format!("{file_label} [{mime_type}] (loading thumbnail...)"));
                                                }
                                                Some(result) => {
                                                    // thumbail finished attempting to load
                                                    let hidden_tint: (u8, u8, u8) = (220, 220, 220);
                                                    let unloaded_tint: (u8, u8, u8) = (200, 200, 200);
                                                    let import_success_tint: (u8, u8, u8) = (200, 200, 255);
                                                    let import_duplicate_tint: (u8, u8, u8) = (200, 255, 200);
                                                    let import_fail_tint: (u8, u8, u8) = (255, 200, 200);

                                                    let response = match result {
                                                        Ok(image) => {
                                                            // thumbnail available
                                                            if is_importable {
                                                                // button, selectable
                                                                let mut image_button =
                                                                    egui::ImageButton::new(image.texture_id(ctx), image.size_vec2())
                                                                        .selected(media_entry.is_selected);

                                                                if media_entry.is_hidden {
                                                                    image_button = image_button.tint(egui::Color32::from_rgb(
                                                                        hidden_tint.0,
                                                                        hidden_tint.1,
                                                                        hidden_tint.2,
                                                                    ));
                                                                } else if media_entry
                                                                    .match_importation_status(data::ImportationStatus::Fail(anyhow::Error::msg("")))
                                                                {
                                                                    image_button = image_button.tint(egui::Color32::from_rgb(
                                                                        import_fail_tint.0,
                                                                        import_fail_tint.1,
                                                                        import_fail_tint.2,
                                                                    ));
                                                                }

                                                                ui.add_sized(widget_size, image_button)
                                                            } else {
                                                                // label, unselectable
                                                                let mut image = egui::widgets::Image::new(image.texture_id(ctx), image.size_vec2());

                                                                // if !media_entry.try_check_if_is_to_be_loaded() {
                                                                //     image = image.tint(egui::Color32::from_rgb(
                                                                //         unloaded_tint.0,
                                                                //         unloaded_tint.1,
                                                                //         unloaded_tint.2,
                                                                //     ));
                                                                if media_entry.match_importation_status(data::ImportationStatus::Success) {
                                                                    image = image.tint(egui::Color32::from_rgb(
                                                                        import_success_tint.0,
                                                                        import_success_tint.1,
                                                                        import_success_tint.2,
                                                                    ));
                                                                } else if media_entry.match_importation_status(data::ImportationStatus::Duplicate) {
                                                                    image = image.tint(egui::Color32::from_rgb(
                                                                        import_duplicate_tint.0,
                                                                        import_duplicate_tint.1,
                                                                        import_duplicate_tint.2,
                                                                    ));
                                                                }

                                                                ui.add_sized(widget_size, image)
                                                            }
                                                        }
                                                        Err(_error) => {
                                                            // couldn't make thumbnail
                                                            let text = egui::RichText::new("?").size(48.0);
                                                            if is_importable {
                                                                let button = egui::Button::new(text);
                                                                ui.add_sized(widget_size, button)
                                                            } else {
                                                                let label = egui::Label::new(text).sense(egui::Sense::hover());
                                                                ui.add_sized(widget_size, label)
                                                            }
                                                        }
                                                    };
                                                    if is_importable {
                                                        if response.clicked() {
                                                            media_entry.is_selected = !media_entry.is_selected;
                                                        }
                                                    }
                                                    if let Some(status_label) = media_entry.get_status_label() {
                                                        response.on_hover_text(format!("{file_label} [{mime_type}] ({status_label})"));
                                                    } else {
                                                        response.on_hover_text(format!("{file_label} [{mime_type}]"));
                                                    }
                                                }
                                            },
                                        }
                                    } else if let Some(Err(error)) = mime_type {
                                        // unknown file type, file couldn't load
                                        if self.hide_errored_entries {
                                            continue;
                                        }
                                        let text = egui::RichText::new("!").color(egui::Color32::from_rgb(255, 149, 138)).size(48.0);
                                        let label = egui::Label::new(text).sense(egui::Sense::hover());
                                        ui
                                            .add_sized(widget_size, label)
                                            .on_hover_text(format!("{file_label} ({error})",));
                                    } else if mime_type.is_none() {
                                        // bytes not yet loaded
                                        // let is_to_be_loaded = media_entry.try_check_if_is_to_be_loaded();

                                        let spinner = egui::Spinner::new();
                                        ui
                                            .add_sized(widget_size, spinner)
                                            .on_hover_text(format!("{file_label} [?] (loading bytes for reading...)"));
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

            // ScrollArea::vertical().show(ui, |ui| {
            //     ui.with_layout(egui::Layout::top_down(egui::Align::Center).with_main_wrap(true), |ui| {
            //         ctx.inspection_ui(ui)
            //     });
            // });
            self.render_previews(ui, ctx);
        });
        self.toasts.show(ctx);
    }
}
