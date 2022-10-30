use super::data;
use crate::data::CompleteDataRequest;
use crate::data::DataRequest;

use anyhow::Result;
use data::EntryInfo;

use egui_extras::RetainedImage;
use poll_promise::Promise;

use parking_lot::Mutex;
use std::sync::Arc;

pub struct GalleryEntry {
    pub is_selected: bool,
    pub is_info_dirty: bool,
    pub did_complete_request: bool,
    pub entry_info: Arc<Mutex<EntryInfo>>,
    pub updated_entry_info: Option<Promise<Result<EntryInfo>>>,
    pub thumbnail: Option<Promise<Result<RetainedImage>>>,
}

//eg exclude![score=5] bookmarked=true blue_eyes brown_hair include![independant=false] type=pool
pub struct FilterOptions {}

impl PartialEq for GalleryEntry {
    fn eq(&self, other: &Self) -> bool {
        if let Some(self_info) = self.entry_info.try_lock() {
            if let Some(other_info) = other.entry_info.try_lock() {
                return *self_info == *other_info;
            }
        }
        true
    }
}

impl GalleryEntry {
    pub fn is_thumbnail_loading(&self) -> bool {
        if let Some(thumbnail_promise) = self.thumbnail.as_ref() {
            thumbnail_promise.ready().is_none()
        } else {
            false
        }
    }
    pub fn is_thumbnail_loaded(&self) -> bool {
        if let Some(thumbnail_promise) = self.thumbnail.as_ref() {
            thumbnail_promise.ready().is_some()
        } else {
            false
        }
    }
    pub fn is_refreshing(&self) -> bool {
        if let Some(info_promise) = self.updated_entry_info.as_ref() {
            info_promise.ready().is_none()
        } else {
            false
        }
    }

    pub fn generate_data_request<T: Send>(&self) -> (DataRequest<T>, Promise<Result<T>>) {
        let entry_info = self.entry_info.lock();
        let (sender, promise) = Promise::new();
        (
            DataRequest {
                entry_id: entry_info.entry_id().clone(),
                sender,
            },
            promise,
        )
    }

    pub fn generate_load_request(&mut self) -> CompleteDataRequest {
        let entry_info = self.entry_info.lock();
        let (info_sender, info_promise) = Promise::new();
        let (thumbnail_sender, thumbnail_promise) = Promise::new();
        self.updated_entry_info = Some(info_promise);
        self.thumbnail = Some(thumbnail_promise);
        self.did_complete_request = true;
        CompleteDataRequest {
            info_request: DataRequest {
                entry_id: entry_info.entry_id().clone(),
                sender: info_sender,
            },
            thumbnail_request: DataRequest {
                entry_id: entry_info.entry_id().clone(),
                sender: thumbnail_sender,
            },
        }
    }
    // pub fn generate_data_request<T: Send>(&self) -> Option<(DataRequest<T>, Promise<Result<T>>)> {
    //     if let Some(entry_info) = self.entry_info.try_lock() {
    //         let (sender, promise) = Promise::new();
    //         Some((
    //             DataRequest {
    //                 entry_id: entry_info.entry_id().clone(),
    //                 sender,
    //             },
    //             promise,
    //         ))
    //     } else {
    //         None
    //     }
    // }

    pub fn generate_entry_info_request(&mut self) -> DataRequest<EntryInfo> {
        let (request, promise) = self.generate_data_request();
        self.updated_entry_info = Some(promise);
        request
        // if let Some((request, promise)) = self.generate_data_request() {
        //     self.updated_entry_info = Some(promise);
        //     Some(request)
        // } else {
        //     None
        // }
    }
    // pub fn generate_entry_info_request(&mut self) -> Option<DataRequest<EntryInfo>> {
    //     if let Some((request, promise)) = self.generate_data_request() {
    //         self.updated_entry_info = Some(promise);
    //         Some(request)
    //     } else {
    //         None
    //     }
    // }

    // pub fn generate_thumbnail_request(&mut self) -> Option<DataRequest<RetainedImage>> {
    //     if let Some(entry_info) = self.entry_info.try_lock() {
    //         let (sender, promise) = Promise::new();
    //         self.thumbnail = Some(promise);
    //         Some(DataRequest {
    //             entry_id: entry_info.entry_id().clone(),
    //             sender,
    //         })
    //     } else {
    //         None
    //     }
    // }

    pub fn get_status_label(&self) -> Option<String> {
        let mut statuses = vec![];

        let mut add = |message: &str| statuses.push(message.to_string());
        if let Some(result) = self.thumbnail.as_ref() {
            if let Some(Err(err)) = result.ready() {
                add(format!("couldn't load thumbnail: {err}").as_str());
            }
        }

        let label = statuses.join(", ");

        if label.len() > 0 {
            Some(label)
        } else {
            None
        }
    }
}

pub fn load_gallery_entries() -> Result<Vec<GalleryEntry>> {
    Ok(data::load_all_entry_info()?
        .into_iter()
        .map(|entry_info| GalleryEntry {
            is_info_dirty: false,
            is_selected: false,
            did_complete_request: false,
            entry_info: Arc::new(Mutex::new(entry_info)),
            thumbnail: None,
            updated_entry_info: None,
        })
        .collect())
}
