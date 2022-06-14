use std::{fs};

use serde::{Serialize, Deserialize};
use figment::{Figment, providers::{Env, Format, Toml, Serialized}};
use toml::ser::Error;

const CONFIG_FILENAME: &str = "config.toml";


#[derive(Debug, Deserialize, Serialize)]
pub struct Path {
    pub root: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub version: u8,
    pub path: Path,
}
impl Default for Config {
    fn default() -> Config {
        Config {
            version: 0,
            path: Path {
                root: "./files".into(),
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

    pub fn load() -> Result<Config, figment::Error> {
        Config::figment().extract()
    }

    pub fn save(config: &Config) -> Result<(), toml::ser::Error>{
        let save_path = CONFIG_FILENAME;
        let toml_string = toml::to_string(config);

        match toml_string {
            Ok(toml_string) => {
                // println!("{toml_string}");
                match fs::write(CONFIG_FILENAME, toml_string) {
                    Ok(()) => {
                        println!("config saved")
                    }
                    Err(error) => {
                        println!("couldn't save config")
                    }
                }
            }
            Err(error) => {
                return Err(error)
            }
        }

        Ok(())
    }
}

