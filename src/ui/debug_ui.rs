use super::UserInterface;

#[derive(Default)]
pub struct DebugUI {

}

impl UserInterface for DebugUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.collapsing("profiler", |ui| {
            // puffin_egui::profiler_ui(ui)
        });
    }
}

