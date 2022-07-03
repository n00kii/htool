use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result};
use figment::{
    providers::{Format, Serialized, Toml},
    Figment,
};
use path_absolutize::*;
use serde::{Deserialize, Serialize};

const CONFIG_FILENAME: &str = "config.toml";

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Path {
    pub root: Option<String>,
    pub landing: String,
    pub database: String,
}

impl Path {
    pub fn current_root(&self) -> Result<PathBuf> {
        if self.root.is_some() {
            return Ok(PathBuf::from(self.root.as_ref().unwrap()).absolutize()?.to_path_buf());
        } else {
            let exe = env::current_exe().context("couldn't get parent path")?;

            let parent_path = exe.parent().ok_or(anyhow::Error::msg("message"))?;
            let parent_path_buf = PathBuf::from(parent_path);
            Ok(parent_path_buf)
        }
    }

    pub fn absolutize_path(&self, local_root_path: &String) -> Result<PathBuf> {
        let root_path_buf = self.current_root()?;
        let local_root_path = PathBuf::from(local_root_path);

        let absolutized_path_buf = root_path_buf.join(&local_root_path);
        Ok(absolutized_path_buf)
    }

    pub fn landing(&self) -> Result<PathBuf> {
        let landing = self.absolutize_path(&self.landing)?;
        fs::create_dir_all(&landing)?;
        Ok(landing)
    }
    pub fn database(&self) -> Result<PathBuf> {
        self.absolutize_path(&self.database)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Ui {
    pub import: Import,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Import {
    pub thumbnail_size: u8,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Hash {
    pub hashing_threads: u8,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
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
                root: Some("./root".into()),
                landing: "landing/".into(),
                database: "data.db".into(),
            },
            hash: Hash { hashing_threads: 10 },
            ui: Ui {
                import: Import { thumbnail_size: 100 },
            },
        }
    }
}

impl Config {
    pub fn figment() -> Figment {
        Figment::from(Serialized::defaults(Config::default())).merge(Toml::file(CONFIG_FILENAME))
        // .merge(Env::prefixed("APP_"))
    }

    pub fn load() -> Result<Config> {
        Config::figment().extract().context("couldn't deserialize config")
    }

    pub fn save(config: &Config) -> Result<()> {
        let toml_string = toml::to_string(config).context("couldn't serialize config")?;
        fs::write(CONFIG_FILENAME, toml_string).context("couldn't write config")?;

        Ok(())
    }
}
