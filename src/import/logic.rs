use super::super::Config;
use anyhow::{Context, Error, Result};
use eframe::egui;
use egui_extras::RetainedImage;
use image::imageops;
use image::io::Reader as ImageReader;
use image_hasher::{HashAlg, HasherConfig};
use poll_promise::Promise;
use rusqlite::{Connection, Result as SqlResult, params};
use std::{
    fs::{self, DirEntry, File, ReadDir},
    io::Read,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, RwLock},
    thread::{self, JoinHandle},
};

pub struct MediaEntry {
    pub is_disabled: bool,
    pub is_ignored: bool,
    pub is_imported: bool,
    pub is_selected: bool,
    pub is_to_be_loaded: Arc<(Mutex<bool>, Condvar)>,

    pub dir_entry: DirEntry,
    pub mime_type: Option<Result<String>>,
    pub file_label: String,

    pub bytes: Option<Promise<Result<Arc<Vec<u8>>>>>,
    pub thumbnail: Option<Promise<Result<RetainedImage>>>,
}

pub fn import_media(media_entries: Vec<&MediaEntry>, config: &Config) -> Result<()> {
    for media_entry in media_entries {
        let bytes = media_entry.bytes.as_ref();
        let db_path = &config.path.database;
        match bytes {
            None => return Err(anyhow::Error::msg("no bytes to import")),
            Some(promise) => {
                match promise.ready() {
                    None => return Err(anyhow::Error::msg("bytes are stil loading")),
                    Some(Err(error)) => return Err(anyhow::Error::msg("failed to load bytes")),
                    Some(Ok(bytes)) => {

                        let bytes = Arc::clone(bytes);
                        let db_path = db_path.clone();
                        thread::spawn(move || -> Result<()> {
                            let bytes = &*bytes as &[u8];
                            println!("{} kB", bytes.len() / 1000);
                            let conn = Connection::open(&db_path)?;

                            println!("starting loading of {} kB", bytes.len());
                            conn.execute(
                                "CREATE TABLE IF NOT EXISTS media_blobs (
                                    id  INTEGER PRIMARY KEY,
                                    data  BLOB
                                )",
                                [], // empty list of parameters.
                            )?;

                            conn.execute(
                                "INSERT INTO media_blobs (data) VALUES (?1)",
                                params![bytes],
                            )?;

                            // conn.close();

                            // conn.execute(
                            //     "CREATE TABLE IF NOT EXISTS media_info (
                            //         id  INTEGER PRIMARY KEY,
                            //         hash TEXT,
                            //         info  BLOB
                            //     )",
                            //     [], // empty list of parameters.
                            // )?;

                            // conn.execute(
                            //     "INSERT INTO media_blobs (id, data) VALUES (?1, ?2)",
                            //     params![me.name, me.data],
                            // )?;
                            
                            // db_path;
                            // bytes;
                            Ok(())
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

struct MediaImporter {
    current_path: Option<PathBuf>,
    hasher: image_hasher::Hasher,
}
struct MediaImportationManager {
    importers: Vec<Arc<Mutex<MediaImporter>>>,
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

    pub fn is_importable(&self) -> bool {
        match &self.bytes {
            None => false,
            Some(promise) => match promise.ready() {
                None => false,
                Some(_) => return !self.is_disabled && !self.is_imported,
            },
        }
    }

    pub fn unload_bytes(&mut self) {
        self.bytes = None
    }

    pub fn get_mime_type(&mut self) -> Option<&Result<String, Error>> {
        match &self.mime_type {
            None => match self.get_bytes().ready() {
                None => {
                    // todo!();
                }
                Some(bytes_result) => match bytes_result {
                    Err(_error) => {
                        self.mime_type = Some(Err(anyhow::Error::msg("failed to load bytes")));
                        self.is_disabled = true;
                    }
                    Ok(bytes) => match infer::get(&bytes) {
                        Some(kind) => {
                            self.mime_type = Some(Ok(kind.mime_type().to_string()));
                        }
                        None => {
                            self.mime_type = Some(Err(anyhow::Error::msg("unknown file type")));
                            self.is_disabled = true;
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
                            let bytes_copy = Arc::clone(bytes);
                            // let arc = Arc::new(bytes);
                            thread::spawn(move || {
                                let bytes = &bytes_copy as &[u8];
                                let image = MediaEntry::load_image_from_memory(bytes, 100);
                                sender.send(image);
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
        let image_cropped = imageops::crop_imm(
            &image,
            if h > w { 0 } else { (w - h) / 2 },
            if w > h { 0 } else { (h - w) / 2 },
            if h > w { w } else { h },
            if w > h { h } else { w },
        )
        .to_image();
        let thumbnail =
            imageops::thumbnail(&image_cropped, thumbnail_size.into(), thumbnail_size.into());
        let size = [thumbnail.width() as usize, thumbnail.height() as usize];
        let pixels = thumbnail.as_flat_samples();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
        Ok(RetainedImage::from_color_image("", color_image))
    }
}

impl MediaImportationManager {
    // pub fn new(config: &Config) -> Self {
    //     let hasher_config = HasherConfig::new().hash_alg(HashAlg::DoubleGradient);
    //     let importers: Vec<Arc<Mutex<MediaImporter>>> = (0..config.hash.hashing_threads)
    //         .map(|_| Arc::new(Mutex::new(MediaImporter::new(&hasher_config))))
    //         .collect();

    //     Self { importers }
    // }

    // pub fn run(&mut self, config: &Config) -> Result<()> {
    //     let dir_entries_arc = Arc::new(Mutex::new(
    //         fs::read_dir(&config.path.landing).context("couldn't read landing dir")?,
    //     ));
    //     let mut threads = vec![];
    //     for importer_arc in &self.importers {
    //         let importer_lock_clone = Arc::clone(importer_arc);
    //         let thread = MediaImporter::start(importer_lock_clone, Arc::clone(&dir_entries_arc));
    //         threads.push(thread);
    //         // thread.unwrap().join().unwrap();
    //     }

    //     for thread in threads {
    //         thread?.join();
    //     }

    //     Ok(())
    // }
}

impl MediaImporter {
    pub fn new(hasher_config: &HasherConfig) -> Self {
        MediaImporter {
            current_path: None,
            hasher: hasher_config.to_hasher(),
        }
    }

    pub fn set_file(&mut self, dir_entry: &DirEntry) {
        self.current_path = Some(dir_entry.path());
    }

    // pub fn start(
    //     self_arc: Arc<Mutex<Self>>,
    //     dir_entries_arc: Arc<Mutex<ReadDir>>,
    // ) -> Result<JoinHandle<Result<()>>> {
    //     let thread = thread::spawn(move || {
    //         let mut self_mutex = self_arc.lock().expect("fuck");
    //         loop {
    //             let dir_entries_mutex = dir_entries_arc.lock();
    //             match dir_entries_mutex {
    //                 Ok(mut dir_entries) => {
    //                     let dir_entry = dir_entries.next().context("couldn't read dir entry")??;
    //                     drop(dir_entries);
    //                     self_mutex.set_file(&dir_entry);
    //                     self_mutex.read_media();
    //                 }
    //                 Err(_e) => {
    //                     println!("dir_entries poisoned! can't access mutex anymore")
    //                 }
    //             }
    //         }
    //         Ok(())
    //     });

    //     Ok(thread)
    // }

    // fn read_media(&self) -> Result<()> {
    //     let path = self.current_path.as_ref().context("context")?;

    //     let img = ImageReader::open(path)?.with_guessed_format()?.decode()?;
    //     let sha_hash = sha256::digest_bytes(&img.as_bytes());
    //     let p_hash = hex::encode(self.hasher.hash_image(&img).as_bytes());
    //     println!(
    //         "path: {} p_hash: {} sha_hash: {}",
    //         path.display(),
    //         p_hash,
    //         sha_hash
    //     );
    //     Ok(())
    // }
}
