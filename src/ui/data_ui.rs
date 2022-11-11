use super::UserInterface;

#[derive(Default)]
pub struct DataUI {

}

impl UserInterface for DataUI {
    fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label("hello world! i am data");
    }
}