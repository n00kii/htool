use super::ui;
use crate::data;
use crate::data::ImportationStatus;
use crate::data::RegistrationForm;
use crate::ui::preview_ui::MediaPreview;
use anyhow::{Error, Result};

use egui_extras::RetainedImage;
use poll_promise::Promise;

use std::collections::HashMap;

use std::{
    fs::{self, DirEntry, File},
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread::{self},
};

// #[derive(Clone)]
pub struct ImportationEntry {
    pub is_selected: bool,

    pub dir_entry: DirEntry,
    pub file_label: String,
    pub file_size: usize,
    pub keep_bytes_loaded: bool,

    pub linking_dir: Option<String>,
    pub bytes: Option<Promise<Result<Arc<Vec<u8>>>>>,
    pub thumbnail: Option<Promise<Result<MediaPreview>>>,
    pub is_archive: bool,
    pub importation_status: Option<Promise<ImportationStatus>>,
}

impl PartialEq for ImportationEntry {
    fn eq(&self, other: &Self) -> bool {
        self.dir_entry.path() == other.dir_entry.path()
    }
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
    extension_filter: &Vec<&String>,
) -> Result<Vec<ImportationEntry>> {
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
                let mut is_archive = false;
                if let Some(ext) = dir_entry_path.extension() {
                    if ext == "zip" {
                        is_archive = true;
                    }
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

                let file_size = dir_entry.metadata()?.len();
                scanned_dir_entries.push(ImportationEntry {
                    thumbnail: None,
                    keep_bytes_loaded: false,
                    file_size: file_size as usize,
                    dir_entry,
                    file_label,
                    bytes: None,
                    is_selected: false,
                    is_archive,
                    importation_status: None,
                    linking_dir: linking_dir.clone(),
                });
            }
        }
    }

    Ok(scanned_dir_entries)
}

impl ImportationEntry {
    pub fn generate_reg_form(&mut self, dir_link_map: Arc<Mutex<HashMap<String, i32>>>) -> Result<RegistrationForm> {
        let bytes = self.bytes.as_ref();
        let fail = |_message: String| -> Result<_, Error> { Err(anyhow::Error::msg("bytes not loaded")) };
        match bytes {
            None => fail("bytes not loaded".into()),
            Some(promise) => match promise.ready() {
                None => fail("bytes are still loading".into()),
                Some(Err(_error)) => fail("failed to load bytes".into()),
                Some(Ok(bytes)) => {
                    let bytes = Arc::clone(bytes);
                    let dir_link_map = Arc::clone(&dir_link_map);
                    let linking_value: Option<i32> = self.dir_entry.path().file_stem().and_then(|fs| fs.to_string_lossy().parse().ok());
                    let linking_dir = self.linking_dir.clone();
                    let (sender, promise) = Promise::new();
                    self.importation_status = Some(promise);
                    Ok(RegistrationForm {
                        bytes,
                        mimetype: mime_guess::from_path(self.dir_entry.path()),
                        linking_value,
                        importation_result_sender: sender,
                        linking_dir,
                        dir_link_map,
                    })
                }
            },
        }
    }

    pub fn load_bytes(&mut self) {
        let path = self.dir_entry.path().clone();
        let promise = Promise::spawn_thread("load_import_entry_bytes", move || {
            let mut file = File::open(path)?;
            let mut bytes: Vec<u8> = vec![];
            file.read_to_end(&mut bytes)?;
            Ok(Arc::new(bytes))
        });
        self.bytes = Some(promise)
    }

    pub fn is_importing(&self) -> bool {
        self.match_importation_status(ImportationStatus::Pending)
            || if let Some(promise) = self.importation_status.as_ref() {
                promise.ready().is_none()
            } else {
                false
            }
    }

    pub fn are_bytes_loaded(&self) -> bool {
        if let Some(bytes_promise) = self.bytes.as_ref() {
            if let Some(bytes_res) = bytes_promise.ready() {
                if let Ok(_bytes) = bytes_res {
                    return true;
                }
            }
        }
        false
    }

    pub fn is_thumbnail_loading(&self) -> bool {
        if let Some(promise) = self.thumbnail.as_ref() {
            promise.ready().is_none()
        } else {
            false
        }
    }

    pub fn match_importation_status(&self, comparison_status: ImportationStatus) -> bool {
        match self.importation_status.as_ref() {
            Some(promise) => match promise.ready() {
                Some(importation_status) => return importation_status == &comparison_status,
                None => false,
            },
            None => false,
        }
    }

    pub fn is_importable(&self) -> bool {
        if let Some(importation_promise) = self.importation_status.as_ref() {
            if let Some(importation_status) = importation_promise.ready() {
                match importation_status {
                    ImportationStatus::Fail(_) => true,
                    _ => false,
                }
            } else {
                false
            }
        } else {
            true
        }
    }

    pub fn get_status_label(&self) -> Option<String> {
        let mut statuses = vec![];
        let mut add = |message: &str| statuses.push(message.to_string());
        if self.is_importing() {
            add("importing...")
        }
        // if self.failed_to_load_type() {
        //     add("unable to read file type")
        // };
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
                        Some(result) => match result {
                            ImportationStatus::Fail(error) => Some(format!("{error}")),
                            _ => None,
                        },
                        _ => None,
                    },
                    _ => None,
                };
                error.unwrap_or("unknown importation error".to_string())
                // "unknown error"
            };
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

    pub fn load_thumbnail(&mut self) {
        if let Some(bytes_promise) = self.bytes.as_ref() {
            if let Some(bytes_res) = bytes_promise.ready() {
                let (sender, promise) = Promise::new();

                match bytes_res {
                    Err(_error) => sender.send(Err(anyhow::Error::msg("no bytes provided to load thumbnail"))),
                    Ok(bytes) => {
                        let bytes = Arc::clone(bytes);
                        thread::spawn(move || {
                            let bytes = &bytes as &[u8];
                            let generate_image = || -> Result<MediaPreview> {
                                let pixels = data::generate_media_thumbnail(bytes, false)?;
                                Ok(MediaPreview::Picture(ui::generate_retained_image(&pixels)?))
                            };
                            sender.send(generate_image());
                        });
                    }
                }

                self.thumbnail = Some(promise);
            }
        }
    }
}
