use anyhow::{Context, Result};
use arc_swap::{ArcSwap, Guard};
use egui::{Color32, Stroke};
use figment::{
    providers::{self, Data, Format, Serialized, Toml, Yaml},
    Figment, Profile,
};
use once_cell::sync::OnceCell;
use path_absolutize::*;
use serde::{
    de::{self, SeqAccess, Visitor},
    ser::SerializeTupleStruct,
    Deserialize, Serialize,
};
use std::{env, fs, marker::PhantomData, path::PathBuf, sync::Arc};

use crate::{tags::Namespace, ui};

// use crate::tags::tags::Namespace;

const CONFIG_FILENAME: &str = "config.yaml";

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
pub struct Themes {
    pub current_theme: Option<String>,
    pub themes: Vec<Theme>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub path: Path,
    pub media: Media,
    pub import: Import,
    pub gallery: Gallery,
    pub misc: Misc,
    pub ui: Ui,
    pub themes: Themes,
    pub namespaces: Vec<Namespace>,
}

impl Themes {
    pub fn current_theme(&self) -> Option<Theme> {
        for theme in &self.themes {
            if let Some(current_theme) = self.current_theme.as_ref() {
                if theme.name == *current_theme {
                    return Some(theme.clone());
                }
            }
        }
        None
    }
    fn stroke(stroke_width: f32, color: Option<Color32>) -> Option<Stroke> {
        if let Some(stroke_color) = color {
            Some(Stroke::new(stroke_width, stroke_color))
        } else {
            None
        }
    }
    pub fn bg_fill_color(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.bg_fill_color.0)
    }
    pub fn secondary_bg_fill_color(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.secondary_bg_fill_color.0)
    }
    pub fn tertiary_bg_fill_color(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.tertiary_bg_fill_color.0)
    }

    pub fn light_blue(&self) -> Color32 {
        self.current_theme()
            .and_then(|t| t.tertiary_bg_fill_color.0)
            .unwrap_or(ui::constants::INFO_COLOR)
    }
    pub fn light_red(&self) -> Color32 {
        self.current_theme()
            .and_then(|t| t.tertiary_bg_fill_color.0)
            .unwrap_or(ui::constants::ERROR_COLOR)
    }
    pub fn light_green(&self) -> Color32 {
        self.current_theme()
            .and_then(|t| t.tertiary_bg_fill_color.0)
            .unwrap_or(ui::constants::SUCCESS_COLOR)
    }
    pub fn light_yellow(&self) -> Color32 {
        self.current_theme()
            .and_then(|t| t.tertiary_bg_fill_color.0)
            .unwrap_or(ui::constants::WARNING_COLOR)
    }
    pub fn blue(&self) -> Color32 {
        self.current_theme()
            .and_then(|t| t.blue.0)
            .unwrap_or(ui::constants::SUGGESTED_BUTTON_FILL)
    }
    pub fn bg_stroke_width(&self) -> Option<f32> {
        self.current_theme().and_then(|t| t.bg_stroke_width)
    }
    pub fn fg_stroke_width(&self) -> Option<f32> {
        self.current_theme().and_then(|t| t.fg_stroke_width)
    }
    pub fn inactive_bg_fill_color(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.inactive_bg_fill_color.0)
    }
    pub fn inactive_bg_stroke_color(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.inactive_bg_stroke_color.0)
    }
    pub fn inactive_fg_stroke_color(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.inactive_fg_stroke_color.0)
    }
    pub fn inactive_bg_stroke(&self) -> Option<Stroke> {
        Self::stroke(
            self.bg_stroke_width().unwrap_or(ui::constants::BG_STROKE_WIDTH),
            self.inactive_bg_stroke_color(),
        )
    }
    pub fn inactive_fg_stroke(&self) -> Option<Stroke> {
        Self::stroke(
            self.fg_stroke_width().unwrap_or(ui::constants::FG_STROKE_WIDTH),
            self.inactive_fg_stroke_color(),
        )
    }
    pub fn hovered_bg_fill_color(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.hovered_bg_fill_color.0)
    }
    pub fn hovered_bg_stroke_color(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.hovered_bg_stroke_color.0)
    }
    pub fn hovered_fg_stroke_color(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.hovered_fg_stroke_color.0)
    }
    pub fn hovered_bg_stroke(&self) -> Option<Stroke> {
        Self::stroke(
            self.bg_stroke_width().unwrap_or(ui::constants::BG_STROKE_WIDTH),
            self.hovered_bg_stroke_color(),
        )
    }
    pub fn hovered_fg_stroke(&self) -> Option<Stroke> {
        Self::stroke(
            self.fg_stroke_width().unwrap_or(ui::constants::FG_STROKE_WIDTH),
            self.hovered_fg_stroke_color(),
        )
    }
    pub fn active_bg_fill_color(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.active_bg_fill_color.0)
    }
    pub fn active_bg_stroke_color(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.active_bg_stroke_color.0)
    }
    pub fn active_fg_stroke_color(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.active_fg_stroke_color.0)
    }
    pub fn active_bg_stroke(&self) -> Option<Stroke> {
        Self::stroke(
            self.bg_stroke_width().unwrap_or(ui::constants::BG_STROKE_WIDTH),
            self.active_bg_stroke_color(),
        )
    }
    pub fn active_fg_stroke(&self) -> Option<Stroke> {
        Self::stroke(
            self.fg_stroke_width().unwrap_or(ui::constants::FG_STROKE_WIDTH),
            self.active_fg_stroke_color(),
        )
    }
    pub fn selected_fg_stroke_color(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.selected_fg_stroke_color.0)
    }
    pub fn selected_bg_fill_color(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.selected_bg_fill_color.0)
    }
    pub fn selected_fg_stroke(&self) -> Option<Stroke> {
        Self::stroke(
            self.fg_stroke_width().unwrap_or(ui::constants::FG_STROKE_WIDTH),
            self.selected_fg_stroke_color(),
        )
    }
    pub fn override_widget_primary(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.override_widget_primary.0)
    }
    pub fn override_widget_secondary(&self) -> Option<Color32> {
        self.current_theme().and_then(|t| t.override_widget_secondary.0)
    }
}
#[derive(Debug, Clone, Default)]
pub struct Color32Opt(pub Option<Color32>);

impl Serialize for Color32Opt {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let state = match self.0.map(|c| ui::color32_to_hex(c)) {
            Some(ref h) => serializer.serialize_some(h),
            None => serializer.serialize_none(),
        };
        state
    }
}

impl<'de> Deserialize<'de> for Color32Opt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        {
            struct Color32OptVisitor(PhantomData<Option<Color32>>);
            struct StrVisitor;
            impl<'de> Visitor<'de> for StrVisitor {
                type Value = String;

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("a string")
                }

                fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                where
                    E: de::Error,
                {
                    Ok(v.to_string())
                }
            }

            impl<'de> Visitor<'de> for Color32OptVisitor {
                type Value = Color32Opt;

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("valid hex string")
                }
                fn visit_unit<E>(self) -> Result<Self::Value, E>
                where
                    E: de::Error,
                {
                    Ok(Color32Opt(None))
                }
                fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    let hex_str = deserializer.deserialize_str(StrVisitor)?;
                    let color_res = ui::color32_from_hex(&hex_str);
                    Ok(Color32Opt(color_res.ok()))
                }
                fn visit_none<E>(self) -> Result<Self::Value, E>
                where
                    E: de::Error,
                {
                    Ok(Color32Opt(None))
                }
            }

            deserializer.deserialize_option(Color32OptVisitor(PhantomData))
        }
    }
}

impl Color32Opt {
    pub fn none() -> Self {
        Self(None)
    }
    pub fn from_array(array: [f32; 4]) -> Self {
        let array = array.map(|f| (f * 255.) as u8);
        Self(Some(Color32::from_rgba_unmultiplied(array[0], array[1], array[2], array[3])))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Theme {
    pub name: String,

    pub bg_fill_color: Color32Opt,
    pub secondary_bg_fill_color: Color32Opt,
    pub tertiary_bg_fill_color: Color32Opt,

    pub bg_stroke_width: Option<f32>,
    pub fg_stroke_width: Option<f32>,

    pub inactive_bg_fill_color: Color32Opt,
    pub inactive_bg_stroke_color: Color32Opt,
    pub inactive_fg_stroke_color: Color32Opt,

    pub hovered_bg_fill_color: Color32Opt,
    pub hovered_bg_stroke_color: Color32Opt,
    pub hovered_fg_stroke_color: Color32Opt,

    pub active_bg_fill_color: Color32Opt,
    pub active_bg_stroke_color: Color32Opt,
    pub active_fg_stroke_color: Color32Opt,

    pub selected_fg_stroke_color: Color32Opt,
    pub selected_bg_fill_color: Color32Opt,

    pub light_blue: Color32Opt,
    pub light_green: Color32Opt,
    pub light_red: Color32Opt,
    pub light_yellow: Color32Opt,

    pub blue: Color32Opt,
    pub red: Color32Opt,

    pub override_widget_primary: Color32Opt,
    pub override_widget_secondary: Color32Opt,
    // pub bg_fill_color: Option<String>,
    // pub secondary_bg_fill_color: Option<String>,
    // pub tertiary_bg_fill_color: Option<String>,

    // pub bg_stroke_width: Option<f32>,
    // pub fg_stroke_width: Option<f32>,

    // pub inactive_bg_fill_color: Option<String>,
    // pub inactive_bg_stroke_color: Option<String>,
    // pub inactive_fg_stroke_color: Option<String>,

    // pub hovered_bg_fill_color: Option<String>,
    // pub hovered_bg_stroke_color: Option<String>,
    // pub hovered_fg_stroke_color: Option<String>,

    // pub active_bg_fill_color: Option<String>,
    // pub active_bg_stroke_color: Option<String>,
    // pub active_fg_stroke_color: Option<String>,

    // pub selected_fg_stroke_color: Option<String>,
    // pub selected_bg_fill_color: Option<String>,

    // pub light_blue: Option<String>,
    // pub light_green: Option<String>,
    // pub light_red: Option<String>,
    // pub light_yellow: Option<String>,

    // pub blue: Option<String>,
    // pub red: Option<String>,
}

// impl Default for Theme {
//     fn default() -> Self {
//         Self {
//             name: String::from("new theme"),

//             bg_fill_color: Color,
//             secondary_bg_fill_color: None,
//             tertiary_bg_fill_color: None,

//             light_blue: None,
//             light_red: None,
//             light_green: None,
//             light_yellow: None,

//             red: None,
//             blue: None,

//             bg_stroke_width: None,
//             fg_stroke_width: None,

//             inactive_bg_fill_color: None,
//             inactive_bg_stroke_color: None,
//             inactive_fg_stroke_color: None,

//             hovered_bg_fill_color: None,
//             hovered_bg_stroke_color: None,
//             hovered_fg_stroke_color: None,

//             active_bg_fill_color: None,
//             active_bg_stroke_color: None,
//             active_fg_stroke_color: None,

//             selected_fg_stroke_color: None,
//             selected_bg_fill_color: None,
//         }
//     }
// }

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
            },
            themes: Themes {
                current_theme: None,
                themes: vec![Theme::default()],
            },
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
        Figment::from(Serialized::defaults(Self::default())).merge(Yaml::file(CONFIG_FILENAME))
    }

    pub fn set(new_config: Config) {
        CONFIG_INSTANCE.get().expect("uninitalized config").store(Arc::new(new_config));
    }

    pub fn load() {
        let config: Config = Config::figment().extract().expect("couldn't load config");
        CONFIG_INSTANCE.set(ArcSwap::from_pointee(config)).expect("couldn't initialize config");
    }

    pub fn save() -> Result<()> {
        let toml_string = serde_yaml::to_string(&**Self::global()).context("couldn't serialize config")?;
        fs::write(CONFIG_FILENAME, toml_string).context("couldn't write config")?;
        Ok(())
    }
}
