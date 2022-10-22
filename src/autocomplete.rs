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

// TODO: make state cache matches
#[derive(Clone)]
struct AutocompleteState {
    // last_ccursor_range: Option<CCursorRange>,
    selected_index: i32,
    option_matches: Vec<AutocompleteOption>,
    are_matches_dirty: bool,
}

#[derive(Clone)]
pub struct AutocompleteOption {
    pub label: String, 
    pub value: String,
    pub color: Option<Color32>,
    pub description: String,
}

impl Default for AutocompleteState {
    fn default() -> Self {
        Self {
            selected_index: 0,
            option_matches: vec![],
            are_matches_dirty: false,
        }
    }
}

impl Autocomplete {
    fn new() -> Self {
        Self { id: None }
    }
}

use eframe::epaint::{pos2, text::cursor::Cursor, vec2, Color32, FontId, Pos2, Rect};
use egui::{text_edit::CCursorRange, Area, Id, Response, Ui, Widget, Modifiers, Event, Key};

use crate::ui;

pub fn create<'a>(search: &'a mut String, options: &'a Vec<AutocompleteOption>, multiline: bool, appear_below: bool) -> impl Widget + 'a {
    move |ui: &mut egui::Ui| autocomplete_ui(ui, search, options, multiline, appear_below)
}

pub fn get_current_word(cursor_index: Option<usize>, text: &String) -> Option<(String, (usize, usize))> {
    if let Some(mut cursor_index) = cursor_index {
        if cursor_index == text.len() && cursor_index > 0 {
            cursor_index -= 1;
        }


        let chars = text.chars().collect::<Vec<_>>();
        let (mut starting_index, mut ending_index) = (cursor_index, cursor_index);
        loop {
            if let Some(char) = chars.get(starting_index) {
                // if we reached whitespace, we either started at whitespace or moved into it
                if char.is_whitespace() {
                    // if starting_index != ending_index, we moved into it, so increment to exclude the whitespace
                    if starting_index != ending_index {
                        starting_index += 1;
                    }
                    // otherwise, just break
                    break;
                }
                if starting_index > 0 {
                    starting_index -= 1;
                    continue;
                }
            }
            break;
        }
        loop {
            if let Some(char) = chars.get(ending_index) {
                if !char.is_whitespace() {
                    ending_index += 1;
                    continue;
                }
            }
            break;
        }

        if starting_index == ending_index {
            return None;
        }

        let current_word = chars[starting_index..ending_index].iter().collect::<String>();
        return Some((current_word, (starting_index, ending_index)));
    }
    None
}

pub fn autocomplete_ui(ui: &mut egui::Ui, search: &mut String, options: &Vec<AutocompleteOption>, multiline: bool, appear_below: bool) -> Response {
    
    let consume_key = |ui: &Ui, key: Key| -> bool {
        ui.ctx().input_mut().consume_key(Modifiers::NONE, key)
    };
    let insert_key = |ui: &Ui, key: Key| {
        ui.ctx().input_mut().events.push(Event::Key { key, pressed: true, modifiers: Modifiers::NONE })
    };
    
    let up_arrow_pressed = consume_key(ui, Key::ArrowUp);
    let down_arrow_pressed = consume_key(ui, Key::ArrowDown);
    let tab_pressed = consume_key(ui, Key::Tab);
    
    let mut tedit = if multiline {
        egui::TextEdit::multiline(search)
    } else {
        egui::TextEdit::singleline(search)
    };
    tedit = tedit.lock_focus(true);
    let mut tedit_output = tedit.show(ui);


    let tedit_response = tedit_output.response;

    if tedit_response.has_focus() {
        let id = ui.make_persistent_id(format!("{:?}_autocomplete", tedit_response.id));
        let mut ac_state: AutocompleteState = ui.ctx().memory().data.get_persisted(id).unwrap_or_default();
        let last_ccursor_range = tedit_output.state.ccursor_range();

        if tedit_response.changed() {
            ac_state.are_matches_dirty = true;
        }

  
        let cursor_index = last_ccursor_range.map(|ccursor_range| ccursor_range.primary.index);

        if let Some((current_word, word_index_range)) = get_current_word(cursor_index, search) {
            let ac_matches = autocomplete(&current_word, options).to_owned();
            if ac_matches.len() > 0 {
                let set_ccursor_range = |range: Option<CCursorRange>| {
                    tedit_output.state.set_ccursor_range(range);
                    tedit_output.state.store(ui.ctx(), tedit_response.id);
                };

                if ac_state.selected_index as usize > ac_matches.len() {
                    ac_state.selected_index = ac_matches.len() as i32 - 1;
                }

                if up_arrow_pressed {
                    ac_state.selected_index = (ac_state.selected_index - 1).max(0);
                    // set_ccursor_range(ac_state.last_ccursor_range);
                } else if down_arrow_pressed {
                    ac_state.selected_index = (ac_state.selected_index + 1).min(ac_matches.len() as i32 - 1);
                    // set_ccursor_range(ac_state.last_ccursor_range);
                } else if tab_pressed {
                    if let Some(ac_match) = ac_matches.get(ac_state.selected_index as usize) {
                        let insert_str = format!("{} ", ac_match.value);
                        search.drain(word_index_range.0..word_index_range.1);
                        search.push_str(insert_str.as_str());

                        let len_diff = insert_str.len() as i32 - (word_index_range.1 as i32 - word_index_range.0 as i32);

                        let ccursor_range = last_ccursor_range.map(|ccursor_range| {
                            let mut ccursor_range = ccursor_range.clone();
                            ccursor_range.primary.index = (len_diff + ccursor_range.primary.index as i32).max(0) as usize;
                            ccursor_range.secondary.index = ccursor_range.primary.index;
                            ccursor_range
                        });
                        set_ccursor_range(ccursor_range);
                        ac_state.selected_index = 0;
                    }
                }

                let mut ac_rect = tedit_response.rect;
                let ac_rect_margin = 3.;
                let ac_rect_padding = 3.;
                let ac_rect_inner_padding = 2.;

                let visuals = ui.style().interact_selectable(&tedit_response, false);
                let icon_font = FontId::default();

                let mut text_height = 0.;
                let mut ac_height = ac_rect_padding * 2.;
                let painter = ui.painter();
                let text_galleys = ac_matches
                    .iter()
                    .map(|option| {
                        let text_galley = painter.layout_no_wrap(
                            option.label.replace("_", " "),
                            icon_font.clone(),
                            option.color.unwrap_or(ui::constants::DEFAULT_TEXT_COLOR),
                        );

                        let desc_galley =
                            painter
                                .layout_no_wrap(option.description.clone(), icon_font.clone(), ui::constants::DEFAULT_TEXT_COLOR);

                        ac_height += text_galley.rect.height() + ac_rect_inner_padding;
                        if text_height == 0. {
                            text_height = text_galley.rect.height();
                        }
                        (text_galley, desc_galley, option.color)
                    })
                    .collect::<Vec<_>>();

                ac_height -= ac_rect_inner_padding;
                if appear_below {
                    ac_rect.set_top(tedit_response.rect.bottom() + ac_rect_margin);
                    ac_rect.set_bottom(tedit_response.rect.bottom() + ac_rect_margin + ac_height);
                } else {
                    ac_rect.set_top(tedit_response.rect.top() - ac_height - ac_rect_margin);
                    ac_rect.set_bottom(tedit_response.rect.top() - ac_rect_margin);
                }
                // ac_rect.set_width(1000.);

                Area::new(id).interactable(true).order(egui::Order::Tooltip).fixed_pos(Pos2::ZERO).show(&ui.ctx(), |ui: &mut Ui| {
                    let screen_rect = ui.ctx().input().screen_rect;
                    // let area_response = ui.allocate_response(screen_rect.size(), Sense::click());
                    // if area_response.clicked() && self.close_on_outside_click {
                    //     self.close();
                    // }
                    // painter.rect_filled(
                    //     screen_rect,
                    //     Rounding::none(),
                    //     self.style.overlay_color,
                    // );
                    let painter = ui.painter();
                    painter.rect(ac_rect, 2., Color32::from_rgb(10, 10, 10), visuals.bg_stroke);
                    let ac_rect_left_top = ac_rect.left_top();
                    let mut index = 0;
                    for (mut text_galley, desc_galley, text_color) in text_galleys {
                        let offset_x = ac_rect_padding;
                        let offset_y = ac_rect_padding + ((index) as f32 * (ac_rect_inner_padding + text_height));

                        let d_offset_x = ac_rect.width() - desc_galley.rect.width() - ac_rect_padding;
                        let text_pos = ac_rect_left_top + vec2(offset_x, offset_y);
                        let desc_pos = ac_rect_left_top + vec2(d_offset_x, offset_y);
                        let interaction_rect = text_galley.rect.clone().translate(text_pos.to_vec2());
                        let text_hovered = tedit_response
                            .ctx
                            .input()
                            .pointer
                            .hover_pos()
                            .map(|p| interaction_rect.contains(p))
                            .unwrap_or(false);
                        let text_selected = ac_state.selected_index == index;

                        if text_selected {
                            text_galley = painter.layout_no_wrap(
                                format!("[ {} ]", text_galley.text().to_string()),
                                icon_font.clone(),
                                text_color.unwrap_or(ui::constants::DEFAULT_TEXT_COLOR),
                            );
                        }

                        painter.galley(text_pos, text_galley);
                        painter.galley(desc_pos, desc_galley);
                        index += 1;
                    }
                });
            }
        }

        // ac_state.last_ccursor_range = last_ccursor_range;
        ui.ctx().memory().data.insert_persisted(id, ac_state);
        ui.ctx().move_to_top(tedit_response.layer_id)
    } else {
        if down_arrow_pressed {
            insert_key(ui, Key::ArrowDown);
        }
        if up_arrow_pressed {
            insert_key(ui, Key::ArrowUp);
        }
    }
    if tab_pressed {
        insert_key(ui, Key::Tab);
    }

    // tedit_response.
    tedit_response
}

fn hamming_distance(a: &String, b: &String) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    let size_difference = a_len.abs_diff(b_len);
    if a.contains(b) || b.contains(a) {
        size_difference
    } else {
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
}

fn autocomplete(search: &String, options: &Vec<AutocompleteOption>) -> Vec<AutocompleteOption> {
    let max_distance_factor = 0.7;
    let min_search_len = 1;
    let mut matches: Vec<(AutocompleteOption, usize)> = vec![];
    if search.len() >= min_search_len {
        for option in options {
            if !option.label.starts_with(search.chars().next().unwrap()) {
                continue;
            }
            let max_distance = (max_distance_factor * option.label.len() as f32) as usize;
            let distance = hamming_distance(&search.to_string(), &option.label);
            if distance <= max_distance {
                matches.push((option.clone(), distance))
            }
        }
        // }
    }

    matches.sort_by_key(|(match_string, distance)| distance.clone());
    matches.into_iter().map(|(match_option, distance)| match_option).collect::<Vec<_>>()
}

// pub fn toggle_ui(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
//     // Widget code can be broken up in four steps:
//     //  1. Decide a size for the widget
//     //  2. Allocate space for it
//     //  3. Handle interactions with the widget (if any)
//     //  4. Paint the widget

//     // 1. Deciding widget size:
//     // You can query the `ui` how much space is available,
//     // but in this example we have a fixed size widget based on the height of a standard button:
//     let desired_size = ui.spacing().interact_size.y * egui::vec2(2.0, 1.0);

//     // 2. Allocating space:
//     // This is where we get a region of the screen assigned.
//     // We also tell the Ui to sense clicks in the allocated region.
//     let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

//     // 3. Interact: Time to check for clicks!
//     if response.clicked() {
//         *on = !*on;
//         response.mark_changed(); // report back that the value changed
//     }

//     // Attach some meta-data to the response which can be used by screen readers:
//     response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, *on, ""));

//     // 4. Paint!
//     // Make sure we need to paint:
//     if ui.is_rect_visible(rect) {
//         // Let's ask for a simple animation from egui.
//         // egui keeps track of changes in the boolean associated with the id and
//         // returns an animated value in the 0-1 range for how much "on" we are.
//         let how_on = ui.ctx().animate_bool(response.id, *on);
//         // We will follow the current style by asking
//         // "how should something that is being interacted with be painted?".
//         // This will, for instance, give us different colors when the widget is hovered or clicked.
//         let visuals = ui.style().interact_selectable(&response, *on);
//         // All coordinates are in absolute screen coordinates so we use `rect` to place the elements.
//         let rect = rect.expand(visuals.expansion);
//         let radius = 0.5 * rect.height();
//         ui.painter().rect(rect, radius, visuals.bg_fill, visuals.bg_stroke);
//         // Paint the circle, animating it from left to right with `how_on`:
//         let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
//         let center = egui::pos2(circle_x, rect.center().y);
//         ui.painter().circle(center, 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);
//     }

//     // All done! Return the interaction response so the user can check what happened
//     // (hovered, clicked, ...) and maybe show a tooltip:
//     response
// }

// /// Here is the same code again, but a bit more compact:
// #[allow(dead_code)]
// fn toggle_ui_compact(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
//     let desired_size = ui.spacing().interact_size.y * egui::vec2(2.0, 1.0);
//     let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
//     if response.clicked() {
//         *on = !*on;
//         response.mark_changed();
//     }
//     response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, *on, ""));

//     if ui.is_rect_visible(rect) {
//         let how_on = ui.ctx().animate_bool(response.id, *on);
//         let visuals = ui.style().interact_selectable(&response, *on);
//         let rect = rect.expand(visuals.expansion);
//         let radius = 0.5 * rect.height();
//         ui.painter().rect(rect, radius, visuals.bg_fill, visuals.bg_stroke);
//         let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
//         let center = egui::pos2(circle_x, rect.center().y);
//         ui.painter().circle(center, 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);
//     }

//     response
// }

// // A wrapper that allows the more idiomatic usage pattern: `ui.add(toggle(&mut my_bool))`
// /// iOS-style toggle switch.
// ///
// /// ## Example:
// /// ``` ignore
// /// ui.add(toggle(&mut my_bool));
// /// ```
// pub fn toggle(on: &mut bool) -> impl egui::Widget + '_ {
//     move |ui: &mut egui::Ui| toggle_ui(ui, on)
// }

// pub fn url_to_file_source_code() -> String {
//     format!("https://github.com/emilk/egui/blob/master/{}", file!())
// }
