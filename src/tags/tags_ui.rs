use super::tags::{Tag, TagData, TagLink};
use crate::{
    config::{self, Config},
    data,
    gallery::gallery_ui::GalleryUI,
    tags::tags::TagLinkType,
    ui::{self, DockedWindow},
};
use anyhow::Result;
use eframe::{
    egui::{self, Response, RichText},
    App,
};
use egui_extras::{Size, TableBuilder};
use egui_notify::{Anchor, Toasts};
use poll_promise::Promise;
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration, vec};

pub struct TagsUi {
    toasts: egui_notify::Toasts,
    pub config: Option<Arc<Config>>,
    pub all_tags: Option<Promise<Result<Vec<TagData>>>>,
    pub new_tag: Tag,
    register_unknown_tags: bool,
    new_implication: TagLink,
    new_alias: TagLink,
    pub root_interface_floating_windows: Option<Rc<RefCell<Vec<ui::FloatingWindowState>>>>,
    tags: Vec<Tag>,
}

impl Default for TagsUi {
    fn default() -> Self {
        Self {
            root_interface_floating_windows: None,
            new_tag: Tag::empty(),
            all_tags: None,
            new_implication: TagLink::empty_implication(),
            register_unknown_tags: false,
            new_alias: TagLink::empty_alias(),
            toasts: Toasts::default().with_anchor(Anchor::BottomLeft),
            config: None,
            tags: vec![
                Tag::new("blue_eyes".to_string(), None, None),
                Tag::new("red_eyes".to_string(), Some("The character has red eyes".to_string()), None),
                Tag::new("red_hair".to_string(), None, None),
                Tag::new(
                    "pumpkin_spice_latte".to_string(),
                    Some("artist".to_string()),
                    Some("A great artist".to_string()),
                ),
                Tag::new(
                    "henreader".to_string(),
                    Some("artist".to_string()),
                    Some("My favorite artist".to_string()),
                ),
                Tag::new("oshino_shinobu".to_string(), Some("character".to_string()), None),
            ],
        }
    }
}

impl ui::DockedWindow for TagsUi {
    fn set_config(&mut self, config: Arc<Config>) {
        self.config = Some(config);
    }
    fn get_config(&self) -> Arc<Config> {
        Arc::clone(self.config.as_ref().unwrap())
    }
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.columns(2, |columns| {
            columns[0].label("actions");
            columns[1].label("tags");
            self.render_actions(&mut columns[0], ctx);
            self.render_tags(&mut columns[1]);
        });
        self.toasts.show(ctx);
    }
}

impl TagsUi {
    fn get_all_tags(&mut self) -> Option<&Promise<Result<Vec<TagData>>>> {
        if self.all_tags.is_none() {
            self.all_tags = Self::load_tag_data(self.get_config());
        }
        self.all_tags.as_ref()
    }
    fn load_tag_data(config: Arc<Config>) -> Option<Promise<Result<Vec<TagData>>>> {
        // let config = self.get_config();
        Some(Promise::spawn_thread("", || data::get_all_tag_data(config)))
    }
    fn render_tags(&mut self, ui: &mut egui::Ui) {
        let config = self.get_config();
        self.get_all_tags();
        let mut do_reload_data = false;
        // let all_tags = self.all_tags.as_ref();
        egui::ScrollArea::both().id_source("tags_info_scroll").show(ui, |ui| {
            if let Some(data_promise) = self.all_tags.as_ref() {
                match data_promise.ready() {
                    Some(Ok(all_tag_data)) => {
                        egui::Grid::new("tags_info").striped(true).max_col_width(400.).show(ui, |ui| {
                            ui.label("count");
                            ui.label("name");
                            ui.label("space");

                            ui.label("implies");
                            ui.label("implied by");
                            ui.label("aliased to");
                            ui.label("aliased from");
                            ui.end_row();

                            for tag_data in all_tag_data {
                                ui.label(tag_data.occurances.to_string());
                                let tag_label = egui::Label::new(&tag_data.tag.name).sense(egui::Sense::click());
                                ui.add(tag_label)
                                    .on_hover_text(if let Some(desc) = tag_data.tag.description.as_ref() {
                                        desc
                                    } else {
                                        "no description"
                                    })
                                    .context_menu(|ui| {
                                        if ui.button(format!("delete tag \"{}\"", tag_data.tag.to_tagstring())).clicked() {
                                            if let Err(e) = data::delete_tag(config.clone(), &tag_data.tag) {
                                                ui::toast_error(&mut self.toasts, format!("failed to delete tag: {e}"));
                                            } else {
                                                do_reload_data = true;
                                                let toast = self.toasts.success(format!("successfully deleted \"{}\"", tag_data.tag.to_tagstring()));
                                                ui::set_default_toast_options(toast);
                                            }
                                        }
                                    });
                                ui.label(if let Some(space) = tag_data.tag.namespace.as_ref() {
                                    space
                                } else {
                                    "none"
                                });

                                let tagstring = tag_data.tag.to_tagstring();
                                let none_text = "none";
                                let mut create_label = |link_type: TagLinkType, target_from_tagstring: bool| {
                                    let iter = tag_data.links.iter().filter(|link| {
                                        (link.link_type == link_type)
                                            && (if target_from_tagstring {
                                                &link.to_tagstring
                                            } else {
                                                &link.from_tagstring
                                            } == &tagstring)
                                    });
                                    let label_text = iter
                                        .clone()
                                        .into_iter()
                                        .map(|tag_link| {
                                            if target_from_tagstring {
                                                tag_link.from_tagstring.clone()
                                            } else {
                                                tag_link.to_tagstring.clone()
                                            }
                                        })
                                        .collect::<Vec<String>>()
                                        .join(", ");
                                    let mut label = egui::Label::new(if label_text.is_empty() { none_text } else { &label_text });
                                    if !label_text.is_empty() {
                                        label = label.sense(egui::Sense::click())
                                    }
                                    ui.add(label).context_menu(|ui| {
                                        for link in iter {
                                            let target_tagstring = if target_from_tagstring {
                                                &link.from_tagstring
                                            } else {
                                                &link.to_tagstring
                                            };
                                            let source_tagstring = if target_from_tagstring {
                                                &link.to_tagstring
                                            } else {
                                                &link.from_tagstring
                                            };
                                            if ui
                                                .button(format!("delete {} to \"{}\"", link_type.to_string(), target_tagstring))
                                                .clicked()
                                            {
                                                if let Err(e) = data::delete_link(Arc::clone(&config), &link) {
                                                    ui::toast_error(&mut self.toasts, format!("failed to delete link: {e}"));
                                                } else {
                                                    do_reload_data = true;
                                                    ui::toast_success(
                                                        &mut self.toasts,
                                                        format!(
                                                            "successfully deleted link ({} from {} to {})",
                                                            link_type.to_string(),
                                                            source_tagstring,
                                                            target_tagstring
                                                        ),
                                                    );
                                                }
                                            }
                                        }
                                    });
                                };
                                create_label(TagLinkType::Implication, false);
                                create_label(TagLinkType::Implication, true);
                                create_label(TagLinkType::Alias, false);
                                create_label(TagLinkType::Alias, true);

                                ui.end_row();
                            }
                        });
                    }
                    Some(Err(e)) => {
                        ui.label(format!("failed to load tag data: {e}"));
                    }
                    None => {
                        ui.spinner();
                    }
                }
            }
        });
        if do_reload_data {
            self.all_tags = TagsUi::load_tag_data(config);
        }
    }
    fn render_actions(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        egui::ScrollArea::both().id_source("tag_actions").show(ui, |ui| {
            ui.vertical(|ui| {
                ui.collapsing("options", |ui| {
                    ui.checkbox(&mut self.register_unknown_tags, "register unknown tags");
                    // if ui.button("flush") {

                    // }
                });
                ui.collapsing("new tag", |ui| {
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui| {
                        egui::Grid::new("new_tag").max_col_width(1000.).show(ui, |ui| {
                            ui.label("name");
                            ui.text_edit_singleline(&mut self.new_tag.name);
                            ui.end_row();

                            if let Some(space) = self.new_tag.namespace.as_mut() {
                                ui.label("namespace");
                                ui.text_edit_singleline(space);
                                ui.end_row();
                            }

                            if let Some(desc) = self.new_tag.description.as_mut() {
                                ui.label("description");
                                ui.text_edit_multiline(desc);
                                ui.end_row();
                            }
                        });
                        ui.add_enabled_ui(!(self.new_tag.name.is_empty()), |ui| {
                            if ui.button("create").clicked() {
                                match data::does_tag_exist(self.get_config(), &self.new_tag) {
                                    Ok(does_exist) => {
                                        if does_exist {
                                            ui::set_default_toast_options(self.toasts.warning(format!("this tag already exists")));
                                        } else {
                                            if let Err(e) = data::register_tag(self.get_config(), &self.new_tag) {
                                                ui::set_default_toast_options(self.toasts.error(format!("failed to register tag: {e}")));
                                            } else {
                                                ui::set_default_toast_options(
                                                    self.toasts.success(format!("successfully registered \"{}\"", self.new_tag.to_tagstring())),
                                                );

                                                self.all_tags = Self::load_tag_data(self.get_config());
                                            };
                                        }
                                    }
                                    Err(e) => {
                                        ui::set_default_toast_options(self.toasts.error(format!("error checking if tag already exists: {e}")));
                                    }
                                }
                            }
                        });
                    });
                });

                let config = self.get_config();
                let mut new_link_edit = |link: &mut TagLink, header_label: &str, middle_label: &str, config: Arc<Config>| {
                    ui.collapsing(header_label, |ui| {
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui| {
                            ui.columns(3, |columns| {
                                columns[0].vertical(|ui| {
                                    ui.label("from");
                                    ui.text_edit_singleline(&mut link.from_tagstring);
                                });
                                columns[1].vertical_centered_justified(|ui| {
                                    let text = RichText::from(format!(">>   {middle_label}   >>")).size(18.);
                                    ui.label(text);
                                });
                                columns[2].vertical(|ui| {
                                    ui.label("to");
                                    ui.text_edit_singleline(&mut link.to_tagstring);
                                });
                            });
                            ui.add_enabled_ui(
                                !(link.from_tagstring.is_empty() || link.to_tagstring.is_empty() || (link.from_tagstring == link.to_tagstring)),
                                |ui| {
                                    if ui.button("create").clicked() {
                                        let mut do_register_link = true;

                                        let mut check_link_tags_exist =
                                            |tagstring: &String| match data::does_tagstring_exist(config.clone(), &tagstring) {
                                                Ok(does_tag_exist) => {
                                                    if !does_tag_exist && self.register_unknown_tags {
                                                    } else if !does_tag_exist {
                                                        ui::set_default_toast_options(self.toasts.error(format!("tag \"{}\" does not exist", tagstring)));
                                                        do_register_link = false;
                                                    }
                                                }
                                                Err(e) => {
                                                    ui::set_default_toast_options(
                                                        self.toasts.error(format!("failed to check if \"{}\" exists: {}", tagstring, e)),
                                                    );

                                                    do_register_link = false;
                                                }
                                            };

                                        check_link_tags_exist(&link.from_tagstring);
                                        check_link_tags_exist(&link.to_tagstring);

                                        if do_register_link {
                                            match data::does_link_exist(config.clone(), link) {
                                                Ok(already_exists) => {
                                                    if already_exists {
                                                        ui::set_default_toast_options(self.toasts.warning(format!("this link already exists")));
                                                    } else {
                                                        // let config = self.get_config();
                                                        if let Err(e) = data::register_tag_link(config.clone(), &link) {
                                                            ui::set_default_toast_options(self.toasts.error(format!("failed to register link: {e}")));
                                                        } else {
                                                            ui::set_default_toast_options(
                                                                self.toasts
                                                                    .success(format!("successfully registered new {}", link.link_type.to_string())),
                                                            );

                                                            self.all_tags = Self::load_tag_data(config);
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    ui::set_default_toast_options(
                                                        self.toasts.error(format!("error checking if link already exists: {e}")),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                },
                            );
                        });
                    });
                };

                new_link_edit(&mut self.new_implication, "new implication", "implies", config.clone());
                new_link_edit(&mut self.new_alias, "new alias", "translates to", config);
            });
        });
    }
}
