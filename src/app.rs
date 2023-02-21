use std::{rc::Rc, sync::{Arc, atomic::{AtomicBool, Ordering}}, cell::RefCell, collections::HashMap};

use egui::{Context, Vec2, Color32};
use egui_notify::Toasts;

use parking_lot::Mutex;
use crate::{ui::{ WindowContainer, gallery_ui::GalleryUI, data_ui::DataUI, self, ToastsRef, widgets::autocomplete::AutocompleteOption}, tags::{self, TagDataRef}, config::Config, data::{self, EntryId}};

pub struct App {
    pub shared_state: Rc<SharedState>,
    pub current_window: String,
    pub windows: Vec<WindowContainer>,
    pub input_database_key: Arc<Mutex<String>>,
}

pub type UpdateFlag = Arc<AtomicBool>;
pub type UpdateList<T> = Arc<Mutex<Vec<T>>>;
pub type AutocompleteOptionsRef = Rc<RefCell<Option<Vec<AutocompleteOption>>>>;

pub struct SharedState {
    pub toasts: ToastsRef,
    pub tag_data_ref: TagDataRef,
    pub autocomplete_options: AutocompleteOptionsRef,
    pub window_title: String,
    pub all_entries_update_flag: UpdateFlag,
    pub updated_entries: UpdateList<EntryId>,
    pub deleted_entries: UpdateList<EntryId>,
    pub tag_data_update_flag: UpdateFlag,
    pub updated_theme_selection: UpdateFlag,
    pub gallery_regenerate_flag: UpdateFlag,
    pub namespace_colors: RefCell<HashMap<String, Color32>>,
    pub database_unlocked: UpdateFlag,
    pub disable_navbar: UpdateList<String>,
    pub database_changed: UpdateFlag,
    pub audio_device: RefCell<egui_video::AudioDevice>,
}

impl SharedState {
    pub fn set_update_flag(flag: &UpdateFlag, new_state: bool) {
        flag.store(new_state, Ordering::Relaxed);
    }
    pub fn raise_update_flag(flag: &UpdateFlag) {
        Self::set_update_flag(flag, true);
    }
    pub fn read_update_flag(flag: &UpdateFlag) -> bool {
        flag.load(Ordering::Relaxed)
    }
    pub fn consume_update_flag(flag: &UpdateFlag) -> bool {
        if Self::read_update_flag(flag) {
            Self::set_update_flag(flag, false);
            true
        } else {
            false
        }
    }
    pub fn add_disabled_reason(list: &UpdateList<String>, reason: &str) {
        list.lock().push(String::from(reason));
    }
    pub fn remove_disabled_reason(list: &UpdateList<String>, reason: &str) {
        list.lock().retain(|l| *l != String::from(reason));
    }
    pub fn set_title(&mut self, addition: Option<String>) {
        if let Some(addition) = addition {
            self.window_title = format!("{}: {}", ui::constants::APPLICATION_NAME, addition)
        } else {
            self.window_title = ui::constants::APPLICATION_NAME.to_string();
        }
    }
    pub fn append_to_update_list<T>(list: &UpdateList<T>, mut new_items: Vec<T>) {
        list.lock().append(&mut new_items)
    }
}


impl App {
    pub fn init() {
        Config::load();
        puffin::set_scopes_on(true);
    }
    
    pub fn new() -> Self {
        let audio_sys = sdl2::init().expect("failed to init sdl2").audio().expect("failed to init audio subsystem");
        
        let shared_state = SharedState {
            audio_device: RefCell::new(egui_video::init_audio_device(&audio_sys).expect("failed to init audio streamer")),
            updated_theme_selection: Arc::new(AtomicBool::new(false)),
            gallery_regenerate_flag: Arc::new(AtomicBool::new(false)),
            tag_data_ref: tags::initialize_tag_data(),
            autocomplete_options: Rc::new(RefCell::new(None)),
            window_title: env!("CARGO_PKG_NAME").to_string(),
            toasts: Arc::new(Mutex::new(Toasts::default().with_anchor(egui_notify::Anchor::BottomLeft))),
            all_entries_update_flag: Arc::new(AtomicBool::new(false)),
            updated_entries: Arc::new(Mutex::new(vec![])),
            deleted_entries: Arc::new(Mutex::new(vec![])),
            tag_data_update_flag: Arc::new(AtomicBool::new(false)),
            database_unlocked: Arc::new(AtomicBool::new(false)),
            namespace_colors: RefCell::new(HashMap::new()),
            disable_navbar: Arc::new(Mutex::new(vec![])),
            database_changed: Arc::new(AtomicBool::new(false)),
            // database_info_modified_flag: Arc::new(AtomicBool::new(false)),
        };
        App {
            shared_state: Rc::new(shared_state),
            windows: vec![],
            current_window: String::new(),
            input_database_key: Arc::new(Mutex::new(String::new())),
        }
    }
    pub fn process_state(&mut self, ctx: &Context) {
        if let Some(mut update_list) = self.shared_state.updated_entries.try_lock() {
            if update_list.len() > 0 {
                if let Some(gallery_container) = self
                    .windows
                    .iter_mut()
                    .find(|container| container.window.downcast_ref::<GalleryUI>().is_some())
                {
                    puffin::profile_scope!("update_gallery_entries");
                    let gallery_window = gallery_container.window.downcast_mut::<GalleryUI>().unwrap();
                    gallery_window.update_entries(&update_list);
                }
            }
            update_list.clear();
        }
        if let Some(mut delete_list) = self.shared_state.deleted_entries.try_lock() {
            if delete_list.len() > 0 {
                if let Some(gallery_container) = self
                    .windows
                    .iter_mut()
                    .find(|container| container.window.downcast_ref::<GalleryUI>().is_some())
                {
                    puffin::profile_scope!("update_gallery_entries");
                    let gallery_window = gallery_container.window.downcast_mut::<GalleryUI>().unwrap();
                    gallery_window.delete_entries(&delete_list);
                }
            }
            delete_list.clear();
        }
        if SharedState::consume_update_flag(&self.shared_state.tag_data_update_flag) {
            tags::reload_tag_data(&self.shared_state.tag_data_ref);
        }
        if SharedState::consume_update_flag(&self.shared_state.updated_theme_selection) {
            App::load_style(ctx);
        }
        if SharedState::consume_update_flag(&self.shared_state.gallery_regenerate_flag) {
            self.generate_gallery_entries();
        }
        if SharedState::consume_update_flag(&self.shared_state.database_changed) {
            self.check_database();
            if let Some(data_ui) = self.find_window::<DataUI>() {
                data_ui.database_info = None;
            }
        }
        *self.shared_state.autocomplete_options.borrow_mut() = tags::generate_autocomplete_options(&self.shared_state);
    }
    pub fn check_database(&mut self) {
        match data::try_unlock_database_with_key(&data::get_database_key()) {
            Ok(true) => {
                self.load_namespace_colors();
                self.generate_gallery_entries();
                tags::reload_tag_data(&self.shared_state.tag_data_ref);
                SharedState::set_update_flag(&self.shared_state.database_unlocked, true);
                SharedState::remove_disabled_reason(&self.shared_state.disable_navbar, ui::constants::DISABLED_LABEL_LOCKED_DATABASE);
            }
            _ => {
                self.current_window = String::new();
                SharedState::set_update_flag(&self.shared_state.database_unlocked, false);
                SharedState::add_disabled_reason(&self.shared_state.disable_navbar, ui::constants::DISABLED_LABEL_LOCKED_DATABASE);
            }
        };
    }

    fn generate_gallery_entries(&mut self) {
        for window in self.windows.iter_mut() {
            if let Some(gallery_ui) = window.window.downcast_mut::<GalleryUI>() {
                gallery_ui.generate_entries();
            }
        }
    }

    pub fn start(mut self) {
        let mut options = eframe::NativeOptions::default();
        options.initial_window_size = Some(Vec2::new(1390.0, 600.0));
        options.icon_data = Some(ui::load_icon());
        // options.decorated = false;
        // options.renderer = Renderer::Wgpu;
        let _ = eframe::run_native(
            ui::constants::APPLICATION_NAME,
            options,
            Box::new(|creation_context| {
                let ctx = &creation_context.egui_ctx;
                Self::load_fonts(ctx);
                Self::load_style(ctx);
                self.load_windows();

                Box::new(self)
            }),
        );
    }
}