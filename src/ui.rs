// todo put common ui stuff in here, like generating thumbnails of specific sizew

use downcast_rs as downcast;
use egui::{pos2, text::LayoutJob, vec2, Align, FontData, FontDefinitions, FontFamily, Layout, Pos2, Rect, Shape, Stroke, TextFormat};
use egui_notify::Toasts;
use std::{
    cell::{RefCell, RefMut},
    fmt::Display,
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
    epaint::{Color32, PathShape},
    wgpu::Color,
    Renderer,
};
use egui_extras::RetainedImage;
use image::{FlatSamples, ImageBuffer, Rgba};
use poll_promise::Promise;

pub mod constants {
    use eframe::epaint::Color32;

    pub const BOOKMARK_ICON: &str = "♥";
    pub const DELETE_ICON: &str = "🗑";
    pub const EDIT_ICON: &str = "✏";
    pub const IMPORT_ICON: &str = "📥";
    pub const EXPORT_ICON: &str = "⎙";
    pub const TAG_ICON: &str = "#";
    pub const GALLERY_ICON: &str = "🖼";
    pub const SEARCH_ICON: &str = "🔎";
    pub const REFRESH_ICON: &str = "⟳";
    pub const ADD_ICON: &str = "➕";
    pub const REMOVE_ICON: &str = "❌";
    pub const SAVE_ICON: &str = "💾";
    pub const COPY_ICON: &str = "📋";

    pub const APPLICATION_ICON_PATH: &str = "src/resources/icon.ico";
    pub const OPTIONS_COLUMN_WIDTH: f32 = 100.;
    pub const SPACER_SIZE: f32 = 10.;
    pub const DEFAULT_TEXT_COLOR: Color32 = Color32::GRAY;

    pub const FAVORITE_ICON_SELECTED_FILL: Color32 = Color32::from_rgb(252, 191, 73);
    pub const FAVORITE_ICON_DESELECTED_FILL: Color32 = Color32::from_rgb(126, 95, 36);
    pub const FAVORITE_ICON_DESELECTED_STROKE: Color32 = Color32::from_rgb(63, 48, 18);

    pub const CAUTION_BUTTON_FILL: Color32 = Color32::from_rgb(87, 38, 34);
    pub const SUGGESTED_BUTTON_FILL: Color32 = Color32::from_rgb(33, 54, 84);
    pub const CAUTION_BUTTON_TEXT_COLOR: Color32 = Color32::from_rgb(242, 148, 148);
    pub const SUGGESTED_BUTTON_TEXT_COLOR: Color32 = Color32::from_rgb(141, 182, 242);

    pub const IMPORT_IMAGE_HIDDEN_TINT: Color32 = Color32::from_rgb(220, 220, 220);
    pub const IMPORT_IMAGE_UNLOADED_TINT: Color32 = Color32::from_rgb(200, 200, 200);
    pub const IMPORT_IMAGE_SUCCESS_TINT: Color32 = Color32::from_rgb(200, 200, 255);
    pub const IMPORT_IMAGE_DUPLICATE_TINT: Color32 = Color32::from_rgb(200, 255, 200);
    pub const IMPORT_IMAGE_FAIL_TINT: Color32 = Color32::from_rgb(255, 200, 200);

    pub const COLOR_LIGHTEN_FACTOR: f32 = 2.;
    pub const COLOR_DARKEN_FACTOR: f32 = 1. / COLOR_LIGHTEN_FACTOR;
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

pub fn icon_text(text: impl Display, icon: &str) -> String {
    format!("{icon} {text}")
}

// def createRegularPolygon(numSides, sideLength, angleOffsetRadians=0, centerPoint=None, alternateInnerSize=None):
// polygon = QPolygonF()
// totalAngleDegrees = 180 * (numSides - 2)
// angleStepDegrees = totalAngleDegrees / numSides
// angleStepDegreeesOfUnitCircle = 180 - angleStepDegrees
// if centerPoint == None: centerPoint = QPoint(0, 0)
// for pointIndex in range(numSides):
//     radius = sideLength

//     if alternateInnerSize and (pointIndex % 2 == 0):
//         radius = alternateInnerSize

//     currentAngleRadians = math.radians(pointIndex * angleStepDegreeesOfUnitCircle) + angleOffsetRadians
//     pointX = radius * math.cos(currentAngleRadians)
//     pointY = radius * math.sin(currentAngleRadians)
//     point = QPointF(pointX, pointY) + centerPoint
//     polygon.append(point)

// return polygon

pub fn scale_color(base_color: Color32, color_factor: f32) -> Color32 {
    Color32::from_rgb(
        (base_color.r() as f32 * color_factor) as u8,
        (base_color.g() as f32 * color_factor) as u8,
        (base_color.b() as f32 * color_factor) as u8,
    )
}

pub fn darker(base_color: Color32) -> Color32 {
    scale_color(base_color, constants::COLOR_DARKEN_FACTOR)
}

pub fn ligher(base_color: Color32) -> Color32 {
    scale_color(base_color, constants::COLOR_LIGHTEN_FACTOR)
}

pub fn shaped_select(ui: &mut Ui, current_value: &mut i64, max_value: usize) {
    let outer_canvas_height = 25.;
    let step_spacing = 0.3; // fraction
    let canvas_padding = 5.;
    let stroke_width = 1.;

    let outer_canvas_width = ui.available_width();
    let stepper_size = vec2(outer_canvas_width, outer_canvas_height);
    let (response, painter) = ui.allocate_painter(stepper_size, Sense::click());
    let outer_step_width = outer_canvas_width / max_value as f32;
    let inner_step_width = outer_step_width * (1. - step_spacing);
    let inner_canvas_height = outer_canvas_height - canvas_padding;
    let mut was_clicked = false;

    if response.double_clicked() {
        *current_value = 0;
    } else if response.clicked() || response.dragged() {
        was_clicked = true;
    }

    for step_index in 0..max_value {
        let is_selected = step_index < *current_value as usize;
        let left = response.rect.left() + step_index as f32 * outer_step_width;
        let top = response.rect.top() + canvas_padding;
        let right = left + outer_step_width;
        let bottom = top + inner_canvas_height;
        let step_center = [(left + right) / 2., (top + bottom) / 2.];
        let step_rect = Rect::from_min_max(pos2(left, top), pos2(right, bottom));
        let shape_radius = inner_step_width / 1.5;
        if was_clicked && ui.rect_contains_pointer(step_rect) {
            *current_value = (step_index as i64) + 1;
        }
        let (fill_color, stroke_color) = if is_selected {
            (constants::FAVORITE_ICON_SELECTED_FILL, darker(constants::FAVORITE_ICON_SELECTED_FILL))
        } else {
            (
                darker(constants::FAVORITE_ICON_SELECTED_FILL),
                darker(darker(constants::FAVORITE_ICON_SELECTED_FILL)),
            )
        };
        let shape = generate_star_shape(shape_radius, step_center, fill_color, Stroke::new(stroke_width, stroke_color));
        painter.add(shape);
    }
}

pub fn star_rating(ui: &mut Ui, current_value: &mut i64, max_value: usize) -> Response {
    let outer_canvas_height = 25.;
    let step_spacing = 0.3; // fraction
    let canvas_padding = 5.;
    let stroke_width = 1.;

    let outer_canvas_width = ui.available_width();
    let stepper_size = vec2(outer_canvas_width, outer_canvas_height);
    let (mut response, painter) = ui.allocate_painter(stepper_size, Sense::click());
    let outer_step_width = outer_canvas_width / max_value as f32;
    let inner_step_width = outer_step_width * (1. - step_spacing);
    let inner_canvas_height = outer_canvas_height - canvas_padding;
    let mut was_clicked = false;
    if response.double_clicked() {
        response.mark_changed();
        *current_value = 0;
    } else if response.clicked() || response.dragged() {
        response.mark_changed();
        was_clicked = true;
    }

    for step_index in 0..max_value {
        let is_selected = step_index < *current_value as usize;
        let left = response.rect.left() + step_index as f32 * outer_step_width;
        let top = response.rect.top() + canvas_padding;
        let right = left + outer_step_width;
        let bottom = top + inner_canvas_height;
        let step_center = [(left + right) / 2., (top + bottom) / 2.];
        let step_rect = Rect::from_min_max(pos2(left, top), pos2(right, bottom));
        let shape_radius = inner_step_width / 1.5;
        if was_clicked && ui.rect_contains_pointer(step_rect) {
            *current_value = (step_index as i64) + 1;
        }
        let (fill_color, stroke_color) = if is_selected {
            (constants::FAVORITE_ICON_SELECTED_FILL, darker(constants::FAVORITE_ICON_SELECTED_FILL))
        } else {
            (
                darker(constants::FAVORITE_ICON_SELECTED_FILL),
                darker(darker(constants::FAVORITE_ICON_SELECTED_FILL)),
            )
        };
        let shape = generate_star_shape(shape_radius, step_center, fill_color, Stroke::new(stroke_width, stroke_color));
        painter.add(shape);
    }
    response
}

pub fn generate_star_shape(radius: f32, center_point: [f32; 2], fill: Color32, stroke: Stroke) -> Shape {
    egui::Shape::Path(PathShape::convex_polygon(
        generate_regular_polygon(10, radius, center_point, Some(std::f32::consts::PI * 0.5), Some(radius * 0.5)),
        fill,
        stroke,
    ))
}

pub fn generate_regular_polygon(
    num_sides: usize,
    side_length: f32,
    center_point: [f32; 2],
    angle_offset_radians: Option<f32>,
    alternate_inner_size: Option<f32>,
) -> Vec<Pos2> {
    let mut points = vec![];
    let total_angle_degrees = 180 * (num_sides - 2);
    let angle_step_degrees = total_angle_degrees / num_sides;
    // of the unit circle centered on the shape, how much angles to turn to each point
    let angle_step_degrees_of_unit_circle = 180 - angle_step_degrees;
    for point_index in 0..num_sides {
        let mut radius = side_length;
        if let Some(alternate_inner_size) = alternate_inner_size {
            if point_index % 2 == 0 {
                radius = alternate_inner_size
            }
        }
        let current_angle_radians =
            ((point_index * angle_step_degrees_of_unit_circle) as f32).to_radians() + angle_offset_radians.unwrap_or_default();
        let point_x = radius * current_angle_radians.cos();
        let point_y = radius * current_angle_radians.sin();
        let point = pos2(point_x, point_y) + center_point.into();
        points.push(point);
    }
    points
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

fn styled_button(text: impl std::fmt::Display, text_color: Color32, fill_color: Color32) -> Button {
    let richtext = RichText::new(text.to_string()).color(text_color);
    let button = Button::new(richtext).fill(fill_color);
    button
}

pub fn caution_button(text: impl std::fmt::Display) -> Button {
    styled_button(text, constants::CAUTION_BUTTON_TEXT_COLOR, constants::CAUTION_BUTTON_FILL)
}
pub fn suggested_button(text: impl std::fmt::Display) -> Button {
    styled_button(text, constants::SUGGESTED_BUTTON_TEXT_COLOR, constants::SUGGESTED_BUTTON_FILL)
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
                    let mut image_button =
                        egui::ImageButton::new(image.texture_id(ctx), image_size).selected(options.is_button_selected.unwrap_or(false));
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
        let image = image::open(path).expect("failed to application load icon").into_rgba8();
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
    fn load_fonts(ctx: &egui::Context) {
        let mut fonts = FontDefinitions::default();

        fonts.font_data.insert(
            String::from("japanese_fallback"),
            FontData::from_static(include_bytes!("resources/NotoSansJP-Regular.otf")),
        );
        fonts.font_data.insert(
            String::from("korean_fallback"),
            FontData::from_static(include_bytes!("resources/NotoSansKR-Regular.otf")),
        );
        fonts.font_data.insert(
            String::from("s_chinese_fallback"),
            FontData::from_static(include_bytes!("resources/NotoSansSC-Regular.otf")),
        );
        fonts.font_data.insert(
            String::from("t_chinese_fallback"),
            FontData::from_static(include_bytes!("resources/NotoSansTC-Regular.otf")),
        );
        fonts.font_data.insert(
            String::from("symbols_fallback"),
            FontData::from_static(include_bytes!("resources/NotoSansSymbols2-Regular.ttf")),
        );

        // Put my font first (highest priority):
        // fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "my_font".to_owned());

        // Put my font as last fallback for monospace:
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .push("japanese_fallback".to_owned());
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .push("korean_fallback".to_owned());
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .push("s_chinese_fallback".to_owned());
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .push("t_chinese_fallback".to_owned());
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .push("symbols_fallback".to_owned());

        ctx.set_fonts(fonts);
    }

    pub fn new() -> Self {
        AppUI {
            windows: vec![],
            current_window: "".into(),
        }
    }

    pub fn start(self) {
        let mut options = eframe::NativeOptions::default();
        options.initial_window_size = Some(Vec2::new(1390.0, 600.0));
        options.icon_data = Some(load_icon(constants::APPLICATION_ICON_PATH));
        options.renderer = Renderer::Wgpu;
        eframe::run_native(
            env!("CARGO_PKG_NAME"),
            options,
            Box::new(|creation_context| {
                Self::load_fonts(&creation_context.egui_ctx);
                Box::new(self)
            }),
        );
    }

    pub fn load_windows(&mut self) {
        self.windows = vec![
            WindowContainer {
                window: Box::new(import_ui::ImporterUI::default()),
                is_open: None,
                title: format!("{} importer", constants::IMPORT_ICON),
            },
            WindowContainer {
                window: Box::new(gallery_ui::GalleryUI::default()),
                is_open: None,

                title: format!("{} gallery", constants::GALLERY_ICON),
            },
            WindowContainer {
                window: Box::new(tags_ui::TagsUI::default()),
                is_open: None,

                title: format!("{} tags", constants::TAG_ICON),
            },
        ];
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
