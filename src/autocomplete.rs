//! Source code example of how to create your own widget.
//! This is meant to be read as a tutorial, hence the plethora of comments.

/// iOS-style toggle switch:
///
/// ``` text
///      _____________
///     /       /.....\
///    |       |.......|
///     \_______\_____/
/// ```
///
/// ## Example:
/// ``` ignore
/// toggle_ui(ui, &mut my_bool);
/// ```
///
/// 

struct Autocomplete {
    id: Option<Id>,
}


#[derive(Clone)]
struct AutocompleteState {
    selected_index: Option<i32>,
    option_matches: Vec<String>,
    are_matches_dirty: bool
}

impl Default for AutocompleteState {
    fn default() -> Self {
       Self {
        selected_index: None,
            option_matches: vec![],
            are_matches_dirty: false,
       }

    }
}

impl Autocomplete {
    fn new() -> Self {
        Self {
            id: None,
        }
    }
}

use eframe::{
    egui::{self, Response, Id},
    epaint::{pos2, vec2, Color32, FontId, Pos2, Rect},
};

pub fn create<'a>(search: &'a mut String, options: &'a Vec<String>) -> impl egui::Widget + 'a {
    move |ui: &mut egui::Ui| autocomplete_ui(ui, search, options)
}

// impl egui::Widget for &mut Autocomplete {
//     fn ui(self, ui: &mut egui::Ui) -> Response {
//         let text_edit = ui.text_edit_singleline(search);
//         todo!()
//     }
// }

pub fn autocomplete_ui(ui: &mut egui::Ui, search: &mut String, options: &Vec<String>) -> Response {

    let response = ui.text_edit_singleline(search);

    if response.has_focus() {
        let ac_matches = autocomplete(search, options);
        if ac_matches.len() > 0 {
            
            let id = ui.make_persistent_id(format!("{:?}_autocomplete", response.id));
            let mut state: AutocompleteState = ui.ctx().memory().data.get_persisted(id).unwrap_or_default();
    
            if ui.ctx().input().key_pressed(egui::Key::ArrowUp) {
                if let Some(selected_index) = state.selected_index.as_mut() {
                    *selected_index = (*selected_index - 1).max(0);
                } else {
                    state.selected_index = Some(0);
                }
            } else if ui.ctx().input().key_pressed(egui::Key::ArrowDown) {
                if let Some(selected_index) = state.selected_index.as_mut() {
                    *selected_index = (*selected_index + 1).min(ac_matches.len() as i32 - 1);
                } else {
                    state.selected_index = Some(0);
                }
            } else if ui.ctx().input().key_pressed(egui::Key::Tab) {
                println!("tab!");
            }

            dbg!(state.selected_index);

            let mut ac_rect = response.rect;
            let ac_rect_margin = 3.;
            let ac_rect_padding = 3.;
            let ac_rect_inner_padding = 2.;

            let visuals = ui.style().interact_selectable(&response, false);
            let icon_font = FontId::default();

            let mut text_height = 0.;
            let mut ac_height = ac_rect_padding * 2.;

            let text_galleys = ac_matches
                .iter()
                .map(|text| {
                    let galley = ui.painter().layout_no_wrap(text.to_string(), icon_font.clone(), Color32::LIGHT_GRAY);

                    ac_height += galley.rect.height() + ac_rect_inner_padding;
                    if text_height == 0. {
                        text_height = galley.rect.height();
                    }
                    galley
                })
                .collect::<Vec<_>>();

            ac_height -= ac_rect_inner_padding;
            ac_rect.set_top(response.rect.top() - ac_height - ac_rect_margin);
            ac_rect.set_bottom(response.rect.top() - ac_rect_margin);

            ui.painter().rect(ac_rect, 2., visuals.bg_fill, visuals.bg_stroke);

            let ac_rect_left_top = ac_rect.left_top();
            let mut index = 0;
            for mut text_galley in text_galleys {
                let offset_x = ac_rect_padding;
                let offset_y = ac_rect_padding + ((index) as f32 * (ac_rect_inner_padding + text_height));
                let text_pos = ac_rect_left_top + vec2(offset_x, offset_y);
                let interaction_rect = text_galley.rect.clone().translate(text_pos.to_vec2());
                let text_hovered = response
                    .ctx
                    .input()
                    .pointer
                    .hover_pos()
                    .map(|p| interaction_rect.contains(p))
                    .unwrap_or(false);
                let text_selected = if let Some(selected_index) = state.selected_index.as_ref() {
                    *selected_index == index
                } else {
                    false
                };
                if text_selected {
                    text_galley = ui
                        .painter()
                        .layout_no_wrap(text_galley.text().to_string(), icon_font.clone(), Color32::BLUE);
                } else if text_hovered {
                    text_galley = ui
                        .painter()
                        .layout_no_wrap(text_galley.text().to_string(), icon_font.clone(), Color32::RED);
                }
                ui.painter().galley(text_pos, text_galley);
                index += 1;
            }

            ui.ctx().memory().data.insert_persisted(id, state);
        }
    }

    response
}

fn hamming_distance(a: &String, b: &String) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    let size_difference = a_len.abs_diff(b_len);
    let mut distance: usize = 0;
    let smaller_size = a_len.min(b_len);
    let (a_trunc, b_trunc) = (&a[..smaller_size], &b[..smaller_size]);
    for (c_a, c_b) in a_trunc.chars().zip(b_trunc.chars()) {
        if c_a != c_b {
            distance += 1;
        }
    }
    distance + size_difference
}

fn autocomplete<'a>(search: &String, options: &'a Vec<String>) -> Vec<&'a String> {
    let words = search.split_whitespace().collect::<Vec<_>>();
    let is_last_char_whitespace = if let Some(last_char) = search.chars().last() {
        last_char.is_whitespace()
    } else {
        false
    };
    let min_distance = 3;
    let min_search_len = 3;
    let current_word = if !is_last_char_whitespace { words.last() } else { None };
    let mut matches: Vec<(&String, usize)> = vec![];
    if let Some(current_word) = current_word {
        if current_word.len() > min_search_len {
            for option_word in options {
                let distance = hamming_distance(&current_word.to_string(), option_word);
                if distance <= min_distance {
                    // println!("{current_word}, {option_word}")
                    matches.push((option_word, distance))
                }
            }
        }
    }

    matches.sort_by_key(|(match_string, distance)| distance.clone());
    matches
        .iter()
        .map(|(match_string, distance)| match_string.clone())
        .collect::<Vec<&String>>()
}

pub fn toggle_ui(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    // Widget code can be broken up in four steps:
    //  1. Decide a size for the widget
    //  2. Allocate space for it
    //  3. Handle interactions with the widget (if any)
    //  4. Paint the widget

    // 1. Deciding widget size:
    // You can query the `ui` how much space is available,
    // but in this example we have a fixed size widget based on the height of a standard button:
    let desired_size = ui.spacing().interact_size.y * egui::vec2(2.0, 1.0);

    // 2. Allocating space:
    // This is where we get a region of the screen assigned.
    // We also tell the Ui to sense clicks in the allocated region.
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    // 3. Interact: Time to check for clicks!
    if response.clicked() {
        *on = !*on;
        response.mark_changed(); // report back that the value changed
    }

    // Attach some meta-data to the response which can be used by screen readers:
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, *on, ""));

    // 4. Paint!
    // Make sure we need to paint:
    if ui.is_rect_visible(rect) {
        // Let's ask for a simple animation from egui.
        // egui keeps track of changes in the boolean associated with the id and
        // returns an animated value in the 0-1 range for how much "on" we are.
        let how_on = ui.ctx().animate_bool(response.id, *on);
        // We will follow the current style by asking
        // "how should something that is being interacted with be painted?".
        // This will, for instance, give us different colors when the widget is hovered or clicked.
        let visuals = ui.style().interact_selectable(&response, *on);
        // All coordinates are in absolute screen coordinates so we use `rect` to place the elements.
        let rect = rect.expand(visuals.expansion);
        let radius = 0.5 * rect.height();
        ui.painter().rect(rect, radius, visuals.bg_fill, visuals.bg_stroke);
        // Paint the circle, animating it from left to right with `how_on`:
        let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
        let center = egui::pos2(circle_x, rect.center().y);
        ui.painter().circle(center, 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);
    }

    // All done! Return the interaction response so the user can check what happened
    // (hovered, clicked, ...) and maybe show a tooltip:
    response
}

/// Here is the same code again, but a bit more compact:
#[allow(dead_code)]
fn toggle_ui_compact(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
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

pub fn url_to_file_source_code() -> String {
    format!("https://github.com/emilk/egui/blob/master/{}", file!())
}
