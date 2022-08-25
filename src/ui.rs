// todo put common ui stuff in here, like generating thumbnails of specific sizew

use downcast_rs as downcast;
use egui_notify::Toasts;
use std::{
    cell::{RefCell, RefMut},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use crate::tags::tags_ui;

use super::gallery::gallery_ui::PreviewUI;
// use super::tags::tags_ui;
// use super
use super::config::Config;
use super::gallery::gallery_ui;
use super::import::import_ui;
use anyhow::Result;
use eframe::{
    egui::{self, Id, Response, ScrollArea, Style, Ui, Visuals, WidgetText},
    emath::Vec2,
    epaint::Color32,
};
use egui_extras::RetainedImage;
use image::{FlatSamples, ImageBuffer, Rgba};
use poll_promise::Promise;

pub fn generate_retained_image(image_buffer: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<RetainedImage> {
    let pixels = image_buffer.as_flat_samples();
    let color_image = egui::ColorImage::from_rgba_unmultiplied([pixels.extents().1, pixels.extents().2], pixels.as_slice());
    Ok(RetainedImage::from_color_image("", color_image))
}
#[derive(PartialEq)]
pub enum ImageResizeMethod {
    Stretch,
    Contain,
}
pub struct RenderLoadingImageOptions {
    pub widget_size: [f32; 2],
    pub is_button: bool,
    pub is_button_selected: Option<bool>,
    pub hover_text_on_none_image: Option<WidgetText>,
    pub hover_text_on_loading_image: Option<WidgetText>,
    pub hover_text: Option<WidgetText>,
    pub image_tint: Option<Color32>,
    pub error_label_text: String,
    pub resize_method: ImageResizeMethod,
}

impl Default for RenderLoadingImageOptions {
    fn default() -> Self {
        RenderLoadingImageOptions {
            resize_method: ImageResizeMethod::Contain,
            widget_size: [100., 100.],
            is_button: false,
            is_button_selected: None,
            hover_text_on_none_image: None,
            hover_text_on_loading_image: None,
            hover_text: None,
            image_tint: None,
            error_label_text: "?".into(),
        }
    }
}

// pub fn initialize_toasts(ctx: &egui::Context) -> egui_toast::Toasts {
//     let toast_padding = 10.;
//     let toasts = egui_toast::Toasts::new(ctx)
//         .anchor((0. + toast_padding, ctx.used_rect().bottom() - 10. - toast_padding))
//         .direction(egui::Direction::BottomUp);
//     toasts
// }

// pub fn default_toast_options() -> egui_toast::ToastOptions {
//     egui_toast::ToastOptions {
//         show_icon: true,
//         ..egui_toast::ToastOptions::with_duration(Duration::from_secs(3))
//     }
// }

pub fn toast_info(toasts: &mut Toasts, caption: impl Into<String>) {
    set_default_toast_options(toasts.info(caption));
}
pub fn toast_success(toasts: &mut Toasts, caption: impl Into<String>) {
    set_default_toast_options(toasts.success(caption));
}
pub fn toast_warning(toasts: &mut Toasts, caption: impl Into<String>) {
    set_default_toast_options(toasts.warning(caption));
}
pub fn toast_error(toasts: &mut Toasts, caption: impl Into<String>) {
    set_default_toast_options(toasts.error(caption));
}

pub fn set_default_toast_options(toast: &mut egui_notify::Toast) {
    toast.set_duration(Some(Duration::from_millis(3000))).set_closable(true);
}

pub fn render_loading_image(
    ui: &mut Ui,
    ctx: &egui::Context,
    image: Option<&Promise<Result<RetainedImage>>>,
    options: RenderLoadingImageOptions,
) -> Option<Response> {
    let bind_hover_text = |response: Response, hover_text_option: &Option<WidgetText>| -> Response {
        let mut response = response;
        if let Some(hover_text) = hover_text_option {
            response = response.on_hover_text(hover_text.clone());
        }
        response
    };
    match image {
        None => {
            let spinner = egui::Spinner::new();
            let mut response = ui.add_sized(options.widget_size, spinner);
            response = bind_hover_text(response, &options.hover_text_on_none_image);
            Some(response)
        }
        Some(image_promise) => match image_promise.ready() {
            None => {
                let spinner = egui::Spinner::new();
                let mut response = ui.add_sized(options.widget_size, spinner);
                response = bind_hover_text(response, &options.hover_text_on_loading_image);
                Some(response)
            }
            Some(Err(_image_error)) => {
                let text = egui::RichText::new(options.error_label_text).size(48.0);

                let mut response = if options.is_button {
                    let button = egui::Button::new(text);
                    ui.add_sized(options.widget_size, button)
                } else {
                    let label = egui::Label::new(text).sense(egui::Sense::hover());
                    ui.add_sized(options.widget_size, label)
                };
                response = bind_hover_text(response, &options.hover_text);
                Some(response)
            }

            Some(Ok(image)) => {
                let image_size: [f32; 2] = match options.resize_method {
                    ImageResizeMethod::Contain => {
                        let image_size = image.size_vec2();
                        if image_size.x > image_size.y {
                            let scaling_ratio = options.widget_size[0] / image_size.x;
                            [options.widget_size[0], scaling_ratio * image_size.y]
                        } else {
                            let scaling_ratio = options.widget_size[1] / image_size.y;
                            [scaling_ratio * image_size.x, options.widget_size[1]]
                        }
                    }
                    ImageResizeMethod::Stretch => options.widget_size,
                };

                let mut response = if options.is_button {
                    let mut image_button = egui::ImageButton::new(image.texture_id(ctx), image_size).selected(options.is_button_selected.unwrap());
                    if let Some(tint) = options.image_tint {
                        image_button = image_button.tint(tint);
                    }
                    ui.add_sized(options.widget_size, image_button)
                } else {
                    let mut image_widget = egui::widgets::Image::new(image.texture_id(ctx), image_size);
                    if let Some(tint) = options.image_tint {
                        image_widget = image_widget.tint(tint);
                    }
                    ui.add_sized(options.widget_size, image_widget)
                };
                response = bind_hover_text(response, &options.hover_text);
                Some(response)
            }
        },
    }
    // todo!();
}

pub trait DockedWindow: downcast::Downcast {
    fn get_config(&self) -> Arc<Config>;
    fn set_config(&mut self, config: Arc<Config>);
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context);
}
pub trait FloatingWindow: downcast::Downcast {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context);
}

downcast::impl_downcast!(FloatingWindow);
downcast::impl_downcast!(DockedWindow);

pub struct FloatingWindowState {
    pub title: String,
    pub widget_id: Id,
    pub is_open: bool,
    pub window: Box<dyn FloatingWindow>,
}

struct DockedWindowState {
    id: String,
    window: Box<dyn DockedWindow>,
}

pub struct UserInterface {
    config: Arc<Config>,
    current_window: String,
    floating_windows: Rc<RefCell<Vec<FloatingWindowState>>>,
    docked_windows: Vec<DockedWindowState>,
}

impl eframe::App for UserInterface {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_floating_windows(ctx);
        self.render_top_bar(ctx);
        self.render_side_panel(ctx);
        self.render_current_window(ctx);
        ctx.request_repaint();
    }
}

impl UserInterface {
    pub fn new(config: Arc<Config>) -> Self {
        UserInterface {
            floating_windows: Rc::new(RefCell::new(vec![])),
            docked_windows: vec![],
            current_window: "".into(),
            config,
        }
    }

    pub fn clone_config(&self) -> Arc<Config> {
        Arc::clone(&self.config)
    }

    pub fn start(app: UserInterface) {
        let mut options = eframe::NativeOptions::default();
        options.initial_window_size = Some(Vec2::new(1390.0, 600.0));
        eframe::run_native(
            "htool2",
            options,
            Box::new(|_creation_context| {
                Box::new(app)
            }),
        );
    }

    pub fn load_docked_windows(&mut self) {
        let mut window_states = vec![
            DockedWindowState {
                window: Box::new(import_ui::ImporterUI::default()),
                id: "importer".to_string(),
            },
            DockedWindowState {
                window: Box::new(gallery_ui::GalleryUI {
                    root_interface_floating_windows: Some(Rc::clone(&self.floating_windows)),
                    ..Default::default()
                }),
                id: "gallery".to_string(),
            },
            DockedWindowState {
                window: Box::new(tags_ui::TagsUi::default()),
                id: "tags".to_string(),
            },
        ];

        for window in window_states.iter_mut() {
            window.window.set_config(self.clone_config());
        }
        self.docked_windows = window_states;
    }

    // pub fn launch_preview_by_hash(config: Arc<Config>, mut floating_windows: RefMut<Vec<FloatingWindowState>>, hash: String) {
    //     for window_state in floating_windows.iter_mut() {
    //         if let Some(preview_ui) = window_state.window.downcast_ref::<PreviewUI>() {
    //             if let Some(info_promise) = preview_ui.media_info.as_ref() {
    //                 if let Some(info_res) = info_promise.ready() {
    //                     if let Ok(media_info) = info_res {
    //                         if media_info.hash == hash {
    //                             window_state.is_open = true;
    //                             return;
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //     }

    //     let mut preview = gallery_ui::PreviewUI::new(Arc::clone(&config), hash.clone());
    //     preview.set_media_info_by_hash(hash.clone());
    //     let mut title = hash.clone();
    //     let widget_id = Id::new(&title);
    //     title.truncate(6);
    //     title.insert_str(0, "preview-");
    //     floating_windows.push(FloatingWindowState {
    //         title,
    //         widget_id,
    //         is_open: true,
    //         window: preview,
    //     });
    // }

    pub fn render_floating_windows(&mut self, ctx: &egui::Context) {
        for window_state in self.floating_windows.borrow_mut().iter_mut() {
            // window_state.window.show(ctx, &mut window_state.is_open);
            egui::Window::new(&window_state.title)
                .id(window_state.widget_id)
                .open(&mut window_state.is_open)
                .default_size([800.0, 400.0])
                .vscroll(false)
                .hscroll(true)
                .show(ctx, |ui| {
                    window_state.window.ui(ui, ctx);
                });
        }
    }

    pub fn remove_floating_window() {}

    fn render_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("app_top_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.visuals_mut().button_frame = false;
                egui::widgets::global_dark_light_mode_switch(ui);
                ui.separator();
                for window_state in self.docked_windows.iter_mut() {
                    let response = ui.selectable_label(self.current_window == window_state.id, window_state.id.clone());
                    if response.clicked() {
                        self.current_window = window_state.id.clone();
                    }
                }
            });
        });
    }

    fn render_side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("egui_demo_panel")
            .resizable(false)
            .default_width(145.0)
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                        ui.add_space(5.0);
                        for window_state in self.floating_windows.borrow_mut().iter_mut() {
                            ui.toggle_value(&mut window_state.is_open, &window_state.title);
                        }
                    });
                });
            });
    }

    fn render_current_window(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.current_window == "".to_string() {
                ui.label("Nothing here but us chickens!");
            } else {
                for window_state in self.docked_windows.iter_mut() {
                    if window_state.id == self.current_window {
                        window_state.window.ui(ui, ctx)
                    }
                }
            }
        });
    }
}
