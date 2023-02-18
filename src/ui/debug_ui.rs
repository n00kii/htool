use super::UserInterface;

#[derive(Default)]
pub struct DebugUI {

}

impl UserInterface for DebugUI {
    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.collapsing("profiler", |_ui| {
            // puffin_egui::profiler_ui(ui)
        });
    }
}

