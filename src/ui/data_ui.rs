use super::{toast_success_lock, SharedState, UserInterface, UpdateFlag, toast_warning_lock};
use crate::ui;
use crate::{config::Config, data};
use anyhow::{Result, anyhow};
use egui::ProgressBar;
use std::fs;
use std::sync::atomic::AtomicBool;
use std::{
    fs::{read, File},
    rc::Rc,
    sync::Arc,
    thread,
};

pub struct DataUI {
    pub shared_state: Rc<SharedState>,
    pub currently_rekeying: UpdateFlag,
    pub database_key: String,
}

impl DataUI {
    pub fn new(shared_state: &Rc<SharedState>) -> Self {
        Self {
            shared_state: Rc::clone(&shared_state),
            database_key: String::new(),
            currently_rekeying: Arc::new(AtomicBool::new(false))
        }
    }
}

const DB_JOURNAL_SUFFIX: &str = "-journal";

impl UserInterface for DataUI {
    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.text_edit_singleline(&mut self.database_key);
        ui.add_enabled_ui(!SharedState::read_update_flag(&self.currently_rekeying), |ui| {
            if ui.button("lock").clicked() {
                data::set_db_key(&String::new());
                SharedState::raise_update_flag(&self.shared_state.database_changed);
            }
            if ui.button("rekey").clicked() {
                let toasts = Arc::clone(&self.shared_state.toasts);
                if self.database_key == data::get_db_key() {
                    toast_warning_lock(&toasts, "that key is already set");
                } else {
                    let new_key = self.database_key.clone();
                    let reasons = Arc::clone(&self.shared_state.disable_navbar);
                    let currently_rekeying = Arc::clone(&self.currently_rekeying);
                    thread::spawn(move || {
                        SharedState::raise_update_flag(&currently_rekeying);
                        SharedState::add_disabled_reason(&reasons, ui::constants::DISABLED_LABEL_REKEY_DATABASE);
                        if let Err(e) = data::rekey_database(&new_key) {
                            toast_success_lock(&toasts, format!("failed to rekey database: {e}"));
                        } else {
                            toast_success_lock(&toasts, "successfully rekeyed database");
                        }
                        SharedState::remove_disabled_reason(&reasons, ui::constants::DISABLED_LABEL_REKEY_DATABASE);
                        SharedState::set_update_flag(&currently_rekeying, false);
                    });
                }
            }
        });
        if SharedState::read_update_flag(&self.currently_rekeying) {
            let mut show_progress = || -> Result<()> {
                let db_path = Config::global().path.database()?;
                let db_journal_name = Config::global().path.database.clone() + DB_JOURNAL_SUFFIX;
                let db_journal_path = db_path.parent().ok_or(anyhow!("invalid parent"))?.join(db_journal_name);

                let max_progress = fs::metadata(db_path)?.len() as f32;
                let current_progress = fs::metadata(db_journal_path)?.len() as f32;

                ui.add(ui::progress_bar(current_progress / max_progress));

                Ok(())
            };

            if let Err(e) = show_progress() {
                ui.label(format!("cant show current progress: {e}"));
            }
            
        }
        ui.label(data::get_db_key());

    }
}
