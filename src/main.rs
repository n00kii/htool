#![allow(dead_code)]

mod config;
mod data;
mod gallery;
mod import;
mod tags;
mod ui;
mod autocomplete;
// mod modal;

use anyhow::Result;
use config::Config;

use std::{env, sync::Arc};


fn main() -> Result<()> {
    Config::load();
    let args: Vec<String> = env::args().collect();
    if let Some(command) = args.get(1) {
        match command.as_str() {
            "test_ui" => {
                let mut app = ui::AppUI::new();
                app.load_windows();
                ui::AppUI::start(app);
                Config::save();
                }

            _ => println!("unknown command {command}"),
        }
    } else {
        println!("no command")
    }

    Ok(())
}
