#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use crate::errors::MediaReadError;
use eframe::epaint::TextureHandle;
use egui_extras::RetainedImage;
use image::io::Reader as ImageReader;
use image::{imageops as ImageOps, DynamicImage};
use std::path::Path;

use super::Config;
use anyhow::{Context, Error, Result};
use eframe::egui::{self, Button, Direction};
use poll_promise::Promise;
use rfd::FileDialog;

use std::{
    env,
    fs::{self, DirEntry, ReadDir},
    path::PathBuf,
    sync::{Arc, Mutex},
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
    started_load: bool,
    attempted_load: bool,
    dir_entry: DirEntry,
    is_selected: bool,
    image: Option<Promise<Result<RetainedImage>>>, // image: Option<DynamicImage>,
                                                   // image_texture: Option<egui::TextureHandle>,
                                                   // thumbnail_texture: Option<egui::TextureHandle>
}

impl MediaEntry {
    pub fn load_image(&mut self) -> Result<RetainedImage> {
        //change to return self.image or blah blah
        // if self.image.is_some() { return Ok(()) }
        // println!("1");
        println!("loading image {:?}", self.dir_entry.file_name());
        let path = self.dir_entry.path();
        // println!("2");
        let image = ImageReader::open(path)?.with_guessed_format()?.decode()?;
        // println!("3");
        let (w, h) = (image.width(), image.height());
        let image_cropped = ImageOps::crop_imm(
            &image,
            if h > w { 0 } else { (w - h) / 2 },
            if w > h { 0 } else { (h - w) / 2 },
            if h > w { w } else { h },
            if w > h { h } else { w },
        )
        .to_image();
        // println!("4");
        let thumbnail = ImageOps::thumbnail(&image_cropped, 100, 100);
        // println!("5");
        let size = [thumbnail.width() as usize, thumbnail.height() as usize];
        // println!("6");/
        // let image_buffer = image.to_rgba8();
        let pixels = thumbnail.as_flat_samples();
        // println!("7");
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
        
        // println!("8");
        
        println!("finished image {:?}", self.dir_entry.file_name());
        Ok(RetainedImage::from_color_image(
            "rust-logo-256x256.png",
            color_image,
        ))
        // println!("9");
        // self.image = Some(ImageReader::open(self.dir_entry.path())?.decode()?);
    }
    // pub fn get_thumbnail_texture(&mut self, config: &Config, context: &eframe::egui::Context) -> Result<&TextureHandle> {
    //     // let image = ImageReader::open(path)?.decode()?;
    //     // let image_cropped = ImageOps::crop_imm(&image, 20, 20, 20, 20).to_image();
    //     // let thumbnail = ImageOps::thumbnail(&image_cropped, 100, 100);
    //     // let size = [thumbnail.width() as usize, thumbnail.height() as usize];
    //     // // let image_buffer = image.to_rgba8();
    //     // let pixels = thumbnail.as_flat_samples();
    //     // Ok(egui::ColorImage::from_rgba_unmultiplied(
    //     //     size,
    //     //     pixels.as_slice(),
    //     // ))
    //     self.load_image();
    //     let image_cropped = ImageOps::crop_imm(self.image.as_ref().context("image not loaded from path")?, 20, 20, 20, 20).to_image();
    //     let thumbnail = ImageOps::thumbnail(&image_cropped, 100, 100);
    //     let size = [thumbnail.width() as usize, thumbnail.height() as usize];
    //     let pixels = thumbnail.as_flat_samples();
    //     self.thumbnail_texture = Some(context.load_texture(self.dir_entry.path().to_str().unwrap_or(""), egui::ColorImage::from_rgba_unmultiplied(
    //         size,
    //         pixels.as_slice(),
    //     )));
    //     self.thumbnail_texture.as_ref().context("couldnt generate thumbnail")

    // Ok(())
    // }
    pub fn load_image_texture(&mut self) {}
}

/*

struct MyImage {
    texture: Option<egui::TextureHandle>,
}

impl MyImage {
    fn ui(&mut self, ui: &mut egui::Ui) {
        let texture: &egui::TextureHandle = self.texture.get_or_insert_with(|| {
            // Load the texture only once.
            ui.ctx().load_texture("my-image", egui::ColorImage::example())
        });

        // Show the image:
        ui.image(texture, texture.size_vec2());
    }
}
*/

// #[derive(Default)]
struct ImporterUI {
    placeholder_image: RetainedImage,
    scanned_dir_entries: Option<Vec<Arc<Mutex<MediaEntry>>>>,
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
            // landing: &config.path.landing,
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
        // let image_buffer = image.to_rgba8();
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
}

impl eframe::App for ImporterUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
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

            ui.horizontal(|ui| {
                ui.heading("importer");
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
                                scanned_dir_entries.push(Arc::new(Mutex::new(MediaEntry {
                                    attempted_load: false,
                                    started_load: false,
                                    image: None,
                                    // image_texture: None,
                                    // thumbnail_texture: None,
                                    dir_entry,
                                    is_selected: false,
                                })));
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
                        for media_entry_arc in scanned_dirs.iter_mut() {
                            let media_entry_mutex = media_entry_arc.lock();
                            match media_entry_mutex {
                                Ok(mut media_entry) => {
                                    media_entry.is_selected = true;
                                }
                                Err(error) => {}
                            }
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
                        for media_entry_arc in scanned_dirs.iter_mut() {
                            let media_entry_mutex = media_entry_arc.lock();
                            match media_entry_mutex {
                                Ok(mut media_entry) => {
                                    media_entry.is_selected = false;
                                }
                                Err(error) => {}
                            }
                        }
                    }
                }

                if ui
                    .add_enabled(self.scanned_dir_entries.is_some(), Button::new("invert"))
                    .clicked()
                {
                    if let Some(scanned_dirs) = &mut self.scanned_dir_entries {
                        for media_entry_arc in scanned_dirs.iter_mut() {
                            let media_entry_mutex = media_entry_arc.lock();
                            match media_entry_mutex {
                                Ok(mut media_entry) => {
                                    media_entry.is_selected = !media_entry.is_selected;
                                }
                                Err(error) => {}
                            }
                        }
                    }
                }

                if ui
                    .add_enabled(
                        self.scanned_dir_entries.is_some()
                            && self.scanned_dir_entries.as_ref().unwrap().iter().any(
                                |media_entry_arc| {
                                    let media_entry_mutex = media_entry_arc.lock();
                                    match media_entry_mutex {
                                        Ok(media_entry) => {
                                            media_entry.is_selected
                                            // media_entry.is_selected = !media_entry.is_selected;
                                        }
                                        Err(error) => false,
                                    }
                                },
                            ),
                        Button::new("import selected"),
                    )
                    .clicked()
                {}
            });

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
                    for media_entry_arc in scanned_dirs.iter_mut() {
                        // take
                        let media_entry_mutex = media_entry_arc.lock(); // <-- this forces main thread to wait until worker threads done using it
                        // TODO: change so loadimage is static fxn, pass dir_entry, remove arc mutex stuff
                        if media_entry_mutex.is_err() {
                            println!("skipped errored mutex");
                            continue;
                        }

                        let mut media_entry = media_entry_mutex.unwrap();

                        // display label stuff 
                        let empty_path = Path::new("");
                        let dir_entry = &media_entry.dir_entry;
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
                        let dir_entry_label = format!("{dir_entry_parent}/{dir_entry_filename}");

                        // create label in left column
                        if files_col
                            .selectable_label(media_entry.is_selected, dir_entry_label)
                            .clicked()
                        {
                            media_entry.is_selected = !media_entry.is_selected;
                        };

                        // promise for loading image
                        let promise = media_entry.image.get_or_insert_with(|| {
                            // clone the arc, make spawn a thread 
                            let media_entry_arc_clone = Arc::clone(&media_entry_arc);
                            let promise = Promise::spawn_thread(
                                format!("load_{dir_entry_filename}"),
                                move || {
                                    // take mutex, then load image (slow)
                                    let media_entry_mutex = media_entry_arc_clone.lock();
                                    let mut media_entry = media_entry_mutex.unwrap();
                                    media_entry.load_image()
                                },
                            );
                            promise
                        });
                    }
                }
            });
        });
    }
}
