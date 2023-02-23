use std::rc::Rc;

use crate::app::SharedState;

use super::UserInterface;

pub struct DebugUI {
    shared_state: Rc<SharedState>
}

impl DebugUI {
    pub fn new(shared_state: &Rc<SharedState>) -> Self {
        Self {
            shared_state: Rc::clone(shared_state)
        }
    }
}

impl UserInterface for DebugUI {
    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.collapsing("profiler", |ui| {
            puffin_egui::profiler_ui(ui)
        });
    }
}

