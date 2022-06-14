use iced::{button};
use iced::{Button, Column, Text};

struct Counter {
    value: i32,
    increment_button: button::State,
    decrement_button: button::State,
}

pub enum Message {
    IncrementPressed,
    DecrementPressed
}

impl Counter {
    pub fn view(&mut self) -> Column<Message> {
        Column::new()
            .push(
                Button::new(&mut self.increment_button, Text::new("+")).on_press(Message::IncrementPressed)
            )
            .push(
                Text::new(self.value.to_string()).size(50),
            )
            .push(Button::new(&mut self.decrement_button, Text::new("-"))).on_press(Message::DecrementPressed)
    }
}