use super::{icon, toast_error_lock, toast_success_lock, toast_warning_lock, UserInterface};
use crate::app::{SharedState, UpdateFlag};
use crate::data::{DatabaseInfo, EntryId};
use crate::ui;
use crate::{config::Config, data};
use anyhow::{anyhow, Result};
use egui::{Align, Color32, Context, Label, Layout, ProgressBar, Rounding, Sense, TextEdit};
use egui_extras::{Column, Size, StripBuilder, TableBuilder};
use egui_modal::Modal;
use parking_lot::Mutex;
use poll_promise::Promise;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{rc::Rc, sync::Arc, thread};

pub struct DataUI {
    pub database_info: Option<Promise<Result<DatabaseInfo>>>,
    pub shared_state: Rc<SharedState>,
    pub currently_rekeying: UpdateFlag,
    takeout_progress: Option<Arc<TakeoutProgress>>,
    duplicates: Option<Promise<Result<Vec<Vec<EntryId>>>>>,
    pub database_key: String,
    pub takeout_path: String,
}

struct TakeoutProgress {
    current_index: AtomicUsize,
    total: AtomicUsize,
    cancel_flag: AtomicBool,
    current_item: Arc<Mutex<String>>,
}

impl DataUI {
    pub fn new(shared_state: &Rc<SharedState>) -> Self {
        Self {
            shared_state: Rc::clone(&shared_state),
            database_info: None,
            database_key: String::new(),
            takeout_path: String::new(),
            currently_rekeying: Arc::new(AtomicBool::new(false)),
            takeout_progress: None,
            duplicates: None,
        }
    }
}

const DB_JOURNAL_SUFFIX: &str = "-journal";

impl UserInterface for DataUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.process_database_info();
        self.render_takeout_progress(ctx);
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
                    self.render_data_info(ui, ctx);
                });
            });
    }
}

impl DataUI {
    fn process_database_info(&mut self) {
        if self.database_info.is_none() {
            self.load_database_info()
        }
    }
    fn load_database_info(&mut self) {
        self.database_info = Some(Promise::spawn_thread("load_db_info", || data::load_database_info()))
    }
    fn render_flush_table_modal(shared_state: &SharedState, ctx: &Context, deletion_label: &str, flush_fn: fn() -> Result<()>) -> Modal {
        let initial_confirm_modal = Modal::new(ctx, format!("initial_flush_table_modal_{deletion_label}"));
        let final_confirm_modal = Modal::new(ctx, format!("final_flush_table_modal_{deletion_label}"));

        initial_confirm_modal.show(|ui| {
            initial_confirm_modal.title(ui, format!("delete {deletion_label}"));
            initial_confirm_modal.frame(ui, |ui| {
                initial_confirm_modal.body(ui, format!("delete records of {deletion_label}?"));
            });
            initial_confirm_modal.buttons(ui, |ui| {
                initial_confirm_modal.button(ui, "cancel");
                if initial_confirm_modal.caution_button(ui, icon!("delete", DELETE_ICON)).clicked() {
                    final_confirm_modal.open()
                };
            });
        });

        final_confirm_modal.show(|ui| {
            final_confirm_modal.title(ui, format!("confirm: delete {deletion_label}?"));
            final_confirm_modal.frame(ui, |ui| {
                final_confirm_modal.body(
                    ui,
                    format!("truly PERMANENTLY delete ALL records of {deletion_label}?\nit will be lost forever."),
                );
            });
            final_confirm_modal.buttons(ui, |ui| {
                final_confirm_modal.button(ui, "cancel");
                if final_confirm_modal.caution_button(ui, icon!("really delete", DELETE_ICON)).clicked() {
                    let toasts = Arc::clone(&shared_state.toasts);
                    let deletion_label = deletion_label.to_string();
                    let update_flag = Arc::clone(&shared_state.database_changed);
                    thread::spawn(move || {
                        if let Err(e) = flush_fn() {
                            ui::toast_error_lock(&toasts, format!("failed to delete {deletion_label}: {e}"));
                        } else {
                            ui::toast_success_lock(&toasts, format!("successfully deleted all records of {deletion_label}"));
                            SharedState::raise_update_flag(&update_flag);
                        };
                    });
                };
            });
        });

        initial_confirm_modal
    }
    fn render_options(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let rekey_modal = self.render_rekey_modal(ctx);
        let export_all_modal = self.render_takeout_modal(ctx);
        ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
            ui.label("data");
            if ui.button(icon!("refresh", REFRESH_ICON)).clicked() {
                self.load_database_info();
            }
            ui::space(ui);
            ui.add_enabled_ui(
                self.database_info
                    .as_ref()
                    .and_then(|p| p.ready().and_then(|r| r.as_ref().ok().and_then(|i| Some(!i.is_unencrypted))))
                    .unwrap_or(false),
                |ui| {
                    if ui.button(icon!("lock", KEY_ICON)).clicked() {
                        data::set_db_key(&String::new());
                        SharedState::raise_update_flag(&self.shared_state.database_changed);
                    }
                },
            );
            if ui.button(icon!("rekey", REKEY_ICON)).clicked() {
                rekey_modal.open();
            }
            ui::space(ui);
            if ui.button(icon!("takeout", EXPORT_ICON)).clicked() {
                export_all_modal.open();
            }
            ui::space(ui);
            if ui.button(icon!("deduplicate", DUPLICATE_ICON)).clicked() {
                dbg!(data::find_duplicates());
            }
        });
    }
    fn render_data_info(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        match self.database_info.as_ref().and_then(|p| p.ready()) {
            Some(Ok(database_info)) => {
                ui.push_id("security_table", |ui| {
                    TableBuilder::new(ui)
                        .column(Column::auto().at_least(ui::constants::OPTIONS_COLUMN_WIDTH))
                        .column(Column::remainder().resizable(true))
                        .header(ui::constants::TABLE_ROW_HEIGHT, |mut header| {
                            header.col(|ui| {
                                ui.label("[security]");
                            });
                        })
                        .body(|mut body| {
                            body.row(ui::constants::TABLE_ROW_HEIGHT, |mut row| {
                                row.col(|ui| {
                                    ui.label("database status");
                                });
                                row.col(|ui| {
                                    if database_info.is_unencrypted {
                                        ui.label("unencrypted");
                                    } else {
                                        ui.label("encrypted");
                                    }
                                });
                            });
                            body.row(ui::constants::TABLE_ROW_HEIGHT, |mut row| {
                                if !database_info.is_unencrypted {
                                    row.col(|ui| {
                                        ui.label("database key");
                                    });
                                    row.col(|ui| {
                                        let mut resp_rect = ui.label(&database_info.current_key).rect;
                                        resp_rect.set_width(ui.available_width());
                                        let spoiler_color = if ui.rect_contains_pointer(resp_rect) {
                                            Color32::TRANSPARENT
                                        } else {
                                            Config::global().themes.tertiary_bg_fill_color().unwrap_or(Color32::BLACK)
                                        };
                                        ui.painter().rect_filled(resp_rect, Rounding::none(), spoiler_color);
                                    });
                                }
                            });
                        });
                });
                ui.separator();
                ui.push_id("size_table", |ui| {
                    TableBuilder::new(ui)
                        .column(Column::auto().at_least(ui::constants::OPTIONS_COLUMN_WIDTH))
                        .column(Column::remainder().resizable(true))
                        .column(Column::remainder().resizable(true))
                        .header(ui::constants::TABLE_ROW_HEIGHT, |mut header| {
                            header.col(|ui| {
                                ui.label("[database table]");
                            });
                            header.col(|ui| {
                                ui.label("[size on disk]");
                            });
                            header.col(|ui| {
                                ui.label("[number of entries]");
                            });
                        })
                        .body(|mut body| {
                            let mut table_row = |label: &str, size: usize, count: usize, flush_fn: fn() -> Result<()>| {
                                let flush_modal = Self::render_flush_table_modal(&self.shared_state, ctx, label, flush_fn);
                                body.row(ui::constants::TABLE_ROW_HEIGHT, |mut row| {
                                    row.col(|ui| {
                                        ui.add(Label::new(label).sense(Sense::click())).context_menu(|ui| {
                                            if ui.add(ui::caution_button(icon!("flush", DELETE_ICON))).clicked() {
                                                flush_modal.open();
                                                ui.close_menu();
                                            }
                                        });
                                    });
                                    row.col(|ui| {
                                        ui.label(ui::readable_byte_size(size as i64, 3, ui::NumericBase::Ten));
                                    });
                                    row.col(|ui| {
                                        ui.label(count.to_string());
                                        // ui.label(if let Some(count) = count_opt {
                                        //     RichText::new(count.to_string())
                                        // } else {
                                        //     RichText::new("n/a").weak()
                                        // });
                                    });
                                });
                            };
                            table_row(
                                "media bytes",
                                database_info.media_bytes_size,
                                database_info.media_bytes_count,
                                data::flush_media_bytes,
                            );
                            table_row(
                                "thumbnails",
                                database_info.thumbnail_cache_size,
                                database_info.thumbnail_cache_count,
                                data::flush_thumbnail_cache,
                            );
                            table_row(
                                "entry info",
                                database_info.entry_info_size + database_info.media_links_size,
                                database_info.entry_info_count,
                                data::flush_entry_info_media_links,
                            );
                            table_row(
                                "entry tags",
                                database_info.entry_tags_size,
                                database_info.entry_tags_count,
                                data::flush_entry_tags,
                            );
                            table_row(
                                "tag definitions",
                                database_info.tag_info_size + database_info.tag_links_size,
                                database_info.tag_info_count,
                                data::flush_tag_definitions,
                            );
                        });
                });
            }
            Some(Err(e)) => {
                ui.label(format!("failed to get database info: {e}"));
            }
            None => {
                ui.centered_and_justified(|ui| {
                    ui.spinner();
                });
            }
        }
    }
    fn render_takeout_progress(&mut self, ctx: &Context) {
        let mut close = false;
        if let Some(takeout_progress) = self.takeout_progress.as_deref() {
            let current = takeout_progress.current_index.load(Ordering::Relaxed) + 1;
            let total = takeout_progress.total.load(Ordering::Relaxed);
            let progress = current as f32 / total as f32;
            egui::Window::new("takeout progress").show(ctx, |ui| {
                if progress != 1. {
                    let current_item = if let Some(current_item) = takeout_progress.current_item.try_lock() {
                        current_item.clone()
                    } else {
                        String::from("[ ? ]")
                    };
                    ui.label(format!("exporting {current_item}..."));
                    ui.add(ProgressBar::new(progress).animate(true).text(format!("{current} / {total}")));
                    ui.separator();
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        if ui.button("cancel").clicked() {
                            takeout_progress.cancel_flag.store(true, Ordering::Relaxed);
                        }
                    });
                } else {
                    ui.vertical_centered_justified(|ui| {
                        ui.label(icon!(format!("{total} items exported"), SUCCESS_ICON));
                        if ui.button(icon!("open takeout folder", OPEN_ICON)).clicked() {
                            if let Err(e) = open::that(&self.takeout_path) {
                                ui::toast_error_lock(&self.shared_state.toasts, format!("failed to open path: {e}"));
                            }
                        }
                        if ui.button("close").clicked() {
                            close = true;
                        }
                    });
                }
            });
        }
        if close {
            self.takeout_progress = None;
        }
    }

    fn render_takeout_modal(&mut self, ctx: &Context) -> Modal {
        let ask_modal = Modal::new(ctx, "export_everything_ask_modal");
        ask_modal.show(|ui| {
            ask_modal.title(ui, icon!("export all items", EXPORT_ICON));
            ask_modal.frame(ui, |ui| {
                ask_modal.body(ui, "export everything?\nchoose a path to export to");
                let tedit_resp = ui.add(
                    TextEdit::singleline(&mut self.takeout_path)
                        .hint_text("click to set path")
                        .interactive(false)
                        .clip_text(false),
                );

                if ui.interact(tedit_resp.rect, tedit_resp.id.with("click_sense"), Sense::click()).clicked() {
                    if let Some(path_buf) = rfd::FileDialog::new().pick_folder() {
                        self.takeout_path = path_buf.as_path().to_string_lossy().to_string();
                    }
                }
                ask_modal.buttons(ui, |ui| {
                    ask_modal.button(ui, "cancel");
                    if ask_modal.suggested_button(ui, icon!("export all", EXPORT_ICON)).clicked() {
                        self.takeout_progress = Some(Arc::new(TakeoutProgress {
                            current_index: AtomicUsize::new(0),
                            total: AtomicUsize::new(0),
                            cancel_flag: AtomicBool::new(false),
                            current_item: Arc::new(Mutex::new(String::new())),
                        }));
                        let takeout_progress = Arc::clone(self.takeout_progress.as_ref().unwrap());
                        let toasts = Arc::clone(&self.shared_state.toasts);
                        let export_path = PathBuf::from(&self.takeout_path);
                        thread::spawn(move || match data::get_all_entry_info() {
                            Ok(mut entry_info) => {
                                entry_info.retain(|e| e.details().is_independant);
                                takeout_progress.total.store(entry_info.len(), Ordering::Relaxed);
                                for (i, entry_info) in entry_info.iter().enumerate() {
                                    if takeout_progress.cancel_flag.load(Ordering::Relaxed) {
                                        break;
                                    }
                                    *takeout_progress.current_item.lock() = ui::pretty_entry_id(entry_info.entry_id());
                                    takeout_progress.current_index.store(i, Ordering::Relaxed);
                                    if let Err(e) = data::export_entry(entry_info.entry_id(), export_path.clone()) {
                                        ui::toast_error_lock(&toasts, format!("failed to export {}: {e}", ui::pretty_entry_id(entry_info.entry_id())))
                                    }
                                }
                            }
                            Err(e) => ui::toast_error_lock(&toasts, format!("failed to load entry ids: {e}")),
                        });
                    }
                });
            });
        });
        ask_modal
    }
    fn render_rekey_modal(&mut self, ctx: &Context) -> Modal {
        let rekey_modal = Modal::new(ctx, "rekey_modal");
        rekey_modal.show(|ui| {
            rekey_modal.title(ui, icon!("rekey database", REKEY_ICON));
            if SharedState::read_update_flag(&self.currently_rekeying) {
                let get_progress = || -> Result<f32> {
                    let db_path = Config::global().path.database()?;
                    let db_journal_name = Config::global().path.database.clone() + DB_JOURNAL_SUFFIX;
                    let db_journal_path = db_path.parent().ok_or(anyhow!("invalid parent"))?.join(db_journal_name);

                    let max_progress = fs::metadata(db_path)?.len() as f32;
                    let current_progress = fs::metadata(db_journal_path)?.len() as f32;

                    Ok(current_progress / max_progress)
                };

                rekey_modal.frame(ui, |ui| match get_progress() {
                    Ok(progress) => {
                        ui.label("current progress:");
                        ui.add(ui::progress_bar(progress));
                        if progress == 1. {
                            rekey_modal.close();
                        }
                    }
                    Err(e) => {
                        ui.label(format!("cant show current progress: {e}"));
                    }
                });
            } else {
                rekey_modal.frame(ui, |ui| {
                    ui.vertical_centered_justified(|ui| {
                        rekey_modal.body(ui, "enter new database key (leave blank to keep database unencrypted):");
                        ui.text_edit_singleline(&mut self.database_key);
                    });
                });
                rekey_modal.buttons(ui, |ui| {
                    rekey_modal.button(ui, "cancel");
                    if rekey_modal.button(ui, icon!("rekey", REKEY_ICON)).clicked() {
                        rekey_modal.open();
                        let toasts = Arc::clone(&self.shared_state.toasts);
                        if self.database_key == data::get_database_key() {
                            toast_warning_lock(&toasts, "that key is already set");
                        } else {
                            let new_key = self.database_key.clone();
                            let reasons = Arc::clone(&self.shared_state.disable_navbar);
                            let currently_rekeying = Arc::clone(&self.currently_rekeying);
                            let database_changed = Arc::clone(&self.shared_state.database_changed);
                            thread::spawn(move || {
                                SharedState::raise_update_flag(&currently_rekeying);
                                SharedState::add_disabled_reason(&reasons, ui::constants::DISABLED_LABEL_REKEY_DATABASE);
                                match data::rekey_database(&new_key) {
                                    Err(e) => {
                                        toast_error_lock(&toasts, format!("failed to rekey database: {e}"));
                                    }
                                    Ok(paths_opt) => {
                                        toast_success_lock(&toasts, "successfully rekeyed database");
                                        if let Some((old_db_path, new_db_path)) = paths_opt {
                                            let replace = || -> Result<()> {
                                                fs::remove_file(&old_db_path)?;
                                                fs::rename(new_db_path, old_db_path)?;
                                                Ok(())
                                            };
                                            if let Err(e) = replace() {
                                                toast_error_lock(&toasts, format!("failed to replace db file: {e}"));
                                            } else {
                                                SharedState::raise_update_flag(&database_changed);
                                            }
                                        }
                                    }
                                }
                                SharedState::remove_disabled_reason(&reasons, ui::constants::DISABLED_LABEL_REKEY_DATABASE);
                                SharedState::set_update_flag(&currently_rekeying, false);
                            });
                        }
                    };
                })
            }
        });
        rekey_modal
    }
}
