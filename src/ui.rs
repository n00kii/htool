// todo put common ui stuff in here, like generating thumbnails of specific sizew

use downcast_rs as downcast;
use egui::{text::LayoutJob, Align, Layout, TextFormat};
use egui_notify::Toasts;
use std::{
    cell::{RefCell, RefMut},
    rc::Rc,
    sync::Arc,
    time::Duration,
    vec,
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
    egui::{self, Button, Id, Response, RichText, ScrollArea, Sense, Style, Ui, Visuals, Widget, WidgetText},
    emath::Vec2,
    epaint::Color32,
};
use egui_extras::RetainedImage;
use image::{FlatSamples, ImageBuffer, Rgba};
use poll_promise::Promise;

pub mod constants {
    use eframe::epaint::Color32;

    pub const DELETE_ICON: &str = "🗑️";
    pub const EDIT_ICON: &str = "✏️";

    pub const ICON_PATH: &str = "icon.ico";
    pub const IMPORT_THUMBNAIL_SIZE: f32 = 100.;
    pub const SPACER_SIZE: f32 = 10.;
    pub const DEFAULT_TEXT_COLOR: Color32 = Color32::GRAY;
    pub const CAUTION_BUTTON_FILL: Color32 = Color32::from_rgb(87, 38, 34);
    pub const SUGGESTED_BUTTON_FILL: Color32 = Color32::from_rgb(33, 54, 84);
    pub const CAUTION_BUTTON_TEXT_COLOR: Color32 = Color32::from_rgb(242, 148, 148);
    pub const SUGGESTED_BUTTON_TEXT_COLOR: Color32 = Color32::from_rgb(141, 182, 242);
}

#[derive(Clone)]
pub struct LayoutJobText {
    pub text: String,
    pub format: TextFormat,
    pub offset: f32,
}

impl<T: Into<String>> From<T> for LayoutJobText {
    fn from(text: T) -> Self {
        Self::new(text)
    }
}

// impl Into<LayoutJobText> for LayoutJobText {
//     fn into(self) -> LayoutJobText {
//         self
//     }
// }

impl Default for LayoutJobText {
    fn default() -> Self {
        Self {
            text: String::new(),
            format: TextFormat::default(),
            offset: 0.0,
        }
    }
}

impl LayoutJobText {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }
    pub fn with_offset(mut self, offset: f32) -> Self {
        self.offset = offset;
        self
    }
    pub fn with_color(mut self, color: Color32) -> Self {
        self.format.color = color;
        self
    }
}

pub fn generate_layout_job(text: Vec<impl Into<LayoutJobText>>) -> LayoutJob {
    let mut job = LayoutJob::default();
    for text_data in text {
        let text_data = text_data.into();
        job.append(&text_data.text, text_data.offset, text_data.format);
    }
    job
}

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
    pub desired_image_size: [f32; 2],
    pub widget_margin: [f32; 2],
    pub is_button: bool,
    pub shrink_to_image: bool,
    pub is_button_selected: Option<bool>,
    pub hover_text_on_none_image: Option<WidgetText>,
    pub hover_text_on_loading_image: Option<WidgetText>,
    pub hover_text_on_error_image: Option<Box<dyn Fn(&anyhow::Error) -> WidgetText>>,
    pub hover_text: Option<WidgetText>,
    pub image_tint: Option<Color32>,
    pub error_label_text: String,
    pub resize_method: ImageResizeMethod,
    pub sense: Vec<Sense>,
}

impl RenderLoadingImageOptions {
    pub fn widget_size(&self, image_size: Option<[f32; 2]>) -> [f32; 2] {
        [
            (if self.shrink_to_image && image_size.is_some() {
                image_size.unwrap()[0]
            } else {
                self.desired_image_size[0]
            }) + self.widget_margin[0],
            (if self.shrink_to_image && image_size.is_some() {
                image_size.unwrap()[1]
            } else {
                self.desired_image_size[1]
            }) + self.widget_margin[1],
        ]
    }
    /// Calculates the image size clamped to self.desired_image_size 
    pub fn scaled_image_size(&self, original_image_size: [f32; 2]) -> [f32; 2] {
        match self.resize_method {
            ImageResizeMethod::Contain => {
                if original_image_size[0] > original_image_size[1] {
                    let scaling_ratio = self.desired_image_size[0] / original_image_size[0];
                    [self.desired_image_size[0], scaling_ratio * original_image_size[1]]
                } else {
                    let scaling_ratio = self.desired_image_size[1] / original_image_size[1];
                    [scaling_ratio * original_image_size[0], self.desired_image_size[1]]
                }
            }
            ImageResizeMethod::Stretch => self.desired_image_size,
        }
    }
    // fn image_size(&self)
}

impl Default for RenderLoadingImageOptions {
    fn default() -> Self {
        RenderLoadingImageOptions {
            resize_method: ImageResizeMethod::Contain,
            shrink_to_image: false,
            desired_image_size: [100., 100.],
            widget_margin: [5., 5.],
            is_button: false,
            is_button_selected: None,
            hover_text_on_none_image: None,
            hover_text_on_error_image: None,
            hover_text_on_loading_image: None,
            hover_text: None,
            image_tint: None,
            error_label_text: "?".into(),
            sense: vec![egui::Sense::click()],
        }
    }
}

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

pub fn does_window_exist(title: &String, windows: &Vec<WindowContainer>) -> bool {
    for window in windows.iter() {
        if &window.title == title {
            return true;
        }
    }
    false
}

pub fn caution_button(text: impl std::fmt::Display) -> Button {
    let richtext = RichText::new("delete").color(constants::CAUTION_BUTTON_TEXT_COLOR);
    let button = Button::new(richtext).fill(constants::CAUTION_BUTTON_FILL);
    button
}

pub enum NumericBase {
    Ten,
    Two,
}
pub fn readable_byte_size(byte_size: i64, precision: usize, base: NumericBase) -> String {
    let byte_size = byte_size as f64;
    let unit_symbols = ["B", "KB", "MB", "GB"];
    let byte_factor: f64 = match base {
        NumericBase::Ten => 1000.,
        NumericBase::Two => 1024.,
    };
    let exponent = (((byte_size.ln() / byte_factor.ln()).floor()) as f64).min((unit_symbols.len() - 1) as f64);
    format!(
        "{number:.precision$} {unit_symbol}",
        number = byte_size / byte_factor.powf(exponent),
        unit_symbol = unit_symbols[exponent as usize]
    )
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
            let mut response = ui.add_sized(options.widget_size(None), spinner);
            response = bind_hover_text(response, &options.hover_text_on_none_image);
            Some(response)
        }
        Some(image_promise) => match image_promise.ready() {
            None => {
                let spinner = egui::Spinner::new();
                let mut response = ui.add_sized(options.widget_size(None), spinner);
                response = bind_hover_text(response, &options.hover_text_on_loading_image);
                Some(response)
            }
            Some(Err(image_error)) => {
                let text = egui::RichText::new(&options.error_label_text).size(48.0);

                let mut response = if options.is_button {
                    let mut button = egui::Button::new(text);
                    for sense in &options.sense {
                        button = button.sense(*sense);
                    }
                    ui.add_sized(options.widget_size(None), button)
                } else {
                    let mut label = egui::Label::new(text);
                    for sense in &options.sense {
                        label = label.sense(*sense);
                    }
                    ui.add_sized(options.widget_size(None), label)
                };
                let hover_text = if let Some(format_error) = options.hover_text_on_error_image {
                    Some(format_error(image_error))
                } else {
                    options.hover_text
                };
                response = bind_hover_text(response, &hover_text);
                Some(response)
            }

            Some(Ok(image)) => {
                let image_size: [f32; 2] = options.scaled_image_size(image.size_vec2().into());

                let mut response = if options.is_button {
                    let mut image_button = egui::ImageButton::new(image.texture_id(ctx), image_size).selected(options.is_button_selected.unwrap());
                    for sense in &options.sense {
                        image_button = image_button.sense(*sense);
                    }
                    if let Some(tint) = options.image_tint {
                        image_button = image_button.tint(tint);
                    }
                    ui.add_sized(options.widget_size(Some(image_size)), image_button)
                } else {
                    let mut image_widget = egui::widgets::Image::new(image.texture_id(ctx), image_size);
                    for sense in &options.sense {
                        image_widget = image_widget.sense(*sense);
                    }
                    if let Some(tint) = options.image_tint {
                        image_widget = image_widget.tint(tint);
                    }
                    ui.add_sized(options.widget_size(Some(image_size)), image_widget)
                };
                response = bind_hover_text(response, &options.hover_text);
                Some(response)
            }
        },
    }
}
pub struct WindowContainer {
    pub title: String,
    pub window: Box<dyn UserInterface>,
    pub is_open: Option<bool>,
}

pub trait UserInterface: downcast::Downcast {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context);
}

downcast::impl_downcast!(UserInterface);

fn load_icon(path: &str) -> eframe::IconData {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::open(path)
            .expect("Failed to open icon path")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };

    eframe::IconData {
        rgba: icon_rgba,
        width: icon_width,
        height: icon_height,
    }
}
pub struct AppUI {
    current_window: String,
    windows: Vec<WindowContainer>,
}

impl eframe::App for AppUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_top_bar(ctx);
        self.render_current_window(ctx);
        ctx.request_repaint();
    }
}

impl AppUI {
    pub fn new() -> Self {
        AppUI {
            windows: vec![],
            current_window: "".into(),
        }
    }

    pub fn start(app: AppUI) {
        let mut options = eframe::NativeOptions::default();
        options.initial_window_size = Some(Vec2::new(1390.0, 600.0));
        options.icon_data = Some(load_icon(constants::ICON_PATH));
        eframe::run_native(env!("CARGO_PKG_NAME"), options, Box::new(|_creation_context| Box::new(app)));
    }

    pub fn load_windows(&mut self) {
        let mut windows = vec![
            WindowContainer {
                window: Box::new(import_ui::ImporterUI::default()),
                is_open: None,
                title: "importer".to_string(),
            },
            WindowContainer {
                window: Box::new(gallery_ui::GalleryUI { ..Default::default() }),
                is_open: None,

                title: "gallery".to_string(),
            },
            WindowContainer {
                window: Box::new(tags_ui::TagsUI::default()),
                is_open: None,

                title: "tags".to_string(),
            },
        ];

        self.windows = windows;
    }

    fn render_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("app_top_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.visuals_mut().button_frame = false;
                egui::widgets::global_dark_light_mode_switch(ui);
                ui.separator();
                for window in self.windows.iter_mut() {
                    let response = ui.selectable_label(self.current_window == window.title, window.title.clone());
                    if response.clicked() {
                        self.current_window = window.title.clone();
                    }
                }
            });
        });
    }

    fn render_current_window(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.current_window == "".to_string() {
                ui.with_layout(Layout::centered_and_justified(egui::Direction::TopDown), |ui| {
                    let text = RichText::new(format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))).size(24.);
                    ui.label(text);
                });
            } else {
                for window in self.windows.iter_mut() {
                    if window.title == self.current_window {
                        window.window.ui(ui, ctx)
                    }
                }
            }
        });
    }
}
