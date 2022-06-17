use std::{fs};

use anyhow::{Context, Result};
use serde::{Serialize, Deserialize};
use figment::{Figment, providers::{Format, Toml, Serialized}};

const CONFIG_FILENAME: &str = "config.toml";

#[derive(Debug, Deserialize, Serialize)]
pub struct Path {
    pub root: String,
    pub landing: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Ui {
    pub import: Import,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Import {
    pub thumbnail_size: u8,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Hash {
    pub hashing_threads: u8,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub version: u8,
    pub path: Path,
    pub hash: Hash,
    pub ui: Ui,
}
impl Default for Config {
    fn default() -> Config {
        Config {
            version: 0,
            path: Path {
                root: "./files".into(),
                landing: "C:/Users/Moruph/OneDrive - Massachusetts Institute of Technology/Shared/htool2/htool2/testing_files".into(),
            },
            hash: Hash { 
                hashing_threads: 10 
            },
            ui: Ui { 
                import: Import { 
                    thumbnail_size: 100 
                } 
            }
        }
    }
}

impl Config {
    pub fn figment() -> Figment {
        Figment::from(Serialized::defaults(Config::default()))
            .merge(Toml::file(CONFIG_FILENAME))
            // .merge(Env::prefixed("APP_"))
    }

    pub fn load() -> Result<Config>{
        Config::figment().extract().context("couldn't deserialize config")
    }

    pub fn save(config: &Config) -> Result<()>{
        let toml_string = toml::to_string(config).context("couldn't serialize config")?;
        fs::write(CONFIG_FILENAME, toml_string).context("couldn't write config")?;

        Ok(())
    }
}

