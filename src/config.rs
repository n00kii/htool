use anyhow::{Context, Result};
use arc_swap::{ArcSwap, Guard};
use egui::{Color32, Stroke};
use figment::{
    providers::{Format, Serialized, Yaml},
    Figment,
};
use once_cell::sync::OnceCell;
use path_absolutize::*;
use serde::{
    de::{self, Visitor},
    Deserialize, Serialize,
};
use std::{fs, marker::PhantomData, path::PathBuf, sync::Arc};

use crate::{tags::Namespace, ui};

// use crate::tags::tags::Namespace;

const CONFIG_FILENAME: &str = "config.yaml";

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Path {
    pub root: String,
    pub landing: String,
    pub database: String,
}

impl Path {
    pub fn current_root(&self) -> Result<PathBuf> {
        // let root = if self.root.is_some() {
        //     PathBuf::from(self.root.as_ref().unwrap()).absolutize()?.to_path_buf()
        // } else {
        //     let exe = env::current_exe().context("couldn't get parent path")?;

        //     let parent_path = exe.parent().ok_or(anyhow::Error::msg("message"))?;
        //     let parent_path_buf = PathBuf::from(parent_path);
        //     parent_path_buf
        // };
        let root = PathBuf::from(&self.root).absolutize()?.to_path_buf();
        fs::create_dir_all(&root)?;
        Ok(root)
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
    pub import_thumbnail_size: usize,
    pub gallery_thumbnail_size: usize,
    pub preview_pool_columns: usize,
    pub preview_pool_size: usize,
    pub preview_reorder_size: usize,
    pub preview_size: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct General {
    pub entry_max_score: usize,
    pub gallery_base_search: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Misc {
    pub entry_short_id_length: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Themes {
    pub current_theme: Option<String>,
    pub themes: Vec<Theme>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub path: Path,
    pub general: General,
    pub misc: Misc,
    pub ui: Ui,
    pub themes: Themes,
}

use paste::paste;
macro_rules! color_opt {
    ($name:ident) => {
        pub fn $name(&self) -> Option<Color32> {
            self.current_theme().and_then(|t| t.$name.0)
        }
    };
}
// macro_rules! color_opt_field {
//     ($name:ident) => {
//         pub name: Color32Opt,
//     };
// }
macro_rules! stroke_opt {
    ($type:tt, $layer:tt) => {
        paste! {
            pub fn [<$type _ $layer _ stroke>](&self) -> Option<Stroke>
            {
                if let Some(stroke_color) = self.[<$type _ $layer _ stroke_color>]() {
                    Some(Stroke::new(self.[<$layer _ stroke_width>]().unwrap_or(ui::constants::[<$layer:upper _STROKE_WIDTH>]), stroke_color))
                } else {
                    None
                }
            }
        }
    };
}

// macro_rules! color_opts_fields {
//     ($type:tt) => {
//         paste! {
//         color_opt_field!([<$type _bg_fill_color>]);
//         color_opt_field!([<$type _bg_stroke_color>]);
//         color_opt_field!([<$type _fg_stroke_color>]);
//         }
//     };
// }
macro_rules! stroke_and_colors {
    ($type:tt) => {
        paste! {
        color_opt!([<$type _bg_fill_color>]);
        color_opt!([<$type _bg_stroke_color>]);
        color_opt!([<$type _fg_stroke_color>]);

        stroke_opt!($type, bg);
        stroke_opt!($type, fg);
        }
    };
}
macro_rules! stroke_and_fill_colors {
    ($name:tt) => {
        paste! {
        color_opt!([<$name _fill_color>]);
        color_opt!([<$name _stroke_color>]);
        }
    };
}

macro_rules! f32_opt {
    ($name:ident) => {
        pub fn $name(&self) -> Option<f32> {
            self.current_theme().and_then(|t| t.$name)
        }
    };
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

    color_opt!(bg_fill_color);
    color_opt!(secondary_bg_fill_color);
    color_opt!(tertiary_bg_fill_color);

    stroke_and_fill_colors!(accent);
    stroke_and_fill_colors!(blue);
    stroke_and_fill_colors!(green);
    stroke_and_fill_colors!(red);
    stroke_and_fill_colors!(yellow);

    f32_opt!(bg_stroke_width);
    f32_opt!(fg_stroke_width);

    stroke_and_colors!(inactive);
    stroke_and_colors!(hovered);
    stroke_and_colors!(active);

    color_opt!(selected_fg_stroke_color);
    color_opt!(selected_bg_fill_color);

    stroke_opt!(selected, fg);

    color_opt!(override_widget_primary);
    color_opt!(override_widget_secondary);
}

#[derive(Debug, Clone, Default)]
pub struct Color32Opt(pub Option<Color32>);

impl Serialize for Color32Opt {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let state = match self.0.map(|c| ui::color32_to_hex(&c)) {
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

    // pub light_blue: Color32Opt,
    // pub light_green: Color32Opt,
    // pub light_red: Color32Opt,
    // pub light_yellow: Color32Opt,
    pub blue_fill_color: Color32Opt,
    pub blue_stroke_color: Color32Opt,
    pub red_fill_color: Color32Opt,
    pub red_stroke_color: Color32Opt,
    pub green_fill_color: Color32Opt,
    pub green_stroke_color: Color32Opt,
    pub yellow_stroke_color: Color32Opt,
    pub yellow_fill_color: Color32Opt,

    pub accent_fill_color: Color32Opt,
    pub accent_stroke_color: Color32Opt,

    pub override_widget_primary: Color32Opt,
    pub override_widget_secondary: Color32Opt,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            path: Path {
                root: "./root/".into(),
                landing: "landing/".into(),
                database: "data.db".into(),
            },
            general: General {
                entry_max_score: 5,
                gallery_base_search: Some(String::from("independant=true limit=5000")),
            },
            misc: Misc { entry_short_id_length: 6 },
            ui: Ui {
                gallery_thumbnail_size: 100,
                import_thumbnail_size: 100,
                thumbnail_resolution: 100,
                preview_pool_columns: 4,
                preview_reorder_size: 600,
                preview_pool_size: 200,
                preview_size: 500,
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
        let config: Config = Self::load_from_file();
        CONFIG_INSTANCE.set(ArcSwap::from_pointee(config)).expect("couldn't initialize config");
    }

    pub fn load_from_file() -> Config{
        Config::figment().extract().expect("couldn't load config")
    }

    pub fn save() -> Result<()> {
        let toml_string = serde_yaml::to_string(&**Self::global()).context("couldn't serialize config")?;
        fs::write(CONFIG_FILENAME, toml_string).context("couldn't write config")?;
        Ok(())
    }
}
