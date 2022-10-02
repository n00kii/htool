use crate::tags::tags::Tag;

use super::super::data;
use super::super::Config;
use super::super::ui;
use anyhow::{Context, Error, Result};
use egui_extras::RetainedImage;
use poll_promise::Promise;
use std::any::Any;
use std::fmt;
use std::sync::Arc;
use downcast_rs as downcast;


pub struct GalleryEntry {
    pub hash: String,
    pub thumbnail: Option<Promise<Result<RetainedImage>>>,
}

pub struct GalleryEntryPlural {
    pub hashes: Vec<String>,
    pub link_id: i32,
    pub thumbnail: Option<Promise<Result<RetainedImage>>>,
}

pub trait GalleryItem: downcast::Downcast {
    fn get_thumbnail(&mut self) -> Option<&Promise<Result<RetainedImage, Error>>>;
    fn get_thumbnail_without_loading(&self) -> Option<&Promise<Result<RetainedImage, Error>>>;
    fn get_status_label(&self) -> Option<String> {
        let mut statuses = vec![];

        let mut add = |message: &str| statuses.push(message.to_string());
        if let Some(result) = &self.get_thumbnail_without_loading() {
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
    fn includes_tags_and(&self) {

    }
}

downcast::impl_downcast!(GalleryItem);

impl GalleryItem for GalleryEntry {
    fn get_thumbnail_without_loading(&self) -> Option<&Promise<Result<RetainedImage, Error>>> {
        self.thumbnail.as_ref()
    }
    fn get_thumbnail(&mut self) -> Option<&Promise<Result<RetainedImage, Error>>> {
        match &self.thumbnail {
            None => {
                let hash = self.hash.clone();
                let promise = Promise::spawn_thread("", move|| {
                    let thumbnail_res = data::load_thumbnail(&hash);
                    match thumbnail_res {
                        Ok(thumbnail_buffer) => {
                            // let image 
                            let image = ui::generate_retained_image(&thumbnail_buffer);
                            image
                        },
                        Err(error) => {
                            Err(error)
                        }
                    }
                });
                self.thumbnail = Some(promise);
                return self.thumbnail.as_ref();
            }
            Some(_promise) => self.thumbnail.as_ref(),
        }
    }
}

impl GalleryItem for GalleryEntryPlural {
    fn get_thumbnail_without_loading(&self) -> Option<&Promise<Result<RetainedImage, Error>>> {
        self.thumbnail.as_ref()
    }
    fn get_thumbnail(&mut self) -> Option<&Promise<Result<RetainedImage, Error>>> {
        match &self.thumbnail {
            None => {
                let link_id = self.link_id;
                let promise = Promise::spawn_thread("", move|| {
                    let thumbnail_res = data::load_thumbnail_plural( link_id);
                    match thumbnail_res {
                        Ok(thumbnail_buffer) => {
                            // let image 
                            let image = ui::generate_retained_image(&thumbnail_buffer);
                            image
                        },
                        Err(error) => {
                            Err(error)
                        }
                    }
                });
                self.thumbnail = Some(promise);
                return self.thumbnail.as_ref();
            }
            Some(_promise) => self.thumbnail.as_ref(),
        }
    }
}

pub fn load_gallery_items(search_string: &String) -> Result<Vec<Box<dyn GalleryItem>>> {
    // TODO: this seems really inefficient
    let search_tags = search_string.split_whitespace().map(|tagstring| Tag::from_tagstring(&tagstring.to_string())).collect::<Vec<_>>();
    dbg!(&search_tags);
    let all_hashes = data::get_all_hashes()?;
    let mut gallery_entries: Vec<GalleryEntry> = Vec::new();
    let mut gallery_entries_plural: Vec<GalleryEntryPlural> = Vec::new();
    let mut resolved_links: Vec<i32> = Vec::new();
    for hash in all_hashes {
        let links = data::get_media_links_of_hash(&hash)?;
        if links.len() > 0 {
            for link_id in links {
                if resolved_links.contains(&link_id) {
                    continue;
                }
                let hashes_of_link = data::get_hashes_of_media_link(link_id)?;
                resolved_links.push(link_id);
                gallery_entries_plural.push(GalleryEntryPlural {
                    hashes: hashes_of_link,
                    link_id,
                    thumbnail: None,
                });
            }
        } else {
            gallery_entries.push(GalleryEntry { hash, thumbnail: None })
        }
    }

    let mut gallery_items: Vec<Box<dyn GalleryItem>> = Vec::new();
    for gallery_entry in gallery_entries {
        let media_info = data::load_media_info(&gallery_entry.hash)?;
        if search_tags.len() > 0 {
            if !media_info.includes_tags_and(&search_tags) {
                continue;
            }
        }
        gallery_items.push(Box::new(gallery_entry));
    }
    for gallery_entry_plural in gallery_entries_plural {
        gallery_items.push(Box::new(gallery_entry_plural));
    }

    Ok(gallery_items)
    // todo!()
}
