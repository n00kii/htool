use anyhow::{Context, Result};
use arc_swap::{ArcSwap, Guard};
use figment::{
    providers::{Format, Serialized, Toml},
    Figment,
};
use once_cell::sync::OnceCell;
use path_absolutize::*;
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::PathBuf,
    sync::{Arc, RwLock, RwLockReadGuard},
};

use crate::tags::tags::Namespace;

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
    pub path: Path,
    pub namespaces: Vec<Namespace>,
}


impl Default for Config {
    fn default() -> Self {
        Self {
            path: Path {
                root: Some("./root".into()),
                landing: "landing/".into(),
                database: "data.db".into(),
            },
            namespaces: vec![],
        }
    }
}

static INSTANCE: OnceCell<ArcSwap<Config>> = OnceCell::new();

impl Config {
    pub fn global() -> Guard<Arc<Config>> {
        INSTANCE.get().expect("uninitalized config").load()
    }

    pub fn clone() -> Config {
        (&**Self::global()).clone()
    }

    pub fn figment() -> Figment {
        Figment::from(Serialized::defaults(Self::default())).merge(Toml::file(CONFIG_FILENAME))
    }

    pub fn set(new_config: Config) {
        INSTANCE.get().expect("uninitalized config").store(Arc::new(new_config));
    }

    pub fn load() {
        let config: Config = Config::figment().extract().expect("couldn't load config");
        INSTANCE.set(ArcSwap::from_pointee(config)).expect("couldn't initialize config");
    }

    pub fn save() -> Result<()> {
        let toml_string = toml::to_string(&**Self::global()).context("couldn't serialize config")?;
        fs::write(CONFIG_FILENAME, toml_string).context("couldn't write config")?;
        Ok(())
    }
}
