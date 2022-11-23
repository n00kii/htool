// todo put common ui stuff in here, like generating thumbnails of specific sizew

use crate::{
    config::Config,
    data::{self, EntryId},
    tags::{self, TagDataRef},
};
use anyhow::Result;
use downcast_rs as downcast;
use eframe::{
    egui::{self, Button, Response, RichText, Sense, Ui, WidgetText},
    emath::Vec2,
    epaint::{Color32, PathShape},
};
use egui::{
    hex_color, pos2, text::LayoutJob, vec2, Align, Align2, CentralPanel, Context, FontData, FontDefinitions, FontFamily, FontId, Frame, Id, Layout,
    Mesh, Painter, Pos2, Rect, Shape, Stroke, Style, TextEdit, TextFormat, TextureId, TopBottomPanel, Window, ProgressBar,
};
use egui_extras::RetainedImage;
use egui_modal::{Modal, ModalStyle};
use egui_notify::{Toast, Toasts};
use egui_video::VideoStream;
use hex_color::HexColor;
use image::{ImageBuffer, Rgba};
use parking_lot::{Mutex, MutexGuard};
use poll_promise::Promise;
use std::{
    cell::RefCell,
    fmt::Display,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
    vec,
};

use self::{autocomplete::AutocompleteOption, constants::CONFIG_TITLE, data_ui::DataUI, gallery_ui::GalleryUI, preview_ui::MediaPreview};

pub type ToastsRef = Arc<Mutex<Toasts>>;

pub mod autocomplete;
pub mod config_ui;
pub mod data_ui;
pub mod debug_ui;
pub mod gallery_ui;
pub mod import_ui;
pub mod preview_ui;
pub mod star_rating;
pub mod tags_ui;

pub mod constants {
    use eframe::epaint::Color32;

    pub const APPLICATION_NAME: &str = env!("CARGO_PKG_NAME");
    pub const APPLICATION_VERSION: &str = env!("CARGO_PKG_VERSION");

    pub const FG_STROKE_WIDTH: f32 = 1.;
    pub const BG_STROKE_WIDTH: f32 = 1.;

    pub const PRETTY_HASH_LENGTH: usize = 6;
    pub const DEFAULT_RED_COLOR: Color32 = Color32::from_rgb(200, 90, 90);
    pub const DEFAULT_BLUE_COLOR: Color32 = Color32::from_rgb(150, 200, 210);
    pub const DEFAULT_YELLOW_COLOR: Color32 = Color32::from_rgb(230, 220, 140);
    pub const DEFAULT_GREEN_COLOR: Color32 = Color32::from_rgb(140, 230, 140);

    pub const INFO_ICON: &str = "ℹ";
    pub const WARNING_ICON: &str = "⚠";
    pub const ERROR_ICON: &str = "！";
    pub const SUCCESS_ICON: &str = "✅";

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
    pub const BOX_ICON: &str = "☐";
    pub const DASH_ICON: &str = "—";
    pub const SAVE_ICON: &str = "💾";
    pub const COPY_ICON: &str = "📋";
    pub const TOOL_ICON: &str = "🔨";
    pub const REORDER_ICON: &str = "↔";
    pub const DATA_ICON: &str = "🗂";
    pub const CONFIG_ICON: &str = "⚙";
    pub const MISC_ICON: &str = "✱";
    pub const SPARKLE_ICON: &str = "✨";
    pub const WINDOW_ICON: &str = "🗖";
    pub const LINK_ICON: &str = "📎";
    pub const OPEN_ICON: &str = "↗";
    pub const MOVIE_ICON: &str = "▶";
    pub const DEBUG_ICON: &str = "🐞";

    pub const GALLERY_TITLE: &str = "gallery";
    pub const IMPORT_TITLE: &str = "importer";
    pub const TAGS_TITLE: &str = "tags";
    pub const CONFIG_TITLE: &str = "config";
    pub const DATA_TITLE: &str = "data";
    pub const DEBUG_TITLE: &str = "debug";

    pub const DISABLED_LABEL_LOCKED_DATABASE: &str = "database is locked";
    pub const DISABLED_LABEL_REKEY_DATABASE: &str = "database is rekeying";

    pub const SPACER_SIZE: f32 = 10.;
    pub const OPTIONS_COLUMN_WIDTH: f32 = 100.;

    pub const FAVORITE_ICON_SELECTED_FILL: Color32 = Color32::from_rgb(252, 191, 73);
    pub const DEFAULT_ACCENT_STROKE_COLOR: Color32 = Color32::from_rgb(141, 182, 242);
    pub const DEFAULT_ACCENT_FILL_COLOR: Color32 = Color32::from_rgb(20, 70, 60);

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
            format: TextFormat {
                color: text_color(),
                ..TextFormat::default()
            },
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
    pub fn with_size(mut self, size: f32) -> Self {
        self.format.font_id.size = size;
        self
    }
}

pub fn icon_text(text: impl Display, icon: &str) -> String {
    format!("{icon} {text}")
}

macro_rules! icon {
    ($text:expr, $icon_name:ident) => {
        crate::ui::icon_text($text, crate::ui::constants::$icon_name)
    };
}

pub(crate) use icon;

pub fn pretty_entry_id(entry_id: &EntryId) -> String {
    match entry_id {
        EntryId::PoolEntry(link_id) => pretty_link_id(link_id),
        EntryId::MediaEntry(hash) => pretty_media_id(hash),
    }
}

pub fn pretty_link_id(link_id: &i32) -> String {
    format!("{}{}", constants::LINK_ICON, link_id)
}

pub fn pretty_media_id(hash: &String) -> String {
    format!("{}{}", constants::GALLERY_ICON, &hash[..constants::PRETTY_HASH_LENGTH])
}

pub fn scale_color(base_color: Color32, color_factor: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (base_color.r() as f32 * color_factor) as u8,
        (base_color.g() as f32 * color_factor) as u8,
        (base_color.b() as f32 * color_factor) as u8,
        (base_color.a()) as u8,
    )
}

pub fn modal(ctx: &egui::Context, id_source: impl std::fmt::Display) -> Modal {
    let red_fill = Config::global().themes.red_fill_color().unwrap_or(darker(constants::DEFAULT_RED_COLOR));
    let red_stroke = Config::global()
        .themes
        .red_stroke_color()
        .unwrap_or(lighter(constants::DEFAULT_RED_COLOR));
    let accent_fill_color = Config::global()
        .themes
        .accent_fill_color()
        .unwrap_or(constants::DEFAULT_ACCENT_FILL_COLOR);
    let accent_stroke_color = Config::global()
        .themes
        .accent_stroke_color()
        .unwrap_or(constants::DEFAULT_ACCENT_STROKE_COLOR);
    let error_color = Config::global().themes.red_stroke_color().unwrap_or(constants::DEFAULT_RED_COLOR);
    let warning_color = Config::global().themes.yellow_stroke_color().unwrap_or(constants::DEFAULT_YELLOW_COLOR);
    let info_color = Config::global().themes.blue_stroke_color().unwrap_or(constants::DEFAULT_BLUE_COLOR);
    let success_color = Config::global().themes.green_stroke_color().unwrap_or(constants::DEFAULT_GREEN_COLOR);
    Modal::new(ctx, id_source).with_style(&ModalStyle {
        caution_button_fill: red_fill,
        caution_button_text_color: red_stroke,
        suggested_button_fill: accent_fill_color,
        suggested_button_text_color: accent_stroke_color,
        info_icon_color: info_color,
        success_icon_color: success_color,
        warning_icon_color: warning_color,
        error_icon_color: error_color,
        ..Default::default()
    })
}

pub fn progress_bar(progress: f32) -> ProgressBar {
    ProgressBar::new(progress).text(format!("{}%", (100. * progress).round()))
}

pub fn darker(base_color: Color32) -> Color32 {
    scale_color(base_color, constants::COLOR_DARKEN_FACTOR)
}

pub fn text_color() -> Color32 {
    Config::global().themes.inactive_fg_stroke_color().unwrap_or(Color32::GRAY)
}

pub fn lighter(base_color: Color32) -> Color32 {
    scale_color(base_color, constants::COLOR_LIGHTEN_FACTOR)
}

pub fn ease_in_cubic(x: f32) -> f32 {
    1. - (1. - x).powi(3)
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

pub fn shrink_rect_scaled(rect: &Rect, scale: f32) -> Rect {
    shrink_rect_scaled2(rect, [scale; 2])
}

pub fn shrink_rect_scaled2(rect: &Rect, scale: [f32; 2]) -> Rect {
    let offset = vec2((1. - scale[0]) * rect.width() * 0.5, (1. - scale[1]) * rect.height() * 0.5);
    let new_rect = Rect::from_min_max(rect.left_top() + offset, rect.right_bottom() - offset);
    new_rect
}

pub fn generate_retained_image(image_buffer: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<RetainedImage> {
    puffin::profile_scope!("retained_image_gen");
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
    error_label_text_size: LabelSize,
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

fn font_id_sized(size: f32) -> FontId {
    let mut default_fid = FontId::default();
    default_fid.size = size;
    default_fid
}

enum LabelSize {
    Exact(f32),
    Relative(f32),
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
            error_label_text_size: LabelSize::Exact(48.),
            sense: vec![egui::Sense::click()],
        }
    }
}

pub fn toast_info_lock(toasts: &ToastsRef, caption: impl Into<String>) {
    toasts_with_cb(toasts, |toasts| toasts.info(caption), |t| t)
}
pub fn toast_success_lock(toasts: &ToastsRef, caption: impl Into<String>) {
    toasts_with_cb(toasts, |toasts| toasts.success(caption), |t| t)
}
pub fn toast_warning_lock(toasts: &ToastsRef, caption: impl Into<String>) {
    toasts_with_cb(toasts, |toasts| toasts.warning(caption), |t| t)
}

pub fn toast_error_lock(toasts: &ToastsRef, caption: impl Into<String>) {
    toasts_with_cb(toasts, |toasts| toasts.error(caption), |t| t)
}

fn toasts_with_cb(toasts: &ToastsRef, toasts_cb: impl FnOnce(&mut Toasts) -> &mut Toast, toast_cb: impl FnOnce(&mut Toast) -> &mut Toast) {
    let mut toasts = toasts.lock();
    let toast = set_default_toast_options_lock(toasts_cb(&mut toasts));
    toast_cb(toast);
}

pub fn set_default_toast_options_lock(toast: &mut Toast) -> &mut Toast {
    toast.set_duration(Some(Duration::from_millis(3000))).set_closable(true)
}

pub fn set_default_toast_options(toast: &mut egui_notify::Toast) -> &mut Toast {
    toast.set_duration(Some(Duration::from_millis(3000))).set_closable(true)
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
    let red_fill = Config::global().themes.red_fill_color().unwrap_or(darker(constants::DEFAULT_RED_COLOR));
    let red_stroke = Config::global()
        .themes
        .red_stroke_color()
        .unwrap_or(lighter(constants::DEFAULT_RED_COLOR));
    styled_button(text, red_stroke, red_fill)
}
pub fn suggested_button(text: impl std::fmt::Display) -> Button {
    let accent_fill_color = Config::global()
        .themes
        .accent_fill_color()
        .unwrap_or(constants::DEFAULT_ACCENT_FILL_COLOR);
    let accent_stroke_color = Config::global()
        .themes
        .accent_stroke_color()
        .unwrap_or(constants::DEFAULT_ACCENT_STROKE_COLOR);
    styled_button(text, accent_stroke_color, accent_fill_color)
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

pub fn paint_image(painter: &Painter, texture_id: &TextureId, rect: Rect) {
    let mut mesh = Mesh::with_texture(texture_id.clone());
    mesh.add_rect_with_uv(rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    painter.add(mesh);
}

pub fn render_loading_preview(
    ui: &mut Ui,
    ctx: &egui::Context,
    image: Option<&mut Promise<Result<MediaPreview>>>,
    options: &RenderLoadingImageOptions,
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
        Some(image_promise) => match image_promise.ready_mut() {
            None => {
                let spinner = egui::Spinner::new();
                let mut response = ui.add_sized(options.widget_size(None), spinner);
                response = bind_hover_text(response, &options.hover_text_on_loading_image);
                Some(response)
            }
            Some(Err(image_error)) => {
                let mut text = egui::RichText::new(&options.error_label_text);

                text = match options.error_label_text_size {
                    LabelSize::Exact(exact_size) => text.size(exact_size),
                    LabelSize::Relative(relative_size) => text.size(relative_size * options.desired_image_size[1]),
                };

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
                let hover_text = if let Some(format_error) = &options.hover_text_on_error_image {
                    Some(format_error(image_error))
                } else {
                    options.hover_text.as_ref().map(|wt| wt.to_owned())
                };
                response = bind_hover_text(response, &hover_text);
                Some(response)
            }

            Some(Ok(image)) => {
                let original_size = match image {
                    MediaPreview::Picture(image) => image.size_vec2(),
                    MediaPreview::Movie(streamer) => vec2(streamer.width as f32, streamer.height as f32),
                };

                // let texture_id
                let image_size: [f32; 2] = options.scaled_image_size(original_size.into());

                let mut response = match image {
                    MediaPreview::Picture(image) => {
                        let response = if options.is_button {
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
                        response
                    }
                    MediaPreview::Movie(streamer) => {
                        streamer.process_state();
                        streamer.ui(ui, image_size)
                    }
                };

                response = bind_hover_text(response, &options.hover_text);
                Some(response)
            }
        },
    }
}

/// Here is the same code again, but a bit more compact:
#[allow(dead_code)]
fn toggle_ui(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let desired_size = ui.spacing().interact_size.y * egui::vec2(2.0, 1.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, *on, ""));

    if ui.is_rect_visible(rect) {
        let how_on = ui.ctx().animate_bool(response.id, *on);
        let visuals = ui.style().interact_selectable(&response, *on);
        let rect = rect.expand(visuals.expansion);
        let radius = 0.5 * rect.height();
        ui.painter().rect(rect, radius, visuals.bg_fill, visuals.bg_stroke);
        let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
        let center = egui::pos2(circle_x, rect.center().y);
        ui.painter().circle(center, 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);
    }

    response
}

// A wrapper that allows the more idiomatic usage pattern: `ui.add(toggle(&mut my_bool))`
/// iOS-style toggle switch.
///
/// ## Example:
/// ``` ignore
/// ui.add(toggle(&mut my_bool));
/// ```
pub fn toggle(on: &mut bool) -> impl egui::Widget + '_ {
    move |ui: &mut egui::Ui| toggle_ui(ui, on)
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
fn load_icon() -> eframe::IconData {
    let (icon_rgba, icon_width, icon_height) = {
        let icon_bytes = include_bytes!("resources/icon.ico");
        let image = image::load_from_memory(icon_bytes).expect("failed to application load icon").into_rgba8();
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
    pub tag_data_update_flag: UpdateFlag,
    pub updated_theme_selection: UpdateFlag,
    pub gallery_regenerate_flag: UpdateFlag,
    pub database_unlocked: UpdateFlag,
    pub disable_navbar: UpdateList<String>,
    pub database_changed: UpdateFlag,
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
            self.window_title = format!("{}: {}", constants::APPLICATION_NAME, addition)
        } else {
            self.window_title = constants::APPLICATION_NAME.to_string();
        }
    }
    pub fn append_to_update_list<T>(list: &UpdateList<T>, mut new_items: Vec<T>) {
        list.lock().append(&mut new_items)
    }
}
pub struct AppUI {
    shared_state: Rc<SharedState>,
    current_window: String,
    windows: Vec<WindowContainer>,
    input_database_key: Arc<Mutex<String>>,
}

impl eframe::App for AppUI {
    fn persist_native_window(&self) -> bool {
        true
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        Config::save().expect("failed to save config");
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        puffin::GlobalProfiler::lock().new_frame();
        self.render_top_bar(ctx);
        self.render_current_window(ctx);
        self.render_toasts(ctx);
        self.process_state(ctx);
        ctx.request_repaint();
    }
}

pub fn color32_to_hex(color: Color32) -> String {
    let hex_color = HexColor::rgba(color.r(), color.g(), color.b(), color.a());
    hex_color.to_string()
}

pub fn color32_from_hex(hex: &str) -> Result<Color32> {
    let hex_color = HexColor::parse(hex)?;
    Ok(Color32::from_rgba_unmultiplied(hex_color.r, hex_color.g, hex_color.b, hex_color.a))
}

impl AppUI {
    pub fn init() {
        Config::load();
        // data::init();
        egui_video::init();
        puffin::set_scopes_on(true);
    }
    fn process_state(&mut self, ctx: &Context) {
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
        if SharedState::consume_update_flag(&self.shared_state.tag_data_update_flag) {
            tags::reload_tag_data(&self.shared_state.tag_data_ref);
        }
        if SharedState::consume_update_flag(&self.shared_state.updated_theme_selection) {
            AppUI::load_style(ctx);
        }
        if SharedState::consume_update_flag(&self.shared_state.gallery_regenerate_flag) {
            self.generate_gallery_entries();
        }
        if SharedState::consume_update_flag(&self.shared_state.database_changed) {
            self.check_database();
        }
        *self.shared_state.autocomplete_options.borrow_mut() = tags::generate_autocomplete_options(&self.shared_state.tag_data_ref);
    }

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
        let shared_state = SharedState {
            updated_theme_selection: Arc::new(AtomicBool::new(false)),
            gallery_regenerate_flag: Arc::new(AtomicBool::new(false)),
            tag_data_ref: tags::initialize_tag_data(),
            autocomplete_options: Rc::new(RefCell::new(None)),
            window_title: env!("CARGO_PKG_NAME").to_string(),
            toasts: Arc::new(Mutex::new(Toasts::default().with_anchor(egui_notify::Anchor::BottomLeft))),
            all_entries_update_flag: Arc::new(AtomicBool::new(false)),
            updated_entries: Arc::new(Mutex::new(vec![])),
            tag_data_update_flag: Arc::new(AtomicBool::new(false)),
            database_unlocked: Arc::new(AtomicBool::new(false)),
            disable_navbar: Arc::new(Mutex::new(vec![])),
            database_changed: Arc::new(AtomicBool::new(false)),
        };
        AppUI {
            shared_state: Rc::new(shared_state),
            windows: vec![],
            current_window: String::new(),
            input_database_key: Arc::new(Mutex::new(String::new()))
        }
    }

    pub fn load_style(ctx: &Context) {
        // ctx.set_style(style)
        let mut style = Style::default();
        let stroke_size = 1.;
        if let Some(color) = Config::global().themes.bg_fill_color() {
            style.visuals.widgets.noninteractive.bg_fill = color;
        }
        if let Some(color) = Config::global().themes.bg_fill_color() {
            style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(stroke_size, scale_color(color, 1.5));
        }
        if let Some(color) = Config::global().themes.tertiary_bg_fill_color() {
            style.visuals.extreme_bg_color = color;
        }
        if let Some(color) = Config::global().themes.secondary_bg_fill_color() {
            style.visuals.faint_bg_color = scale_color(color, 1.2);
        }
        if let Some(color) = Config::global().themes.inactive_bg_fill_color() {
            style.visuals.widgets.inactive.bg_fill = color;
        }
        if let Some(stroke) = Config::global().themes.inactive_bg_stroke() {
            style.visuals.widgets.inactive.bg_stroke = stroke;
        }
        if let Some(stroke) = Config::global().themes.inactive_fg_stroke() {
            style.visuals.widgets.noninteractive.fg_stroke = stroke;
            style.visuals.widgets.inactive.fg_stroke = stroke;
        }
        if let Some(color) = Config::global().themes.hovered_bg_fill_color() {
            style.visuals.widgets.hovered.bg_fill = color;
        }
        if let Some(stroke) = Config::global().themes.hovered_bg_stroke() {
            style.visuals.widgets.hovered.bg_stroke = stroke;
        }
        if let Some(stroke) = Config::global().themes.hovered_fg_stroke() {
            style.visuals.widgets.hovered.fg_stroke = stroke;
        }
        if let Some(color) = Config::global().themes.active_bg_fill_color() {
            style.visuals.widgets.active.bg_fill = color;
        }
        if let Some(stroke) = Config::global().themes.active_bg_stroke() {
            style.visuals.widgets.active.bg_stroke = stroke;
        }
        if let Some(stroke) = Config::global().themes.active_fg_stroke() {
            style.visuals.widgets.active.fg_stroke = stroke;
        }
        if let Some(stroke) = Config::global().themes.selected_fg_stroke() {
            style.visuals.selection.stroke = stroke;
        }
        if let Some(color) = Config::global().themes.selected_bg_fill_color() {
            style.visuals.selection.bg_fill = color
        }

        ctx.set_style(style)
    }

    pub fn start(mut self) {
        let mut options = eframe::NativeOptions::default();
        options.initial_window_size = Some(Vec2::new(1390.0, 600.0));
        options.icon_data = Some(load_icon());
        // options.decorated = false;
        // options.renderer = Renderer::Wgpu;
        eframe::run_native(
            constants::APPLICATION_NAME,
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

    pub fn load_windows(&mut self) {
        let gallery_window = gallery_ui::GalleryUI::new(&self.shared_state);
        self.windows = vec![
            WindowContainer {
                window: Box::new(import_ui::ImporterUI::new(&self.shared_state)),
                is_open: None,
                title: icon!(constants::IMPORT_TITLE, IMPORT_ICON),
            },
            WindowContainer {
                window: Box::new(gallery_window),
                is_open: None,

                title: icon!(constants::GALLERY_TITLE, GALLERY_ICON),
            },
            WindowContainer {
                window: Box::new(tags_ui::TagsUI::new(&self.shared_state)),
                is_open: None,

                title: icon!(constants::TAGS_TITLE, TAG_ICON),
            },
            WindowContainer {
                window: Box::new(config_ui::ConfigUI::new(&self.shared_state)),
                is_open: Some(false),

                title: icon!(constants::CONFIG_TITLE, CONFIG_ICON),
            },
            WindowContainer {
                window: Box::new(data_ui::DataUI::new(&self.shared_state)),
                is_open: None,

                title: icon!(constants::DATA_TITLE, DATA_ICON),
            },
            WindowContainer {
                window: Box::new(debug_ui::DebugUI::default()),
                is_open: Some(false),

                title: icon!(constants::DEBUG_TITLE, DEBUG_ICON),
            },
        ];
        self.check_database();
    }

    fn check_database(&mut self) {
        match data::try_unlock_database_with_key(&String::new()) {
            Ok(true) => {
                self.generate_gallery_entries();
                tags::reload_tag_data(&self.shared_state.tag_data_ref);
                SharedState::set_update_flag(&self.shared_state.database_unlocked, true);
                SharedState::remove_disabled_reason(&self.shared_state.disable_navbar, constants::DISABLED_LABEL_LOCKED_DATABASE);
            }
            _ => {
                self.current_window = String::new();
                SharedState::set_update_flag(&self.shared_state.database_unlocked, false);
                SharedState::add_disabled_reason(&self.shared_state.disable_navbar, constants::DISABLED_LABEL_LOCKED_DATABASE);
            },
        };
    }

    fn generate_gallery_entries(&mut self) {
        for window in self.windows.iter_mut() {
            if let Some(gallery_ui) = window.window.downcast_mut::<GalleryUI>() {
                gallery_ui.generate_entries();
            }
        }
    }

    fn render_top_bar(&mut self, ctx: &egui::Context) {
        let lock_exclusions = vec![icon!(CONFIG_TITLE, CONFIG_ICON)];
        let disable_reasons = self.shared_state.disable_navbar.try_lock().as_deref().map(|v| v.clone()).unwrap_or_default();
        let disabled_message = format!("disabled because {}", disable_reasons.join(", "));
        egui::TopBottomPanel::top("app_top_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.visuals_mut().button_frame = false;
                for window in self.windows.iter_mut() {
                    ui.add_enabled_ui(disable_reasons.is_empty() || lock_exclusions.contains(&window.title), |ui| {
                        let response = ui.selectable_label(
                            self.current_window == window.title || matches!(window.is_open, Some(true)),
                            window.title.clone(),
                        ).on_disabled_hover_text(&disabled_message);
                        if response.clicked() {
                            if let Some(is_open) = window.is_open.as_mut() {
                                *is_open ^= true;
                            } else {
                                self.current_window = window.title.clone();
                            }
                        }
                    });
                }
            });
        });
    }

    fn render_custom_frame(ctx: &Context, frame: &mut eframe::Frame, shared_state: &SharedState, add_contents: impl FnOnce(&mut Ui)) {
        let text_color = ctx.style().visuals.text_color();
        let height = 20.;

        CentralPanel::default().frame(Frame::none()).show(ctx, |ui| {
            let max_rect = ui.max_rect();
            let painter = ui.painter();

            painter.rect(max_rect.shrink(1.), 0., ctx.style().visuals.window_fill(), Stroke::none());
            painter.text(
                max_rect.center_top() + vec2(0., height / 2.),
                Align2::CENTER_CENTER,
                &shared_state.window_title,
                FontId::proportional(height * 0.8),
                text_color,
            );
            let button_size = Vec2::splat(height);
            let button_margin = 5.;
            let mut button_pos_iter = (0..3)
                .into_iter()
                .map(|i| max_rect.right_top() - vec2((i + 1) as f32 * button_size.x + button_margin, 0.));
            let (close_button_pos, maximize_button_pos, minimize_button_pos) = (
                button_pos_iter.next().unwrap(),
                button_pos_iter.next().unwrap(),
                button_pos_iter.next().unwrap(),
            );
            let button_text_size = height - 4.;
            let close_button_rect = Rect::from_min_size(close_button_pos, button_size);
            let maximize_button_rect = Rect::from_min_size(maximize_button_pos, button_size);
            let minimize_button_rect = Rect::from_min_size(minimize_button_pos, button_size);

            let close_button = Button::new(RichText::new(constants::REMOVE_ICON).size(button_text_size)).frame(false);
            let maximize_button = Button::new(RichText::new(constants::BOX_ICON).size(button_text_size)).frame(false);
            let minimize_button = Button::new(RichText::new(constants::DASH_ICON).size(button_text_size)).frame(false);

            let close_response = ui.put(close_button_rect, close_button);
            let maximize_response = ui.put(maximize_button_rect, maximize_button);
            let minimize_response = ui.put(minimize_button_rect, minimize_button);
            if close_response.clicked() {
                frame.close()
            }
            if maximize_response.clicked() {
                frame.set_fullscreen(true)
            }
            if minimize_response.clicked() {
                frame.set_visible(false)
            }
            let title_bar_rect = {
                let mut title_bar_rect = max_rect;
                title_bar_rect.max.y = title_bar_rect.min.y + height;
                title_bar_rect
            };
            let title_bar_response = ui.interact(title_bar_rect, Id::new("title_bar"), Sense::click());
            if title_bar_response.is_pointer_button_down_on() {
                frame.drag_window();
            }
            let content_rect = {
                let mut content_rect = max_rect;
                content_rect.min.y = title_bar_rect.max.y;
                content_rect
            }
            .shrink(4.0);
            let mut content_ui = ui.child_ui(content_rect, *ui.layout());
            add_contents(&mut content_ui);
        });
    }

    fn render_current_window(&mut self, ctx: &egui::Context) {
        //egui::CentralPanel::default().frame(Frame {

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.current_window.is_empty() {
                ui.with_layout(Layout::centered_and_justified(egui::Direction::TopDown), |ui| {
                    let job_text = generate_layout_job(vec![
                        LayoutJobText::from(constants::APPLICATION_NAME).with_size(24.),
                        LayoutJobText::from(format!(" v{}", constants::APPLICATION_VERSION)).with_size(18.),
                    ]);
                    let splash_rect = ui.label(job_text).rect;
                    if !SharedState::read_update_flag(&self.shared_state.database_unlocked) {
                        let unlock_flag = Arc::clone(&self.shared_state.database_unlocked);
                        let toasts = Arc::clone(&self.shared_state.toasts);
                        let disabled_navbar_reasons = Arc::clone(&self.shared_state.disable_navbar);
                        let regenerate_flag = Arc::clone(&self.shared_state.gallery_regenerate_flag);
                        let input_db_key_arc = Arc::clone(&self.input_database_key);
                        if let Some(input_db_key) = self.input_database_key.try_lock().as_deref_mut() {
                            let input_db_key_clone = input_db_key.clone();
                            let login_tedit_rect = Rect::from_center_size(splash_rect.center() + vec2(0., 40.), vec2(200., 10.));
                            let button_rect = login_tedit_rect.translate(vec2(0., login_tedit_rect.height() + 15.));
                            let text_edit = TextEdit::singleline(input_db_key).hint_text("enter database key...").password(true);
                            let button = Button::new("unlock");
                            ui.put(login_tedit_rect, text_edit);
                            if ui.put(button_rect, button).clicked() {
                                thread::spawn(move || {
                                    match data::try_unlock_database_with_key(&input_db_key_clone) {
                                        Ok(true) => {
                                            data::set_db_key(&input_db_key_clone);
                                            SharedState::set_update_flag(&unlock_flag, true);
                                            SharedState::set_update_flag(&regenerate_flag, true);
                                            SharedState::remove_disabled_reason(&disabled_navbar_reasons, constants::DISABLED_LABEL_LOCKED_DATABASE);
                                            input_db_key_arc.lock().clear();
                                            toast_success_lock(&toasts, "successfully unlocked database");
                                        }
                                        Ok(false) => {
                                            toast_error_lock(&toasts, "invalid key or invalid database");
                                        }
                                        Err(e) => {
                                            toast_error_lock(&toasts, format!("failed to unlock: {e}"));
                                        }
                                    };
                                });
                            }

                        }

                    }
                });
            } else {
                for window in self.windows.iter_mut() {
                    if window.title == self.current_window {
                        window.window.ui(ui, ctx)
                    }
                }
            }
        });
        for window in self.windows.iter_mut() {
            if let Some(is_open) = window.is_open.as_mut() {
                if *is_open {
                    Window::new(&window.title).open(is_open).show(ctx, |ui| window.window.ui(ui, ctx));
                }
            }
        }
    }

    fn render_toasts(&self, ctx: &Context) {
        if let Some(mut toasts) = self.shared_state.toasts.try_lock() {
            toasts.show(ctx)
        }
    }
}
