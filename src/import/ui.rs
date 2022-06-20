#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::emath::{Align, Vec2};
use rfd::FileDialog;
use std::path::PathBuf;
use std::{path::Path, fs};
use std::sync::Condvar;

use super::super::Config;
use eframe::egui::{self, Button, Direction, ScrollArea, Ui, ProgressBar};
use super::logic::{MediaEntry, import_media};

use std::{
    sync::{Arc, Mutex},
};

pub fn launch(config: Config) {
    let mut options = eframe::NativeOptions::default();
    options.initial_window_size = Some(Vec2::new(1350.0, 600.0));

    let mut app = ImporterUI::default();
    app.set_config(config);
    eframe::run_native(
        "htool2",
        options,
        Box::new(|_cc| Box::new(app)),
    );
}

struct ImporterUI {
    config: Option<Config>,
    scanned_dir_entries: Option<Vec<MediaEntry>>,
    alternate_scan_dir: Option<PathBuf>,
    delete_files_on_import: bool,
    hide_ignored_entries: bool,
    hide_errored_entries: bool,
    skip_importing_ignored: bool,
    scan_chunk_size: u16,
    scan_chunk_indices: (i32, i32),
}

impl Default for ImporterUI {
    fn default() -> Self {
        let config = None;
        let scan_chunk_size: u16 = 1000;
        Self {
            scan_chunk_indices: (0, scan_chunk_size as i32),
            scan_chunk_size,
            delete_files_on_import: false,
            hide_ignored_entries: true,
            hide_errored_entries: true,
            skip_importing_ignored: true,
            scanned_dir_entries: None,
            alternate_scan_dir: None,
            config,
        }
    }
}

impl ImporterUI {
    fn get_scan_dir(&self) -> PathBuf {
        let landing_result = self.get_config().path.landing();
        let landing = landing_result.unwrap_or_else(|_| PathBuf::from(""));
        if self.alternate_scan_dir.is_some() { self.alternate_scan_dir.as_ref().unwrap().clone() } else { landing }
    }
    
    fn set_config(&mut self, config: Config) {
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
        ui.horizontal(|ui| {
            ui.heading("options");
            if ui
                .button(if self.scanned_dir_entries.is_some() {
                    "re-scan"
                } else {
                    "scan"
                })
                .clicked()
            {
                let dir_entries_iter = fs::read_dir(self.get_scan_dir());
                if let Ok(dir_entries_iter) = dir_entries_iter {
                    let mut scanned_dir_entries = vec![];
                    for (index, dir_entry_res) in dir_entries_iter.enumerate() {
                        let index = index as i32;
                        if let Ok(dir_entry) = dir_entry_res {
                            let empty_path = Path::new("");
                            let dir_entry_path = dir_entry.path();
                            let dir_entry_parent = dir_entry_path
                                .parent()
                                .unwrap_or(&empty_path)
                                .file_name()
                                .unwrap_or(empty_path.as_os_str())
                                .to_str()
                                .unwrap_or("");
                            let dir_entry_filename = dir_entry_path
                                .file_name()
                                .unwrap_or(empty_path.as_os_str())
                                .to_str()
                                .unwrap_or("");
                            let file_label = format!("{dir_entry_parent}/{dir_entry_filename}");
                            self.scan_chunk_indices = (0, self.scan_chunk_size.into());
                            let is_to_be_loaded = if self.scan_chunk_indices.0 <= index && index < self.scan_chunk_indices.1 { true } else { false };
                            scanned_dir_entries.push(MediaEntry {
                                is_ignored: false,
                                is_to_be_loaded: Arc::new((Mutex::new(is_to_be_loaded), Condvar::new())),
                                is_disabled: false,
                                is_imported: false,
                                thumbnail: None,
                                mime_type: None,
                                dir_entry,
                                file_label,
                                bytes: None,
                                is_selected: false,
                            });
                        }
                    }
                    self.scanned_dir_entries = Some(scanned_dir_entries);
                } else {
                    println!("error reading from dir")
                }
            }
            if ui
                .add_enabled(
                    self.scanned_dir_entries.is_some(),
                    Button::new("prev chunk"),
                )
                .clicked()
            {
                let chunk_size = self.scan_chunk_size as i32;
                self.scan_chunk_indices.0 = (self.scan_chunk_indices.0 - chunk_size).max(0);
                self.scan_chunk_indices.1 = (self.scan_chunk_indices.1 - chunk_size).max(chunk_size);
                self.update_chunk();
            }
            if ui
            .add_enabled(
                self.scanned_dir_entries.is_some(),
                Button::new("next chunk"),
            )
            .clicked()
            {
                let chunk_size = self.scan_chunk_size as i32;
                let total_count = self.scanned_dir_entries.as_ref().unwrap().len() as i32;
                self.scan_chunk_indices.0 = (self.scan_chunk_indices.0 + chunk_size).min(total_count - chunk_size);
                self.scan_chunk_indices.1 = (self.scan_chunk_indices.1 + chunk_size).min(total_count);
                self.update_chunk();

            }
            if ui.add(egui::DragValue::new(&mut self.scan_chunk_size).speed(100).clamp_range(10..=10000).prefix("chunk: ")).changed() {
                self.scan_chunk_indices.1 = self.scan_chunk_indices.0 + self.scan_chunk_size as i32
            };
            if ui
                .add_enabled(
                    self.scanned_dir_entries.is_some(),
                    Button::new("select all"),
                )
                .clicked()
            {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_importable()) {
                        media_entry.is_selected = true;
                    }
                }
            }

            if ui
                .add_enabled(
                    self.scanned_dir_entries.is_some(),
                    Button::new("deselect all"),
                )
                .clicked()
            {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_importable()) {
                        media_entry.is_selected = false;
                    }
                }
            }

            if ui
                .add_enabled(self.scanned_dir_entries.is_some(), Button::new("invert"))
                .clicked()
            {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_importable()) {
                        media_entry.is_selected = !media_entry.is_selected;
                    }
                }
            }

            ui.checkbox(&mut self.hide_errored_entries, "hide errored");
            ui.checkbox(&mut self.hide_ignored_entries, "hide ignored");
            ui.checkbox(&mut self.skip_importing_ignored, "don't import ignored");
            
            if ui
            .add_enabled(
                self.scanned_dir_entries.is_some()
                && self
                .scanned_dir_entries
                .as_ref()
                .unwrap()
                .iter()
                            .any(|media_entry| media_entry.is_selected),
                            Button::new("ignore selected"),
                        )
                        .clicked()
            {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_selected) {
                        media_entry.is_ignored = true;
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
                            Button::new("unignore selected"),
                        )
                        .clicked()
            {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_selected) {
                        media_entry.is_ignored = false;
                    }
                }
            }

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
                    Button::new("import selected"),
                )
                .clicked()
            {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    let media_entries = scanned_dirs.iter().filter(|media_entry| media_entry.is_selected).collect::<Vec<&MediaEntry>>();
                    let import_result = import_media(media_entries, self.config.as_ref().unwrap());
                    println!("{:?}", import_result);
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_selected) {
                        media_entry.is_imported = true;
                        media_entry.is_ignored = false;
                        media_entry.is_selected = false;
                    }
                }
            }
        });
    }

    fn update_chunk(&mut self) {
        if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
            for (index, media_entry) in scanned_dirs.iter_mut().enumerate() {
                let index = index as i32;
                let is_to_be_loaded = if self.scan_chunk_indices.0 <= index && index < self.scan_chunk_indices.1 { true } else { false };
                media_entry.set_load_status(is_to_be_loaded);
            }
        }
    }

    fn render_files_col(&mut self, cols: &mut [Ui]) {
        let files_col = &mut cols[0];
        files_col.label("file");
        if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
            ScrollArea::vertical().id_source("files_col").show(
                files_col,
                |files_col_scroll| {
                    for media_entry in scanned_dirs.iter_mut() {
                        // display label stuff
                        files_col_scroll.add_enabled_ui(
                            media_entry.is_importable(),
                            |files_col_scroll| {
                                let mut text = egui::RichText::new(format!("{}", media_entry.file_label));
                                if media_entry.is_ignored { text = text.color(egui::Color32::RED); }
                                let mut response = files_col_scroll.selectable_label(
                                    media_entry.is_selected,
                                    text,
                                );
                                if response.clicked() {
                                    media_entry.is_selected = !media_entry.is_selected;
                                };
                                let disabled_reason = 
                                if media_entry.is_imported { 
                                    "already imported"
                                } else if media_entry.is_disabled {
                                    "can't read file"
                                } else if !media_entry.is_importable() {
                                    "bytes not loaded"
                                } else {
                                    "unknown error"
                                };
                                if media_entry.is_ignored { response = response.on_hover_text("(ignored)"); }
                                response.on_disabled_hover_text(format!("({disabled_reason})"));
                            },
                        );
                    }
                },
            );
        }
    }

    fn render_previews_col(&mut self, cols: &mut [Ui], ctx: &egui::Context) {
        let previews_col = &mut cols[1];
        previews_col.label("preview");
        let thumbnail_size = self.get_config().ui.import.thumbnail_size;
        if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
            // iterate through each mediaentry to draw its name on the sidebar, and to load its image
            // wrapped in an arc mutex for multithreading purposes
            ScrollArea::vertical().id_source("previews_col").show(
                previews_col,
                |previews_col_scroll| {
                    let layout = egui::Layout::from_main_dir_and_cross_align(
                        Direction::LeftToRight,
                        Align::Center,
                    )
                    .with_main_wrap(true);
                    previews_col_scroll.allocate_ui(
                        Vec2::new(previews_col_scroll.available_size_before_wrap().x, 0.0),
                        |scroll_wrap| {
                            scroll_wrap.with_layout(layout, |scroll_wrap| {
                                for media_entry in scanned_dirs.iter_mut() {
                                    if media_entry.is_ignored && self.hide_ignored_entries { continue; }
                                    let preview_size =
                                        (thumbnail_size + 10) as f32;
                                    let preview_size2 = [preview_size, preview_size];
                                    let label_clone = media_entry.file_label.clone();
                                    let is_importable = media_entry.is_importable();
                                    let mime_type = media_entry.get_mime_type();
                                    if let Some(Ok(mime_type)) = mime_type { // mime type read
                                        let mime_type = mime_type.clone();
                                        match media_entry.get_thumbnail() {
                                            None => {
                                                let spinner = egui::Spinner::new();
                                                scroll_wrap.add_sized(preview_size2, spinner).on_hover_text(format!("{label_clone} [{mime_type}] (loading bytes for thumbnail...)"));
                                            }
                                            Some(promise) => {
                                                match promise.ready() {
                                                    None => {
                                                        let spinner = egui::Spinner::new();
                                                        scroll_wrap
                                                        .add_sized(preview_size2, spinner)
                                                        .on_hover_text(format!("{label_clone} [{mime_type}] (loading thumbnail...)"));
                        
                                                    }

                                                    Some(Err(_error)) => {
                                                        let text = egui::RichText::new("?")
                                                            .color(egui::Color32::from_rgb(
                                                                252, 229, 124
                                                            ))
                                                            .size(48.0);
                                                        let label = egui::Label::new(text)
                                                            .sense(egui::Sense::hover());
                                                        scroll_wrap
                                                            .add_sized(preview_size2, label)
                                                            .on_hover_text(format!(
                                                                "{} [{mime_type}] (couldn't generate thumbnail)",
                                                                label_clone
                                                            ));

                                                        // preview_frame.put(rect, label);
                                                    }
                                                    Some(Ok(image)) => {
                                                        if is_importable {
                                                            let mut image_button =
                                                            egui::ImageButton::new(
                                                                image.texture_id(ctx),
                                                                image.size_vec2(),
                                                            )
                                                            .selected(media_entry.is_selected);
                                                            
                                                            if media_entry.is_ignored {
                                                                image_button = image_button.tint(egui::Color32::from_rgba_premultiplied(255, 100, 100, 255));
                                                            }
                                                            let response = scroll_wrap.add_sized(
                                                                preview_size2,
                                                                image_button,
                                                            );
                                                            if response
                                                                .clicked()
                                                            {
                                                                media_entry.is_selected =
                                                                    !media_entry.is_selected;
                                                            }
                                                            response.on_hover_text(format!("{label_clone} [{mime_type}]{}", if media_entry.is_ignored { " (ignored)" } else { "" }));
                                                            
                                                        } else {
 
                                                            let mut image = egui::widgets::Image::new(image.texture_id(ctx), image.size_vec2());
                                                            let ignored_reason = 
                                                            if media_entry.is_ignored { 
                                                                "ignored" 
                                                            } else if media_entry.is_imported {
                                                                image = image.tint(egui::Color32::from_rgba_premultiplied(100, 100, 255, 255));
                                                                "already imported"
                                                            } else { 
                                                                image = image.tint(egui::Color32::from_rgba_premultiplied(100, 100, 100, 255));
                                                                "not in current scan chunk"
                                                            };
                                                            let response = scroll_wrap.add_sized(preview_size2, image);
                                                            response.on_hover_text(format!("{label_clone} [{mime_type}] ({ignored_reason})"));
       
                                                        }

                                                    }
                                                }
                                            }
                                        }
                                    } else if let Some(Err(error)) = mime_type  { // unknown file type
                                        if self.hide_errored_entries { continue; }
                                        let text = egui::RichText::new("!")
                                            .color(egui::Color32::from_rgb(
                                                255, 149, 138,
                                            ))
                                            .size(48.0);
                                        let label = egui::Label::new(text)
                                            .sense(egui::Sense::hover());
                                        scroll_wrap
                                            .add_sized(preview_size2, label)
                                            .on_hover_text(format!(
                                                "{label_clone} ({error})",
                                                
                                            ));
                                    } else if mime_type.is_none() { // bytes not loaded
                                        let is_to_be_loaded = media_entry.try_check_if_is_to_be_loaded();

                                        if is_to_be_loaded {
                                            let spinner = egui::Spinner::new();
                                            scroll_wrap
                                                .add_sized(preview_size2, spinner)
                                                .on_hover_text(format!("{label_clone} [?] (loading bytes for reading...)"));
                                        } else {
                                            let text = egui::RichText::new("...")
                                                    .color(egui::Color32::from_rgb(
                                                        252, 229, 124
                                                    ))
                                                    .size(48.0);
                                                let label = egui::Label::new(text)
                                                    .sense(egui::Sense::hover());
                                                scroll_wrap
                                                    .add_sized(preview_size2, label)
                                                    .on_hover_text(format!(
                                                        "{} [?] (not yet scanned)",
                                                        label_clone
                                                    ));
                                        }
                                    }
                                }
                            });
                        },
                    );
                },
            );
        }
    }
    fn render_cols(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.columns(2, |cols| {

            self.render_files_col(cols);
            self.render_previews_col(cols, ctx);

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

            self.render_scan_directory_selection(ui);
            self.render_options(ui);
            self.render_progress(ui);
            self.render_cols(ui, ctx);

        });
    }
}
