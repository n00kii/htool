// todo put common ui stuff in here, like generating thumbnails of specific sizew

use crate::{
    data::EntryId,
    tags::{self, TagDataRef},
};
use downcast_rs as downcast;
use egui::{
    pos2, text::LayoutJob, vec2, Context, FontData, FontDefinitions, FontFamily, Layout, Mesh, Painter, Pos2, Rect, Shape, Stroke, TextFormat,
    TextureId,
};
use egui_notify::{Toast, Toasts};
use std::{
    cell::RefCell,
    fmt::Display,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
    vec,
};

use anyhow::Result;
use eframe::{
    egui::{self, Button, Response, RichText, Sense, Ui, WidgetText},
    emath::Vec2,
    epaint::{Color32, PathShape},
};
use egui_extras::RetainedImage;
use image::{ImageBuffer, Rgba};
use poll_promise::Promise;

use self::{autocomplete::AutocompleteOption, gallery_ui::GalleryUI};

pub type ToastsRef = Arc<Mutex<Toasts>>;

pub mod autocomplete;
pub mod gallery_ui;
pub mod import_ui;
pub mod preview_ui;
pub mod star_rating;
pub mod tags_ui;

pub mod constants {
    use eframe::epaint::Color32;

    pub const ERROR_COLOR: Color32 = Color32::from_rgb(200, 90, 90);
    pub const INFO_COLOR: Color32 = Color32::from_rgb(150, 200, 210);
    pub const WARNING_COLOR: Color32 = Color32::from_rgb(230, 220, 140);
    pub const SUCCESS_COLOR: Color32 = Color32::from_rgb(140, 230, 140);

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
    pub const SAVE_ICON: &str = "💾";
    pub const COPY_ICON: &str = "📋";
    pub const TOOL_ICON: &str = "🔨";
    pub const REORDER_ICON: &str = "↔";
    pub const DATA_ICON: &str = "💽";
    pub const CONFIG_ICON: &str = "⚙️";
    pub const LINK_ICON: &str = "📎";

    pub const GALLERY_TITLE: &str = "gallery";
    pub const IMPORT_TITLE: &str = "importer";
    pub const TAGS_TITLE: &str = "tags";
    pub const CONFIG_TITLE: &str = "config";
    pub const DATA_TITLE: &str = "data";

    pub const SPACER_SIZE: f32 = 10.;
    pub const OPTIONS_COLUMN_WIDTH: f32 = 100.;
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
    pub fn with_size(mut self, size: f32) -> Self {
        self.format.font_id.size = size;
        self
    }
}

pub fn icon_text(text: impl Display, icon: &str) -> String {
    format!("{icon} {text}")
}

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

pub fn toast_info(toasts: &mut Toasts, caption: impl Into<String>) -> &mut Toast {
    set_default_toast_options(toasts.info(caption))
}
pub fn toast_success(toasts: &mut Toasts, caption: impl Into<String>) -> &mut Toast {
    set_default_toast_options(toasts.success(caption))
}
pub fn toast_warning(toasts: &mut Toasts, caption: impl Into<String>) -> &mut Toast {
    set_default_toast_options(toasts.warning(caption))
}
pub fn toast_error(toasts: &mut Toasts, caption: impl Into<String>) -> &mut Toast {
    set_default_toast_options(toasts.error(caption))
}

pub fn toast_info_lock(toasts: &ToastsRef, caption: impl Into<String>) {
    if let Ok(mut toasts) = toasts.lock() {
        set_default_toast_options(toasts.info(caption));
    }
}
pub fn toast_success_lock(toasts: &ToastsRef, caption: impl Into<String>) {
    if let Ok(mut toasts) = toasts.lock() {
        set_default_toast_options(toasts.success(caption));
    }
}
pub fn toast_warning_lock(toasts: &ToastsRef, caption: impl Into<String>) {
    if let Ok(mut toasts) = toasts.lock() {
        set_default_toast_options(toasts.warning(caption));
    }
}
pub fn toast_error_lock(toasts: &ToastsRef, caption: impl Into<String>) {
    if let Ok(mut toasts) = toasts.lock() {
        set_default_toast_options(toasts.error(caption));
    }
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

pub fn paint_image(painter: &Painter, texture_id: &TextureId, rect: Rect) {
    let mut mesh = Mesh::with_texture(texture_id.clone());
    mesh.add_rect_with_uv(rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    painter.add(mesh);
}

pub fn render_loading_image(
    ui: &mut Ui,
    ctx: &egui::Context,
    image: Option<&Promise<Result<RetainedImage>>>,
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
                let hover_text = if let Some(format_error) = &options.hover_text_on_error_image {
                    Some(format_error(image_error))
                } else {
                    options.hover_text.as_ref().map(|wt| wt.to_owned())
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

pub type UpdateFlag = Arc<Mutex<bool>>;
pub type UpdateList<T> = Arc<Mutex<Vec<T>>>;
pub type AutocompleteOptionsRef = Rc<RefCell<Option<Vec<AutocompleteOption>>>>;
pub struct SharedState {
    pub toasts: ToastsRef,
    pub tag_data_ref: TagDataRef,
    pub autocomplete_options: AutocompleteOptionsRef,

    pub all_entries_update_flag: UpdateFlag,
    pub updated_entries: UpdateList<EntryId>,
    pub tag_data_update_flag: UpdateFlag,
}

impl SharedState {
    pub fn set_update_flag(flag: &UpdateFlag, new_state: bool) {
        if let Ok(mut flag) = flag.lock() {
            *flag = new_state;
        }
    }
    pub fn append_to_update_list<T>(list: &UpdateList<T>, mut new_items: Vec<T>) {
        if let Ok(mut list) = list.lock() {
            list.append(&mut new_items)
        }
    }
}
pub struct AppUI {
    shared_state: Rc<SharedState>,
    current_window: String,
    windows: Vec<WindowContainer>,
}

impl eframe::App for AppUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_top_bar(ctx);
        self.render_current_window(ctx);
        self.render_toasts(ctx);
        self.process_state();
        ctx.request_repaint();
    }
}

impl AppUI {
    fn process_state(&mut self) {
        if let Ok(mut update_list) = self.shared_state.updated_entries.try_lock() {
            // dbg!(&update_list);
            if update_list.len() > 0 {
                if let Some(gallery_container) = self
                    .windows
                    .iter_mut()
                    .find(|container| container.window.downcast_ref::<GalleryUI>().is_some())
                {
                    let gallery_window = gallery_container.window.downcast_mut::<GalleryUI>().unwrap();
                    gallery_window.update_entries(&update_list);
                }
            }
            update_list.clear();
        }
        if let Ok(mut was_updated) = self.shared_state.tag_data_update_flag.try_lock() {
            if *was_updated {
                tags::reload_tag_data(&self.shared_state.tag_data_ref)
            }
            *was_updated = false;
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
            tag_data_ref: tags::initialize_tag_data(),
            autocomplete_options: Rc::new(RefCell::new(None)),
            toasts: Arc::new(Mutex::new(Toasts::default().with_anchor(egui_notify::Anchor::BottomLeft))),
            all_entries_update_flag: Arc::new(Mutex::new(false)),
            updated_entries: Arc::new(Mutex::new(vec![])),
            tag_data_update_flag: Arc::new(Mutex::new(false)),
        };
        AppUI {
            shared_state: Rc::new(shared_state),
            windows: vec![],
            current_window: "".into(),
        }
    }

    pub fn start(mut self) {
        let mut options = eframe::NativeOptions::default();
        options.initial_window_size = Some(Vec2::new(1390.0, 600.0));
        options.icon_data = Some(load_icon());
        // options.renderer = Renderer::Wgpu;
        eframe::run_native(
            env!("CARGO_PKG_NAME"),
            options,
            Box::new(|creation_context| {
                Self::load_fonts(&creation_context.egui_ctx);
                self.load_windows();

                Box::new(self)
            }),
        );
    }

    pub fn load_windows(&mut self) {
        let mut gallery_window = gallery_ui::GalleryUI::new(&self.shared_state);
        gallery_window.load_gallery_entries();
        self.windows = vec![
            WindowContainer {
                window: Box::new(import_ui::ImporterUI::default()),
                is_open: None,
                title: icon_text(constants::IMPORT_TITLE, constants::IMPORT_ICON),
            },
            WindowContainer {
                window: Box::new(gallery_window),
                is_open: None,

                title: icon_text(constants::GALLERY_TITLE, constants::GALLERY_ICON),
            },
            WindowContainer {
                window: Box::new(tags_ui::TagsUI::new(&self.shared_state)),
                is_open: None,

                title: icon_text(constants::TAGS_TITLE, constants::TAG_ICON),
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
                    let job_text = generate_layout_job(vec![
                        LayoutJobText::from(env!("CARGO_PKG_NAME")).with_size(24.),
                        LayoutJobText::from(format!(" v{}", env!("CARGO_PKG_VERSION"))).with_size(18.),
                    ]);
                    // let text = RichText::new(format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))).size(24.);
                    ui.label(job_text);
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

    fn render_toasts(&self, ctx: &Context) {
        if let Ok(mut toasts) = self.shared_state.toasts.try_lock() {
            toasts.show(ctx)
        }
    }
}
