use crate::data::EntryId;
use crate::data::ThumbnailRequest;
use crate::tags::tags::Tag;

use super::super::data;
use super::super::ui;
use super::super::Config;
use anyhow::{Context, Error, Result};
use data::EntryInfo;
use downcast_rs as downcast;
use egui_extras::RetainedImage;
use poll_promise::Promise;
use std::any::Any;
use std::fmt;
use std::sync::Arc;
// use std::sync::Mutex;
use parking_lot::Mutex;
use std::thread;
use std::time::Duration;

pub struct GalleryEntry {
    pub is_info_dirty: bool,
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
    // pub fn load_thumbnail(&mut self) {
    //     if let Some(entry_info) = self.entry_info.try_lock() {
    //         let entry_id = entry_info.entry_id().clone();
    //         self.thumbnail = Some(Promise::spawn_thread(format!("load_gallery_entry_thumbail_({:?})", self.entry_info.try_lock().map(|info| info.entry_id().clone())), move || {
    //             match data::load_thumbnail_with_conn(&entry_id) {
    //                 Ok(thumbnail_buffer) => {
    //                     let image = ui::generate_retained_image(&thumbnail_buffer);
    //                     image
    //                 }
    //                 Err(error) => Err(error),
    //             }
    //         }));
    //     }
    // }

    pub fn generate_thumbnail_request(&mut self) -> Option<ThumbnailRequest> {
        if let Some(entry_info) = self.entry_info.try_lock() {
            let (sender, promise) = Promise::new();
            self.thumbnail = Some(promise);
            Some(ThumbnailRequest {
                entry_id: entry_info.entry_id().clone(),
                thumbnail_sender: sender,
            })
        } else {
            None
        }
    }

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
            entry_info: Arc::new(Mutex::new(entry_info)),
            thumbnail: None,
            updated_entry_info: None,
        })
        .collect())
}
