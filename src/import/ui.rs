#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use super::super::data;
use super::super::Config;
use super::import::scan_directory;
use super::import::{import_media, MediaEntry};
use eframe::egui::{self, Button, Direction, ProgressBar, ScrollArea, Ui};
use eframe::emath::{Align, Vec2};
use rfd::FileDialog;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Condvar;
use std::{fs, path::Path};

use std::sync::{Arc, Mutex};

pub fn launch(config: Arc<Config>) {
    let mut options = eframe::NativeOptions::default();
    options.initial_window_size = Some(Vec2::new(1390.0, 600.0));

    let mut app = ImporterUI::default();
    app.set_config(config);
    eframe::run_native("htool2", options, Box::new(|_cc| Box::new(app)));
}

struct ImporterUI {
    config: Option<Arc<Config>>,
    scanned_dir_entries: Option<Vec<MediaEntry>>,
    alternate_scan_dir: Option<PathBuf>,
    delete_files_on_import: bool,
    show_hidden_entries: bool,
    hide_errored_entries: bool,
    import_hidden_entries: bool,
    scan_chunk_size: u16,
    scan_chunk_indices: (i32, i32),
    dir_link_map: Arc<Mutex<HashMap<String, i32>>>,
}

impl Default for ImporterUI {
    fn default() -> Self {
        let config = None;
        let scan_chunk_size: u16 = 1000;
        Self {
            scan_chunk_indices: (0, scan_chunk_size as i32),
            scan_chunk_size,
            delete_files_on_import: false,
            show_hidden_entries: false,
            hide_errored_entries: true,
            import_hidden_entries: true,
            scanned_dir_entries: None,
            alternate_scan_dir: None,
            config,
            dir_link_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl ImporterUI {
    fn get_scan_dir(&self) -> PathBuf {
        let landing_result = self.get_config().path.landing();
        let landing = landing_result.unwrap_or_else(|_| PathBuf::from(""));
        if self.alternate_scan_dir.is_some() {
            self.alternate_scan_dir.as_ref().unwrap().clone()
        } else {
            landing
        }
    }

    fn set_config(&mut self, config: Arc<Config>) {
        self.config = Some(config);
    }

    fn get_config(&self) -> &Config {
        self.config.as_ref().unwrap()
    }

    fn render_scan_directory_selection(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading("scan directory");
            if ui.button("change").clicked() {
                if let Some(path) = FileDialog::new().pick_folder() {
                    self.alternate_scan_dir = Some(path);
                    self.scanned_dir_entries = None
                }
            }
            if self.alternate_scan_dir.as_ref().is_some() && ui.button("remove").clicked() {
                self.alternate_scan_dir = None;
                self.scanned_dir_entries = None
            }
            ui.label(format!("{}", self.get_scan_dir().display()));
        });
    }

    fn render_options(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.heading("options");
            if ui.button(if self.scanned_dir_entries.is_some() { "re-scan" } else { "scan" }).clicked() {
                self.scan_chunk_indices = (0, self.scan_chunk_size.into());
                let media_entries = scan_directory(self.get_scan_dir(), Some(self.scan_chunk_indices), 0, None);
                if let Ok(media_entries) = media_entries {
                    self.scanned_dir_entries = Some(media_entries);
                    for media_entry in self.scanned_dir_entries.as_ref().unwrap() {
                        if let Some(linking_dir) = &media_entry.linking_dir {

                        }
                    }
                }
            }
            if ui.add_enabled(self.scanned_dir_entries.is_some(), Button::new("prev chunk")).clicked() {
                let chunk_size = self.scan_chunk_size as i32;
                self.scan_chunk_indices.0 = (self.scan_chunk_indices.0 - chunk_size).max(0);
                self.scan_chunk_indices.1 = (self.scan_chunk_indices.1 - chunk_size).max(chunk_size);
                self.update_chunk();
            }
            if ui.add_enabled(self.scanned_dir_entries.is_some(), Button::new("next chunk")).clicked() {
                let chunk_size = self.scan_chunk_size as i32;
                let total_count = self.scanned_dir_entries.as_ref().unwrap().len() as i32;
                self.scan_chunk_indices.0 = (self.scan_chunk_indices.0 + chunk_size).min(total_count - chunk_size);
                self.scan_chunk_indices.1 = (self.scan_chunk_indices.1 + chunk_size).min(total_count);
                self.update_chunk();
            }
            if ui
                .add(
                    egui::DragValue::new(&mut self.scan_chunk_size)
                        .speed(100)
                        .clamp_range(10..=10000)
                        .prefix("chunk: "),
                )
                .changed()
            {
                self.scan_chunk_indices.1 = self.scan_chunk_indices.0 + self.scan_chunk_size as i32
            };
            if ui.add_enabled(self.scanned_dir_entries.is_some(), Button::new("select all")).clicked() {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_importable()) {
                        media_entry.is_selected = true;
                    }
                }
            }

            if ui.add_enabled(self.scanned_dir_entries.is_some(), Button::new("deselect all")).clicked() {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_importable()) {
                        media_entry.is_selected = false;
                    }
                }
            }

            if ui.add_enabled(self.scanned_dir_entries.is_some(), Button::new("invert")).clicked() {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_importable()) {
                        media_entry.is_selected = !media_entry.is_selected;
                    }
                }
            }

            ui.checkbox(&mut self.hide_errored_entries, "hide errored");
            ui.checkbox(&mut self.show_hidden_entries, "show hidden");
            ui.checkbox(&mut self.import_hidden_entries, "don't import hidden");
            ui.checkbox(&mut self.delete_files_on_import, "delete files on import");

            if ui
                .add_enabled(
                    self.scanned_dir_entries.is_some()
                        && self
                            .scanned_dir_entries
                            .as_ref()
                            .unwrap()
                            .iter()
                            .any(|media_entry| media_entry.is_selected),
                    Button::new("hide selected"),
                )
                .clicked()
            {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_selected) {
                        media_entry.is_hidden = true;
                    }
                }
            }

            if ui
                .add_enabled(
                    self.scanned_dir_entries.is_some()
                        && self
                            .scanned_dir_entries
                            .as_ref()
                            .unwrap()
                            .iter()
                            .any(|media_entry| media_entry.is_selected),
                    Button::new("unhide selected"),
                )
                .clicked()
            {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_selected) {
                        media_entry.is_hidden = false;
                    }
                }
            }

            if ui
                .add_enabled(
                    self.scanned_dir_entries.is_some() && self.scanned_dir_entries.as_ref().unwrap().iter().any(|media_entry| media_entry.is_hidden),
                    Button::new("select hidden"),
                )
                .clicked()
            {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_hidden) {
                        media_entry.is_selected = true;
                    }
                }
            }

            if ui
                .add_enabled(
                    self.scanned_dir_entries.is_some() && self.scanned_dir_entries.as_ref().unwrap().iter().any(|media_entry| media_entry.is_hidden),
                    Button::new("deselect hidden"),
                )
                .clicked()
            {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_hidden) {
                        media_entry.is_selected = false;
                    }
                }
            }

            if ui
                .add_enabled(
                    self.scanned_dir_entries.is_some()
                        && self
                            .scanned_dir_entries
                            .as_ref()
                            .unwrap()
                            .iter()
                            .any(|media_entry| media_entry.is_selected),
                    Button::new("import selected"),
                )
                .clicked()
            {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    let media_entries = scanned_dirs
                        .iter_mut()
                        .filter(|media_entry| media_entry.is_selected)
                        .collect::<Vec<&mut MediaEntry>>();

                    let dir_link_map = Arc::clone(&self.dir_link_map);
                    let import_result = import_media(media_entries, dir_link_map, Arc::clone(self.config.as_ref().unwrap()));
                    println!("{:?}", import_result);
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_selected) {
                        // media_entry.is_imported = true; // TODO: figure out how to convey importaton status
                        media_entry.is_hidden = false;
                        media_entry.is_selected = false;
                    }
                }
            }
            egui::widgets::global_dark_light_mode_switch(ui);
        });
    }

    fn update_chunk(&mut self) {
        if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
            for (index, media_entry) in scanned_dirs.iter_mut().enumerate() {
                let index = index as i32;
                let is_to_be_loaded = if self.scan_chunk_indices.0 <= index && index < self.scan_chunk_indices.1 {
                    true
                } else {
                    false
                };
                media_entry.set_load_status(is_to_be_loaded);
            }
        }
    }

    fn render_files(&mut self, ui: &mut Ui) {
        ui.vertical(|files_col| {
            files_col.label("file");
            if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                ScrollArea::vertical().id_source("files_col").show(files_col, |files_col_scroll| {
                    for media_entry in scanned_dirs.iter_mut() {
                        if media_entry.is_hidden && !self.show_hidden_entries {
                            continue;
                        }

                        // display label stuff
                        files_col_scroll.add_enabled_ui(media_entry.is_importable(), |files_col_scroll| {
                            let mut text = egui::RichText::new(format!("{}", media_entry.file_label));
                            if media_entry.is_hidden {
                                text = text.color(egui::Color32::from_rgb(255, 150, 150));
                            }
                            let mut response = files_col_scroll.selectable_label(media_entry.is_selected, text);
                            if response.clicked() {
                                media_entry.is_selected = !media_entry.is_selected;
                            };
                            let disabled_reason = media_entry.get_status_label();
                            if media_entry.is_hidden {
                                response = response.on_hover_text("(hidden)");
                            }
                            response.on_disabled_hover_text(format!("({})", disabled_reason.unwrap_or("unknown error".to_string())));
                        });
                    }
                });
            }
        });
    }

    fn render_previews(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.vertical(|previews_col| {
            previews_col.label("preview");

            let thumbnail_size = self.get_config().ui.import.thumbnail_size;
            if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                // iterate through each mediaentry to draw its name on the sidebar, and to load its image
                // wrapped in an arc mutex for multithreading purposes
                ScrollArea::vertical()
                    .id_source("previews_col")
                    .show(previews_col, |previews_col_scroll| {
                        let layout = egui::Layout::from_main_dir_and_cross_align(Direction::LeftToRight, Align::Center).with_main_wrap(true);
                        previews_col_scroll.allocate_ui(Vec2::new(previews_col_scroll.available_size_before_wrap().x, 0.0), |scroll_wrap| {
                            scroll_wrap.with_layout(layout, |scroll_wrap| {
                                for media_entry in scanned_dirs.iter_mut() {
                                    if media_entry.is_hidden && !self.show_hidden_entries {
                                        continue;
                                    }
                                    let widget_size = (thumbnail_size + 10) as f32;
                                    let widget_size = [widget_size, widget_size];
                                    let file_label = media_entry.file_label.clone();
                                    let is_importable = media_entry.is_importable();
                                    let mime_type = media_entry.get_mime_type();
                                    if let Some(Ok(mime_type)) = mime_type {
                                        // mime type is known, file loaded
                                        let mime_type = mime_type.clone();
                                        match media_entry.get_thumbnail(thumbnail_size) {
                                            None => {
                                                // nothing, bytes havent been loaded yet
                                                let spinner = egui::Spinner::new();
                                                scroll_wrap
                                                    .add_sized(widget_size, spinner)
                                                    .on_hover_text(format!("{file_label} [{mime_type}] (loading bytes for thumbnail...)"));
                                            }
                                            Some(promise) => match promise.ready() {
                                                // thumbail has started loading
                                                None => {
                                                    // thumbnail still loading
                                                    let spinner = egui::Spinner::new();
                                                    scroll_wrap
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
                                                                    .match_importation_status(data::ImportationResult::Fail(anyhow::Error::msg("")))
                                                                {
                                                                    image_button = image_button.tint(egui::Color32::from_rgb(
                                                                        import_fail_tint.0,
                                                                        import_fail_tint.1,
                                                                        import_fail_tint.2,
                                                                    ));
                                                                }

                                                                scroll_wrap.add_sized(widget_size, image_button)
                                                            } else {
                                                                // label, unselectable
                                                                let mut image = egui::widgets::Image::new(image.texture_id(ctx), image.size_vec2());

                                                                if !media_entry.try_check_if_is_to_be_loaded() {
                                                                    image = image.tint(egui::Color32::from_rgb(
                                                                        unloaded_tint.0,
                                                                        unloaded_tint.1,
                                                                        unloaded_tint.2,
                                                                    ));
                                                                } else if media_entry.match_importation_status(data::ImportationResult::Success) {
                                                                    image = image.tint(egui::Color32::from_rgb(
                                                                        import_success_tint.0,
                                                                        import_success_tint.1,
                                                                        import_success_tint.2,
                                                                    ));
                                                                } else if media_entry.match_importation_status(data::ImportationResult::Duplicate) {
                                                                    image = image.tint(egui::Color32::from_rgb(
                                                                        import_duplicate_tint.0,
                                                                        import_duplicate_tint.1,
                                                                        import_duplicate_tint.2,
                                                                    ));
                                                                }

                                                                scroll_wrap.add_sized(widget_size, image)
                                                            }
                                                        }
                                                        Err(error) => {
                                                            // couldn't make thumbnail
                                                            let text = egui::RichText::new("?").size(48.0);
                                                            if is_importable {
                                                                let button = egui::Button::new(text);
                                                                scroll_wrap.add_sized(widget_size, button)
                                                            } else {
                                                                let label = egui::Label::new(text).sense(egui::Sense::hover());
                                                                scroll_wrap.add_sized(widget_size, label)
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
                                        scroll_wrap
                                            .add_sized(widget_size, label)
                                            .on_hover_text(format!("{file_label} ({error})",));
                                    } else if mime_type.is_none() {
                                        // bytes not yet loaded
                                        let is_to_be_loaded = media_entry.try_check_if_is_to_be_loaded();

                                        if is_to_be_loaded {
                                            let spinner = egui::Spinner::new();
                                            scroll_wrap
                                                .add_sized(widget_size, spinner)
                                                .on_hover_text(format!("{file_label} [?] (loading bytes for reading...)"));
                                        } else {
                                            let text = egui::RichText::new("...").color(egui::Color32::from_rgb(252, 229, 124)).size(48.0);
                                            let label = egui::Label::new(text).sense(egui::Sense::hover());
                                            scroll_wrap
                                                .add_sized(widget_size, label)
                                                .on_hover_text(format!("{file_label} [?] (not yet scanned)"));
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
        if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
            let mut loaded_count: f32 = 0.0;
            let total_count = scanned_dirs.len() as f32;
            for media_entry in scanned_dirs.iter_mut() {
                if media_entry.get_mime_type().is_some() {
                    loaded_count += 1.0;
                }
            }
            progress = loaded_count / total_count;
        }

        progress
    }

    fn render_progress(&mut self, ui: &mut Ui) {
        let progress = self.get_load_progress();
        if self.scanned_dir_entries.is_some() && progress < 1.0 {
            let progress_bar = ProgressBar::new(progress).text("loading media...").show_percentage();
            ui.add(progress_bar);
        }
    }
}

impl eframe::App for ImporterUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                self.render_scan_directory_selection(ui);
                self.render_progress(ui);
            });
            ui.with_layout(egui::Layout::left_to_right(), |ui| {
                self.render_options(ui);
                self.render_files(ui);
                self.render_previews(ui, ctx);
            });
        });
    }
}
