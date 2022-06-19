#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

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

use super::Config;
use anyhow::{Context, Error, Result};
use eframe::egui::{self, Button, Direction, Grid, ScrollArea, Ui};
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
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "htool2",
        options,
        Box::new(|_cc| Box::new(ImporterUI::default())),
    );
}

struct MediaEntry {
    is_disabled: bool,
    dir_entry: DirEntry,
    file_label: String,
    mime_type: Option<String>,
    is_selected: bool,
    bytes: Option<Promise<Result<Vec<u8>>>>,
    thumbnail: Option<Promise<Result<RetainedImage>>>,
}

impl MediaEntry {
    pub fn get_bytes(&mut self) -> &Promise<Result<Vec<u8>, Error>> {
        self.bytes.get_or_insert_with(|| {
            let path = self.dir_entry.path().clone();
            let promise = Promise::spawn_thread("", move || {
                let mut file = File::open(path)?;
                let mut bytes: Vec<u8> = vec![];
                file.read_to_end(&mut bytes)?;
                Ok(bytes)
            });
            promise
        })
    }

    pub fn get_thumbnail(&mut self) -> Option<&Promise<Result<RetainedImage, Error>>> {
        match &self.thumbnail {
            None => match self.get_bytes().ready() {
                None => None,
                Some(result) => {
                    let (sender, promise) = Promise::new();
                    match result {
                        Err(error) => {
                            self.is_disabled = true;
                            sender.send(Err(anyhow::Error::msg("error")))
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
            Some(promise) => {
                if let Some(Err(error)) = promise.ready() {
                    self.is_disabled = true;
                }
                self.thumbnail.as_ref()
            }
        }
    }

    pub fn load_image_from_memory(image_data: &[u8], thumbnail_size: u8) -> Result<RetainedImage> {
        println!("loading from memory, size: {} kB", image_data.len() / 1000);
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

    pub fn load_media(path_arc: PathBuf, thumbnail_size: u8) -> Result<RetainedImage> {
        // file.read_to_end(&mut );
        let image = ImageReader::open(&path_arc)?
            .with_guessed_format()?
            .decode()?;
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

        Ok(RetainedImage::from_color_image(
            &path_arc.display().to_string(),
            color_image,
        ))
    }
}

struct ImporterUI {
    placeholder_image: RetainedImage,
    scanned_dir_entries: Option<Vec<MediaEntry>>,
    alternate_scan_dir: Option<String>,
    config: Config,
}

impl Default for ImporterUI {
    fn default() -> Self {
        let config = Config::load().expect("couldn't load config");

        Self {
            placeholder_image: RetainedImage::from_color_image(
                "placeholder",
                egui::ColorImage::example(),
            ),
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

    fn load_thumbnail_from_path(
        path: &std::path::Path,
    ) -> Result<egui::ColorImage, image::ImageError> {
        let image = ImageReader::open(path)?.decode()?;
        let image_cropped = ImageOps::crop_imm(&image, 20, 20, 20, 20).to_image();
        let thumbnail = ImageOps::thumbnail(&image_cropped, 100, 100);
        let size = [thumbnail.width() as usize, thumbnail.height() as usize];
        let pixels = thumbnail.as_flat_samples();
        Ok(egui::ColorImage::from_rgba_unmultiplied(
            size,
            pixels.as_slice(),
        ))
    }

    fn load_image_from_memory(image_data: &[u8]) -> Result<egui::ColorImage, image::ImageError> {
        let image = image::load_from_memory(image_data)?;
        let size = [image.width() as _, image.height() as _];
        let image_buffer = image.to_rgba8();
        let pixels = image_buffer.as_flat_samples();
        Ok(egui::ColorImage::from_rgba_unmultiplied(
            size,
            pixels.as_slice(),
        ))
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
                    for dir_entry_res in dir_entries_iter {
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
                            scanned_dir_entries.push(MediaEntry {
                                is_disabled: false,
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
                    Button::new("select all"),
                )
                .clicked()
            {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| !media_entry.is_disabled) {
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
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| !media_entry.is_disabled) {
                        media_entry.is_selected = false;
                    }
                }
            }

            if ui
                .add_enabled(self.scanned_dir_entries.is_some(), Button::new("invert"))
                .clicked()
            {
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    for media_entry in scanned_dirs.iter_mut().filter(|media_entry| !media_entry.is_disabled) {
                        media_entry.is_selected = !media_entry.is_selected;
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
            {}
        });
    }

    fn render_cols(&mut self, files_col: &mut &mut Ui) {}
}

impl eframe::App for ImporterUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // let mut style = (*ctx.style()).clone();

            // style.debug.debug_on_hover = true;
            // ctx.set_style(style);

            self.render_scan_directory_selection(ui);
            self.render_options(ui);

            ui.columns(2, |cols| {
                let mut cols_iter = cols.iter_mut();
                let files_col = &mut cols_iter.next().unwrap();
                let previews_col = &mut cols_iter.next().unwrap();
                files_col.label("file");
                previews_col.label("preview");

                // scan button was clicked, and scanned_dirs has loaded with mediaentries
                if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                    // iterate through each mediaentry to draw its name on the sidebar, and to load its image
                    // wrapped in an arc mutex for multithreading purposes
                    ScrollArea::vertical().id_source("files_col").show(
                        files_col,
                        |files_col_scroll| {
                            for media_entry in scanned_dirs.iter_mut() {
                                // display label stuff
                                files_col_scroll.add_enabled_ui(
                                    !media_entry.is_disabled,
                                    |files_col_scroll| {
                                        let response = files_col_scroll.selectable_label(
                                            media_entry.is_selected,
                                            &media_entry.file_label,
                                        );
                                        if response.clicked() {
                                            media_entry.is_selected = !media_entry.is_selected;
                                        };
                                        // response.on_hover_text(format!("{}", &media_entry.file_label));
                                    },
                                );
                            }
                        },
                    );

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
                                            let preview_size =
                                                (&self.config.ui.import.thumbnail_size + 10) as f32;
                                            let preview_size2 = [preview_size, preview_size];
                                            let label_clone = media_entry.file_label.clone();
                                            match media_entry.get_thumbnail() {
                                                None => {
                                                    let spinner = egui::Spinner::new();
                                                    scroll_wrap.add_sized(preview_size2, spinner);
                                                    // preview_frame.put(rect, spinner);
                                                }
                                                Some(promise) => {
                                                    match promise.ready() {
                                                        None => {
                                                            let spinner = egui::Spinner::new();
                                                            scroll_wrap
                                                                .add_sized(preview_size2, spinner);
                                                            // preview_frame.put(rect, spinner);
                                                        }

                                                        Some(Err(error)) => {
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
                                                                    "{} ({error})",
                                                                    label_clone
                                                                ));

                                                            // preview_frame.put(rect, label);
                                                        }
                                                        Some(Ok(image)) => {
                                                            // image.show(scroll_wrap);
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
                                                            response.on_hover_text(format!("{}", label_clone));
                                                            // preview_frame.put(rect, image_button);
                                                        }
                                                    }
                                                }
                                            }
                                            // });
                                        }
                                    });
                                },
                            );
                        },
                    );
                }
            });
        });
    }
}
