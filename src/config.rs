use anyhow::{Context, Result};
use arc_swap::{ArcSwap, Guard};
use figment::{
    providers::{Format, Serialized, Toml},
    Figment,
};
use once_cell::sync::OnceCell;
use path_absolutize::*;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf, sync::Arc};

use crate::tags::Namespace;

// use crate::tags::tags::Namespace;

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
    pub thumbnail_resolution: usize,
    // pub import: Import,
    // pub gallery: Gallery,
    // pub short_id_length: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Import {
    pub thumbnail_size: usize,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Gallery {
    pub thumbnail_size: usize,
    pub short_id_length: usize,
    pub preview_size: usize,
    pub base_search: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Media {
    pub max_score: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Misc {
    pub concurrent_db_operations: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub path: Path,
    pub namespaces: Vec<Namespace>,
    pub media: Media,
    pub import: Import,
    pub gallery: Gallery,
    pub misc: Misc,
    // pub short_id_len
    pub ui: Ui,
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
            media: Media { max_score: 5 },
            ui: Ui { thumbnail_resolution: 100 },
            import: Import { thumbnail_size: 100 },
            gallery: Gallery {
                thumbnail_size: 100,
                preview_size: 500,
                short_id_length: 6,
                base_search: Some(String::from("independant=true limit=5000")),
            },
            misc: Misc {
                concurrent_db_operations: 20,
            }, // },
        }
    }
}

static CONFIG_INSTANCE: OnceCell<ArcSwap<Config>> = OnceCell::new();

impl Config {
    pub fn global() -> Guard<Arc<Config>> {
        CONFIG_INSTANCE.get().expect("uninitalized config").load()
    }

    pub fn clone() -> Config {
        (&**Self::global()).clone()
    }

    pub fn figment() -> Figment {
        Figment::from(Serialized::defaults(Self::default())).merge(Toml::file(CONFIG_FILENAME))
    }

    pub fn set(new_config: Config) {
        CONFIG_INSTANCE.get().expect("uninitalized config").store(Arc::new(new_config));
    }

    pub fn load() {
        let config: Config = Config::figment().extract().expect("couldn't load config");
        CONFIG_INSTANCE.set(ArcSwap::from_pointee(config)).expect("couldn't initialize config");
    }

    pub fn save() -> Result<()> {
        let toml_string = toml::to_string(&**Self::global()).context("couldn't serialize config")?;
        fs::write(CONFIG_FILENAME, toml_string).context("couldn't write config")?;
        Ok(())
    }
}
