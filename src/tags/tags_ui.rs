use super::tags::{self, Namespace, Tag, TagData, TagDataRef, TagLink};
use crate::{
    config::{self, Config},
    data,
    gallery::gallery_ui::GalleryUI,
    tags::tags::TagLinkType,
    ui::{self, LayoutJobText, SharedState, UpdateFlag, UserInterface, WindowContainer, AutocompleteOptionsRef}, autocomplete,
};
use anyhow::{Error, Result};
use chrono::{DateTime, Utc};
use eframe::{
    egui::{self, Grid, Layout, Response, RichText, Window},
    emath::Align,
    App,
};
use egui::{text::LayoutJob, Button, Color32, Direction, FontFamily, FontId, Galley, Label, ScrollArea, Sense, TextFormat};
use egui_extras::{Size, StripBuilder, TableBuilder};
use egui_modal::Modal;
use egui_notify::{Anchor, Toasts};
use poll_promise::Promise;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
    vec,
};
use ui::ToastsRef;

pub struct TagsUI {
    filter_query: String,
    filter_tagstrings: Vec<String>,
    modify_windows: Vec<WindowContainer>,
    register_unknown_tags: bool,
    shared_state: Rc<SharedState>,
    link_pending_delete: Option<TagLink>,
    tag_pending_delete: Option<Tag>,
}

impl TagsUI {
    pub fn new(shared_state: &Rc<SharedState>) -> Self {
        Self {
            link_pending_delete: None,
            tag_pending_delete: None,
            filter_query: String::new(),
            filter_tagstrings: vec![],
            shared_state: Rc::clone(&shared_state),

            modify_windows: vec![],
            register_unknown_tags: false,
        }
    }
}

impl ui::UserInterface for TagsUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        StripBuilder::new(ui)
            .size(Size::exact(0.)) // FIXME: not sure why this is adding more space.
            .size(Size::exact(ui::constants::OPTIONS_COLUMN_WIDTH))
            .size(Size::exact(ui::constants::SPACER_SIZE))
            .size(Size::remainder())
            .horizontal(|mut strip| {
                strip.empty();
                strip.cell(|ui| {
                    self.render_options(ui, ctx);
                });
                strip.cell(|ui| {
                    ui.with_layout(Layout::left_to_right(Align::Center).with_cross_justify(true), |ui| {
                        ui.separator();
                    });
                });
                strip.cell(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} filter", ui::constants::SEARCH_ICON));
                        ui.with_layout(Layout::top_down(Align::Center).with_cross_justify(true), |ui| {
                            let response = ui.text_edit_singleline(&mut self.filter_query);
                            if response.changed() {
                                self.filter_tagstrings = self.filter_query.split_whitespace().map(|str| str.to_string()).collect::<Vec<_>>();
                            }
                        });
                    });
                    ui.add_space(ui::constants::SPACER_SIZE);
                    self.render_tags(ui);
                });
            });
        self.render_modify_windows(ctx);
    }
}

impl TagsUI {
    fn render_modify_windows(&mut self, ctx: &egui::Context) {
        self.modify_windows.retain(|window| window.is_open.unwrap());
        for modify_window in self.modify_windows.iter_mut() {
            Window::new(&modify_window.title)
                .open(modify_window.is_open.as_mut().unwrap())
                .vscroll(false)
                .hscroll(false)
                .resizable(false)
                .show(ctx, |ui| modify_window.window.ui(ui, ctx));
        }
    }
    fn render_options(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
            ui.label("tags");
            if ui.button(format!("{} refresh", ui::constants::REFRESH_ICON)).clicked() {
                tags::reload_tag_data(&self.shared_state.tag_data_ref);
            }
            ui.add_space(ui::constants::SPACER_SIZE);
            if ui.button(format!("{} new tag", ui::constants::ADD_ICON)).clicked() {
                let title = "new tag".to_string();
                if !ui::does_window_exist(&title, &self.modify_windows) {
                    self.modify_windows.push(WindowContainer {
                        title,
                        window: Box::new(ModifyTagUI::new(
                            None,
                            &self.shared_state.tag_data_ref,
                            &self.shared_state.entry_info_update_flag,
                            &self.shared_state.tag_data_update_flag,
                            &self.shared_state.toasts,
                        )),
                        is_open: Some(true),
                    })
                }
            }
            if ui.button(format!("{} new tag link", ui::constants::ADD_ICON)).clicked() {
                let title = "new tag link".to_string();
                if !ui::does_window_exist(&title, &self.modify_windows) {
                    self.modify_windows.push(WindowContainer {
                        title,
                        window: Box::new(ModifyTagLinkUI::new(
                            &self.shared_state
                        )),
                        is_open: Some(true),
                    })
                }
            }
            ui.add_space(ui::constants::SPACER_SIZE);
            if ui.button("namespaces").clicked() {
                let title = "namespaces".to_string();
                if !ui::does_window_exist(&title, &self.modify_windows) {
                    self.modify_windows.push(WindowContainer {
                        title,
                        window: Box::new(ModifyNamespacesUI::new(Arc::clone(&self.shared_state.toasts))),
                        is_open: Some(true),
                    })
                }
            }
            ui.add_space(ui::constants::SPACER_SIZE);
            if ui.button(ui::icon_text("patch tags", ui::constants::TOOL_ICON)).clicked() {}
        });
    }
    fn load_tag_description(tag: &mut Tag, loaded_tag_data: &TagDataRef) {
        if let Some(Ok(tag_data)) = loaded_tag_data.borrow().ready() {
            for tag_data in tag_data {
                if tag_data.tag == *tag {
                    tag.description = tag_data.tag.description.clone();
                }
            }
        }
        // }
    }
    fn render_tags(&mut self, ui: &mut egui::Ui) {
        let mut do_reload_data = false;
        let delete_tag_modal = Modal::new(ui.ctx(), "tag_delete_modal");
        let link_delete_modal = Modal::new(ui.ctx(), "link_delete_modal");
        if let Some(tag_pending_delete) = self.tag_pending_delete.as_ref() {
            delete_tag_modal.show(|ui| {
                delete_tag_modal.frame(ui, |ui| {
                    delete_tag_modal.body(
                        ui,
                        ui::generate_layout_job(vec!["delete ".into(), tag_pending_delete.to_layout_job_text(), "?".into()]),
                    );
                });
                delete_tag_modal.buttons(ui, |ui| {
                    delete_tag_modal.button(ui, "cancel");
                    if delete_tag_modal.caution_button(ui, "delete").clicked() {
                        let toasts = Arc::clone(&self.shared_state.toasts);
                        let tag_pending_delete = tag_pending_delete.clone();
                        let tag_data_update_flag = Arc::clone(&self.shared_state.tag_data_update_flag);
                        let entry_info_update_flag = Arc::clone(&self.shared_state.entry_info_update_flag);
                        thread::spawn(move || {
                            if let Err(e) = data::delete_tag(&tag_pending_delete) {
                                Self::toast_failed_delete_tag(&tag_pending_delete.to_tagstring(), &e, &toasts);
                            } else {
                                Self::toast_success_delete_tag(&tag_pending_delete.to_tagstring(), &toasts);
                                SharedState::set_update_flag(&tag_data_update_flag, true);
                                SharedState::set_update_flag(&entry_info_update_flag, true);

                            }
                        });
                    }
                })
            });
        }
        if let Some(link_pending_delete) = self.link_pending_delete.as_ref() {
            link_delete_modal.show(|ui| {
                link_delete_modal.frame(ui, |ui| {
                    let mut job_text = link_pending_delete.to_layout_job_text();
                    job_text.insert(0, "delete ".into());
                    job_text.push("?".into());
                    link_delete_modal.body(ui, ui::generate_layout_job(job_text));
                });
                link_delete_modal.buttons(ui, |ui| {
                    link_delete_modal.button(ui, "cancel");
                    if link_delete_modal.caution_button(ui, "delete").clicked() {
                        if let Err(e) = data::delete_tag_link(&link_pending_delete) {
                            Self::toast_failed_delete_link(link_pending_delete, &e, &self.shared_state.toasts)
                        } else {
                            do_reload_data = true;
                            Self::toast_success_delete_link(link_pending_delete, &self.shared_state.toasts)
                        }
                    }
                })
            });
        }
        ui.label("tags");
        egui::ScrollArea::both().id_source("tags_info_scroll").show(ui, |ui| {
            // if let Some(data_promise) = self.loaded_tag_data.as_ref() {
            match self.shared_state.tag_data_ref.borrow().ready() {
                Some(Ok(all_tag_data)) => {
                    TableBuilder::new(ui)
                        .striped(true)
                        .column(Size::remainder())
                        .column(Size::remainder())
                        .column(Size::remainder())
                        .column(Size::remainder())
                        .column(Size::remainder())
                        .column(Size::remainder())
                        .column(Size::remainder())
                        .header(20.0, |mut header| {
                            header.col(|ui| {
                                ui.label("count");
                            });
                            header.col(|ui| {
                                ui.label("name");
                            });
                            header.col(|ui| {
                                ui.label("space");
                            });
                            header.col(|ui| {
                                ui.label("implies");
                            });
                            header.col(|ui| {
                                ui.label("implied by");
                            });
                            header.col(|ui| {
                                ui.label("aliased to");
                            });
                            header.col(|ui| {
                                ui.label("aliased from");
                            });
                        })
                        .body(|mut body| {
                            for tag_data in all_tag_data {
                                if !self.filter_tagstrings.is_empty() {
                                    let mut passes_filter = false;
                                    let tagstring = tag_data.tag.to_tagstring();
                                    for filter_tagstring in self.filter_tagstrings.iter() {
                                        if tagstring.contains(filter_tagstring) {
                                            passes_filter = true;
                                            break;
                                        }
                                    }
                                    if !passes_filter {
                                        continue;
                                    };
                                }

                                body.row(18., |mut row| {
                                    row.col(|ui| {
                                        ui.label(tag_data.occurances.to_string());
                                    });
                                    row.col(|ui| {
                                        let tag_label = egui::Label::new(tag_data.tag.to_rich_text()).sense(egui::Sense::click());
                                        let response = ui.add(tag_label).context_menu(|ui| {
                                            let delete_jt = LayoutJobText::new(format!("{} delete ", ui::constants::DELETE_ICON,));
                                            let edit_jt = LayoutJobText::new(format!("{} edit ", ui::constants::EDIT_ICON,));
                                            let tag_jt = LayoutJobText::new(&tag_data.tag.name)
                                                .with_color(tag_data.tag.namespace_color().unwrap_or(ui::constants::DEFAULT_TEXT_COLOR));
                                            let delete_lj = ui::generate_layout_job(vec![delete_jt, tag_jt.clone()]);
                                            let edit_lj = ui::generate_layout_job(vec![edit_jt, tag_jt]);
                                            if ui.button(edit_lj).clicked() {
                                                ui.close_menu();
                                                Self::launch_tag_modify_window(
                                                    tag_data.tag.clone(),
                                                    &mut self.modify_windows,
                                                    &self.shared_state.toasts,
                                                    &self.shared_state.tag_data_ref,
                                                    &self.shared_state.entry_info_update_flag,
                                                    &self.shared_state.tag_data_update_flag,
                                                )
                                            }
                                            if ui.button(delete_lj).clicked() {
                                                ui.close_menu();
                                                self.tag_pending_delete = Some(tag_data.tag.clone());
                                                delete_tag_modal.open();
                                            }
                                        });
                                        if let Some(desc) = tag_data.tag.description.as_ref() {
                                            response.on_hover_text(desc);
                                        }
                                    });
                                    row.col(|ui| {
                                        ui.label(if let Some(space) = tag_data.tag.namespace.as_ref() {
                                            RichText::new(space)
                                        } else {
                                            RichText::new("none").weak()
                                        });
                                    });
                                    let tagstring = tag_data.tag.to_tagstring();

                                    let mut create_label = |link_type: TagLinkType, target_from_tagstring: bool| {
                                        let iter = tag_data.links.iter().filter(|link| {
                                            (link.link_type == link_type)
                                                && (if target_from_tagstring {
                                                    &link.to_tagstring
                                                } else {
                                                    &link.from_tagstring
                                                } == &tagstring)
                                        });
                                        let tag_labels_iter = iter.clone().into_iter().map(|tag_link| {
                                            if target_from_tagstring {
                                                Label::new(Tag::from_tagstring(&tag_link.from_tagstring).to_rich_text())
                                            } else {
                                                Label::new(Tag::from_tagstring(&tag_link.to_tagstring).to_rich_text())
                                            }
                                        });

                                        let tag_labels = tag_labels_iter.clone().collect::<Vec<_>>();
                                        let tag_labels_clone = tag_labels_iter.clone().collect::<Vec<_>>();

                                        row.col(|ui| {
                                            if tag_labels.is_empty() {
                                                ui.label(RichText::new("none").weak());
                                            } else {
                                                ui.push_id(
                                                    format!("{}_{}_{target_from_tagstring}", tag_data.tag.to_tagstring(), link_type.to_string()),
                                                    |ui| {
                                                        ui.horizontal(|ui| {
                                                            for label in tag_labels {
                                                                ui.add(label);
                                                            }
                                                        })
                                                        .response
                                                        .on_hover_ui(|ui| {
                                                            ui.vertical(|ui| {
                                                                for label in tag_labels_clone {
                                                                    ui.add(label);
                                                                }
                                                            });
                                                        })
                                                        .context_menu(|ui| {
                                                            for link in iter {
                                                                let target_tagstring = if target_from_tagstring {
                                                                    &link.from_tagstring
                                                                } else {
                                                                    &link.to_tagstring
                                                                };
                                                                let job_text_1 = LayoutJobText::new(format!(
                                                                    "{} {} with ",
                                                                    ui::constants::DELETE_ICON,
                                                                    &link_type.to_string()
                                                                ));
                                                                let job_text_2 = LayoutJobText::new(Tag::from_tagstring(&target_tagstring).name)
                                                                    .with_color(
                                                                        Tag::from_tagstring(&target_tagstring)
                                                                            .namespace_color()
                                                                            .unwrap_or(ui::constants::DEFAULT_TEXT_COLOR),
                                                                    );
                                                                let job = ui::generate_layout_job(vec![job_text_1, job_text_2]);
                                                                if ui.button(job).clicked() {
                                                                    self.link_pending_delete = Some(link.clone());
                                                                    link_delete_modal.open();
                                                                }
                                                            }
                                                        });
                                                    },
                                                );
                                            }
                                        });
                                    };
                                    create_label(TagLinkType::Implication, false);
                                    create_label(TagLinkType::Implication, true);
                                    create_label(TagLinkType::Alias, false);
                                    create_label(TagLinkType::Alias, true);
                                });
                            }
                        });
                }
                Some(Err(e)) => {
                    ui.label(format!("failed to load tag data: {e}"));
                }
                None => {
                    ui.spinner();
                } // }
            }
        });
        if do_reload_data {
            tags::reload_tag_data(&self.shared_state.tag_data_ref);
        }
    }
    fn toast_fail_modify_tag(old_tagstring: &String, new_tagstring: &String, error: &Error, toasts: &ToastsRef) {
        ui::toast_error_lock(
            toasts,
            format!("failed tag modification ( \"{old_tagstring}\" --> \"{new_tagstring}\"): {error} "),
        );
    }
    fn launch_tag_modify_window(
        tag: Tag,
        modify_windows: &mut Vec<WindowContainer>,
        toasts: &ToastsRef,
        loaded_tag_data: &TagDataRef,
        entry_info_update_flag: &UpdateFlag,
        tag_data_update_flag: &UpdateFlag,
    ) {
        let title = format!("edit \"{}\"", tag.to_tagstring());

        if !ui::does_window_exist(&title, modify_windows) {
            modify_windows.push(WindowContainer {
                title,
                window: Box::new(ModifyTagUI::new(
                    Some(tag),
                    &loaded_tag_data,
                    &entry_info_update_flag,
                    &tag_data_update_flag,
                    &toasts,
                )),
                is_open: Some(true),
            })
        }
    }

    pub fn toast_failed_check_link_exists(link: &TagLink, toasts: &ToastsRef) {
        ui::toast_error_lock(toasts, format!("error checking if link {} exists", link.to_string()));
    }
    pub fn toast_failed_check_tag_exists(tagstring: &String, toasts: &ToastsRef) {
        ui::toast_error_lock(toasts, format!("error checking if tag \"{tagstring}\" exists"));
    }
    pub fn toast_failed_delete_link(link: &TagLink, error: &Error, toasts: &ToastsRef) {
        ui::toast_error_lock(toasts, format!("failed to delete link {}: {error}", link.to_string()));
    }
    pub fn toast_failed_delete_tag(tagstring: &String, error: &Error, toasts: &ToastsRef) {
        ui::toast_error_lock(toasts, format!("failed to delete tag \"{tagstring}\": {error}"));
    }
    pub fn toast_failed_new_link(link: &TagLink, error: &Error, toasts: &ToastsRef) {
        ui::toast_error_lock(toasts, format!("failed to register link {}: {error}", link.to_string()));
    }
    pub fn toast_failed_new_tag(tagstring: &String, error: &Error, toasts: &ToastsRef) {
        ui::toast_error_lock(toasts, format!("failed to register tag \"{tagstring}\": {error}"));
    }
    pub fn toast_link_already_exists(link: &TagLink, toasts: &ToastsRef) {
        ui::toast_warning_lock(toasts, format!("link {} already exists", link.to_string()));
    }
    pub fn toast_success_delete_link(link: &TagLink, toasts: &ToastsRef) {
        ui::toast_success_lock(toasts, format!("successfully deleted link {}", link.to_string()));
    }
    pub fn toast_success_delete_tag(tagstring: &String, toasts: &ToastsRef) {
        ui::toast_success_lock(toasts, format!("successfully deleted tag\"{}\"", tagstring));
    }
    pub fn toast_success_modify_tag(old_tagstring: &String, new_tagstring: &String, toasts: &ToastsRef) {
        ui::toast_success_lock(toasts, format!("successfully modified tag (\"{old_tagstring}\" --> \"{new_tagstring}\")"));
    }
    pub fn toast_fail_invalid_number_tags(expected_number: usize, recieved_number: usize, toasts: &ToastsRef) {
        ui::toast_error_lock(toasts, format!("expected {expected_number} tags, got {recieved_number}"));
    }
    pub fn toast_success_new_link(link: &TagLink, toasts: &ToastsRef) {
        ui::toast_success_lock(toasts, format!("successfully registered link {}", link.to_string()));
    }
    pub fn toast_success_new_tag(tagstring: &String, toasts: &ToastsRef) {
        ui::toast_success_lock(toasts, format!("successfully registered tag \"{tagstring}\""));
    }
    pub fn toast_tag_already_exists(tagstring: &String, toasts: &ToastsRef) {
        ui::toast_warning_lock(toasts, format!("tag \"{tagstring}\" already exists"));
    }
    pub fn toast_tag_doesnt_exist(tagstring: &String, toasts: &ToastsRef) {
        ui::toast_error_lock(toasts, format!("tag \"{tagstring}\" doesn't exist"));
    }
}

struct ModifyTagLinkUI {
    toasts: ToastsRef,
    // link: TagLink,
    autocomplete_options: AutocompleteOptionsRef,
    link_type: TagLinkType,
    from_tagstrings: String,
    to_tagstrings: String,
    loaded_tag_data: TagDataRef,
}

impl ModifyTagLinkUI {
    fn new(shared_state: &SharedState) -> Self {
        Self {
            toasts: Arc::clone(&shared_state.toasts),
            from_tagstrings: String::new(),
            to_tagstrings: String::new(),
            autocomplete_options: Rc::clone(&shared_state.autocomplete_options),
            link_type: TagLinkType::Implication,
            loaded_tag_data: Rc::clone(&shared_state.tag_data_ref),
        }
    }
}

impl ui::UserInterface for ModifyTagLinkUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui| {
            let valid = !(self.from_tagstrings.is_empty() || self.to_tagstrings.is_empty());
            egui::Grid::new("new_link").max_col_width(1000.).show(ui, |ui| {
                ui.label("create");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.link_type, TagLinkType::Implication, TagLinkType::Implication.to_string());
                    ui.selectable_value(&mut self.link_type, TagLinkType::Alias, TagLinkType::Alias.to_string());
                });
                ui.end_row();

                if let Some(autocomplete_options) = self.autocomplete_options.borrow().as_ref() {

                    ui.label("from");
                    ui.add(autocomplete::create(&mut self.from_tagstrings, autocomplete_options, false, true));
                    ui.end_row();
                    
                    ui.label("to");
                    ui.add(autocomplete::create(&mut self.to_tagstrings, autocomplete_options, false, true));
                    ui.end_row();
                }
            });
            ui.add_enabled_ui(valid, |ui| {
                if ui.button("create").clicked() {
                    fn check_exists(tagstring: &String, do_register: &mut bool, toasts: &ToastsRef) {
                        if let Ok(does_exist) = data::does_tagstring_exist(tagstring) {
                            if !does_exist {
                                *do_register = false;
                                TagsUI::toast_tag_doesnt_exist(tagstring, toasts)
                            }
                        } else {
                            *do_register = false;
                            TagsUI::toast_failed_check_tag_exists(tagstring, toasts)
                        }
                    }

                    let toasts = &self.toasts;
                    for from_tag in Tag::from_tagstrings(&self.from_tagstrings) {
                        let mut do_register = true;
                        check_exists(&from_tag.to_tagstring(), &mut do_register, toasts);
                        for to_tag in Tag::from_tagstrings(&self.to_tagstrings) {
                            check_exists(&to_tag.to_tagstring(), &mut do_register, toasts);
                            if do_register {
                                let link = TagLink {
                                    from_tagstring: from_tag.to_tagstring(),
                                    to_tagstring: to_tag.to_tagstring(),
                                    link_type: self.link_type.clone(),
                                };
                                if let Ok(does_link_exist) = data::does_tag_ink_exist(&link) {
                                    if does_link_exist {
                                        TagsUI::toast_link_already_exists(&link, toasts)
                                    } else {
                                        if let Err(e) = data::register_tag_link(&link) {
                                            TagsUI::toast_failed_new_link(&link, &e, toasts);
                                        } else {
                                            TagsUI::toast_success_new_link(&link, toasts);
                                            tags::reload_tag_data(&mut self.loaded_tag_data);
                                        }
                                    }
                                } else {
                                    TagsUI::toast_failed_check_link_exists(&link, toasts)
                                }
                            }
                        }
                    }
                }
            });
        });
    }
}

struct ModifyTagUI {
    toasts: ToastsRef,
    loaded_tag_data: TagDataRef,
    entry_info_update_flag: UpdateFlag,
    tag_data_update_flag: UpdateFlag,
    is_new_tag: bool,
    old_tag: Option<Tag>,
    tag_strings: String,
    description: String,
    // tag: Tag,
}

impl ModifyTagUI {
    fn new(
        tag: Option<Tag>,
        loaded_tag_data: &TagDataRef,
        entry_info_update_flag: &UpdateFlag,
        tag_data_update_flag: &UpdateFlag,
        toasts: &ToastsRef,
    ) -> Self {
        Self {
            toasts: Arc::clone(&toasts),
            loaded_tag_data: Rc::clone(&loaded_tag_data),
            is_new_tag: tag.is_none(),
            old_tag: tag.clone(),
            entry_info_update_flag: Arc::clone(&entry_info_update_flag),
            tag_data_update_flag: Arc::clone(&tag_data_update_flag),
            tag_strings: tag.clone().map(|tag| tag.to_tagstring()).unwrap_or(String::new()),
            description: tag.map(|tag| tag.description.unwrap_or(String::new())).unwrap_or(String::new()),
        }
    }
}

impl UserInterface for ModifyTagUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui| {
            egui::Grid::new("new_tag").max_col_width(1000.).show(ui, |ui| {
                // ui.label("name");
                // ui.text_edit_singleline(&mut self.tag.name);
                // ui.end_row();

                // if let Some(space) = self.tag_strings.as_mut() {
                ui.label("tag");
                ui.text_edit_singleline(&mut self.tag_strings);
                ui.end_row();
                // }

                // if let Some(desc) = self.description.as_mut() {
                ui.label("description");
                ui.text_edit_multiline(&mut self.description);
                ui.end_row();
                // }
            });
            ui.add_enabled_ui(!(self.tag_strings.is_empty()), |ui| {
                if self.is_new_tag {
                    if ui.button("create").clicked() {
                        for mut tag in Tag::from_tagstrings(&self.tag_strings) {
                            tag.description = Some(self.description.clone());
                            if let Ok(does_exist) = data::does_tag_exist(&tag) {
                                if does_exist {
                                    TagsUI::toast_tag_already_exists(&tag.to_tagstring(), &self.toasts);
                                } else {
                                    if let Err(e) = data::register_tag(&tag) {
                                        TagsUI::toast_failed_new_tag(&tag.to_tagstring(), &e, &self.toasts);
                                    } else {
                                        TagsUI::toast_success_new_tag(&tag.to_tagstring(), &self.toasts);
                                        tags::reload_tag_data(&mut self.loaded_tag_data);
                                    }
                                }
                            } else {
                                TagsUI::toast_failed_check_tag_exists(&tag.to_tagstring(), &self.toasts)
                            }
                        }
                    }
                } else {
                    if ui.button("save").clicked() {
                        let tags = Tag::from_tagstrings(&self.tag_strings);
                        match &tags[..] {
                            [new_tag] => {
                                let old_tag = self.old_tag.clone();
                                let new_tag = new_tag.clone();
                                let entry_info_update_flag = Arc::clone(&self.entry_info_update_flag);
                                let tag_data_update_flag = Arc::clone(&self.tag_data_update_flag);
                                let toasts = Arc::clone(&self.toasts);
                                //FIXME this will fuck things up if the modify fails lol
                                self.old_tag = Some(new_tag.clone());
                                thread::spawn(move || {
                                    if let Err(e) = data::rename_tag(&old_tag.as_ref().unwrap(), &new_tag) {
                                        TagsUI::toast_fail_modify_tag(
                                            &old_tag.as_ref().unwrap().to_tagstring(),
                                            &new_tag.to_tagstring(),
                                            &e,
                                            &toasts,
                                        );
                                    } else {
                                        TagsUI::toast_success_modify_tag(&old_tag.as_ref().unwrap().to_tagstring(), &new_tag.to_tagstring(), &toasts);
                                        SharedState::set_update_flag(&tag_data_update_flag, true);
                                        SharedState::set_update_flag(&entry_info_update_flag, true);
                                    }
                                });
                            }
                            _ => {
                                TagsUI::toast_fail_invalid_number_tags(1, tags.len(), &self.toasts);
                            }
                        }
                    }
                    if ui.add(ui::caution_button("delete")).clicked() {}
                }
            });
        });
    }
}

struct ModifyNamespacesUI {
    toasts: ToastsRef,
    editing: Vec<usize>,
    new_namespace: Namespace,
}

impl ModifyNamespacesUI {
    fn new(toasts: ToastsRef) -> Self {
        Self {
            toasts,
            editing: vec![],
            new_namespace: Namespace::empty(),
        }
    }
}

impl UserInterface for ModifyNamespacesUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut config = Config::clone();
        ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
            TableBuilder::new(ui)
                .column(Size::remainder())
                .column(Size::relative(0.1).at_most(100.))
                .header(20., |mut header| {
                    header.col(|ui| {
                        ui.label("namespace");
                    });
                    header.col(|ui| {
                        ui.label("color");
                    });
                })
                .body(|mut body| {
                    body.row(18., |mut row| {
                        row.col(|ui| {
                            ui.horizontal(|ui| {
                                ui.add_enabled_ui(!self.new_namespace.name.is_empty(), |ui| {
                                    if ui.button("new").clicked() {
                                        config.namespaces.push(self.new_namespace.clone());
                                        Config::set(config.clone());
                                    }
                                });
                                ui.text_edit_singleline(&mut self.new_namespace.name);
                            });
                        });
                        row.col(|ui| {
                            ui.color_edit_button_rgb(&mut self.new_namespace.color);
                        });
                    });
                    let mut config_changed = false;
                    let mut deleted = vec![];
                    for (index, namespace) in config.namespaces.iter_mut().enumerate() {
                        body.row(18., |mut row| {
                            row.col(|ui| {
                                if self.editing.contains(&index) {
                                    let response = ui.text_edit_singleline(&mut namespace.name);
                                    if response.lost_focus() {
                                        self.editing.retain(|x| x != &index);
                                    } else if response.changed() {
                                        config_changed = true;
                                    }
                                    response.request_focus()
                                } else {
                                    let text = RichText::new(&namespace.name).color(namespace.color32());
                                    let label = Label::new(text).sense(Sense::click());
                                    ui.add(label).context_menu(|ui| {
                                        if ui.button("edit").clicked() {
                                            self.editing.push(index)
                                        }
                                        if ui.button("delete").clicked() {
                                            self.editing.clear();
                                            deleted.push(index);
                                        }
                                    });
                                }
                            });
                            row.col(|ui| {
                                if ui.color_edit_button_rgb(&mut namespace.color).changed() {
                                    config_changed = true;
                                };
                            });
                        })
                    }
                    deleted.sort();
                    deleted.reverse();
                    for d_index in deleted {
                        config.namespaces.remove(d_index);
                        config_changed = true;
                    }
                    if config_changed {
                        Config::set(config);
                    }
                });
        });
    }
}
