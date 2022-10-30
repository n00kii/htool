#![allow(dead_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod data;
mod gallery;
mod import;
mod tags;
mod ui;
mod util;
// mod modal;

use anyhow::Context;
use config::Config;

fn main() {
    Config::load();
    data::init();
    ui::AppUI::new().start();
    Config::save().context("failed to save config").unwrap();
}
