use std::{fmt::Display, rc::Rc, sync::{atomic::Ordering}};

use super::{
    widgets::autocomplete::{self, AutocompleteOption}, UserInterface,
};
use crate::{config::Config, app::SharedState};
use crate::ui;
use crate::ui::icon;
use egui::{Align, DragValue, Grid, Layout, Response, Ui};
use egui_extras::{Size, StripBuilder};
use enum_iterator::{all, Sequence};

pub struct ConfigUI {
    current_section: ConfigSection,
    config_copy: Config,
    shared_state: Rc<SharedState>,
}

#[derive(Default, PartialEq, Sequence, Copy, Clone)]
enum ConfigSection {
    #[default]
    Paths,
    General,
    Ui,
    Misc,
    Themes,
}

impl Display for ConfigSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigSection::General => write!(f, "{}", icon!("general", CONFIG_ICON)),
            ConfigSection::Ui => write!(f, "{}", icon!("ui", FONT_ICON)),
            ConfigSection::Paths => write!(f, "{}", icon!("paths", FOLDER_ICON)),
            ConfigSection::Misc => write!(f, "{}", icon!("misc", MISC_ICON)),
            ConfigSection::Themes => write!(f, "{}", icon!("themes", SPARKLE_ICON)),
        }
    }
}

impl ConfigUI {
    pub fn new(shared_state: &Rc<SharedState>) -> Self {
        Self {
            current_section: ConfigSection::default(),
            shared_state: Rc::clone(&shared_state),
            config_copy: (**Config::global()).clone(),
        }
    }
    fn render_sections(&mut self, ui: &mut Ui) {
        ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
            for section in all::<ConfigSection>() {
                ui.selectable_value(&mut self.current_section, section, section.to_string());
            }
        });
    }

    fn render_current_section(&mut self, ui: &mut Ui) {
        let mut config_changed = false;
        let mut hook = |r: Response| -> Response {
            if r.changed() {
                config_changed = true;
            }
            r
        };
        match self.current_section {
            ConfigSection::Paths => {
                Grid::new("paths_config").num_columns(2).show(ui, |ui| {
                    ui.label("root path");
                    hook(ui.text_edit_singleline(&mut self.config_copy.path.root));
                    ui.end_row();
                    ui.label("data path");
                    if hook(ui.text_edit_singleline(&mut self.config_copy.path.database)).lost_focus() {
                        SharedState::set_update_flag(&self.shared_state.database_changed, true);
                    };
                    ui.end_row();
                    ui.label("landing path");
                    hook(ui.text_edit_singleline(&mut self.config_copy.path.landing));
                    ui.end_row();
                });
            }
            ConfigSection::General => {
                Grid::new("general_config").num_columns(2).show(ui, |ui| {
                    ui.label("max entry score");
                    hook(ui.add(DragValue::new(&mut self.config_copy.general.entry_max_score).clamp_range(2..=10)));
                    ui.end_row();
                    ui.label("base gallery search");
                    hook(ui.text_edit_multiline(self.config_copy.general.gallery_base_search.get_or_insert(String::new())));
                    ui.end_row();
                });
            }
            ConfigSection::Ui => {
                Grid::new("ui_config").num_columns(2).show(ui, |ui| {
                    ui.label("thumbnail resolution");
                    hook(ui.add(DragValue::new(&mut self.config_copy.ui.thumbnail_resolution).clamp_range(25..=1000)));
                    ui.end_row();
                    ui.label("importer thumbnail size");
                    hook(ui.add(DragValue::new(&mut self.config_copy.ui.import_thumbnail_size).clamp_range(25..=1000)));
                    ui.end_row();
                    ui.label("gallery thumbnail size");
                    hook(ui.add(DragValue::new(&mut self.config_copy.ui.gallery_thumbnail_size).clamp_range(25..=1000)));
                    ui.end_row();
                    ui.label("preview size");
                    hook(ui.add(DragValue::new(&mut self.config_copy.ui.preview_size).clamp_range(25..=1000)));
                    ui.end_row();
                    ui.label("pool preview size");
                    hook(ui.add(DragValue::new(&mut self.config_copy.ui.preview_pool_size).clamp_range(25..=1000)));
                    ui.end_row();
                    ui.label("preview pool columns");
                    hook(ui.add(DragValue::new(&mut self.config_copy.ui.preview_pool_columns).clamp_range(2..=10)));
                    ui.end_row();
                    ui.label("pool reordering preview size");
                    hook(ui.add(DragValue::new(&mut self.config_copy.ui.preview_reorder_size).clamp_range(25..=1000)));
                    ui.end_row();
                });
            }
            ConfigSection::Misc => {
                Grid::new("misc_config").num_columns(2).show(ui, |ui| {
                    ui.label("entry short-id length");
                    hook(ui.add(DragValue::new(&mut self.config_copy.misc.entry_short_id_length).clamp_range(0..=16)));
                });
            }
            ConfigSection::Themes => {
                Grid::new("theme_config").num_columns(2).show(ui, |ui| {
                    ui.label("current theme");
                    let mut options: Vec<AutocompleteOption> = self
                        .config_copy
                        .themes
                        .themes
                        .iter()
                        .map(|t| AutocompleteOption {
                            label: t.name.clone(),
                            value: t.name.clone(),
                            color: None,
                            description: String::from("theme"),
                            succeeding_space: false,
                        })
                        .collect();
                    options.insert(
                        0,
                        AutocompleteOption {
                            label: String::from("none"),
                            value: String::new(),
                            color: None,
                            description: String::from("theme"),
                            succeeding_space: false,
                        },
                    );
                    if hook(ui.add(autocomplete::create(
                        self.config_copy.themes.current_theme.get_or_insert(String::new()),
                        &options,
                        false,
                        false,
                    )))
                    .changed()
                    {
                        self.shared_state.updated_theme_selection.store(true, Ordering::Relaxed);
                        let current_theme = self.config_copy.themes.current_theme.as_mut().unwrap();
                        *current_theme = current_theme.trim().to_string();
                    }
                });
            }
        }
        if config_changed {
            Config::set(self.config_copy.clone())
        }
    }
}

impl UserInterface for ConfigUI {
    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.vertical_centered_justified(|ui| {
            if ui.button("load from file").clicked() {
                let config = Config::load_from_file();
                self.config_copy = config.clone();
                Config::set(config);
                self.shared_state.updated_theme_selection.store(true, Ordering::Relaxed);
            };
            if ui.button("save to file").clicked() {
                if let Err(e) = Config::save() {
                    ui::toast_error_lock(&self.shared_state.toasts, format!("failed to save config: {e}"));
                } else {
                    ui::toast_success_lock(&self.shared_state.toasts, "successfully saved config");
                };
            };
            ui::space(ui);
        });
        StripBuilder::new(ui)
            .size(Size::exact(0.))
            .size(Size::exact(ui::constants::OPTIONS_COLUMN_WIDTH))
            .size(Size::exact(ui::constants::SPACER_SIZE))
            .size(Size::remainder())
            .size(Size::exact(0.))
            .horizontal(|mut strip| {
                strip.empty();
                strip.cell(|ui| self.render_sections(ui));
                strip.cell(|ui| {
                    ui.with_layout(Layout::left_to_right(Align::Center).with_cross_justify(true), |ui| {
                        ui.separator();
                    });
                });
                strip.cell(|ui| self.render_current_section(ui));
                strip.empty();
            });
    }
}
