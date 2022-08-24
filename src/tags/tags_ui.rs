use super::tags::{Tag, TagData, TagLink, TagOperation};
use crate::{
    config::{self, Config},
    data::Data,
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
use poll_promise::Promise;
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub struct TagsUi {
    pub config: Option<Arc<Config>>,
    pub all_tags: Option<Promise<Result<Vec<TagData>>>>,
    pub new_tag: Tag,
    register_unknown_tags: bool,
    new_implication: TagLink,
    new_alias: TagLink,
    pub root_interface_floating_windows: Option<Rc<RefCell<Vec<ui::FloatingWindowState>>>>,
    tags: Vec<Tag>,
}

fn resolve_tag_operation(tag_op: TagOperation) {}

impl Default for TagsUi {
    fn default() -> Self {
        Self {
            root_interface_floating_windows: None,
            new_tag: Tag::empty(),
            all_tags: None,
            new_implication: TagLink::empty_implication(),
            register_unknown_tags: false,
            new_alias: TagLink::empty_alias(),
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
        // ui.horizontal(|ui| {
        let mut toasts = ui::initialize_toasts(ctx);
        ui.columns(2, |columns| {
            columns[0].label("actions");
            columns[1].label("tags");
            self.render_actions(&mut columns[0], ctx, &mut toasts);
            self.render_tags(&mut columns[1]);
        });
        toasts.show()
        // });
    }
}

impl TagsUi {
    fn get_all_tags(&mut self) -> Option<&Promise<Result<Vec<TagData>>>> {
        if self.all_tags.is_none() {
            self.all_tags = Self::load_tag_data(self.get_config());
        }
        self.all_tags.as_ref()
    }
    fn load_tag_data(config: Arc<Config>) -> Option<Promise<Result<Vec<TagData>>>>{
        // let config = self.get_config();
        Some(Promise::spawn_thread("", || Data::get_all_tag_data(config)))
    }
    fn render_tags(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::both().id_source("tags_info_scroll").show(ui, |ui| {
            if let Some(data_promise) = self.get_all_tags() {
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
                                ui.label(&tag_data.tag.name)
                                    .on_hover_text(if let Some(desc) = tag_data.tag.description.as_ref() {
                                        desc
                                    } else {
                                        "no description"
                                    });
                                ui.label(if let Some(space) = tag_data.tag.namespace.as_ref() {
                                    space
                                } else {
                                    "none"
                                });

                                let tagstring = tag_data.tag.to_tagstring();
                                let implies_iter = tag_data
                                    .links
                                    .iter()
                                    .filter(|link| (link.link_type == TagLinkType::Implication) && (link.from_tagstring == tagstring));
                                let implied_by_iter = tag_data
                                    .links
                                    .iter()
                                    .filter(|link| (link.link_type == TagLinkType::Implication) && (link.from_tagstring != tagstring));

                                let aliased_to_iter = tag_data
                                    .links
                                    .iter()
                                    .filter(|link| (link.link_type == TagLinkType::Alias) && (link.from_tagstring == tagstring));
                                let aliased_from_iter = tag_data
                                    .links
                                    .iter()
                                    .filter(|link| (link.link_type == TagLinkType::Alias) && (link.from_tagstring != tagstring));

                                let implies_label_text = implies_iter.clone().map(|tag_link| tag_link.to_tagstring.clone()).collect::<Vec<String>>().join(", ");
                                let implies_by_label_text = implied_by_iter.clone().map(|tag_link| tag_link.from_tagstring.clone()).collect::<Vec<String>>().join(", ");
                                let aliased_to_label_text = aliased_to_iter.clone().map(|tag_link| tag_link.to_tagstring.clone()).collect::<Vec<String>>().join(", ");
                                let aliased_from_label_text = aliased_from_iter.clone()
                                    .map(|tag_link| tag_link.from_tagstring.clone())
                                    .collect::<Vec<String>>()
                                    .join(", ");

                                let implies_label = egui::Label::new(if implies_label_text.is_empty() { "none" } else { &implies_label_text }).sense(egui::Sense::click());
                                let implies_by_label_text = egui::Label::new(if implies_by_label_text.is_empty() { "none" } else { &implies_by_label_text }).sense(egui::Sense::click());
                                let aliased_to_label_text = egui::Label::new(if aliased_to_label_text.is_empty() { "none" } else { &aliased_to_label_text }).sense(egui::Sense::click());
                                let aliased_from_label_text = egui::Label::new(if aliased_from_label_text.is_empty() { "none" } else { &aliased_from_label_text }).sense(egui::Sense::click());
                                
                                
                                ui.add(implies_label).context_menu(|ui| {
                                    if ui.button("delete").clicked() {
                                        println!("bruh");
                                    }
                                });

                                ui.add(implies_by_label_text);
                                ui.add(aliased_to_label_text);
                                ui.add(aliased_from_label_text);
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
    }
    fn render_actions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, toasts: &mut egui_toast::Toasts) {
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
                                let new_tag = self.new_tag.someified();
                                let tags = vec![new_tag.clone()];
                                match Data::filter_to_unknown_tags(self.get_config(), &tags) {
                                    Ok(unknown_tags) => {
                                        if unknown_tags.len() == 0 {
                                            toasts.warning(format!("this tag already exists"), ui::default_toast_options());
                                        } else {
                                            if let Err(e) = Data::register_tags(self.get_config(), &tags) {
                                                toasts.error(format!("failed to register tag: {e}"), ui::default_toast_options());
                                            } else {
                                                toasts.success(
                                                    format!("successfully registered {}", new_tag.to_tagstring()),
                                                    ui::default_toast_options(),
                                                );
                                                self.all_tags = Self::load_tag_data(self.get_config());
                                            };
                                        }
                                    }
                                    Err(e) => {
                                        toasts.error(format!("error checking if tag already exists: {e}"), ui::default_toast_options());
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
                            ui.add_enabled_ui(!(link.from_tagstring.is_empty() || link.to_tagstring.is_empty() || (link.from_tagstring == link.to_tagstring)), |ui| {
                                if ui.button("create").clicked() {
                                    let mut do_register_link = true;

                                    let mut check_link_tags_exist = |tagstring: &String| {
                                        match Data::does_tagstring_exist(config.clone(), &tagstring) {
                                            Ok(does_tag_exist) => {
                                                if !does_tag_exist && self.register_unknown_tags {
                                                    
                                                } else if !does_tag_exist {
                                                    toasts.error(format!("tag {} does not exist", tagstring), ui::default_toast_options());
                                                    do_register_link = false;
                                                }
                                            }
                                            Err(e) => {
                                                toasts.error(format!("failed to check if {} exists: {}", tagstring, e), ui::default_toast_options());
                                                do_register_link = false;
                                            }
                                        
                                        }
                                    };

                                    check_link_tags_exist(&link.from_tagstring);
                                    check_link_tags_exist(&link.to_tagstring);

                                    if do_register_link {
                                        match Data::does_link_exist(config.clone(), link) {
                                            Ok(already_exists) => {
                                                if already_exists {
                                                    toasts.warning(format!("this link already exists"), ui::default_toast_options());
                                                } else {
                                                    // let config = self.get_config();
                                                    let links = vec![link.clone()];
                                                    if let Err(e) = Data::register_tag_links(config.clone(), &links) {
                                                        toasts.error(format!("failed to register link: {e}"), ui::default_toast_options());
                                                    } else {
                                                        toasts.success(
                                                            format!("successfully registered new {}", link.link_type.to_string()),
                                                            ui::default_toast_options(),
                                                        );
                                                        self.all_tags = Self::load_tag_data(config);
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                toasts.error(format!("error checking if link already exists: {e}"), ui::default_toast_options());
                                            }
                                        }
                                    }
                                }
                            });
                        });
                    });
                };

                new_link_edit(&mut self.new_implication, "new implication", "implies", config.clone());
                new_link_edit(&mut self.new_alias, "new alias", "translates to", config);

            });
        });
    }
}
