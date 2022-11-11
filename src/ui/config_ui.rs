use super::UserInterface;

#[derive(Default)]
pub struct ConfigUI {

}

impl UserInterface for ConfigUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label("hello world! i am config");
    }
}