#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use data::ImportationStatus;

// hide console window on Windows in release
use super::super::data;
use super::super::ui;
use super::super::ui::DockedWindow;
use super::super::Config;
use super::import::scan_directory;
use super::import::{import_media, MediaEntry};
use eframe::egui::{self, Button, Direction, ProgressBar, ScrollArea, Ui};
use eframe::emath::{Align, Vec2};
use poll_promise::Promise;
use rfd::FileDialog;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Condvar;
use std::{fs, path::Path};

use std::sync::{Arc, Mutex};
const MAX_CONCURRENT_BYTE_LOADING: u32 = 25;
pub struct ImporterUI {
    config: Option<Arc<Config>>,
    media_entries: Option<Vec<MediaEntry>>,
    alternate_scan_dir: Option<PathBuf>,
    delete_files_on_import: bool,
    show_hidden_entries: bool,
    hide_errored_entries: bool,
    import_hidden_entries: bool,
    page_count: usize,
    page_index: usize,
    scan_extension_filter: HashMap<String, HashMap<String, bool>>,
    dir_link_map: Arc<Mutex<HashMap<String, i32>>>,
}

impl Default for ImporterUI {
    fn default() -> Self {
        let config = None;
        Self {
            delete_files_on_import: false,
            show_hidden_entries: false,
            hide_errored_entries: true,
            import_hidden_entries: true,
            media_entries: None,
            alternate_scan_dir: None,
            config,
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
    fn get_scan_dir(&self) -> PathBuf {
        let landing_result = self.get_config().path.landing();
        let landing = landing_result.unwrap_or_else(|_| PathBuf::from(""));
        if self.alternate_scan_dir.is_some() {
            self.alternate_scan_dir.as_ref().unwrap().clone()
        } else {
            landing
        }
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
                    self.media_entries = None
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
                        for media_entry in self.media_entries.as_ref().unwrap() {
                            if let Some(linking_dir) = &media_entry.linking_dir {}
                        }
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
                                for (extension, do_include) in extensions.iter_mut() {
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
                        if let Some(scanned_dirs) = &mut self.media_entries {
                            for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_importable()) {
                                media_entry.is_selected = true;
                            }
                        }
                    }
        
                    if ui.add_enabled(self.media_entries.is_some(), Button::new("deselect all")).clicked() {
                        if let Some(scanned_dirs) = &mut self.media_entries {
                            for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_importable()) {
                                media_entry.is_selected = false;
                            }
                        }
                    }
        
                    if ui.add_enabled(self.media_entries.is_some(), Button::new("invert")).clicked() {
                        if let Some(scanned_dirs) = &mut self.media_entries {
                            for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_importable()) {
                                media_entry.is_selected = !media_entry.is_selected;
                            }
                        }
                    }
                });
    
                ui.collapsing("view", |ui| {
                    ui.checkbox(&mut self.hide_errored_entries, "hide errored");
                    ui.checkbox(&mut self.show_hidden_entries, "show hidden");
                });
    
    
                ui.collapsing("hide", |ui| {
                    if ui
                        .add_enabled(
                            self.media_entries.is_some() && self.media_entries.as_ref().unwrap().iter().any(|media_entry| media_entry.is_selected),
                            Button::new("hide selected"),
                        )
                        .clicked()
                    {
                        if let Some(scanned_dirs) = &mut self.media_entries {
                            for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_selected) {
                                media_entry.is_hidden = true;
                            }
                        }
                    }
        
                    if ui
                        .add_enabled(
                            self.media_entries.is_some() && self.media_entries.as_ref().unwrap().iter().any(|media_entry| media_entry.is_selected),
                            Button::new("unhide selected"),
                        )
                        .clicked()
                    {
                        if let Some(scanned_dirs) = &mut self.media_entries {
                            for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_selected) {
                                media_entry.is_hidden = false;
                            }
                        }
                    }
        
                    if ui
                        .add_enabled(
                            self.media_entries.is_some() && self.media_entries.as_ref().unwrap().iter().any(|media_entry| media_entry.is_hidden),
                            Button::new("select hidden"),
                        )
                        .clicked()
                    {
                        if let Some(scanned_dirs) = &mut self.media_entries {
                            for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_hidden) {
                                media_entry.is_selected = true;
                            }
                        }
                    }
        
                    if ui
                        .add_enabled(
                            self.media_entries.is_some() && self.media_entries.as_ref().unwrap().iter().any(|media_entry| media_entry.is_hidden),
                            Button::new("deselect hidden"),
                        )
                        .clicked()
                    {
                        if let Some(scanned_dirs) = &mut self.media_entries {
                            for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_hidden) {
                                media_entry.is_selected = false;
                            }
                        }
                    }
                });
    
                ui.collapsing("import", |ui| {
                    ui.checkbox(&mut self.import_hidden_entries, "don't import hidden");
                    ui.checkbox(&mut self.delete_files_on_import, "delete files on import");
        
                    if ui
                        .add_enabled(
                            self.media_entries.is_some() && self.media_entries.as_ref().unwrap().iter().any(|media_entry| media_entry.is_selected),
                            Button::new("import selected"),
                        )
                        .clicked()
                    {
                        if let Some(scanned_dirs) = &mut self.media_entries {
                            let media_entries = scanned_dirs
                                .iter_mut()
                                .filter(|media_entry| media_entry.is_selected)
                                .collect::<Vec<&mut MediaEntry>>();
        
                            for media_entry in media_entries {
                                let (sender, promise) = Promise::new();
                                sender.send(Arc::new(ImportationStatus::PendingBytes));
                                media_entry.importation_status = Some(promise);
                            }
        
                            for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_selected) {
                                media_entry.is_hidden = false;
                                media_entry.is_selected = false;
                            }
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
                        if media_entry.is_hidden && !self.show_hidden_entries {
                            continue;
                        }

                        // display label stuff
                        files_col_scroll.add_enabled_ui(media_entry.is_importable(), |files_col_scroll| {
                            let mut label = media_entry.file_label.clone();
                            let max_len = 30;
                            if label.len() > max_len {
                                label = match label.char_indices().nth(max_len) {
                                    None => label,
                                    Some((idx, _)) => label[..idx].to_string(),
                                };
                                label.push_str("...");
                            }
                            let mut text = egui::RichText::new(format!("{}", label));
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
            if let Some(scanned_dirs) = self.media_entries.as_mut() {
                // iterate through each mediaentry to draw its name on the sidebar, and to load its image
                // wrapped in an arc mutex for multithreading purposes

                ScrollArea::vertical()
                    .id_source("previews_col")
                    .show(previews_col, |previews_col_scroll| {
                        let layout = egui::Layout::from_main_dir_and_cross_align(Direction::LeftToRight, Align::Center).with_main_wrap(true);
                        previews_col_scroll.allocate_ui(Vec2::new(previews_col_scroll.available_size_before_wrap().x, 0.0), |scroll_wrap| {
                            scroll_wrap.with_layout(layout, |scroll_wrap| {
                                scroll_wrap.ctx().request_repaint();
                                let mut num_loading = ImporterUI::get_number_of_loading_bytes(scanned_dirs);
                                for (index, media_entry) in scanned_dirs.iter_mut().enumerate() {
                                    media_entry.unload_bytes_if_unnecessary();

                                    if media_entry.is_hidden && !self.show_hidden_entries {
                                        continue;
                                    }

                                    // Only render previews for this current page
                                    if index < self.page_count * self.page_index || index >= self.page_count * (self.page_index + 1) {
                                        continue;
                                    }

                                    // If this entry still needs to load, let it if possible
                                    if media_entry.is_loading_or_needs_to_load() && !media_entry.are_bytes_loaded() {
                                        if num_loading < MAX_CONCURRENT_BYTE_LOADING {
                                            num_loading += 1;
                                            media_entry.set_load_status(true);
                                        }
                                    }

                                    if media_entry.are_bytes_loaded() {
                                        println!("{:?}", media_entry.file_label)
                                    }

                                    // If this entry needs to load bytes for import, get bytes
                                    if media_entry.bytes.is_none() && media_entry.match_importation_status(ImportationStatus::PendingBytes) {
                                        media_entry.get_bytes();
                                    } else if media_entry.are_bytes_loaded() && media_entry.match_importation_status(ImportationStatus::PendingBytes)
                                    {
                                        // Otherwise if bytes are loaded, start the import
                                        let dir_link_map = Arc::clone(&self.dir_link_map);
                                        import_media(media_entry, dir_link_map, Arc::clone(self.config.as_ref().unwrap()));
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
                                                                    .match_importation_status(data::ImportationStatus::Fail(anyhow::Error::msg("")))
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
                                                                } else if media_entry.match_importation_status(data::ImportationStatus::Success) {
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
        if let Some(scanned_dirs) = &mut self.media_entries {
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
        if self.media_entries.is_some() && progress < 1.0 {
            let progress_bar = ProgressBar::new(progress).text("loading media...").show_percentage();
            ui.add(progress_bar);
        }
    }
}

impl ui::DockedWindow for ImporterUI {
    fn set_config(&mut self, config: Arc<Config>) {
        self.config = Some(config);
    }
    fn get_config(&self) -> Arc<Config> {
        Arc::clone(self.config.as_ref().unwrap())
    }
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            self.render_scan_directory_selection(ui);
            self.render_progress(ui);
        });
        ui.with_layout(egui::Layout::left_to_right(), |ui| {
            self.render_options(ui);
            self.render_files(ui);
            self.render_previews(ui, ctx);
        });
    }
}
