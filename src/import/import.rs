use super::super::ui;
use super::super::Config;
use crate::config::Import;
use crate::data;
use crate::data::ImportationStatus;
use anyhow::{Context, Error, Result};
use eframe::egui;
use egui_extras::RetainedImage;
use image::io::Reader as ImageReader;
use image::{error, imageops};
use image_hasher::{HashAlg, HasherConfig};
use infer::Type;
use poll_promise::Promise;
use rusqlite::{params, Connection, Result as SqlResult};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use std::{
    fs::{self, DirEntry, File, ReadDir},
    io::Read,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, RwLock},
    thread::{self, JoinHandle},
};

pub struct MediaEntry {
    pub is_unloadable: bool,
    pub is_hidden: bool,
    pub is_imported: bool,
    pub is_selected: bool,
    pub disable_bytes_loading: bool,
    pub is_to_be_loaded: Arc<(Mutex<bool>, Condvar)>,

    pub dir_entry: DirEntry,
    pub mime_type: Option<Result<Type>>,
    pub file_label: String,
    pub linking_dir: Option<String>,
    pub bytes: Option<Promise<Result<Arc<Vec<u8>>>>>,
    pub thumbnail: Option<Promise<Result<RetainedImage>>>,
    pub modified_thumbnail: Option<RetainedImage>,

    pub importation_status: Option<Promise<Arc<ImportationStatus>>>,
}

pub fn import_media(media_entry: &mut MediaEntry, dir_link_map: Arc<Mutex<HashMap<String, i32>>>, config: Arc<Config>) {
        let bytes = media_entry.bytes.as_ref();
        let mut fail = |message: String| {
            let (sender, promise) = Promise::new();
            media_entry.importation_status = Some(promise);
            sender.send(Arc::new(ImportationStatus::Fail(anyhow::Error::msg(message))));
        };
        match bytes {
            None => {
                fail("bytes not loaded".into());
            }
            Some(promise) => match promise.ready() {
                None => {
                    fail("bytes are still loading".into());
                }
                Some(Err(_error)) => {
                    fail("failed to load bytes".into());
                }
                Some(Ok(bytes)) => {
                    let filekind = match &media_entry.mime_type {
                        Some(Ok(kind)) => Some(kind.clone()),
                        Some(Err(_error)) => None,
                        None => None,
                    };

                    let bytes = Arc::clone(bytes);
                    let config = Arc::clone(&config);
                    let dir_link_map = Arc::clone(&dir_link_map);
                    let linking_dir = media_entry.linking_dir.clone();
                    media_entry.importation_status = Some(Promise::spawn_thread("", move || {
                        let bytes = &*bytes as &[u8];
                        let result = data::register_media(config, bytes, filekind, linking_dir, dir_link_map);
                        Arc::new(result)
                    }))
                }
            },
        }
    // Ok(())
}

fn reverse_path_truncate(path: &PathBuf, num_components: u8) -> PathBuf {
    let mut reverse_components = path.components().rev();
    let mut desired_components = vec![];
    let mut truncated_path = PathBuf::from("");
    for _ in 0..num_components {
        desired_components.push(reverse_components.next());
    }

    let reversed_components = desired_components.into_iter().rev();

    for component in reversed_components {
        if let Some(component) = component {
            let path = PathBuf::from(component.as_os_str());
            truncated_path = truncated_path.join(path);
        }
    }
    truncated_path
}

pub fn scan_directory(
    directory_path: PathBuf,
    directory_level: u8,
    linking_dir: Option<String>,
    extension_filter: &Vec<&String>
) -> Result<Vec<MediaEntry>> {
    // println!("{extension_filter:?}");
    let dir_entries_iter = fs::read_dir(directory_path)?;
    let mut scanned_dir_entries = vec![];
    'dir_entries: for (_index, dir_entry_res) in dir_entries_iter.enumerate() {
        if let Ok(dir_entry) = dir_entry_res {
            // TODO: don't error whole function for one entry (? below)
            if dir_entry.metadata()?.is_dir() {
                let linking_dir = if let Some(linking_dir) = &linking_dir {
                    Some(linking_dir.clone())
                } else {
                    if directory_level == 0 {
                        let dir_name = dir_entry.file_name().into_string();
                        if let Ok(dir_name) = dir_name {
                            Some(dir_name)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

                let media_entries = scan_directory(dir_entry.path(), directory_level + 1, linking_dir, extension_filter)?;
                scanned_dir_entries.extend(media_entries);
            } else {
                let dir_entry_path = dir_entry.path();

                if let Some(ext) = dir_entry_path.extension() {
                    for exclude_ext in extension_filter {
                        if ext.to_str().unwrap_or("") == exclude_ext.as_str() {
                            continue 'dir_entries;
                        }
                    }
                }
                
                let file_label = reverse_path_truncate(&dir_entry_path, 2 + directory_level)
                    .to_str()
                    .unwrap_or("")
                    .to_string()
                    .replace("\\", "/"); 

                scanned_dir_entries.push(MediaEntry {
                    is_hidden: false,
                    is_to_be_loaded: Arc::new((Mutex::new(false), Condvar::new())),
                    is_unloadable: false,
                    is_imported: false,
                    thumbnail: None,
                    mime_type: None,
                    dir_entry,
                    file_label,
                    bytes: None,
                    is_selected: false,
                    modified_thumbnail: None,
                    importation_status: None,
                    linking_dir: linking_dir.clone(),
                    disable_bytes_loading: false,
                });
            }
        }
    }

    Ok(scanned_dir_entries)
}

impl MediaEntry {
    pub fn get_bytes(&mut self) -> &Promise<Result<Arc<Vec<u8>>, Error>> {
        self.bytes.get_or_insert_with(|| {
            let path = self.dir_entry.path().clone();
            let load_condition = Arc::clone(&self.is_to_be_loaded);
            let promise = Promise::spawn_thread("", move || {
                let (lock, cond_var) = &*load_condition;
                let mut start_loading = lock.lock().unwrap();
                while !*start_loading {
                    start_loading = cond_var.wait(start_loading).unwrap()
                }

                let mut file = File::open(path)?;
                let mut bytes: Vec<u8> = vec![];
                file.read_to_end(&mut bytes)?;
                Ok(Arc::new(bytes))
            });
            promise
        })
    }

    pub fn is_importing(&self) -> bool {
        if let Some(promise) = self.importation_status.as_ref() {
            promise.ready().is_none() || self.match_importation_status(ImportationStatus::PendingBytes)
        } else {
            false
        }
    }

    pub fn are_bytes_loaded(&self) -> bool {
        if let Some(bytes_promise) = self.bytes.as_ref() {
            if let Some(bytes_res) = bytes_promise.ready() {
                if let Ok(_bytes) = bytes_res {
                    return true
                }
            }
        }
        false
    }

    pub fn is_loading_or_needs_to_load(&self) -> bool {
        let is_loading_thumbnail = if let Some(promise) = self.thumbnail.as_ref() {
            promise.ready().is_none()
        } else {
            true
        };

        let is_loading_mime_type = self.mime_type.is_none();
        let failed_mime_type = if let Some(mime_type_res) = self.mime_type.as_ref() {
            if let Err(_) = mime_type_res {
                true
            } else {
                false
            }
        } else {
            false
        };

        (is_loading_thumbnail || is_loading_mime_type) && !failed_mime_type
    }

    pub fn match_importation_status(&self, comparison_status: ImportationStatus) -> bool {
        match &self.importation_status {
            Some(promise) => match promise.ready() {
                Some(importation_result) => return *importation_result.as_ref() == comparison_status,
                None => {}
            },
            _ => {}
        }
        false
    }

    pub fn is_importable(&self) -> bool {
        let cannot_be_reimported =
            self.match_importation_status(ImportationStatus::Duplicate) || self.match_importation_status(ImportationStatus::Success);

        return !self.is_unloadable && !self.is_imported && !cannot_be_reimported;

    }

    pub fn unload_bytes_if_unnecessary(&mut self) {
        // If bytes are loaded,
        // we only need bytes to be loaded if thumbnail is still loading, or we are trying to import
        if let Some(promise) = self.bytes.as_ref() {
            if let Some(bytes_res) = promise.ready() {
                if let Ok(_bytes) = bytes_res {
                    if !(self.is_loading_or_needs_to_load() || self.is_importing()) {
                        self.bytes = None;
                    }
                }
            }
        }
    }

    pub fn get_status_label(&self) -> Option<String> {
        let mut statuses = vec![];
        let mut add = |message: &str| statuses.push(message.to_string());
        if self.is_hidden {
            add("hidden");
        };
        if self.is_imported {
            add("already imported")
        };
        if self.is_unloadable {
            add("unable to load")
        };
        if !self.try_check_if_is_to_be_loaded() {
            add("not yet loaded")
        };
        if self.match_importation_status(ImportationStatus::Success) {
            add("imported")
        }
        if self.match_importation_status(ImportationStatus::Duplicate) {
            add("duplicate")
        }
        if self.match_importation_status(ImportationStatus::Fail(anyhow::Error::msg(""))) {
            let error_message = {
                let error = match &self.importation_status {
                    Some(promise) => match promise.ready() {
                        Some(result) => match result.as_ref() {
                            ImportationStatus::Fail(error) => Some(format!("{error}")),
                            _ => None,
                        },
                        _ => None,
                    },
                    _ => None,
                };
                error.unwrap_or("unknown error".to_string())
                // "unknown error"
            };
            // statuses.push(format!("import failed due to: {error_message}"));
            add(format!("import failed due to: {error_message}").as_str())
        }
        if let Some(result) = &self.thumbnail {
            if let Some(Err(_err)) = result.ready() {
                add("couldn't generate thumbnail")
            }
        }

        let label = statuses.join(", ");

        if label.len() > 0 {
            Some(label)
        } else {
            None
        }
    }

    pub fn get_mime_type(&mut self) -> Option<&Result<Type, Error>> {
        match &self.mime_type {
            None => match self.get_bytes().ready() {
                None => {
                    // todo!();
                }
                    Some(bytes_result) => match bytes_result {
                        Err(_error) => {
                            self.mime_type = Some(Err(anyhow::Error::msg("failed to load bytes")));
                            self.is_unloadable = true;
                        }
                        Ok(bytes) => match infer::get(&bytes) {
                            Some(kind) => {
                                self.mime_type = Some(Ok(kind));
                            }
                            None => {
                                self.mime_type = Some(Err(anyhow::Error::msg("unknown file type")));
                                self.is_unloadable = true;
                            }
                        },
                    },
                },
            Some(_result) => {
                // todo!();
            }
        }
        self.mime_type.as_ref()
    }

    pub fn get_thumbnail(&mut self, thumbnail_size: u8) -> Option<&Promise<Result<RetainedImage, Error>>> {
        match &self.thumbnail {
            None => match self.get_bytes().ready() {
                None => {
                    // println!("hmm");
                    None
                },
                Some(result) => {
                    let (sender, promise) = Promise::new();
                    match result {
                        Err(_error) => {
                            // self.is_disabled = true;
                            sender.send(Err(anyhow::Error::msg("no bytes provided to load thumbnail")))
                        }
                        Ok(bytes) => {
                                let bytes = Arc::clone(bytes);
                                // let arc = Arc::new(bytes);
                                thread::spawn(move || {
                                    let bytes = &bytes as &[u8];
                                    // println!("{:?}", bytes.len());
                                    let image_res = MediaEntry::load_thumbnail(bytes, thumbnail_size);
                                    // println!("{:?}", image_res.is_err());
                                    sender.send(image_res);
                                });
                            }
                        }
                        self.thumbnail = Some(promise);
                        self.thumbnail.as_ref()
                    }
            },
            Some(_promise) => self.thumbnail.as_ref(),
        }
    }

    pub fn try_check_if_is_to_be_loaded(&self) -> bool {
        let (lock, _cond_var) = &*self.is_to_be_loaded;
        let is_to_be_loaded = lock.try_lock();
        match is_to_be_loaded {
            Err(_error) => {
                // lock being aquired by something else
                false
            }
            Ok(is_to_be_loaded) => *is_to_be_loaded,
        }
    }

    pub fn set_load_status(&mut self, load_status: bool) {
        // if !load_status {
        //     self.unload_bytes();
        // } else {
        //     // self.get_bytes();
        // }
        let (lock, cond_var) = &*self.is_to_be_loaded;
        let mut is_to_be_loaded = lock.lock().unwrap();
        *is_to_be_loaded = load_status;
        cond_var.notify_all();
    }

    pub fn load_thumbnail(image_data: &[u8], thumbnail_size: u8) -> Result<RetainedImage> {
        let pixels = data::generate_thumbnail(image_data, thumbnail_size)?;
        let img = ui::generate_retained_image(&pixels)?;
        Ok(img)
    }
}
