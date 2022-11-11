use anyhow::Error;
use data::ImportationStatus;
use egui::Context;

use egui_extras::Size;
use egui_extras::StripBuilder;
use egui_modal::Icon;
use egui_modal::Modal;
use tempfile::tempdir;
use zip::ZipArchive;

use crate::util::BatchPollBuffer;
use crate::util::PollBuffer;

use super::super::data;
use super::super::ui;
use super::super::Config;
use crate::import::scan_directory;
use crate::import::ImportationEntry;
use anyhow::Result;
use eframe::egui::{self, Button, Direction, ProgressBar, ScrollArea, Ui};
use eframe::emath::{Align, Vec2};
use poll_promise::Promise;
use rfd::FileDialog;
use std::cell::RefCell;
use std::collections::HashMap;

use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;

use std::sync::{Arc, Mutex};
use std::thread;

pub struct ImporterUI {
    toasts: egui_notify::Toasts,
    import_failures_list: Arc<Mutex<Vec<(String, Error)>>>,
    importation_entries: Option<Vec<Rc<RefCell<ImportationEntry>>>>,
    alternate_scan_dir: Option<PathBuf>,
    hide_errored_entries: bool,
    skip_thumbnails: bool,
    scan_extension_filter: HashMap<String, HashMap<String, bool>>,
    batch_import_status: Option<Promise<Arc<Result<()>>>>,
    dir_link_map: Arc<Mutex<HashMap<String, i32>>>,
    thumbnail_buffer: PollBuffer<ImportationEntry>,
    import_buffer: BatchPollBuffer<ImportationEntry>,
    is_import_status_window_open: bool,
    is_filters_window_open: bool,
    pending_archive_extracts: Option<Vec<ArchiveExtractionProgress>>, // TODO: doesnt need to be an option
    waiting_for_extracts: bool,
}

struct ArchiveExtractionProgress {
    current_progress: Arc<Mutex<f32>>,
    import_entry_path: PathBuf,
    // current_archive: Arc<String>,
    promise: Promise<Result<Vec<ImportationEntry>>>,
}

impl Default for ImporterUI {
    fn default() -> Self {
        let thumbnail_buffer = PollBuffer::new(
            Some(5_000_000),
            None,
            Some(ImporterUI::buffer_add),
            Some(ImporterUI::thumbnail_buffer_poll),
            Some(ImporterUI::buffer_entry_size),
        );

        let import_poll_buffer = PollBuffer::new(
            Some(30_000_000),
            Some(100),
            Some(ImporterUI::buffer_add),
            Some(ImporterUI::import_buffer_poll),
            Some(ImporterUI::buffer_entry_size),
        );

        let import_buffer = BatchPollBuffer::new(import_poll_buffer);

        Self {
            toasts: egui_notify::Toasts::default().with_anchor(egui_notify::Anchor::BottomLeft),
            import_failures_list: Arc::new(Mutex::new(vec![])),
            thumbnail_buffer,
            pending_archive_extracts: None,
            import_buffer,
            skip_thumbnails: false,
            is_import_status_window_open: false,
            is_filters_window_open: false,
            hide_errored_entries: true,
            batch_import_status: None,
            waiting_for_extracts: false,
            importation_entries: None,
            alternate_scan_dir: None,
            scan_extension_filter: HashMap::from([
                (
                    "image".into(),
                    HashMap::from([("png".into(), true), ("jpg".into(), true), ("webp".into(), true)]),
                ),
                (
                    "video".into(),
                    HashMap::from([("gif".into(), true), ("mp4".into(), true), ("webm".into(), true)]),
                ),
                ("archive".into(), HashMap::from([("zip".into(), true), ("rar".into(), true)])),
            ]),

            dir_link_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl ImporterUI {
    fn buffer_add(media_entry: &Rc<RefCell<ImportationEntry>>) {
        if media_entry.borrow().bytes.is_none() {
            media_entry.borrow_mut().load_bytes();
        }
    }

    // fn is_pending_extractions(&self) -> bool {
    //     self.pending_archive_extracts.is_some()
    // }

    fn thumbnail_buffer_poll(media_entry: &Rc<RefCell<ImportationEntry>>) -> bool {
        let mut media_entry = media_entry.borrow_mut();
        if media_entry.are_bytes_loaded() {
            if media_entry.thumbnail.is_none() {
                media_entry.load_thumbnail(); //FIXME: replace w config
            }
        }
        media_entry.is_thumbnail_loading() || media_entry.thumbnail.is_none()
    }
    fn buffer_entry_size(media_entry: &Rc<RefCell<ImportationEntry>>) -> usize {
        media_entry.borrow().file_size
    }

    fn import_buffer_poll(media_entry: &Rc<RefCell<ImportationEntry>>) -> bool {
        media_entry.borrow().is_importing()
        // true
    }

    fn get_scan_dir(&self) -> PathBuf {
        let landing_result = Config::global().path.landing();
        let landing = landing_result.unwrap_or_else(|_| PathBuf::from(""));
        if self.alternate_scan_dir.is_some() {
            self.alternate_scan_dir.as_ref().unwrap().clone()
        } else {
            landing
        }
    }

    fn is_importing(&self) -> bool {
        match self.batch_import_status.as_ref() {
            None => false,
            Some(import_promise) => match import_promise.ready() {
                Some(_import_res) => false,
                None => true,
            },
        }
    }

    fn is_any_entry_selected(&self) -> bool {
        self.get_selected_media_entries().len() > 0
    }

    fn filter_media_entries(&self, predicate: impl Fn(&Rc<RefCell<ImportationEntry>>) -> bool) -> Vec<Rc<RefCell<ImportationEntry>>> {
        if let Some(media_entries) = self.importation_entries.as_ref() {
            media_entries
                .iter()
                .filter(|media_entry| predicate(media_entry))
                .map(|media_entry| Rc::clone(&media_entry))
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    }

    fn get_importable_media_entries(&self) -> Vec<Rc<RefCell<ImportationEntry>>> {
        self.filter_media_entries(|media_entry| media_entry.borrow().is_importable())
    }

    fn get_selected_media_entries(&self) -> Vec<Rc<RefCell<ImportationEntry>>> {
        self.filter_media_entries(|media_entry| media_entry.borrow().is_selected)
    }

    fn get_archive_import_entries(&self) -> Vec<Rc<RefCell<ImportationEntry>>> {
        self.filter_media_entries(|import_entry| import_entry.borrow().is_archive)
    }

    fn get_pending_archive_import_entries(&self) -> Vec<Rc<RefCell<ImportationEntry>>> {
        self.filter_media_entries(|import_entry| {
            import_entry.borrow().is_archive && import_entry.borrow().match_importation_status(ImportationStatus::Pending)
        })
    }

    fn get_pending_import_entries(&self) -> Vec<Rc<RefCell<ImportationEntry>>> {
        self.filter_media_entries(|import_entry| import_entry.borrow().match_importation_status(ImportationStatus::Pending))
    }

    fn get_number_of_loading_bytes(media_entries: &Vec<ImportationEntry>) -> u32 {
        let mut num_loading = 0;
        for media_entry in media_entries {
            if media_entry.are_bytes_loaded() {
                num_loading += 1;
            }
        }

        num_loading
    }

    fn render_scan_directory_selection(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("scan directory");
            if ui.button("change").clicked() {
                if let Some(path) = FileDialog::new().pick_folder() {
                    self.alternate_scan_dir = Some(path);
                    self.importation_entries = None;
                    self.thumbnail_buffer.entries.clear();
                    self.import_buffer.poll_buffer.entries.clear();
                }
            }
            if self.alternate_scan_dir.as_ref().is_some() && ui.button("remove").clicked() {
                self.alternate_scan_dir = None;
                self.importation_entries = None
            }
            ui.label(format!("{}", self.get_scan_dir().display()));
        });
    }

    fn render_options(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.vertical_centered_justified(|ui| {
            ui.label("options");
            ScrollArea::vertical().id_source("options").auto_shrink([false, false]).show(ui, |ui| {
                if ui
                    .button(format!(
                        "{} {}",
                        ui::constants::SEARCH_ICON,
                        if self.importation_entries.is_some() { "re-scan" } else { "scan" }
                    ))
                    .clicked()
                {
                    let mut extension_filter = vec![];
                    for (_extension_group, extensions) in &self.scan_extension_filter {
                        for (extension, do_include) in extensions {
                            if !do_include {
                                extension_filter.push(extension)
                            }
                        }
                    }
                    match scan_directory(self.get_scan_dir(), 0, None, &extension_filter) {
                        Ok(media_entries) => {
                            let warning_amount = 15000;
                            let amount_entries = media_entries.len();
                            ui::toast_info(&mut self.toasts, format!("found {amount_entries} entries"));
                            if amount_entries > warning_amount {
                                ui::toast_warning(
                                    &mut self.toasts,
                                    format!(
                                        "more than {warning_amount} entries: performance may be limited. \
                                        disabling previews or splitting this directory into multiple \
                                        directories may help."
                                    ),
                                )
                                .set_closable(true)
                                .set_duration(None);
                            }
                            // dbg!(media_entries.len());
                            self.importation_entries = Some(media_entries.into_iter().map(|i| Rc::new(RefCell::new(i))).collect());
                        }
                        Err(e) => {
                            ui::toast_error(&mut self.toasts, format!("failed to scan directory: {e}"));
                        }
                    }
                }
                ui.add_space(ui::constants::SPACER_SIZE);
                if ui.button("filters").clicked() {
                    self.is_filters_window_open = !self.is_filters_window_open
                }

                ui.add_space(ui::constants::SPACER_SIZE);
                ui.group(|ui| {
                    ui.checkbox(&mut self.skip_thumbnails, "disable previews");
                });

                ui.add_space(ui::constants::SPACER_SIZE);

                if ui.add_enabled(self.importation_entries.is_some(), Button::new("select all")).clicked() {
                    for media_entry in self.get_importable_media_entries() {
                        media_entry.borrow_mut().is_selected = true;
                    }
                }

                if ui.add_enabled(self.importation_entries.is_some(), Button::new("deselect all")).clicked() {
                    for media_entry in self.get_importable_media_entries() {
                        media_entry.borrow_mut().is_selected = false;
                    }
                }

                if ui.add_enabled(self.importation_entries.is_some(), Button::new("invert")).clicked() {
                    for media_entry in self.get_importable_media_entries() {
                        let current_state = media_entry.borrow().is_selected;
                        media_entry.borrow_mut().is_selected = !current_state;
                    }
                }

                ui.add_space(ui::constants::SPACER_SIZE);
                let prompt = self.render_extraction_prompt(ctx);

                if ui
                    .add_enabled(
                        self.is_any_entry_selected(),
                        ui::suggested_button(format!("{} import", ui::constants::IMPORT_ICON)),
                    )
                    .clicked()
                {
                    let selected_media_entries = self.get_selected_media_entries();
                    let mut selected_archives_exist = false;
                    ui::toast_info(&mut self.toasts, format!("marked {} media for importing", selected_media_entries.len()));
                    for media_entry in self.get_selected_media_entries() {
                        media_entry.borrow_mut().importation_status = Some(Promise::from_ready(ImportationStatus::Pending));
                        media_entry.borrow_mut().is_selected = false;
                        if media_entry.borrow().is_archive {
                            selected_archives_exist = true;
                        }
                    }
                    if selected_archives_exist {
                        self.waiting_for_extracts = true;
                        prompt.open();
                    } else {
                        self.is_import_status_window_open = true;
                    }
                }
                // });
            });
        });
    }

    fn process_extractions(&mut self) {
        let max_concurrent_extractions = 2;
        let mut new_entries = vec![];
        let mut set_none = false;
        let mut pending_import_entries = self.get_pending_archive_import_entries();
        let next_pending = pending_import_entries.pop();
        if let Some(pending_extractions) = self.pending_archive_extracts.as_mut() {
            if (pending_import_entries.len() == 0) && (pending_extractions.len() == 0) && next_pending.is_none() {
                set_none = true;
                self.is_import_status_window_open = true;
                self.waiting_for_extracts = false;
            }

            let mut i = 0;
            while i < pending_extractions.len() {
                if pending_extractions[i].promise.ready().is_some() {
                    let finished_extraction = pending_extractions.remove(i);
                    if let Ok(import_entries_res) = finished_extraction.promise.try_take() {
                        match import_entries_res {
                            Ok(import_entries) => {
                                let import_entries = import_entries.into_iter().map(|i| Rc::new(RefCell::new(i)));
                                new_entries.extend(import_entries);
                            }
                            Err(_e) => {
                                ui::toast_error(&mut self.toasts, "failed extraction");
                                //FIXME: better messagew
                            }
                        }
                    }
                } else {
                    i += 1;
                }
            }
            if pending_extractions.len() < max_concurrent_extractions {
                if let Some(next_pending) = next_pending {
                    let (sender, promise) = Promise::new();
                    let prog = ArchiveExtractionProgress {
                        current_progress: Arc::new(Mutex::new(0.)),
                        import_entry_path: next_pending.borrow().dir_entry.path(),
                        promise,
                    };
                    let current_progress = Arc::clone(&prog.current_progress);
                    let entry_path = prog.import_entry_path.clone();
                    pending_extractions.push(prog);

                    thread::spawn(move || {
                        let extract = || -> Result<()> {
                            let temp_dir = tempdir()?;

                            let file = File::open(entry_path)?;
                            let mut archive = ZipArchive::new(file)?;
                            for i in 0..archive.len() {
                                let mut current_progress = current_progress.lock().unwrap();
                                *current_progress = i as f32 / archive.len() as f32;
                                drop(current_progress);

                                let mut next_inner_file = archive.by_index(i)?;
                                if let Some(next_inner_file_name) = next_inner_file.enclosed_name() {
                                    let next_inner_file_path = temp_dir.path().join(next_inner_file_name);
                                    let mut output_file = File::create(&next_inner_file_path)?;
                                    io::copy(&mut next_inner_file, &mut output_file)?;
                                }
                            }

                            let mut import_entries = scan_directory(
                                temp_dir.path().to_path_buf(),
                                0,
                                Some(temp_dir.path().as_os_str().to_string_lossy().to_string()),
                                &vec![],
                            );
                            if let Ok(import_entries) = import_entries.as_mut() {
                                import_entries.iter_mut().for_each(|i| {
                                    i.keep_bytes_loaded = true;
                                    i.importation_status = Some(Promise::from_ready(ImportationStatus::Pending));
                                    i.load_bytes();
                                    i.bytes.as_ref().unwrap().block_until_ready();
                                })
                            }
                            sender.send(import_entries);
                            temp_dir.close()?;
                            Ok(())
                        };
                        extract()
                    });
                    // remove extracting entry
                    if let Some(import_entries) = self.importation_entries.as_mut() {
                        import_entries.retain(|i| &*i.borrow() != &*next_pending.borrow())
                    }
                }
            }
        }
        if let Some(import_entries) = self.importation_entries.as_mut() {
            import_entries.append(&mut new_entries);
        }
        if set_none {
            self.pending_archive_extracts = None;
        }
    }

    fn render_files(&mut self, ui: &mut Ui) {
        ui.vertical(|files_col| {
            files_col.label("file");
            if let Some(scanned_dirs) = &mut self.importation_entries {
                ScrollArea::vertical()
                    .id_source("files_col")
                    .auto_shrink([false; 2])
                    .show(files_col, |files_col_scroll| {
                        for media_entry in scanned_dirs.iter_mut() {
                            // display label stuff
                            let is_importable = media_entry.borrow().is_importable();
                            files_col_scroll.add_enabled_ui(is_importable, |files_col_scroll| {
                                let mut label = media_entry.borrow().file_label.clone();
                                let max_len = 25;
                                let mut was_truncated = false;
                                if !self.skip_thumbnails {
                                    if label.len() > max_len {
                                        was_truncated = true;
                                        label = match label.char_indices().nth(max_len) {
                                            None => label,
                                            Some((idx, _)) => label[..idx].to_string(),
                                        };
                                        label.push_str("...");
                                    }
                                }
                                if media_entry
                                    .borrow()
                                    .match_importation_status(ImportationStatus::Fail(anyhow::Error::msg("")))
                                {
                                    label = ui::icon_text(label, ui::constants::ERROR_ICON);
                                } else if media_entry.borrow().match_importation_status(ImportationStatus::Success) {
                                    label = ui::icon_text(label, ui::constants::SUCCESS_ICON);
                                } else if media_entry.borrow().match_importation_status(ImportationStatus::Duplicate) {
                                    label = ui::icon_text(label, ui::constants::WARNING_ICON);
                                }

                                let text = egui::RichText::new(format!("{}", label));
                                // } else if media_entry.borrow().failed_to_load_type() {
                                //     text = text.strikethrough();
                                // }
                                let is_selected = media_entry.borrow().is_selected;
                                let mut response = files_col_scroll.selectable_label(is_selected, text);
                                if response.clicked() && is_importable {
                                    let new_state = !is_selected;
                                    media_entry.borrow_mut().is_selected = new_state;
                                };
                                let _disabled_reason = media_entry.borrow().get_status_label();
                                if let Some(status) = media_entry.borrow().get_status_label() {
                                    response = response.on_hover_text(format!("({status})"));
                                }
                                if was_truncated {
                                    response.on_hover_text_at_pointer(&media_entry.borrow().file_label);
                                }
                            });
                        }
                    });
            }
        });
    }

    fn process_media(&mut self) {
        self.thumbnail_buffer.poll();
        self.import_buffer.poll();

        if let Some(media_entries) = self.importation_entries.as_ref() {
            for media_entry in media_entries {
                //unload bytes if uneccesary
                if !media_entry.borrow().keep_bytes_loaded && media_entry.borrow().are_bytes_loaded() {
                    if !media_entry.borrow().is_importing()
                        && !((media_entry.borrow().is_thumbnail_loading() || media_entry.borrow().thumbnail.is_none()) && !self.skip_thumbnails)
                    {
                        // dbg!("bytes unloaded");
                        media_entry.borrow_mut().bytes = None
                    }
                }
                if !self.skip_thumbnails {
                    if media_entry.borrow().thumbnail.is_none() && !self.thumbnail_buffer.is_full() {
                        let _ = self.thumbnail_buffer.try_add_entry(&media_entry);
                    }
                } else {
                    self.thumbnail_buffer.entries.clear();
                    media_entry.borrow_mut().thumbnail = None;
                }
                if !self.waiting_for_extracts && media_entry.borrow().match_importation_status(ImportationStatus::Pending) {
                    let _ = self.import_buffer.try_add_entry(&media_entry);
                }
                if media_entry.borrow().match_importation_status(ImportationStatus::Duplicate) || media_entry.borrow().match_importation_status(ImportationStatus::Success) {
                    media_entry.borrow_mut().keep_bytes_loaded = false;
                }
            }
            if self.import_buffer.ready_for_batch_action() {
                let reg_forms = self
                    .import_buffer
                    .poll_buffer
                    .entries
                    .iter()
                    .filter_map(|media_entry| media_entry.borrow_mut().generate_reg_form(Arc::clone(&self.dir_link_map)).ok())
                    .collect::<Vec<_>>();

                self.import_buffer
                    .run_action("batch_import", || data::register_media_with_forms(reg_forms))
            }
        }
    }

    fn render_previews(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.label("preview");

            if let Some(importation_entries) = self.importation_entries.as_mut() {
                // iterate through each mediaentry to draw its name on the sidebar, and to load its image
                // wrapped in an arc mutex for multithreading purposes
                ScrollArea::vertical()
                    .id_source("previews_col")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let layout = egui::Layout::from_main_dir_and_cross_align(Direction::LeftToRight, Align::Center).with_main_wrap(true);
                        ui.allocate_ui(Vec2::new(ui.available_size_before_wrap().x, 0.0), |ui| {
                            ui.with_layout(layout, |ui| {
                                for (_index, importation_entry) in importation_entries.iter().enumerate() {
                                    let mut importation_entry = importation_entry.borrow_mut();
                                    let file_label = importation_entry.file_label.clone();
                                    let file_label_clone = file_label.clone();

                                    let mut options = ui::RenderLoadingImageOptions::default();
                                    let thumbnail_size = Config::global().import.thumbnail_size as f32;
                                    options.widget_margin = [10., 10.];
                                    options.desired_image_size = [thumbnail_size, thumbnail_size];
                                    options.hover_text_on_loading_image = Some(format!("{file_label} (loading thumbnail...)",).into());
                                    options.hover_text_on_error_image = Some(Box::new(move |error| format!("{file_label_clone} ({error})").into()));
                                    options.hover_text_on_none_image = Some(format!("{file_label} (waiting to load image...)").into());
                                    options.hover_text = Some(if let Some(status_label) = importation_entry.get_status_label() {
                                        format!("{file_label} ({status_label})").into()
                                    } else {
                                        format!("{file_label}").into()
                                    });
                                    options.image_tint = if importation_entry.match_importation_status(data::ImportationStatus::Success) {
                                        Some(ui::constants::IMPORT_IMAGE_SUCCESS_TINT)
                                    } else if importation_entry.match_importation_status(data::ImportationStatus::Fail(anyhow::Error::msg(""))) {
                                        Some(ui::constants::IMPORT_IMAGE_FAIL_TINT)
                                    } else if importation_entry.match_importation_status(data::ImportationStatus::Duplicate) {
                                        Some(ui::constants::IMPORT_IMAGE_DUPLICATE_TINT)
                                    } else {
                                        None
                                    };
                                    options.is_button = importation_entry.is_importable();
                                    options.is_button_selected = Some(importation_entry.is_selected);
                                    let response = ui::render_loading_preview(ui, ctx, importation_entry.thumbnail.as_mut(), &options);
                                    if let Some(response) = response.as_ref() {
                                        if response.clicked() && importation_entry.is_importable() {
                                            importation_entry.is_selected = !importation_entry.is_selected
                                        }
                                    }
                                }
                            });
                        });
                    });
            }
        });
    }

    fn render_filters_window(&mut self, ctx: &Context) {
        egui::Window::new("filters")
            .open(&mut self.is_filters_window_open)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("filters").max_col_width(100.).striped(true).show(ui, |ui| {
                    for (extension_group, extensions) in self.scan_extension_filter.iter_mut() {
                        let mut any_selected = extensions.values().any(|&x| x);
                        if ui.checkbox(&mut any_selected, extension_group).changed() {
                            for (_extension, do_include) in extensions.iter_mut() {
                                *do_include = any_selected;
                            }
                        }

                        ui.vertical(|ui| {
                            for (extension, do_include) in extensions.iter_mut() {
                                ui.checkbox(do_include, extension);
                            }
                        });
                        ui.end_row();
                    }
                });
            });
    }

    fn render_extraction_prompt(&mut self, ctx: &Context) -> Modal {
        let prompt_show = ui::modal(ctx, "extraction_progress");
        let prompt_ask = ui::modal(ctx, "extraction_prompt");
        // need to prompt_show before pr
        prompt_show.show(|ui| {
            prompt_show.title(ui, "extracting");
            prompt_show.frame(ui, |ui| {
                if let Some(pending_extractions) = self.pending_archive_extracts.as_ref() {
                    if pending_extractions.len() == 0 {
                        prompt_show.close();
                    }
                    for pending_extraction in pending_extractions {
                        if let Ok(progress) = pending_extraction.current_progress.try_lock() {
                            ui.label(format!(
                                "extracting {}...",
                                pending_extraction.import_entry_path.as_os_str().to_string_lossy()
                            ));
                            ui.add(ProgressBar::new(*progress));
                            ui.separator();
                        }
                    }
                }
            });
            prompt_show.buttons(ui, |ui| {
                if prompt_show.button(ui, "cancel import").clicked() {
                    self.waiting_for_extracts = false;
                    for entry in self.get_pending_import_entries() {
                        entry.borrow_mut().is_selected = true;
                        entry.borrow_mut().importation_status = None;
                    }
                }
                if prompt_show.button(ui, "stop, and continue importation").clicked() {
                    self.waiting_for_extracts = false;
                    self.pending_archive_extracts = None;
                }
            });
        });
        prompt_ask.show(|ui| {
            prompt_ask.title(ui, "extract?");
            prompt_ask.frame(ui, |ui| {
                prompt_ask.body_and_icon(
                    ui,
                    format!(
                        "there are {} selected entries that are archives; extract them? if no, they will not be imported.",
                        self.get_pending_archive_import_entries().len()
                    ),
                    Icon::Info,
                );
            });
            prompt_ask.buttons(ui, |ui| {
                if prompt_ask.button(ui, "cancel import").clicked() {
                    self.waiting_for_extracts = false;
                    for entry in self.get_pending_import_entries() {
                        entry.borrow_mut().is_selected = true;
                        entry.borrow_mut().importation_status = None;
                    }
                }
                if prompt_ask.button(ui, "don't extract").clicked() {
                    self.waiting_for_extracts = false;
                    self.is_import_status_window_open = true;
                    for entry in self.get_pending_archive_import_entries() {
                        entry.borrow_mut().importation_status = None;
                    }
                }
                if prompt_ask.button(ui, "extract into pools").clicked() {
                    self.pending_archive_extracts = Some(vec![]);
                    prompt_show.open();
                }
            })
        });

        prompt_ask
    }

    fn render_import_status_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("import status")
            .open(&mut self.is_import_status_window_open)
            .resizable(false)
            .show(ctx, |ui| {
                if let Some(media_entries) = &mut self.importation_entries {
                    let mut total_failed = 0;
                    let mut total_succeeded = 0;
                    let mut total_duplicates = 0;
                    let mut total_not_started = 0;
                    let mut total_currently_importing = 0;
                    let mut total_not_selected_for_import = 0;
                    for media_entry in media_entries.iter_mut() {
                        if let Some(importation_status_promise) = media_entry.borrow().importation_status.as_ref() {
                            if let Some(importation_status) = importation_status_promise.ready() {
                                match importation_status {
                                    ImportationStatus::Pending => total_not_started += 1,
                                    ImportationStatus::Success => total_succeeded += 1,
                                    ImportationStatus::Duplicate => total_duplicates += 1,
                                    ImportationStatus::Fail(_) => total_failed += 1,
                                }
                            } else {
                                total_currently_importing += 1;
                            }
                        } else {
                            total_not_selected_for_import += 1;
                        }
                    }
                    let total_selected_for_import = media_entries.len() - total_not_selected_for_import;

                    ui.vertical_centered(|ui| {
                        ui.label(format!(
                            "{} / {total_selected_for_import} entries processed",
                            total_succeeded + total_duplicates + total_failed
                        ));
                        ui.separator();
                        ui.label(format!("{total_currently_importing} entries currently processing"));
                        ui.label(format!("{total_not_started} entries in queue"));
                        ui.label(format!("{total_succeeded} import successes"));
                        ui.separator();
                        ui.label(format!("{total_duplicates} duplicate entries"));
                        ui.separator();
                        ui.label(format!("{total_failed} import failures"));
                    });
                }
            });
    }
}

impl ui::UserInterface for ImporterUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.process_media();
        self.process_extractions();
        self.render_scan_directory_selection(ui);
        ui.separator();

        if self.skip_thumbnails {
            StripBuilder::new(ui)
                .size(Size::exact(0.))
                .size(Size::exact(ui::constants::OPTIONS_COLUMN_WIDTH))
                .size(Size::remainder())
                .horizontal(|mut strip| {
                    strip.empty();
                    strip.cell(|ui| {
                        self.render_options(ui, ctx);
                    });

                    strip.cell(|ui| {
                        self.render_files(ui);
                    });
                });
        } else {
            StripBuilder::new(ui)
                .size(Size::exact(0.))
                .size(Size::exact(ui::constants::OPTIONS_COLUMN_WIDTH))
                .size(Size::exact(ui::constants::OPTIONS_COLUMN_WIDTH * 2. + 10.))
                .size(Size::remainder())
                .horizontal(|mut strip| {
                    strip.empty();
                    strip.cell(|ui| {
                        self.render_options(ui, ctx);
                    });

                    strip.cell(|ui| {
                        self.render_files(ui);
                    });
                    strip.cell(|ui| {
                        self.render_previews(ui, ctx);
                    });
                });
        }
        self.render_import_status_window(ctx);
        self.render_filters_window(ctx);
        self.toasts.show(ctx);
    }
}
