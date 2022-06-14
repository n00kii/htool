use custom_error::custom_error;
use hex;
use iced::pane_grid::Direction;
use image::io::Reader as ImageReader;
use image_hasher::{HashAlg, HasherConfig};
// use pihash::{cache, hash};
use sha256;

mod config;
use config::Config;

use std::{
    env, error,
    fs::{self, DirEntry, ReadDir},
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    thread,
};

use std::path::Path;
use path_absolutize::*;

fn main() {
    let path = "C:/Users/Moruph/OneDrive - Massachusetts Institute of Technology/Shared/htool2/htool2/testing_files";
    match read_images(&path) {
        Ok(()) => {
            println!("hashes complete!")
        }
        Err(e) => {
            println!("something went wrong: {e}")
        }
    }
}

custom_error! { MediaReadError
    UndefinedPath = "no path was set before trying to read media",
}

struct MediaHasher {
    current_path: Option<PathBuf>,
    hasher: image_hasher::Hasher,
}

struct MediaRegistrant {
    root_path: String,
    hashers: Vec<MediaHasher>,
}

impl MediaRegistrant {
    pub fn new() {}
}

impl MediaHasher {
    pub fn new(hasher_config: &HasherConfig) -> MediaHasher {
        MediaHasher {
            current_path: None,
            hasher: hasher_config.to_hasher(),
        }
    }

    pub fn set_file(&mut self, dir_entry: DirEntry) {
        self.current_path = Some(dir_entry.path());
    }

    // pub fn take_file(&self, dir_entries:Arc<Mutex<ReadDir>> ) -> Result<(), PoisonError<MutexGuard<ReadDir>>> {
    pub fn take_file_and_read(&mut self, dir_entries_arc: Arc<Mutex<ReadDir>>) {
        // let dir_entry = dir_entries.lock().unwrap().next().unwrap().unwrap();
        // self.set_file(dir_entry);
        println!("started");
        while true {
            let dir_entries = dir_entries_arc.lock();
            match dir_entries {
                Ok(mut dir_entries) => {
                    // println!("uhoh, arc was poisoned, did another thread panic?")
                    let dir_entry = dir_entries.next();
                    drop(dir_entries);
                    match dir_entry {
                        Some(Ok(dir_entry)) => {
                            self.set_file(dir_entry);
                            match self.read_media() {
                                Ok(()) => {
                                    println!("success");
                                }
                                Err(error) => {
                                    println!("something happened {error}");
                                    break;
                                }
                            }
                        }
                        Some(Err(error)) => {
                            // couldnt read dir entry, kill
                            println!("error reading dir {error}");
                            break;
                        }
                        None => {
                            // no more paths, kill
                            println!("no more paths available");
                            break;
                        }
                    }
                }

                Err(error) => {
                    println!("uhoh, arc was poisoned, did another thread panic? {error}");
                    break;
                }
            }
        }
    }

    fn read_media(&self) -> Result<(), Box<dyn error::Error>> {
        let path = self
            .current_path
            .as_ref()
            .ok_or(MediaReadError::UndefinedPath)?;
       
            let img = ImageReader::open(path)?.with_guessed_format()?.decode()?;
        let sha_hash = sha256::digest_bytes(&img.as_bytes());
        let p_hash = hex::encode(self.hasher.hash_image(&img).as_bytes());
        println!(
            "path: {} p_hash: {} sha_hash: {}",
            path.display(),
            p_hash,
            sha_hash
        );
        Ok(())
    }
}

fn read_images(path: &str) -> Result<(), Box<dyn error::Error>> {
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
                        println!("config: {:?}", config);
                        println!("{:?}", &config.path.root);
                        println!("{:?}", Path::new(&config.path.root).absolutize());
                        println!("{:?}", Config::save(&config));

                        // let p = Path::new(&config.path.root);
                        // assert_eq!("/path/to/123/456", p.absolutize().unwrap().to_str().unwrap());
                    }
                    Err(error) => {
                        println!("config load error: {:?}", error)
                    }
                }
            }
            "hash" => {
                println!("hashing!");

                let dir_entries_arc = Arc::new(Mutex::new(fs::read_dir(path)?));
                let mut handles = vec![];

                for _ in 0..10 {
                    let dir_entries_arc = Arc::clone(&dir_entries_arc);
                    let handle = thread::spawn(move || {
                        let hasher_config = HasherConfig::new().hash_alg(HashAlg::DoubleGradient);
                        let mut media_register = MediaHasher::new(&hasher_config);
                        media_register.take_file_and_read(dir_entries_arc)
                        // let dir_entry = dir_entries.lock().unwrap();
                    });
                    handles.push(handle);
                }

                for handle in handles {
                    handle.join().unwrap();
                }

                // for dir_entry in dir_entries {
                //     media_register.set_file(dir_entry?);
                //     media_register.read_media()?;
                // }

                // let paths = fs::read_dir(path)?;
                // for path in paths {
                //     let full_path = path?.path();
                //     let img = ImageReader::open(&full_path)?
                //         .with_guessed_format()?
                //         .decode()?;
                //     let sha_hash = sha256::digest_bytes(&img.as_bytes());
                //     let p_hash = hex::encode(hasher.hash_image(&img).as_bytes());
                //     println!(
                //         "path: {} p_hash: {} sha_hash: {}",
                //         &full_path.display(),
                //         p_hash,
                //         sha_hash
                //     )
                // }
            }
            _ => println!("?"),
        },
        _ => println!("wrong number of args provided"),
    }
    Ok(())
}
