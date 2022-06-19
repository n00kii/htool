// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use crate::errors::MediaReadError;
use crossbeam_utils;
use eframe::emath::{Align, Pos2, Vec2};
use eframe::epaint::TextureHandle;
use egui::Frame;
use egui_extras::RetainedImage;
use image::io::Reader as ImageReader;
use image::{imageops as ImageOps, DynamicImage};
use std::io::Read;
use std::path::Path;
use std::sync::Condvar;

use super::Config;
use anyhow::{Context, Error, Result};
use eframe::egui::{self, Button, Direction, Grid, ScrollArea, Ui, ProgressBar};
use poll_promise::Promise;
use rfd::FileDialog;

use std::{
    env,
    fs::{self, DirEntry, File, ReadDir},
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex, RwLock},
    thread::{self, JoinHandle},
};

pub fn main() {
    let mut options = eframe::NativeOptions::default();
    options.initial_window_size = Some(Vec2::new(1220.0, 600.0));
    // println!("{:?}", options.initial_window_size);
    eframe::run_native(
        "htool2",
        options,
        Box::new(|_cc| Box::new(ImporterUI::default())),
    );
}

struct MediaEntry {
    is_disabled: bool,
    is_ignored: bool,
    is_imported: bool,
    is_to_be_loaded: Arc<(Mutex<bool>, Condvar)>,
    dir_entry: DirEntry,
    file_label: String,
    is_selected: bool,
    mime_type: Option<Result<String>>,
    bytes: Option<Promise<Result<Vec<u8>>>>,
    thumbnail: Option<Promise<Result<RetainedImage>>>,
}

impl MediaEntry {
    pub fn get_bytes(&mut self) -> &Promise<Result<Vec<u8>, Error>> {
        self.bytes.get_or_insert_with(|| {
            let path = self.dir_entry.path().clone();
            let load_condition = Arc::clone(&self.is_to_be_loaded);
            let promise = Promise::spawn_thread("", move || {
                let (lock, cond_var) = &*load_condition;
                let mut start_loading = lock.lock().unwrap();
                while !*start_loading { start_loading = cond_var.wait(start_loading).unwrap() }

                let mut file = File::open(path)?;
                let mut bytes: Vec<u8> = vec![];
                file.read_to_end(&mut bytes)?;
                Ok(bytes)
            });
            promise
        })
    }

    pub fn is_importable(&self) -> bool {
        match &self.bytes {
            None => false,
            Some(promise) => {
                match promise.ready() {
                    None => false,
                    Some(_) => {
                        return !self.is_disabled && !self.is_imported && !self.is_ignored
                    }
                }
            }
        }
    }

    pub fn unload_bytes(&mut self) {
        self.bytes = None
    }

    pub fn unload_thumbnail(&mut self) {
        self.thumbnail = None
    }

    pub fn get_mime_type(&mut self) -> Option<&Result<String, Error>> {
        match &self.mime_type {
            None => match self.get_bytes().ready() {
                None => {
                    // todo!();
                }
                Some(bytes_result) => {
                    match bytes_result {
                        Err(_error) => {
                            self.mime_type = Some(Err(anyhow::Error::msg("failed to load bytes")));
                            self.is_disabled = true;
                        }
                        Ok(bytes) => {
                            match infer::get(&bytes) {
                                Some(kind) => {
                                    self.mime_type = Some(Ok(kind.mime_type().to_string()));
                                },
                                None => {
                                    self.mime_type = Some(Err(anyhow::Error::msg("unknown file type")));
                                    self.is_disabled = true;
                                },
                            }
                        }
                    }
                }   
            },
            Some(_result) => {
                // todo!();
            }
        }
        self.mime_type.as_ref()
    }

    pub fn get_thumbnail(&mut self) -> Option<&Promise<Result<RetainedImage, Error>>> {
        match &self.thumbnail {
            None => match self.get_bytes().ready() {
                None => None,
                Some(result) => {
                    let (sender, promise) = Promise::new();
                    match result {
                        Err(_error) => {
                            // self.is_disabled = true;
                            sender.send(Err(anyhow::Error::msg("failed to load bytes")))
                        }
                        Ok(bytes) => {
                            let bytes_copy = bytes.clone();
                            thread::spawn(move || {
                                let bytes_2 = &bytes_copy as &[u8];
                                let image = MediaEntry::load_image_from_memory(bytes_2, 100);
                                sender.send(image);
                            });
                        }
                    }
                    self.thumbnail = Some(promise);
                    self.thumbnail.as_ref()
                }
            },
            Some(_promise) => {

                self.thumbnail.as_ref()
            }
        }
    }

    pub fn try_check_if_is_to_be_loaded(&self) -> bool {
        let (lock, cond_var) = &*self.is_to_be_loaded;
        let is_to_be_loaded = lock.try_lock();
        match is_to_be_loaded {
            Err(error) => { // lock being aquired by something else
                false
            }
            Ok(is_to_be_loaded) => {
                *is_to_be_loaded
            },
        }
    }

    pub fn set_load_status(&mut self, load_status: bool) {
        if !load_status {
            self.unload_bytes();
        } else {
            self.get_bytes();
        }
        let (lock, cond_var) = &*self.is_to_be_loaded;
        let mut is_to_be_loaded = lock.lock().unwrap();
        *is_to_be_loaded = load_status;
        cond_var.notify_all();
        
    }

    pub fn load_image_from_memory(image_data: &[u8], thumbnail_size: u8) -> Result<RetainedImage> {
        // println!("loading from memory, size: {} kB", image_data.len() / 1000);
        let image = image::load_from_memory(image_data)?;
        let (w, h) = (image.width(), image.height());
        let image_cropped = ImageOps::crop_imm(
            &image,
            if h > w { 0 } else { (w - h) / 2 },
            if w > h { 0 } else { (h - w) / 2 },
            if h > w { w } else { h },
            if w > h { h } else { w },
        )
        .to_image();
        let thumbnail =
            ImageOps::thumbnail(&image_cropped, thumbnail_size.into(), thumbnail_size.into());
        let size = [thumbnail.width() as usize, thumbnail.height() as usize];
        let pixels = thumbnail.as_flat_samples();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
        Ok(RetainedImage::from_color_image("", color_image))
    }

}

struct ImporterUI {
    config: Config,
    scanned_dir_entries: Option<Vec<MediaEntry>>,
    alternate_scan_dir: Option<String>,
    delete_files_on_import: bool,
    hide_ignored_entries: bool,
    scan_chunk_size: u16,
    scan_chunk_indices: (i32, i32),
}

impl Default for ImporterUI {
    fn default() -> Self {
        let config = Config::load().expect("couldn't load config");
        let scan_chunk_size: u16 = 1000;
        Self {
            scan_chunk_indices: (0, scan_chunk_size as i32),
            scan_chunk_size,
            delete_files_on_import: false,
            hide_ignored_entries: true,
            scanned_dir_entries: None,
            alternate_scan_dir: None,
            config,
        }
    }
}

impl ImporterUI {
    fn scan_dir(&self) -> &String {
        self.alternate_scan_dir
            .as_ref()
            .unwrap_or(&self.config.path.landing)
    }

    fn render_scan_directory_selection(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading("scan directory");
            if ui.button("change").clicked() {
                if let Some(path) = FileDialog::new().pick_folder() {
                    self.alternate_scan_dir = Some(path.display().to_string());
                    self.scanned_dir_entries = None
                }
            }
            if self.alternate_scan_dir.as_ref().is_some() && ui.button("remove").clicked() {
                self.alternate_scan_dir = None;
                self.scanned_dir_entries = None
            }
            ui.label(format!("{}", self.scan_dir()));
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
                let dir_entries_iter = fs::read_dir(self.scan_dir());
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

            ui.checkbox(&mut self.hide_ignored_entries, "hide ignored");
            
            if ui
            .add_enabled(
                self.scanned_dir_entries.is_some()
                && self
                .scanned_dir_entries
                .as_ref()
                .unwrap()
                .iter()
                            .any(|media_entry| media_entry.is_selected),
                            Button::new("toggle ignore for selected"),
                        )
                        .clicked()
            {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| media_entry.is_selected) {
                        media_entry.is_ignored = !media_entry.is_ignored;
                        if media_entry.is_ignored { media_entry.is_selected = false }
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
                            .any(|media_entry| media_entry.is_selected) && self.get_load_progress() == 1.0,
                    Button::new("import selected"),
                )
                .clicked()
            {
                
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
                                let response = files_col_scroll.selectable_label(
                                    media_entry.is_selected,
                                    &media_entry.file_label,
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
                                } else if media_entry.is_ignored{
                                    "ignored"
                                } else {
                                    "unknown error"
                                };
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
                                        (&self.config.ui.import.thumbnail_size + 10) as f32;
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
                                                        let mut spinner_response = scroll_wrap
                                                        .add_sized(preview_size2, spinner)
                                                        .on_hover_text(format!("{label_clone} [{mime_type}] (loading thumbnail...)"));
                        
                                                    }

                                                    Some(Err(error)) => {
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
                                                            let image_button =
                                                                egui::ImageButton::new(
                                                                    image.texture_id(ctx),
                                                                    image.size_vec2(),
                                                                )
                                                                .selected(media_entry.is_selected);
    
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
                                                            response.on_hover_text(format!("{label_clone} [{mime_type}]"));
                                                            
                                                        } else {
 
                                                            let image = egui::widgets::Image::new(image.texture_id(ctx), image.size_vec2());
                                                            let response = scroll_wrap.add_sized(preview_size2, image);
                                                            let ignored_reason = 
                                                            if media_entry.is_ignored { 
                                                                "ignored" 
                                                            } else { 
                                                                "not in current scan chunk"
                                                            };
                                                            response.on_hover_text(format!("{label_clone} [{mime_type}] ({ignored_reason})"));
       
                                                        }

                                                    }
                                                }
                                            }
                                        }
                                    } else if let Some(Err(error)) = mime_type  { // unknown file type
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
