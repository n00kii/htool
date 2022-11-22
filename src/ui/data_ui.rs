use std::fs::{File, read};

use crate::{config::Config, data};
use anyhow::Result;
use super::UserInterface;

#[derive(Default)]
pub struct DataUI {
    pub database_key: String,
    pub database_unlocked: bool,
}

impl UserInterface for DataUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.text_edit_singleline(&mut self.database_key);
        if ui.button("rekey").clicked() {
            dbg!(data::rekey_database(&self.database_key));
        }
        // ui.label("hello world! i am data");
        // let encry_db_path = "test.db";
        // let decry_db_path = "detest.db";
        // let test_key = "hello world".as_bytes();
        // if ui.button("encrypt").clicked() {
        //     let db_path = Config::global().path.database().expect("failed to get db path");
        //     println!("got db path");
        //     let mut db_bytes = read(db_path).expect("failed to read db");
        //     println!("read db");
        //     sqlcrypto::encrypt(&mut db_bytes, test_key).expect("failed to encry db");
        //     println!("encrypted");
        //     std::fs::write(encry_db_path, db_bytes).expect("failed to write new db");
        //     println!("wrote");
        // }
        // if ui.button("decrypt").clicked() {
        //     let mut db_bytes = read(encry_db_path).expect("failed to read encry_db");
        //     println!("read encry db");
        //     sqlcrypto::decrypt(&mut db_bytes, test_key, 1024).expect("failed to encry db");
        //     println!("decry");
        //     std::fs::write(decry_db_path, db_bytes).expect("failed to write new db");
        //     println!("wrote");
        // }
    }
}