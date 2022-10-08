use crate::data::EntryId;
use crate::tags::tags::Tag;

use super::super::data;
use super::super::ui;
use super::super::Config;
use anyhow::{Context, Error, Result};
use downcast_rs as downcast;
use egui_extras::RetainedImage;
use poll_promise::Promise;
use std::any::Any;
use std::fmt;
use std::sync::Arc;
use std::thread;
use data::EntryInfo;
use std::time::Duration;

pub struct GalleryEntry {
    // pub entry_id: EntryId,
    pub entry_info: EntryInfo,
    pub thumbnail: Option<Promise<Result<RetainedImage>>>,
}

impl PartialEq for GalleryEntry {
    fn eq(&self, other: &Self) -> bool {
        self.entry_info == other.entry_info 
    }
}

impl GalleryEntry {
    pub fn is_loading(&self) -> bool {
        if let Some(thumbnail_promise) = self.thumbnail.as_ref() {
            thumbnail_promise.ready().is_none()
        } else {
            false
        }
    }
    pub fn is_loaded(&self) -> bool {
        if let Some(thumbnail_promise) = self.thumbnail.as_ref() {
            thumbnail_promise.ready().is_some()
        } else {
            false
        }
    }
    pub fn load_thumbnail(&mut self) {
        let entry_id = self.entry_info.entry_id().clone();
        self.thumbnail = Some(Promise::spawn_thread("load_gallery_entry_thumbail", move || {
            match data::load_thumbnail(&entry_id) {
                Ok(thumbnail_buffer) => {
                    // let image
                    let image = ui::generate_retained_image(&thumbnail_buffer);
                    image
                }
                Err(error) => Err(error),
            }
        }));
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


pub fn load_gallery_entries(search_string: &String) -> Result<Vec<GalleryEntry>> {
    // TODO: this seems really inefficient


    let all_hashes = data::get_all_hashes()?;
    let mut gallery_entries: Vec<GalleryEntry> = Vec::new();
    let mut resolved_links: Vec<i32> = Vec::new();
    for hash in all_hashes {
        let links = data::get_media_links_of_hash(&hash)?;
        if links.len() > 0 {
            for link_id in links {
                if resolved_links.contains(&link_id) {
                    continue;
                } else {
                    resolved_links.push(link_id);
                }
                let entry_id = EntryId::PoolEntry(link_id);
                gallery_entries.push(GalleryEntry {
                    entry_info: data::load_entry_info(&entry_id)?,
                    thumbnail: None,
                });
            }
        } else {
            let entry_id = EntryId::MediaEntry(hash);
            gallery_entries.push(GalleryEntry {
                // entry_id: EntryId::MediaEntry(hash),
                entry_info: data::load_entry_info(&entry_id)?,
                thumbnail: None,
            })
        }
    }

    // gallery_entries.retain(|gallery_entry| {
    //     if let EntryId::MediaEntry(hash) = &gallery_entry.entry_info.entry_id() {
    //         let media_info_res = data::load_media_info(hash);
    //         if let Ok(media_info) = media_info_res {
    //             if !media_info.details.includes_tags_and(&search_tags) {
    //                 return false;
    //             }
    //         }
    //     }
    //     true
    // });

    Ok(gallery_entries)
}
