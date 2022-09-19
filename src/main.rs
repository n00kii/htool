#![allow(dead_code)]

mod config;
mod data;
mod gallery;
mod import;
mod tags;
mod ui;
mod autocomplete;
mod modal;

use anyhow::Result;
use config::Config;

use std::{env, sync::Arc};


fn main() -> Result<()> {
    let config = Arc::new(Config::load()?);
    let args: Vec<String> = env::args().collect();
    if let Some(command) = args.get(1) {
        match command.as_str() {
            "test_ui" => {
                let mut app = ui::UserInterface::new(Arc::clone(&config));
                app.load_docked_windows();
                ui::UserInterface::start(app);
            }

            _ => println!("unknown command {command}"),
        }
    } else {
        println!("no command")
    }

    Ok(())
}
