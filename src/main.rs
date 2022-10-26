#![allow(dead_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod autocomplete;
mod config;
mod data;
mod gallery;
mod import;
mod tags;
mod ui;
mod util;
// mod modal;

use anyhow::{Result, Context};
use config::Config;

use std::{env, sync::Arc};

fn main() {
    Config::load();
    data::init();
    ui::AppUI::new().start();
    Config::save().context("failed to save config").unwrap();
}
