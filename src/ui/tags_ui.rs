use super::{
    icon,
    tags::{self, Namespace, Tag, TagDataRef, TagLink},
    widgets::autocomplete,
};
use crate::{
    config::{Color32Opt},
    data::{self, EntryId},
    tags::TagLinkType,
    ui::{self, LayoutJobText, UserInterface, WindowContainer}, app::{SharedState, UpdateList, UpdateFlag},
};
use anyhow::{Error, Result};

use eframe::{
    egui::{self, Layout, RichText, Window},
    emath::Align,
};
use egui::{Color32, Label, Sense};
use egui_extras::{Column, Size, StripBuilder, TableBuilder};

use std::{rc::Rc, sync::Arc, thread, vec};
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

fn render_content_with_options(ui: &mut egui::Ui, options_ui: impl FnOnce(&mut egui::Ui), content_ui: impl FnOnce(&mut egui::Ui)) {
    StripBuilder::new(ui)
        .size(Size::exact(0.)) // FIXME: not sure why this is adding more space.
        .size(Size::exact(ui::constants::OPTIONS_COLUMN_WIDTH))
        .size(Size::exact(ui::constants::SPACER_SIZE))
        .size(Size::remainder())
        .horizontal(|mut strip| {
            strip.empty();
            strip.cell(|ui| {
                options_ui(ui);
            });
            strip.cell(|ui| {
                ui.with_layout(Layout::left_to_right(Align::Center).with_cross_justify(true), |ui| {
                    ui.separator();
                });
            });
            strip.cell(|ui| {
                content_ui(ui);
            });
        });
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
    fn render_options(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
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
                        window: Box::new(ModifyTagUI::new(None, &self.shared_state)),
                        is_open: Some(true),
                    })
                }
            }
            if ui.button(format!("{} new tag link", ui::constants::ADD_ICON)).clicked() {
                let title = "new tag link".to_string();
                if !ui::does_window_exist(&title, &self.modify_windows) {
                    self.modify_windows.push(WindowContainer {
                        title,
                        window: Box::new(ModifyTagLinkUI::new(&self.shared_state)),
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
                        window: Box::new(ModifyNamespacesUI::new(&self.shared_state)),
                        is_open: Some(true),
                    })
                }
            }
            ui.add_space(ui::constants::SPACER_SIZE);
            // if ui.button(ui::icon_text("patch tags", ui::constants::TOOL_ICON)).clicked() {}
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
        let delete_tag_modal = ui::modal(ui.ctx(), "tag_delete_modal");
        let link_delete_modal = ui::modal(ui.ctx(), "link_delete_modal");
        if let Some(tag_pending_delete) = self.tag_pending_delete.as_ref() {
            delete_tag_modal.show(|ui| {
                delete_tag_modal.frame(ui, |ui| {
                    delete_tag_modal.body(
                        ui,
                        ui::generate_layout_job(vec![
                            "delete ".into(),
                            tag_pending_delete.to_layout_job_text(&self.shared_state),
                            "?".into(),
                        ]),
                    );
                });
                delete_tag_modal.buttons(ui, |ui| {
                    delete_tag_modal.button(ui, "cancel");
                    if delete_tag_modal.caution_button(ui, "delete").clicked() {
                        let toasts = Arc::clone(&self.shared_state.toasts);
                        let tag_pending_delete = tag_pending_delete.clone();
                        let tag_data_update_flag = Arc::clone(&self.shared_state.tag_data_update_flag);
                        let updated_entries_list = Arc::clone(&self.shared_state.updated_entries);
                        thread::spawn(move || {
                            let updated_entries = data::get_entries_with_tag(&tag_pending_delete);
                            if let Err(e) = data::delete_tag(&tag_pending_delete) {
                                Self::toast_failed_delete_tag(&tag_pending_delete.to_tagstring(), &e, &toasts);
                            } else {
                                Self::toast_success_delete_tag(&tag_pending_delete.to_tagstring(), &toasts);
                                SharedState::set_update_flag(&tag_data_update_flag, true);
                                if let Ok(modified_entries) = updated_entries {
                                    SharedState::append_to_update_list(&updated_entries_list, modified_entries)
                                }
                            }
                        });
                    }
                })
            });
        }
        if let Some(link_pending_delete) = self.link_pending_delete.as_ref() {
            link_delete_modal.show(|ui| {
                link_delete_modal.frame(ui, |ui| {
                    let mut job_text = link_pending_delete.to_layout_job_text(&self.shared_state);
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
                        .column(Column::auto())
                        .column(Column::remainder())
                        .column(Column::remainder().resizable(true))
                        .column(Column::remainder().resizable(true))
                        .column(Column::remainder().resizable(true))
                        .column(Column::remainder().resizable(true))
                        .column(Column::remainder().resizable(true))
                        .header(ui::constants::TABLE_ROW_HEIGHT, |mut header| {
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
                                        let tag_label = egui::Label::new(tag_data.tag.to_rich_text(&self.shared_state)).sense(egui::Sense::click());
                                        let response = ui.add(tag_label).context_menu(|ui| {
                                            let delete_jt = LayoutJobText::new(format!("{} delete ", ui::constants::DELETE_ICON,));
                                            let edit_jt = LayoutJobText::new(format!("{} edit ", ui::constants::EDIT_ICON,));
                                            let tag_jt = LayoutJobText::new(&tag_data.tag.name)
                                                .with_color(tag_data.tag.namespace_color(&self.shared_state).unwrap_or(ui::text_color()));
                                            let delete_lj = ui::generate_layout_job(vec![delete_jt, tag_jt.clone()]);
                                            let edit_lj = ui::generate_layout_job(vec![edit_jt, tag_jt]);
                                            if ui.button(edit_lj).clicked() {
                                                ui.close_menu();
                                                Self::launch_tag_modify_window(tag_data.tag.clone(), &mut self.modify_windows, &self.shared_state)
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
                                                Label::new(Tag::from_tagstring(&tag_link.from_tagstring).to_rich_text(&self.shared_state))
                                            } else {
                                                Label::new(Tag::from_tagstring(&tag_link.to_tagstring).to_rich_text(&self.shared_state))
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
                                                                    "{} delete {} with ",
                                                                    ui::constants::DELETE_ICON,
                                                                    &link_type.to_string()
                                                                ));
                                                                let job_text_2 = LayoutJobText::new(Tag::from_tagstring(&target_tagstring).name)
                                                                    .with_color(
                                                                        Tag::from_tagstring(&target_tagstring)
                                                                            .namespace_color(&self.shared_state)
                                                                            .unwrap_or(ui::text_color()),
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
            &toasts,
            format!("failed tag modification ( \"{old_tagstring}\" --> \"{new_tagstring}\"): {error} "),
        );
    }
    fn launch_tag_modify_window(tag: Tag, modify_windows: &mut Vec<WindowContainer>, shared_state: &SharedState) {
        let title = format!("edit \"{}\"", tag.to_tagstring());

        if !ui::does_window_exist(&title, modify_windows) {
            modify_windows.push(WindowContainer {
                title,
                window: Box::new(ModifyTagUI::new(Some(tag), shared_state)),
                is_open: Some(true),
            })
        }
    }

    pub fn toast_failed_check_link_exists(link: &TagLink, toasts: &ToastsRef) {
        ui::toast_error_lock(&toasts, format!("error checking if link {} exists", link.to_string()));
    }
    pub fn toast_failed_check_tag_exists(tagstring: &String, toasts: &ToastsRef) {
        ui::toast_error_lock(&toasts, format!("error checking if tag \"{tagstring}\" exists"));
    }
    pub fn toast_failed_delete_link(link: &TagLink, error: &Error, toasts: &ToastsRef) {
        ui::toast_error_lock(&toasts, format!("failed to delete link {}: {error}", link.to_string()));
    }
    pub fn toast_failed_delete_tag(tagstring: &String, error: &Error, toasts: &ToastsRef) {
        ui::toast_error_lock(&toasts, format!("failed to delete tag \"{tagstring}\": {error}"));
    }
    pub fn toast_failed_new_link(link: &TagLink, error: &Error, toasts: &ToastsRef) {
        ui::toast_error_lock(&toasts, format!("failed to register link {}: {error}", link.to_string()));
    }
    pub fn toast_failed_new_tag(tagstring: &String, error: &Error, toasts: &ToastsRef) {
        ui::toast_error_lock(&toasts, format!("failed to register tag \"{tagstring}\": {error}"));
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
        ui::toast_error_lock(&toasts, format!("expected {expected_number} tags, got {recieved_number}"));
    }
    pub fn toast_fail_update_tags(error: &Error, toasts: &ToastsRef) {
        ui::toast_error_lock(&toasts, format!("failed to update tags: {error}"));
    }
    pub fn toast_success_new_link(link: &TagLink, toasts: &ToastsRef) {
        ui::toast_success_lock(toasts, format!("successfully registered link {}", link.to_string()));
    }
    pub fn toast_success_update_tags(amount_updated: usize, toasts: &ToastsRef) {
        ui::toast_success_lock(toasts, format!("successfully updated {amount_updated} entries"));
    }
    pub fn toast_success_new_tag(tagstring: &String, toasts: &ToastsRef) {
        ui::toast_success_lock(toasts, format!("successfully registered tag \"{tagstring}\""));
    }
    pub fn toast_tag_already_exists(tagstring: &String, toasts: &ToastsRef) {
        ui::toast_warning_lock(toasts, format!("tag \"{tagstring}\" already exists"));
    }
    pub fn toast_tag_doesnt_exist(tagstring: &String, toasts: &ToastsRef) {
        ui::toast_error_lock(&toasts, format!("tag \"{tagstring}\" doesn't exist"));
    }
}

struct ModifyTagLinkUI {
    // toasts: ToastsRef,
    // link: TagLink,
    // autocomplete_options: AutocompleteOptionsRef,
    link_type: TagLinkType,
    from_tagstrings: String,
    to_tagstrings: String,
    shared_state: Rc<SharedState>, // loaded_tag_data: TagDataRef,
}

impl ModifyTagLinkUI {
    fn new(shared_state: &Rc<SharedState>) -> Self {
        Self {
            // toasts: Arc::clone(&shared_state.toasts),
            from_tagstrings: String::new(),
            to_tagstrings: String::new(),
            shared_state: Rc::clone(&shared_state),
            // autocomplete_options: Rc::clone(&shared_state.autocomplete_options),
            link_type: TagLinkType::Implication,
            // loaded_tag_data: Rc::clone(&shared_state.tag_data_ref),
        }
    }
}

impl ui::UserInterface for ModifyTagLinkUI {
    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui| {
            let valid = !(self.from_tagstrings.is_empty() || self.to_tagstrings.is_empty());
            egui::Grid::new("new_link").max_col_width(1000.).show(ui, |ui| {
                ui.label("create");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.link_type, TagLinkType::Implication, TagLinkType::Implication.to_string());
                    ui.selectable_value(&mut self.link_type, TagLinkType::Alias, TagLinkType::Alias.to_string());
                });
                ui.end_row();

                if let Some(autocomplete_options) = self.shared_state.autocomplete_options.borrow().as_ref() {
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

                    let toasts = &self.shared_state.toasts;
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
                                if let Ok(does_link_exist) = data::does_tag_link_exist(&link) {
                                    if does_link_exist {
                                        TagsUI::toast_link_already_exists(&link, toasts)
                                    } else {
                                        if let Err(e) = data::register_tag_link(&link) {
                                            TagsUI::toast_failed_new_link(&link, &e, toasts);
                                        } else {
                                            TagsUI::toast_success_new_link(&link, toasts);
                                            let updated_entries = Arc::clone(&self.shared_state.updated_entries);
                                            let tag_update_flag = Arc::clone(&self.shared_state.tag_data_update_flag);
                                            let toasts = Arc::clone(&toasts);
                                            let from_tag = from_tag.clone();
                                            thread::spawn(move || {
                                                let reresolution = || -> Result<Vec<EntryId>> {
                                                    let affected_entries = data::get_entries_with_tag(&from_tag)?;
                                                    data::reresolve_tags_of_entries(&affected_entries)?;
                                                    Ok(affected_entries)
                                                };

                                                match reresolution() {
                                                    Ok(affected_entries) => {
                                                        TagsUI::toast_success_update_tags(affected_entries.len(), &toasts);
                                                        updated_entries.lock().extend(affected_entries);
                                                        SharedState::set_update_flag(&tag_update_flag, true);
                                                    }
                                                    Err(e) => TagsUI::toast_fail_update_tags(&e, &toasts),
                                                }
                                            });
                                            tags::reload_tag_data(&self.shared_state.tag_data_ref);
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
    updated_entries_list: UpdateList<EntryId>,
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
        shared_state: &SharedState, // loaded_tag_data: &TagDataRef,
                                    // entry_info_update_flag: &UpdateFlag,
                                    // tag_data_update_flag: &UpdateFlag,
                                    // toasts: &ToastsRef,
    ) -> Self {
        Self {
            toasts: Arc::clone(&shared_state.toasts),
            loaded_tag_data: Rc::clone(&shared_state.tag_data_ref),
            is_new_tag: tag.is_none(),
            old_tag: tag.clone(),
            updated_entries_list: Arc::clone(&shared_state.updated_entries),
            tag_data_update_flag: Arc::clone(&shared_state.tag_data_update_flag),
            tag_strings: tag.clone().map(|tag| tag.to_tagstring()).unwrap_or(String::new()),
            description: tag.map(|tag| tag.description.unwrap_or(String::new())).unwrap_or(String::new()),
        }
    }
}

impl UserInterface for ModifyTagUI {
    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
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
                                let updated_entries_list = Arc::clone(&self.updated_entries_list);
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
                                        if let Ok(modified_entries) = data::get_entries_with_tag(&new_tag) {
                                            SharedState::append_to_update_list(&updated_entries_list, modified_entries)
                                        } else {
                                        }
                                        SharedState::set_update_flag(&tag_data_update_flag, true);
                                    }
                                });
                            }
                            _ => {
                                TagsUI::toast_fail_invalid_number_tags(1, tags.len(), &self.toasts);
                            }
                        }
                    }
                }
            });
        });
    }
}

struct ModifyNamespacesUI {
    editing_indices: Vec<(usize, String, Color32, Color32)>,
    shared_state: Rc<SharedState>,
    new_namespace: Namespace,
    save_delay: Option<usize>,
}

impl ModifyNamespacesUI {
    fn new(shared_state: &Rc<SharedState>) -> Self {
        Self {
            shared_state: Rc::clone(shared_state),
            editing_indices: vec![],
            new_namespace: Namespace::empty(),
            save_delay: None,
        }
    }
}

impl UserInterface for ModifyNamespacesUI {
    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        let mut shared_colors = self.shared_state.namespace_colors.borrow_mut();
        ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
            TableBuilder::new(ui)
                .column(Column::auto())
                .column(Column::auto())
                .column(Column::initial(0.1).at_most(100.))
                // .header(ui::constants::TABLE_ROW_HEIGHT, |mut header| {
                //     header.col(|ui| {
                //         ui.label("namespace");
                //     });
                //     header.col(|ui| {
                //         ui.label("color");
                //     });
                // })
                .body(|mut body| {
                    body.row(ui::constants::TABLE_ROW_HEIGHT, |mut row| {
                        row.col(|ui| {
                            ui.add_enabled_ui(!self.new_namespace.name.is_empty(), |ui| {
                                if ui.button(icon!("new", ADD_ICON)).clicked() {
                                    shared_colors.insert(self.new_namespace.name.clone(), self.new_namespace.color32());
                                    self.new_namespace = Namespace::empty();
                                }
                            });
                        });
                        row.col(|ui| {
                            ui.text_edit_singleline(&mut self.new_namespace.name);
                        });
                        row.col(|ui| {
                            let mut color_array = self.new_namespace.color_array();
                            if ui.color_edit_button_rgba_unmultiplied(&mut color_array).changed() {
                                self.new_namespace.color = Color32Opt::from_array(color_array);
                            }
                        });
                    });
                    let mut to_stop_editing = vec![];
                    let mut to_start_editing = vec![];

                    for (index, (namespace, color)) in shared_colors.clone().iter().enumerate() {
                        body.row(ui::constants::TABLE_ROW_HEIGHT, |mut row| {
                            let mut is_editing = self.editing_indices.iter_mut().find(|(i, _, _, _)| *i == index);
                            row.col(|ui| {
                                StripBuilder::new(ui)
                                    .size(Size::remainder())
                                    .size(Size::remainder())
                                    .horizontal(|mut strip| {
                                        if let Some((_, namespace_mut, color_mut, original_color)) = is_editing.as_mut() {
                                            strip.cell(|ui| {
                                                if ui
                                                    .add_enabled(!namespace_mut.is_empty(), ui::suggested_button(ui::constants::SAVE_ICON))
                                                    .clicked()
                                                {
                                                    shared_colors.remove(namespace);
                                                    shared_colors.insert(namespace_mut.clone(), color_mut.clone());
                                                    to_stop_editing.push(index);
                                                    let toasts = Arc::clone(&self.shared_state.toasts);
                                                    let shared_colors = shared_colors.clone();
                                                    thread::spawn(move || {
                                                        if let Err(e) = data::set_namespace_colors(&shared_colors) {
                                                            ui::toast_error_lock(&toasts, format!("failed to save namespaces: {e}"))
                                                        };
                                                    });
                                                }
                                            });

                                            strip.cell(|ui| {
                                                if ui.button(ui::constants::REMOVE_ICON).clicked() {
                                                    shared_colors.insert(namespace.clone(), original_color.clone());
                                                    to_stop_editing.push(index);
                                                }
                                            });
                                        } else {
                                            strip.cell(|ui| {
                                                if ui.add(ui::caution_button(ui::constants::DELETE_ICON)).clicked() {
                                                    shared_colors.remove(namespace);
                                                    let toasts = Arc::clone(&self.shared_state.toasts);
                                                    let namespace = namespace.clone();
                                                    thread::spawn(move || {
                                                        if let Err(e) = data::delete_namespace_color(&namespace) {
                                                            ui::toast_error_lock(&toasts, format!("failed to delete namespace: {e}"))
                                                        };
                                                    });
                                                }
                                            });
                                            strip.cell(|ui| {
                                                if ui.button(ui::constants::EDIT_ICON).clicked() {
                                                    to_start_editing.push((index, namespace.clone(), color.clone(), color.clone()));
                                                }
                                            });
                                        }
                                    });
                            });
                            row.col(|ui| {
                                if let Some((_, namespace_mut, _, _)) = is_editing.as_mut() {
                                    ui.text_edit_singleline(namespace_mut);
                                } else {
                                    let text = RichText::new(namespace).color(*color);
                                    let label = Label::new(text).sense(Sense::click());
                                    ui.add(label);
                                }
                            });
                            row.col(|ui| {
                                if let Some((_, _, color_mut, _)) = is_editing.as_mut() {
                                    let mut color_array_f32 = color_mut.to_array().map(|u| u as f32 / 255.);
                                    if ui.color_edit_button_rgba_unmultiplied(&mut color_array_f32).changed() {
                                        let color_array_us = color_array_f32.map(|v| (v * 255.) as u8);
                                        *color_mut = Color32::from_rgba_unmultiplied(
                                            color_array_us[0],
                                            color_array_us[1],
                                            color_array_us[2],
                                            color_array_us[3],
                                        );

                                        shared_colors.insert(namespace.clone(), color_mut.clone());
                                    }
                                }
                            });
                        });
                    }
                    for remove_index in to_stop_editing {
                        self.editing_indices.retain(|(i, _, _, _)| *i != remove_index);
                    }
                    self.editing_indices.append(&mut to_start_editing);

                    if self.save_delay.is_some() {
                        if self.save_delay.unwrap() > 0 {
                            let save_delay = self.save_delay.as_mut().unwrap();
                            *save_delay = *save_delay - 1
                        } else {
                            let namespaces = shared_colors.clone();
                            self.save_delay = None;
                            thread::spawn(move || dbg!(data::set_namespace_colors(&namespaces)));
                        }
                    }
                });
        });
    }
}
