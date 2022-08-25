use anyhow::{Context, Result};
use hex;
// use iced::pane_grid::Direction;
use image::io::Reader as ImageReader;
use image_hasher::{HashAlg, HasherConfig};
// use pihash::{cache, hash};
use sha256;

mod ui;
mod config;
mod data;
mod gallery;
mod import;
mod tags;

use config::Config;
use data::Data;

use std::{
    env,
    fs::{self, DirEntry, ReadDir},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use path_absolutize::*;
use std::path::Path;

fn main() -> Result<()> {
    let config = Arc::new(Config::load()?);
    let args: Vec<String> = env::args().collect();
    if let Some(command) = args.get(1) {
        match command.as_str() {

            "test" => {
                let mut app = ui::UserInterface::new(Arc::clone(&config));
                app.load_docked_windows();
                // app.launch_preview();
                ui::UserInterface::start(app);
            }

            _ => println!("unknown command {command}")
        }
    } else {
        println!("no command")
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

impl MediaImportationManager {
    pub fn new(config: &Config) -> Self {
        let hasher_config = HasherConfig::new().hash_alg(HashAlg::DoubleGradient);
        let importers: Vec<Arc<Mutex<MediaImporter>>> = (0..config.hash.hashing_threads)
            .map(|_| Arc::new(Mutex::new(MediaImporter::new(&hasher_config))))
            .collect();

        Self { importers }
    }

    pub fn run(&mut self, config: &Config) -> Result<()> {
        let landing_path = config.path.landing()?;
        let dir_entries_arc = Arc::new(Mutex::new(fs::read_dir(landing_path).context("couldn't read landing dir")?));
        let mut threads = vec![];
        for importer_arc in &self.importers {
            let importer_lock_clone = Arc::clone(importer_arc);
            let thread = MediaImporter::start(importer_lock_clone, Arc::clone(&dir_entries_arc));
            threads.push(thread);
            // thread.unwrap().join().unwrap();
        }

        for thread in threads {
            thread?.join();
        }

        Ok(())
    }
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

    pub fn start(self_arc: Arc<Mutex<Self>>, dir_entries_arc: Arc<Mutex<ReadDir>>) -> Result<JoinHandle<Result<()>>> {
        let thread = thread::spawn(move || {
            let mut self_mutex = self_arc.lock().expect("fuck");
            loop {
                let dir_entries_mutex = dir_entries_arc.lock();
                match dir_entries_mutex {
                    Ok(mut dir_entries) => {
                        let dir_entry = dir_entries.next().ok_or(anyhow::Error::msg("message"))??;
                        drop(dir_entries);
                        self_mutex.set_file(&dir_entry);
                        self_mutex.read_media();
                    }
                    Err(_e) => {
                        println!("dir_entries poisoned! can't access mutex anymore")
                    }
                }
            }
            Ok(())
        });

        Ok(thread)
    }

    fn read_media(&self) -> Result<()> {
        let path = self.current_path.as_ref().ok_or(anyhow::Error::msg("message"))?;

        let img = ImageReader::open(path)?.with_guessed_format()?.decode()?;
        let sha_hash = sha256::digest_bytes(&img.as_bytes());
        let p_hash = hex::encode(self.hasher.hash_image(&img).as_bytes());
        println!("path: {} p_hash: {} sha_hash: {}", path.display(), p_hash, sha_hash);
        Ok(())
    }
}

fn read_images(path: &str) -> Result<()> {
    let args: Vec<String> = env::args().collect();
    match args.len() {
        2 => match args[1].as_str() {
            "hello" => {
                println!("hello!")
            }
            "settings" => {
                let config = Config::load();

                // configment.extract_inner("path")
                // let path: config::Path = ;
                match config {
                    Ok(config) => {
                        // let pathBuf = PathBuf::from();
                        // println!("config: {:?}", config);
                        // println!("{:?}", &config.path.root);
                        // println!("{:?}", Path::new(&config.path.root).absolutize());
                        // println!("{:?}", Config::save(&config));

                        // let p = Path::new(&config.path.root);
                        // assert_eq!("/path/to/123/456", p.absolutize().unwrap().to_str().unwrap());
                    }
                    Err(error) => {
                        println!("config load error: {:?}", error)
                    }
                }
            }
            "ui" => {
                // ui::main();
            }
            "hash" => {
                println!("hashing!");

                let _dir_entries_arc = Arc::new(Mutex::new(fs::read_dir(path)?));
                let config = Config::load()?;
                let mut importation_manager = MediaImportationManager::new(&config);
                match importation_manager.run(&config) {
                    Ok(()) => {
                        println!("nice!")
                    }
                    Err(e) => {
                        println!("ugh, {e}")
                    }
                }
            }
            _ => println!("?"),
        },
        _ => println!("wrong number of args provided"),
    }
    Ok(())
}
